//! A package's build in one harness's vendor store: everything that package
//! needs in that harness — the plugin it installs, the root `${PLUGIN_ROOT}`
//! names on every route, the hook wrapper — in one directory under
//! `$UZE_HOME/store/<vendor>/`, the only part of the home a harness reads.
//!
//! A build is rendered from the package's sources and owned by one receipt,
//! [`BUILD_KIND`], whose detail holds what decides whether it is current
//! (the build key), how it was made (`origin`, `source`) and the digest of
//! every file UZE wrote into it. Nothing of UZE's bookkeeping sits in the
//! build itself, so nothing of it reaches a vendor's copy.
//!
//! - **Publish** renders a build only when its key differs from the one its
//!   receipt recorded: a file a hook added to an unchanged build stays.
//! - **Inspect** is by content: every file the receipt lists still has its
//!   digest (`Matched`), one was edited or deleted (`Drifted`), or the
//!   directory is gone (`Missing`). A file somebody *added* is not listed,
//!   so it is not drift.
//! - **Detach** removes a matched build, after everything that names it.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    home::UzeHome,
    integration::{
        AttachmentInspection, AttachmentReceipt, AttachmentState, BUILD_IDENTITY, BUILD_KIND,
        ManagedArtifact,
    },
    store::{StoredPackage, is_valid_qualified_id},
};

use crate::shared::package_root::{CARRIED_DIR, PluginRoot};

/// What one harness's vendor store is called and how its builds are named.
pub(crate) struct VendorStore {
    /// The vendor word `UzeHome::vendor_store_dir` takes.
    pub vendor: &'static str,
    /// Bumped whenever this harness's rendering of a build changes, so an
    /// upgrade that renders differently replaces every build.
    pub dialect: u32,
    /// The directory name of a package's build, from its id.
    pub build_name: fn(&str) -> String,
    /// What a build leaves out of the plugin root because this harness
    /// would misread it there: every build keeps it under
    /// [`CARRIED_DIR`], where `${PLUGIN_ROOT}` references into it resolve.
    pub carried: &'static [&'static str],
}

/// The package id itself, for a harness that takes it as a directory name.
pub(crate) fn package_id_name(package_id: &str) -> String {
    package_id.to_owned()
}

impl VendorStore {
    pub(crate) fn root(&self, uze_home: &UzeHome) -> PathBuf {
        uze_home.vendor_store_dir(self.vendor)
    }

    pub(crate) fn build_dir(&self, uze_home: &UzeHome, package_id: &str) -> PathBuf {
        uze_home.build_dir(self.vendor, &(self.build_name)(package_id))
    }

    /// The root `${PLUGIN_ROOT}` names on this harness for the package whose
    /// sources are `package_root`: its build. A root outside the sources
    /// (a package read in place by a test or a launcher) is its own, whole.
    pub(crate) fn root_of(&self, package_root: &Path) -> PluginRoot {
        match UzeHome::installed_package_of(package_root) {
            Some((home, id)) => self.build_root(self.build_dir(&home, id.as_str())),
            None => PluginRoot::whole(package_root),
        }
    }

    /// The build at `build` as the root `${PLUGIN_ROOT}` names.
    pub(crate) fn build_root(&self, build: impl Into<PathBuf>) -> PluginRoot {
        PluginRoot::carrying(build, self.carried)
    }
}

/// Whether a build carries the harness's own plugin envelope as its author
/// wrote it, or one UZE generated (ADR-013's precedence, per package and per
/// harness). A build with neither is a package root only: no plugin is
/// installed from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Origin {
    Explicit,
    Generated,
}

impl Origin {
    const fn word(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Generated => "generated",
        }
    }

    fn read(word: &str) -> Option<Self> {
        [Self::Explicit, Self::Generated]
            .into_iter()
            .find(|origin| origin.word() == word)
    }
}

/// Where the bytes a build was rendered from came from: the commit the lock
/// pins, or a checkout the operator develops in (`uze market link`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildSource {
    Pinned,
    Linked,
}

impl BuildSource {
    const fn word(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::Linked => "linked",
        }
    }

    fn read(word: &str) -> Option<Self> {
        [Self::Pinned, Self::Linked]
            .into_iter()
            .find(|source| source.word() == word)
    }
}

/// Everything that decides whether a build is current. Any part changing —
/// the package's bytes, the UZE that rendered it, this harness's rendering,
/// where it sits, the name it is installed under — renders it again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildKey {
    pub digest: String,
    pub uze: String,
    pub dialect: u32,
    pub root: PathBuf,
    pub name: String,
}

impl BuildKey {
    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "digest": self.digest,
            "uze": self.uze,
            "dialect": self.dialect,
            "root": self.root,
            "name": self.name,
        })
    }

    fn read(value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| value.get(key)?.as_str().map(str::to_owned);
        Some(Self {
            digest: text("digest")?,
            uze: text("uze")?,
            dialect: u32::try_from(value.get("dialect")?.as_u64()?).ok()?,
            root: PathBuf::from(text("root")?),
            name: text("name")?,
        })
    }
}

/// What a build receipt records, read back.
#[derive(Clone, Debug)]
pub(crate) struct RecordedBuild {
    pub root: PathBuf,
    pub key: Option<BuildKey>,
    pub origin: Option<Origin>,
    pub source: Option<BuildSource>,
    pub files: BTreeMap<String, String>,
}

impl RecordedBuild {
    pub(crate) fn of(receipt: &AttachmentReceipt) -> Option<Self> {
        let ManagedArtifact::IntegrationOwned { kind, detail, .. } = &receipt.artifact else {
            return None;
        };
        if kind != BUILD_KIND {
            return None;
        }
        let root = PathBuf::from(detail.get("root")?.as_str()?);
        let word = |key: &str| detail.get(key).and_then(serde_json::Value::as_str);
        Some(Self {
            root,
            key: detail.get("key").and_then(BuildKey::read),
            origin: word("origin").and_then(Origin::read),
            source: word("source").and_then(BuildSource::read),
            files: detail
                .get("files")
                .and_then(serde_json::Value::as_object)
                .map(|files| {
                    files
                        .iter()
                        .filter_map(|(path, digest)| {
                            Some((path.clone(), digest.as_str()?.to_owned()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// Whether `artifact` is a build receipt's.
pub(crate) fn is_build(artifact: &ManagedArtifact) -> bool {
    matches!(artifact, ManagedArtifact::IntegrationOwned { kind, .. } if kind == BUILD_KIND)
}

/// The version every build key records: an upgrade of UZE renders again.
pub(crate) const UZE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One build to publish.
pub(crate) struct Publication<'a> {
    pub store: &'a VendorStore,
    pub uze_home: &'a UzeHome,
    pub integration_id: &'a str,
    pub package: &'a StoredPackage,
    pub digest: &'a str,
    pub source: BuildSource,
    pub recorded: Option<&'a AttachmentReceipt>,
}

impl<'a> Publication<'a> {
    pub(crate) fn new(
        store: &'a VendorStore,
        uze_home: &'a UzeHome,
        integration_id: &'a str,
        package: &'a StoredPackage,
        inputs: &'a uze_core::integration::BuildInputs<'a>,
        recorded: Option<&'a AttachmentReceipt>,
    ) -> Self {
        Self {
            store,
            uze_home,
            integration_id,
            package,
            digest: inputs.digest,
            source: if inputs.linked {
                BuildSource::Linked
            } else {
                BuildSource::Pinned
            },
            recorded,
        }
    }
}

/// Renders the build when its key changed, and answers with the receipt
/// owning it. `render` writes the build into a fresh staging directory (its
/// first argument) for the root it will be swapped into (its second, the
/// one every path inside it names) and answers with its origin.
pub(crate) fn publish(
    publication: Publication<'_>,
    render: impl FnOnce(&Path, &Path) -> Result<Option<Origin>>,
) -> Result<AttachmentReceipt> {
    let Publication {
        store,
        uze_home,
        integration_id,
        package,
        digest,
        source,
        recorded,
    } = publication;
    let root = store.build_dir(uze_home, package.id.as_str());
    let key = BuildKey {
        digest: digest.to_owned(),
        uze: UZE_VERSION.to_owned(),
        dialect: store.dialect,
        root: root.clone(),
        name: package.active_name.clone(),
    };
    if let Some(recorded) = recorded {
        let inspection = inspect(recorded);
        // The same bytes from a link and then from the pinned commit are two
        // builds: the receipt says which source it was rendered from.
        let same_key = RecordedBuild::of(recorded)
            .is_some_and(|build| build.key.as_ref() == Some(&key) && build.source == Some(source));
        match inspection.state {
            AttachmentState::Matched if same_key => return Ok(recorded.clone()),
            // An edit is the person's: kept as it is, and said by inspection
            // until they resolve it, never written over by an update.
            AttachmentState::Drifted => {
                tracing::warn!(
                    build = %root.display(),
                    reason = %inspection.reason,
                    "a build was edited after UZE wrote it; it is kept"
                );
                return Ok(recorded.clone());
            }
            _ => {}
        }
    }
    // Bytes missing: what was built stays, and nothing is built again until
    // they return.
    if !package.root.is_dir() {
        return match recorded {
            Some(recorded) => Ok(recorded.clone()),
            None => Err(UzeError::MissingPath(package.root.clone())),
        };
    }
    let mut origin = None;
    uze_core::persistence::replace_dir(&root, |staging| {
        origin = render(staging, &root)?;
        Ok(())
    })?;
    let files = file_digests(&root)?;
    let mut detail = BTreeMap::new();
    detail.insert("root".to_owned(), serde_json::json!(root));
    detail.insert("key".to_owned(), key.to_value());
    detail.insert(
        "origin".to_owned(),
        origin.map_or(serde_json::Value::Null, |origin| {
            serde_json::json!(origin.word())
        }),
    );
    detail.insert("source".to_owned(), serde_json::json!(source.word()));
    detail.insert("files".to_owned(), serde_json::json!(files));
    Ok(AttachmentReceipt {
        package_id: package.id.as_str().to_owned(),
        resource_identity: Some(BUILD_IDENTITY.to_owned()),
        integration: integration_id.to_owned(),
        artifact: ManagedArtifact::IntegrationOwned {
            kind: BUILD_KIND.to_owned(),
            selector: (store.build_name)(package.id.as_str()),
            detail,
        },
    })
}

/// The digest of every file under `root`, by its path relative to it, in
/// `/` form on every platform.
pub(crate) fn file_digests(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut digests = BTreeMap::new();
    collect(root, root, &mut digests)?;
    Ok(digests)
}

fn collect(root: &Path, dir: &Path, digests: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(UzeError::read(dir))? {
        let entry = entry.map_err(UzeError::read(dir))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(UzeError::read(&path))?;
        if kind.is_dir() {
            collect(root, &path, digests)?;
        } else if kind.is_file() {
            let bytes = fs::read(&path).map_err(UzeError::read(&path))?;
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            digests.insert(relative, uze_core::digest::sha256(&bytes));
        }
    }
    Ok(())
}

/// A build's state, by content (see the module doc).
pub(crate) fn inspect(receipt: &AttachmentReceipt) -> AttachmentInspection {
    let Some(build) = RecordedBuild::of(receipt) else {
        return state(AttachmentState::Blocked, "the build receipt is unreadable");
    };
    if !build.root.is_dir() {
        return state(AttachmentState::Missing, "the build is absent");
    }
    let edited: Vec<&str> = build
        .files
        .iter()
        .filter(|(relative, digest)| {
            fs::read(build.root.join(relative.as_str()))
                .map(|bytes| &uze_core::digest::sha256(&bytes) != *digest)
                .unwrap_or(true)
        })
        .map(|(relative, _)| relative.as_str())
        .collect();
    if edited.is_empty() {
        state(AttachmentState::Matched, "the build matches its receipt")
    } else {
        AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: format!(
                "the build at {} was changed after UZE wrote it: {}",
                build.root.display(),
                edited.join(", ")
            ),
        }
    }
}

/// Removes a matched build. Anything else is answered with its inspection
/// and left where it is.
pub(crate) fn detach(receipt: &AttachmentReceipt) -> Result<AttachmentInspection> {
    let inspection = inspect(receipt);
    if inspection.state != AttachmentState::Matched {
        return Ok(inspection);
    }
    let build = RecordedBuild::of(receipt).expect("an inspected build receipt");
    // The root comes from the ledger: refuse one that is not a build of a
    // package id rather than remove whatever it names.
    let named = build
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| is_valid_qualified_id(&receipt.package_id) && !name.starts_with('.'));
    if !named {
        return Ok(state(
            AttachmentState::Blocked,
            "the build receipt names no build directory",
        ));
    }
    fs::remove_dir_all(&build.root).map_err(UzeError::write(&build.root))?;
    Ok(state(AttachmentState::Missing, "the build was removed"))
}

/// Every build receipt `integration_id` holds, recorded in the ledger.
pub(crate) fn recorded_builds(
    uze_home: &UzeHome,
    integration_id: &str,
) -> Result<Vec<(AttachmentReceipt, RecordedBuild)>> {
    Ok(uze_core::state::receipts(uze_home, None)?
        .into_iter()
        .filter(|receipt| receipt.integration == integration_id)
        .filter_map(|receipt| RecordedBuild::of(&receipt).map(|build| (receipt, build)))
        .collect())
}

/// The build `integration_id` holds for one package.
pub(crate) fn recorded_build(
    uze_home: &UzeHome,
    integration_id: &str,
    package_id: &str,
) -> Result<Option<RecordedBuild>> {
    Ok(uze_core::state::receipts(uze_home, Some(package_id))?
        .into_iter()
        .filter(|receipt| receipt.integration == integration_id)
        .find_map(|receipt| RecordedBuild::of(&receipt)))
}

fn state(state: AttachmentState, reason: &str) -> AttachmentInspection {
    AttachmentInspection {
        state,
        reason: reason.to_owned(),
    }
}

/// Mirrors the whole package into a build being rendered, as real files:
/// everything but `excluded` and the package's rules where the package has
/// it, and each of `carried` (the paths this harness would misread at a
/// plugin root, [`VendorStore::carried`]) under [`CARRIED_DIR`] as well, so
/// a `${PLUGIN_ROOT}` reference into one names a file that exists.
/// `excluded` is either `carried` (a generated build) or nothing (a build
/// the author shipped the harness's envelope for). The rules reach a
/// harness per project and never machine-wide.
pub(crate) fn mirror_package(
    package: &StoredPackage,
    staging: &Path,
    excluded: &[&str],
    carried: &[&str],
) -> Result<()> {
    let package_root =
        uze_core::path::canonical(&package.root).map_err(UzeError::read(&package.root))?;
    let rules = &[crate::rules::PACKAGE_RULES_DIR];
    let excluded = [excluded, rules].concat();
    crate::shared::tree::mirror_tree(&package.root, staging, &package_root, &excluded)?;
    let carried_root = staging.join(CARRIED_DIR);
    for path in carried {
        crate::shared::tree::mirror_path(&package.root, path, &carried_root, &package_root)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
