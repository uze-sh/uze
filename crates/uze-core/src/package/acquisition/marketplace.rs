//! `marketplace.json` — the contract a marketplace root answers "which
//! plugins exist here, and where" with.
//!
//! Reading a manifest and resolving a plugin name against a directory is a
//! pure, deterministic, offline primitive, and has no opinion on how that
//! directory got there. Nothing here names a specific plugin; adding one to
//! `marketplace.json` and its directory under the root is the entire
//! integration surface.
//!
//! # A marketplace is a Git repository
//!
//! [`repository_of`] is the one thing in this module that is not pure, and
//! it is here because it states what a marketplace *is*: a Git repository,
//! whether it is a URL or a directory on this machine. Not because Git is
//! how the bytes travel — a plain directory would do that — but because a
//! commit is the only thing that answers the two questions a distribution
//! layer exists to answer: *are these the same bytes I installed?* and *is
//! there something newer?* A directory answers neither; the best it can do
//! is a timestamp, which says when UZE last looked rather than whether
//! anything changed.
//!
//! A local checkout is therefore not a lesser kind of marketplace. It is
//! the same kind, read from a clone that happens to be on this disk, and
//! it pins exactly as well.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    acquisition::PackageSource,
    error::{Result, UzeError},
};

/// Where a marketplace is read from, and what it is called elsewhere.
///
/// The two differ for a local clone: `fetch` is the directory on this
/// machine, and `identity` is the URL another machine resolves the same
/// repository by. A lock records the identity — a path only this machine
/// has is not something a project can be reproduced from — while the
/// machine that has the clone keeps reading it locally.
///
/// `subpath` is where the catalogue sits inside that repository. It is
/// beside the identity rather than in it: two marketplaces in one
/// repository are one repository read at two directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketplaceRepository {
    pub fetch: String,
    pub identity: String,
    pub subpath: MarketplaceSubpath,
}

/// The directory of a repository a marketplace's catalogue sits in: its
/// root, or a relative path below it.
///
/// The one answer to "where is the catalogue, and where is a plugin": every
/// reader, mirrored or linked, asks it rather than composing a path of its
/// own, which is how a reader comes to read the root of a marketplace that
/// is not there.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketplaceSubpath(Option<String>);

impl MarketplaceSubpath {
    /// The repository's root.
    pub fn root() -> Self {
        Self(None)
    }

    /// A typed or recorded subpath, normalized. Judged lexically, like a
    /// plugin's `source`: a mirror has no directory to canonicalize
    /// against, so a `..` or an absolute path is refused as written.
    pub fn parse(subpath: &Path) -> Result<Self> {
        let escapes = || UzeError::MarketplaceSubpathEscapes {
            subpath: subpath.display().to_string(),
        };
        if subpath.is_absolute() {
            return Err(escapes());
        }
        let mut parts = Vec::new();
        for component in subpath.components() {
            match component {
                std::path::Component::Normal(part) => {
                    parts.push(part.to_str().ok_or_else(escapes)?)
                }
                std::path::Component::CurDir => {}
                _ => return Err(escapes()),
            }
        }
        Ok(Self((!parts.is_empty()).then(|| parts.join("/"))))
    }

    /// [`Self::parse`] of an optional subpath, absent meaning the root.
    pub fn of(subpath: Option<&Path>) -> Result<Self> {
        subpath.map_or_else(|| Ok(Self::root()), Self::parse)
    }

    /// Where `directory` sits inside the checkout whose top level is
    /// `toplevel`, refusing one outside it.
    pub fn within_checkout(toplevel: &Path, directory: &Path) -> Result<Self> {
        let toplevel = toplevel
            .canonicalize()
            .unwrap_or_else(|_| toplevel.to_path_buf());
        let canonical = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.to_path_buf());
        let relative =
            canonical
                .strip_prefix(&toplevel)
                .map_err(|_| UzeError::MarketplaceSubpathEscapes {
                    subpath: directory.display().to_string(),
                })?;
        Self::parse(relative)
    }

    pub fn is_root(&self) -> bool {
        self.0.is_none()
    }

    /// As a repository-relative path, `None` at the root — the spelling a
    /// source and a lock entry carry.
    pub fn as_path(&self) -> Option<PathBuf> {
        self.0.as_ref().map(PathBuf::from)
    }

    /// The catalogue's path relative to the repository.
    pub fn manifest_path(&self) -> String {
        self.join(crate::anchor::MARKETPLACE_MANIFEST_NAME)
    }

    /// The repository-relative directory `plugin` occupies, `.` for the
    /// whole repository. Containment is [`plugin_subdirectory`]'s: a plugin
    /// is inside its marketplace, and so inside this directory.
    pub fn plugin_path(&self, manifest: &MarketplaceManifest, plugin: &str) -> Result<String> {
        let within = plugin_subdirectory(manifest, plugin)?;
        Ok(if within == "." {
            self.0.clone().unwrap_or_else(|| ".".to_owned())
        } else {
            self.join(&within)
        })
    }

    /// The catalogue's directory inside `checkout`, a working tree of the
    /// repository. Canonicalized and checked, because a working tree can
    /// hold a symbolic link where the history holds a directory.
    pub fn directory_in(&self, checkout: &Path) -> Result<PathBuf> {
        let Some(subpath) = &self.0 else {
            return Ok(checkout.to_path_buf());
        };
        let joined = checkout.join(subpath);
        let canonical = joined
            .canonicalize()
            .map_err(|_| UzeError::MissingPath(joined.clone()))?;
        let top = checkout.canonicalize().map_err(|source| UzeError::Read {
            path: checkout.to_path_buf(),
            source,
        })?;
        if !canonical.starts_with(&top) {
            return Err(UzeError::MarketplaceSubpathEscapes {
                subpath: subpath.clone(),
            });
        }
        Ok(canonical)
    }

    fn join(&self, relative: &str) -> String {
        match &self.0 {
            Some(subpath) => format!("{subpath}/{relative}"),
            None => relative.to_owned(),
        }
    }
}

impl std::fmt::Display for MarketplaceSubpath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_deref().unwrap_or("."))
    }
}

/// The repository behind a marketplace source, refusing anything that is
/// not one.
///
/// A local directory must be a Git work tree with at least one commit. Its
/// identity is `origin` when it has one — the URL a teammate would clone,
/// in its canonical spelling — and otherwise the checkout's own absolute
/// path, which is still a valid Git URL and is honest about resolving
/// nowhere else.
pub fn repository_of(source: &PackageSource) -> Result<MarketplaceRepository> {
    match source {
        PackageSource::Git {
            url, subdirectory, ..
        } => Ok(MarketplaceRepository {
            fetch: url.clone(),
            identity: super::forge::canonical(url),
            subpath: MarketplaceSubpath::of(subdirectory.as_deref())?,
        }),
        PackageSource::Local { path } => {
            let toplevel =
                git_answer(path, &["rev-parse", "--show-toplevel"]).ok_or_else(|| {
                    UzeError::MarketplaceNotARepository {
                        path: path.to_path_buf(),
                    }
                })?;
            // A repository with no commit has no revision to pin and
            // nothing to compare a later one against.
            if git_answer(path, &["rev-parse", "HEAD"]).is_none() {
                return Err(UzeError::MarketplaceNotARepository {
                    path: path.to_path_buf(),
                });
            }
            let identity = git_answer(path, &["remote", "get-url", "origin"]).map_or_else(
                || toplevel.clone(),
                |origin| super::forge::canonical(&origin),
            );
            let subpath = MarketplaceSubpath::within_checkout(Path::new(&toplevel), path)?;
            Ok(MarketplaceRepository {
                fetch: toplevel,
                identity,
                subpath,
            })
        }
        PackageSource::Embedded { id } => Err(UzeError::AcquisitionFailed(format!(
            "`{id}` is built into UZE and is not a repository"
        ))),
    }
}

/// Whether `checkout` has nothing in it yet — absent, or an empty
/// directory — so a link there is a clone to make rather than one to read.
pub fn checkout_is_vacant(checkout: &Path) -> bool {
    match std::fs::read_dir(checkout) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Clones the repository behind `source` into `checkout`, for a
/// marketplace linked on a machine that has no working copy of it yet.
///
/// Unlike the mirrors acquisition keeps, this clone is the operator's: they
/// develop in it and push from it, so Git runs with their own environment
/// and credentials. It tries every way the machine may reach the
/// repository, in the order acquisition does, and keeps the first that
/// answers.
pub fn clone_checkout(source: &PackageSource, checkout: &Path) -> Result<()> {
    let fetch = repository_of(source)?.fetch;
    let parent = checkout
        .parent()
        .ok_or_else(|| UzeError::MissingPath(checkout.to_path_buf()))?;
    std::fs::create_dir_all(parent).map_err(|source| UzeError::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut urls: Vec<String> = Vec::new();
    for transport in super::forge::transports(&fetch)? {
        if !urls.contains(&transport.url) {
            urls.push(transport.url);
        }
    }
    let target = checkout.to_string_lossy();
    let mut failures = Vec::new();
    for url in &urls {
        let answered = uze_git::write_within(
            parent,
            &["clone", "--", url, &target],
            uze_git::NETWORK_TIMEOUT,
        )
        .map_err(|error| error.to_string())
        .and_then(uze_git::Output::successful);
        match answered {
            Ok(_) => return Ok(()),
            Err(reason) => failures.push(format!("{url}: {reason}")),
        }
    }
    Err(UzeError::AcquisitionFailed(format!(
        "could not clone {fetch} into {}:\n  {}",
        checkout.display(),
        failures.join("\n  ")
    )))
}

/// One Git question with a single-line answer, or `None` when Git said no.
/// Every caller here is asking something whose absence is an answer — not
/// a repository, no commit yet, no remote — so a non-zero exit is not an
/// error to propagate.
fn git_answer(root: &Path, args: &[&str]) -> Option<String> {
    let output = uze_git::read(root, args).ok()?;
    let stdout = output.successful().ok()?;
    let answer = stdout.trim();
    (!answer.is_empty()).then(|| answer.to_owned())
}

#[derive(Clone, Debug, Deserialize)]
pub struct MarketplaceManifest {
    pub name: String,
    #[serde(default)]
    pub owner: Option<MarketplaceOwner>,
    pub plugins: Vec<MarketplacePluginEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MarketplaceOwner {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MarketplacePluginEntry {
    pub name: String,
    /// A path relative to the marketplace root. Nothing else is supported
    /// yet — a bare Git or registry reference here would need its own
    /// acquisition pass this module deliberately does not perform.
    pub source: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Parses and validates a `marketplace.json` payload. Validation is limited
/// to what this module itself must be able to rely on: well-formed JSON
/// matching the shape, a marketplace name every package id it qualifies
/// can carry, and no two plugins claiming the same name. It does
/// not check that any `source` exists — that happens per-lookup in
/// [`resolve_plugin_source`], scoped to the one entry actually requested.
pub fn parse_manifest(bytes: &[u8]) -> Result<MarketplaceManifest> {
    let manifest: MarketplaceManifest =
        serde_json::from_slice(bytes).map_err(|source| UzeError::Json {
            path: PathBuf::from("marketplace.json"),
            source,
        })?;
    // Checked on read rather than only on record: a Git marketplace's mirror
    // is keyed by this name before anything is recorded.
    if !crate::store::is_valid_package_name(&manifest.name) {
        return Err(UzeError::InvalidMarketplaceName(manifest.name));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &manifest.plugins {
        if !seen.insert(entry.name.as_str()) {
            return Err(UzeError::DuplicateMarketplacePlugin(entry.name.clone()));
        }
    }
    Ok(manifest)
}

/// The repository-relative directory `plugin_name` occupies, judged
/// without a filesystem.
///
/// [`resolve_plugin_source`] answers the same question against a directory
/// that exists, by canonicalizing and comparing. A mirror has no directory
/// to canonicalize against — the point of asking is to decide what to write
/// out — so containment is enforced lexically instead: no component may be
/// `..`, and the path may not be absolute. Both are what would let a
/// manifest name a directory outside the repository it belongs to.
pub fn plugin_subdirectory(manifest: &MarketplaceManifest, plugin_name: &str) -> Result<String> {
    let entry = manifest
        .plugins
        .iter()
        .find(|entry| entry.name == plugin_name)
        .ok_or_else(|| UzeError::UnknownPackage(plugin_name.to_owned()))?;
    let source = Path::new(&entry.source);
    let unsafe_reference = || UzeError::UnsafePathReference {
        path: PathBuf::from("marketplace.json"),
        reference: entry.source.clone(),
    };
    if source.is_absolute() {
        return Err(unsafe_reference());
    }
    let mut parts = Vec::new();
    for component in source.components() {
        match component {
            std::path::Component::Normal(part) => {
                parts.push(part.to_str().ok_or_else(unsafe_reference)?)
            }
            std::path::Component::CurDir => {}
            _ => return Err(unsafe_reference()),
        }
    }
    if parts.is_empty() {
        // The marketplace root itself: the whole repository is the plugin.
        return Ok(".".to_owned());
    }
    Ok(parts.join("/"))
}

/// Resolves `plugin_name` against `manifest` to an absolute, canonicalized
/// directory under `marketplace_root`. Rejects a `source` that does not
/// exist and one that escapes `marketplace_root` (a `../` or an absolute
/// path smuggled into the manifest) — the same containment rule package
/// acquisition already holds every source to.
pub fn resolve_plugin_source(
    manifest: &MarketplaceManifest,
    plugin_name: &str,
    marketplace_root: &Path,
) -> Result<PathBuf> {
    let entry = manifest
        .plugins
        .iter()
        .find(|entry| entry.name == plugin_name)
        .ok_or_else(|| UzeError::UnknownPackage(plugin_name.to_owned()))?;
    let canonical_root = marketplace_root
        .canonicalize()
        .map_err(|source| UzeError::Read {
            path: marketplace_root.to_path_buf(),
            source,
        })?;
    let joined = canonical_root.join(&entry.source);
    let canonical_source = joined
        .canonicalize()
        .map_err(|_| UzeError::MissingPath(joined.clone()))?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(UzeError::UnsafePathReference {
            path: marketplace_root.to_path_buf(),
            reference: entry.source.clone(),
        });
    }
    Ok(canonical_source)
}

#[cfg(test)]
mod subdirectory_tests {
    use super::*;

    fn manifest(source: &str) -> MarketplaceManifest {
        parse_manifest(
            format!(r#"{{"name":"m","plugins":[{{"name":"flow","source":"{source}"}}]}}"#)
                .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn a_plain_subdirectory_is_returned_as_a_repository_relative_path() {
        assert_eq!(
            plugin_subdirectory(&manifest("./plugins/flow"), "flow").unwrap(),
            "plugins/flow"
        );
    }

    #[test]
    fn the_marketplace_root_itself_is_the_whole_repository() {
        assert_eq!(plugin_subdirectory(&manifest("."), "flow").unwrap(), ".");
    }

    #[test]
    fn a_source_climbing_out_of_the_repository_is_refused() {
        assert!(matches!(
            plugin_subdirectory(&manifest("../elsewhere"), "flow"),
            Err(UzeError::UnsafePathReference { .. })
        ));
        assert!(matches!(
            plugin_subdirectory(&manifest("plugins/../../elsewhere"), "flow"),
            Err(UzeError::UnsafePathReference { .. })
        ));
    }

    #[test]
    fn an_absolute_source_is_refused() {
        assert!(matches!(
            plugin_subdirectory(&manifest("/etc"), "flow"),
            Err(UzeError::UnsafePathReference { .. })
        ));
    }

    #[test]
    fn a_subpath_resolves_the_catalogue_and_its_plugins_below_it() {
        let subpath = MarketplaceSubpath::parse(Path::new("./aikit/")).unwrap();
        assert_eq!(subpath.as_path(), Some(PathBuf::from("aikit")));
        assert_eq!(subpath.manifest_path(), "aikit/marketplace.json");
        assert_eq!(
            subpath.plugin_path(&manifest("./flow"), "flow").unwrap(),
            "aikit/flow"
        );
        assert_eq!(
            subpath.plugin_path(&manifest("."), "flow").unwrap(),
            "aikit"
        );
    }

    #[test]
    fn the_root_subpath_reads_exactly_what_a_root_marketplace_always_read() {
        let root = MarketplaceSubpath::of(None).unwrap();
        assert_eq!(MarketplaceSubpath::parse(Path::new(".")).unwrap(), root);
        assert_eq!(root.as_path(), None);
        assert_eq!(root.manifest_path(), "marketplace.json");
        assert_eq!(
            root.plugin_path(&manifest("./plugins/flow"), "flow")
                .unwrap(),
            "plugins/flow"
        );
        assert_eq!(root.plugin_path(&manifest("."), "flow").unwrap(), ".");
    }

    #[test]
    fn a_subpath_leaving_the_repository_is_refused() {
        for escaping in ["..", "../elsewhere", "aikit/../../elsewhere", "/etc"] {
            assert!(
                matches!(
                    MarketplaceSubpath::parse(Path::new(escaping)),
                    Err(UzeError::MarketplaceSubpathEscapes { .. })
                ),
                "{escaping}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_working_tree_directory_linked_outside_the_checkout_is_refused() {
        let root = uze_testkit::temp::scratch("subpath-symlink");
        let checkout = root.join("checkout");
        let outside = root.join("outside");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, checkout.join("aikit")).unwrap();

        let refused = MarketplaceSubpath::parse(Path::new("aikit"))
            .unwrap()
            .directory_in(&checkout);

        assert!(
            matches!(refused, Err(UzeError::MarketplaceSubpathEscapes { .. })),
            "{refused:?}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unknown_plugin_is_not_a_containment_failure() {
        assert!(matches!(
            plugin_subdirectory(&manifest("plugins/flow"), "other"),
            Err(UzeError::UnknownPackage(_))
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_manifest(root: &Path, plugins_json: &str) {
        fs::write(
            root.join("marketplace.json"),
            format!(r#"{{"name":"test","plugins":[{plugins_json}]}}"#),
        )
        .unwrap();
    }

    fn plugin_dir(root: &Path, relative: &str) -> PathBuf {
        let dir = root.join(relative);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.json"), r#"{"name":"x"}"#).unwrap();
        dir
    }

    #[test]
    fn a_well_formed_manifest_parses() {
        let manifest = parse_manifest(
            br#"{"name":"uze","plugins":[{"name":"uze","source":"./plugins/uze"}]}"#,
        )
        .unwrap();
        assert_eq!(manifest.name, "uze");
        assert_eq!(manifest.plugins.len(), 1);
    }

    #[test]
    fn a_marketplace_named_outside_the_rule_is_refused_with_the_name_it_meant() {
        let refused = parse_manifest(br#"{"name":"My_Market","plugins":[]}"#)
            .unwrap_err()
            .to_string();
        assert!(refused.contains("`My_Market`"), "{refused}");
        assert!(refused.contains("try `my-market`"), "{refused}");
    }

    #[test]
    fn a_malformed_manifest_is_rejected() {
        assert!(parse_manifest(b"not json").is_err());
        assert!(
            parse_manifest(br#"{"name":"uze"}"#).is_err(),
            "missing plugins field"
        );
    }

    #[test]
    fn a_duplicate_plugin_name_is_rejected() {
        let error = parse_manifest(
            br#"{"name":"uze","plugins":[
                {"name":"foo","source":"./a"},
                {"name":"foo","source":"./b"}
            ]}"#,
        )
        .unwrap_err();
        assert!(matches!(error, UzeError::DuplicateMarketplacePlugin(name) if name == "foo"));
    }

    #[test]
    fn resolves_relative_source_to_an_absolute_canonical_path() {
        let root = uze_testkit::temp::scratch("resolve");
        fs::create_dir_all(&root).unwrap();
        let dir = plugin_dir(&root, "plugins/uze");
        write_manifest(&root, r#"{"name":"uze","source":"./plugins/uze"}"#);
        let manifest = parse_manifest(&fs::read(root.join("marketplace.json")).unwrap()).unwrap();

        let resolved = resolve_plugin_source(&manifest, "uze", &root).unwrap();
        assert_eq!(resolved, dir.canonicalize().unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn two_distinct_plugins_resolve_independently_with_no_special_casing() {
        let root = uze_testkit::temp::scratch("two-plugins");
        fs::create_dir_all(&root).unwrap();
        let first = plugin_dir(&root, "plugins/uze");
        let second = plugin_dir(&root, "plugins/rust-guidelines");
        write_manifest(
            &root,
            r#"{"name":"uze","source":"./plugins/uze"},{"name":"rust-guidelines","source":"./plugins/rust-guidelines"}"#,
        );
        let manifest = parse_manifest(&fs::read(root.join("marketplace.json")).unwrap()).unwrap();

        assert_eq!(
            resolve_plugin_source(&manifest, "uze", &root).unwrap(),
            first.canonicalize().unwrap()
        );
        assert_eq!(
            resolve_plugin_source(&manifest, "rust-guidelines", &root).unwrap(),
            second.canonicalize().unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unknown_plugin_name_is_rejected() {
        let root = uze_testkit::temp::scratch("unknown");
        fs::create_dir_all(&root).unwrap();
        plugin_dir(&root, "plugins/uze");
        write_manifest(&root, r#"{"name":"uze","source":"./plugins/uze"}"#);
        let manifest = parse_manifest(&fs::read(root.join("marketplace.json")).unwrap()).unwrap();

        assert!(matches!(
            resolve_plugin_source(&manifest, "does-not-exist", &root),
            Err(UzeError::UnknownPackage(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_source_directory_is_rejected() {
        let root = uze_testkit::temp::scratch("missing-source");
        fs::create_dir_all(&root).unwrap();
        write_manifest(&root, r#"{"name":"uze","source":"./plugins/uze"}"#);
        let manifest = parse_manifest(&fs::read(root.join("marketplace.json")).unwrap()).unwrap();

        assert!(matches!(
            resolve_plugin_source(&manifest, "uze", &root),
            Err(UzeError::MissingPath(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_source_escaping_the_marketplace_root_is_rejected() {
        let root = uze_testkit::temp::scratch("escape");
        fs::create_dir_all(root.join("marketplace")).unwrap();
        plugin_dir(&root, "outside");
        write_manifest(
            &root.join("marketplace"),
            r#"{"name":"evil","source":"../outside"}"#,
        );
        let manifest =
            parse_manifest(&fs::read(root.join("marketplace").join("marketplace.json")).unwrap())
                .unwrap();

        assert!(matches!(
            resolve_plugin_source(&manifest, "evil", &root.join("marketplace")),
            Err(UzeError::UnsafePathReference { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
