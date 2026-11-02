//! The marketplace Claude Code and Codex install native plugins from
//! (ADR-013): one per harness, `uze-store`, rooted at that harness's vendor
//! store (`shared::build`).
//!
//! The marketplace holds builds and nothing else: a package that ships the
//! harness's own envelope is built from it, one that does not gets an
//! envelope UZE generates, and which of the two a build is (its origin) is
//! recorded by the build's receipt, decided per package and per harness.
//! The catalogue lists every build whose receipt has an origin, and is
//! written from those receipts, never from the package bytes, so a package
//! whose bytes are missing keeps its place.
//!
//! Which packages the catalogue lists, how a build is rendered, what a
//! plugin receipt records and when publication is current are one
//! lifecycle; a [`MarketplaceDialect`] supplies only what the vendor spells
//! differently — its envelope and catalogue files, its JSON, how it
//! preserves a Skill's invocation policy and its `plugin` CLI verbs.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    capability::{CapabilityKind, Resource},
    exposure::{PackageEnvelope, PackageExposurePlan},
    home::UzeHome,
    integration::{
        AttachmentInspection, AttachmentReceipt, ManagedArtifact, PublicationStatus,
        RegistrationRead, VendorRegistration, VendorRegistrationKind,
    },
    router::CompatibilityRoute,
    skill::SkillInvocationPolicy,
    store::StoredPackage,
};

use crate::shared::build::{RecordedBuild, VendorStore};
use crate::shared::earlier::{
    RETIRED_MARKETPLACE, names_retired_marketplace, remove_runtime_leftover,
};
use crate::shared::plan::blocked;

pub(crate) use crate::shared::build::Origin;

/// The one marketplace UZE registers with a harness: its vendor store.
pub(crate) const MARKETPLACE: &str = "uze-store";

/// How the catalogue names its owner in a harness's marketplace UI.
const DISPLAY_NAME: &str = "UZE";

/// What one vendor spells differently about its marketplace.
pub(crate) trait MarketplaceDialect {
    /// The harness's vendor store: the marketplace's root.
    const STORE: &'static VendorStore;
    /// The harness's hooks, whose wrapper each build carries.
    const HOOKS: crate::hooks::HookTarget;
    /// The name `uze setup` takes for this harness.
    const SETUP_NAME: &'static str;
    /// How messages name the catalogue (`Claude marketplace`).
    const CATALOGUE_NOUN: &'static str;
    /// The directory a package's own envelope manifest lives in.
    const ENVELOPE_DIR: &'static str;
    /// The catalogue file, relative to its marketplace root.
    const CATALOGUE_PATH: &'static str;
    /// Receipt `kind` for a package installed from its own envelope.
    const EXPLICIT_KIND: &'static str;
    /// Receipt `kind` for a package installed from a generated envelope.
    const GENERATED_KIND: &'static str;
    const EXPLICIT_EVIDENCE: &'static str;
    const GENERATED_EVIDENCE: &'static str;
    /// Whether the harness loads a plugin's `agents/` and names each one
    /// under the plugin: a generated envelope then carries the package's
    /// agents, and no agent is delivered beside it.
    const ENVELOPE_CARRIES_AGENTS: bool;
    /// Whether registering the marketplace again under its name from
    /// another root moves it there (measured on Claude Code), rather than
    /// being refused until the old registration is removed (Codex).
    const ADD_REPOINTS: bool;

    /// The whole catalogue document around its `plugins`.
    fn catalogue_document(
        name: &str,
        display_name: &str,
        plugins: Vec<serde_json::Value>,
    ) -> serde_json::Value;

    /// One build's catalogue entry: `name` is the plugin's active name,
    /// `source` the build relative to the marketplace root, and `build` the
    /// build itself, whose manifest says what the entry describes.
    fn catalogue_entry(name: &str, source: String, build: &Path) -> serde_json::Value;

    /// Which resources the package's own envelope provides (ADR-013 §2).
    fn explicit_coverage(package: &StoredPackage, resources: &[&Resource]) -> BTreeSet<String>;

    /// Whether a generated envelope carries this Skill policy faithfully
    /// (ADR-030 §6); one it cannot is left to capability-level delivery.
    fn envelope_preserves(policy: SkillInvocationPolicy) -> bool;

    /// Writes the generated manifest and every surface it declares into
    /// `dir`, a build the package was just mirrored into, for the root
    /// `root` it is swapped into.
    fn render_envelope(package: &StoredPackage, dir: &Path, root: &Path) -> Result<()>;

    /// The root the harness has a marketplace called `name` registered at.
    fn registered_root(executable: &Path, home: &Path, name: &str) -> Option<PathBuf>;
    /// The same, read from the harness's own record as a file and never by
    /// starting it: `None` when the record says nothing readable, and
    /// `Some(None)` when it says the marketplace is not registered.
    fn recorded_root(_home: &Path, _name: &str) -> Option<Option<PathBuf>> {
        None
    }
    fn add_marketplace(executable: &Path, home: &Path, root: &Path) -> Result<()>;
    /// Whether the harness already has `selector` installed, for a harness
    /// whose install is not relied on to be idempotent.
    fn plugin_installed(_executable: &Path, _home: &Path, _selector: &str) -> bool {
        false
    }
    fn install_plugin(executable: &Path, home: &Path, selector: &str) -> Result<()>;
    fn inspect_plugin(
        executable: &Path,
        home: &Path,
        selector: &str,
        marketplace_root: &Path,
        detail: &BTreeMap<String, serde_json::Value>,
    ) -> AttachmentInspection;
    fn remove_plugin(executable: &Path, home: &Path, selector: &str) -> Result<()>;
    /// Unregisters a marketplace by name. What happens to the plugins
    /// installed from it is the vendor's, as measured
    /// (`experiments/{claude,codex}/source-tie.py`): Claude Code uninstalls
    /// them; Codex drops them from its listing but keeps each one's
    /// `[plugins."<id>"] enabled = true`, so they come back installed and
    /// enabled once the marketplace is added again.
    fn remove_marketplace(executable: &Path, home: &Path, name: &str) -> Result<()>;
}

pub(crate) fn has_envelope<D: MarketplaceDialect>(package: &StoredPackage) -> bool {
    package
        .root
        .join(D::ENVELOPE_DIR)
        .join("plugin.json")
        .is_file()
}

/// Whether UZE can safely synthesize an envelope: none of the package's
/// own, and at least one structural surface to generate from — a
/// conventional `skills/` directory, a root `mcp.json`, or `agents/` for a
/// harness whose plugins carry agents.
pub(crate) fn generatable<D: MarketplaceDialect>(package: &StoredPackage) -> bool {
    !has_envelope::<D>(package)
        && (package.root.join("skills").is_dir()
            || package.root.join("mcp.json").is_file()
            || (D::ENVELOPE_CARRIES_AGENTS && package.root.join("agents").is_dir()))
}

/// Which envelope a package's build carries on this harness, if any.
pub(crate) fn origin_of<D: MarketplaceDialect>(package: &StoredPackage) -> Option<Origin> {
    if has_envelope::<D>(package) {
        Some(Origin::Explicit)
    } else if generatable::<D>(package) {
        Some(Origin::Generated)
    } else {
        None
    }
}

/// The marketplace's root: the harness's vendor store.
pub(crate) fn marketplace_root<D: MarketplaceDialect>(uze_home: &UzeHome) -> PathBuf {
    D::STORE.root(uze_home)
}

/// Where a package's build sits in the marketplace.
#[cfg(test)]
pub(crate) fn generated_package_dir<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    package_id: &str,
) -> PathBuf {
    D::STORE.build_dir(uze_home, package_id)
}

fn catalogue_path<D: MarketplaceDialect>(uze_home: &UzeHome) -> PathBuf {
    marketplace_root::<D>(uze_home).join(D::CATALOGUE_PATH)
}

/// Renders one package's build for this harness into `dir`, for the root
/// `root` it is swapped into: the package as its author wrote it when it
/// ships this harness's envelope; otherwise the package without what this
/// harness would misread, with a generated envelope when anything is safely
/// representable; and the hook wrapper wherever the package has hooks.
pub(crate) fn render_build<D: MarketplaceDialect>(
    package: &StoredPackage,
    dir: &Path,
    root: &Path,
) -> Result<Option<Origin>> {
    let origin = origin_of::<D>(package);
    match origin {
        Some(Origin::Explicit) => {
            crate::shared::build::mirror_package(package, dir, &[], D::STORE.carried)?;
        }
        Some(Origin::Generated) => {
            crate::shared::build::mirror_package(package, dir, D::STORE.carried, D::STORE.carried)?;
            D::render_envelope(package, dir, root)?;
        }
        None => {
            crate::shared::build::mirror_package(package, dir, D::STORE.carried, D::STORE.carried)?;
        }
    }
    crate::hooks::render_wrapper(D::HOOKS, package, dir)?;
    Ok(origin)
}

/// Renders a package's build in place, with no receipt: what a test of a
/// rendering reads.
#[cfg(test)]
pub(crate) fn materialize_generated_package<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    package: &StoredPackage,
) -> Result<PathBuf> {
    let dir = generated_package_dir::<D>(uze_home, package.id.as_str());
    uze_core::persistence::replace_dir(&dir, |staging| {
        render_build::<D>(package, staging, &dir).map(|_| ())
    })?;
    Ok(dir)
}

/// The marketplace's root, as the tests of a rendering name it.
#[cfg(test)]
pub(crate) fn generated_root<D: MarketplaceDialect>(uze_home: &UzeHome) -> PathBuf {
    marketplace_root::<D>(uze_home)
}

/// Removes a package's build by id, with no receipt: what a test of a
/// rendering takes back.
#[cfg(test)]
pub(crate) fn remove_generated_package<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    package_id: &str,
) -> Result<()> {
    remove_generated_dir(package_id, |id| generated_package_dir::<D>(uze_home, id))
}

/// Publishes `package`'s build as a delivery does, recording its receipt.
#[cfg(test)]
pub(crate) fn publish_for_test<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    integration_id: &str,
    package: &StoredPackage,
) -> AttachmentReceipt {
    let inputs = uze_core::integration::BuildInputs {
        digest: "digest",
        linked: false,
    };
    let receipt = crate::shared::build::publish(
        crate::shared::build::Publication::new(
            D::STORE,
            uze_home,
            integration_id,
            package,
            &inputs,
            None,
        ),
        |dir, root| render_build::<D>(package, dir, root),
    )
    .unwrap();
    uze_core::state::record_receipt(uze_home, receipt.clone()).unwrap();
    receipt
}

/// Every build the catalogue lists: each build receipt of this integration
/// with an origin, for a package in `packages` when they are given. Sorted
/// by package, so the catalogue is the same document whatever order the
/// ledger keeps.
fn listed_builds(
    uze_home: &UzeHome,
    integration_id: &str,
    packages: Option<&[StoredPackage]>,
) -> Result<Vec<(String, RecordedBuild)>> {
    let mut listed: Vec<(String, RecordedBuild)> =
        crate::shared::build::recorded_builds(uze_home, integration_id)?
            .into_iter()
            .filter(|(receipt, build)| {
                build.origin.is_some()
                    && packages.is_none_or(|packages| {
                        packages
                            .iter()
                            .any(|package| package.id.as_str() == receipt.package_id)
                    })
            })
            .map(|(receipt, build)| (receipt.package_id, build))
            .collect();
    listed.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(listed)
}

/// The catalogue the listed builds make.
fn catalogue_of<D: MarketplaceDialect>(builds: &[(String, RecordedBuild)]) -> serde_json::Value {
    let plugins = builds
        .iter()
        .map(|(_, build)| {
            let name = build
                .key
                .as_ref()
                .map(|key| key.name.clone())
                .unwrap_or_default();
            let source = format!(
                "./{}",
                build
                    .root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            );
            D::catalogue_entry(&name, source, &build.root)
        })
        .collect();
    D::catalogue_document(MARKETPLACE, DISPLAY_NAME, plugins)
}

/// The catalogue `packages`' builds make, as [`republish`] writes it.
#[cfg(test)]
pub(crate) fn catalogue_document<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    integration_id: &str,
    packages: &[StoredPackage],
) -> serde_json::Value {
    catalogue_of::<D>(&listed_builds(uze_home, integration_id, Some(packages)).unwrap())
}

/// The package-level plan: the package's own envelope when it ships one,
/// else a generated one when anything is safely representable (ADR-013 §3).
/// Read-only either way.
pub(crate) fn package_plan<D: MarketplaceDialect>(
    package: &StoredPackage,
    resources: &[&Resource],
) -> Option<PackageExposurePlan> {
    let (provided, evidence, envelope) = match origin_of::<D>(package)? {
        Origin::Explicit => (
            D::explicit_coverage(package, resources),
            D::EXPLICIT_EVIDENCE,
            PackageEnvelope::Own,
        ),
        Origin::Generated => (
            generated_exact_coverage::<D>(package, resources),
            D::GENERATED_EVIDENCE,
            PackageEnvelope::Generated,
        ),
    };
    Some(PackageExposurePlan {
        package_id: package.id.clone(),
        route: CompatibilityRoute::Native,
        envelope,
        provided_resource_identities: provided,
        evidence: evidence.to_owned(),
    })
}

/// The resources a generated envelope provides, computed against what it
/// can preserve rather than by re-reading a manifest it wrote, so generation
/// and coverage agree by construction: a Skill under `skills/` whose policy
/// the envelope carries, an MCP server named in the package's `mcp.json`,
/// and every agent when the harness's plugins carry agents.
pub(crate) fn generated_exact_coverage<D: MarketplaceDialect>(
    package: &StoredPackage,
    resources: &[&Resource],
) -> BTreeSet<String> {
    let declared_mcp = canonical_mcp_server_names(package);
    resources
        .iter()
        .filter(|resource| match resource.capability.kind {
            CapabilityKind::AgentSkill => {
                in_skills_dir(package, resource)
                    && D::envelope_preserves(resource.skill_invocation())
            }
            CapabilityKind::Mcp => resource
                .resource_name
                .as_ref()
                .is_some_and(|name| declared_mcp.contains(name)),
            CapabilityKind::Agent => D::ENVELOPE_CARRIES_AGENTS,
            _ => false,
        })
        .map(|resource| resource.identity())
        .collect()
}

fn in_skills_dir(package: &StoredPackage, resource: &Resource) -> bool {
    resource
        .capability
        .path
        .strip_prefix(&package.root)
        .ok()
        .and_then(Path::parent)
        .is_some_and(|parent| parent.starts_with("skills"))
}

fn canonical_mcp_server_names(package: &StoredPackage) -> BTreeSet<String> {
    canonical_mcp_servers(package)
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default()
}

/// The `mcpServers` object of the package's canonical `mcp.json`.
pub(crate) fn canonical_mcp_servers(
    package: &StoredPackage,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    fs::read(package.root.join("mcp.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.get("mcpServers")?.as_object().cloned())
}

/// `description` and `version` from a manifest, each defaulted when absent
/// or unreadable — never invented beyond that.
pub(crate) fn manifest_fields(manifest: &Path, default_description: &str) -> (String, String) {
    let document = fs::read(manifest)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let field = |key: &str, default: &str| {
        document
            .as_ref()
            .and_then(|value| value.get(key)?.as_str())
            .unwrap_or(default)
            .to_owned()
    };
    (
        field("description", default_description),
        field("version", "0.1.0"),
    )
}

/// Removes the directory `dir_for` places a package's plugin in, by the id
/// its receipt carries.
#[cfg(test)]
pub(crate) fn remove_generated_dir(
    package_id: &str,
    dir_for: impl FnOnce(&str) -> PathBuf,
) -> Result<()> {
    // The id comes from the receipt ledger, not a constructor: refuse one
    // that could not have been a real package id instead of joining it into
    // a path and removing whatever the traversal lands on.
    if !uze_core::store::is_valid_qualified_id(package_id) {
        return Err(UzeError::ExposureUnavailable(format!(
            "refusing to remove generated envelope for malformed package id `{package_id}`"
        )));
    }
    let dir = dir_for(package_id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(UzeError::write(dir))?;
    }
    Ok(())
}

/// Rewrites the catalogue from the build receipts of the packages in
/// `packages`, swapped in whole, after every build it lists.
pub(crate) fn republish<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
    integration_id: &str,
    packages: &[StoredPackage],
) -> Result<()> {
    let builds = listed_builds(uze_home, integration_id, Some(packages))?;
    write_catalogue::<D>(uze_home, &builds)?;
    retire_earlier_roots::<D>(executable, command_home, uze_home)
}

/// What earlier releases staged under the runtime tree, among them the
/// roots they registered marketplaces at (`generated`, the earlier
/// `uze-store`, and `explicit`, `uze-local`): each goes once no receipt
/// names a path inside it and the harness has no marketplace registered
/// there, since the harness reads every marketplace it registered.
fn retire_earlier_roots<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
) -> Result<()> {
    crate::shared::earlier::retire_unnamed_attachments(uze_home, D::STORE.vendor, || {
        [MARKETPLACE, RETIRED_MARKETPLACE]
            .into_iter()
            .filter_map(|name| registered_at::<D>(executable, command_home, name))
            .collect()
    })
}

/// Where the harness has the marketplace `name` registered: read from its
/// own record when that says, and asked of the harness otherwise.
fn registered_at<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    name: &str,
) -> Option<PathBuf> {
    D::recorded_root(command_home, name)
        .unwrap_or_else(|| D::registered_root(executable, command_home, name))
}

fn write_catalogue<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    builds: &[(String, RecordedBuild)],
) -> Result<()> {
    if builds.is_empty() && !catalogue_path::<D>(uze_home).is_file() {
        return Ok(());
    }
    uze_core::persistence::write_atomic(
        &catalogue_path::<D>(uze_home),
        &serde_json::to_vec_pretty(&catalogue_of::<D>(builds)).expect("catalogue is serializable"),
    )
}

/// Whether the catalogue lists exactly the builds of `packages`, and every
/// build it lists is still there.
pub(crate) fn publication<D: MarketplaceDialect>(
    uze_home: &UzeHome,
    integration_id: &str,
    packages: &[StoredPackage],
) -> PublicationStatus {
    let noun = D::CATALOGUE_NOUN;
    let rerun = format!("re-run `uze setup {}`", D::SETUP_NAME);
    let builds = match listed_builds(uze_home, integration_id, Some(packages)) {
        Ok(builds) => builds,
        Err(error) => {
            return PublicationStatus::Unpublished(format!(
                "the receipt ledger cannot be read ({error}); {rerun}"
            ));
        }
    };
    let expected = catalogue_of::<D>(&builds);
    let current = match fs::read(catalogue_path::<D>(uze_home)) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(actual) if actual == expected => Ok(()),
            Ok(_) => Err(format!(
                "the {noun} does not match the installed package set; {rerun}"
            )),
            Err(error) => Err(format!("the {noun} is unreadable ({error}); {rerun}")),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if builds.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "no {noun} has been written for the installed packages; {rerun}"
                ))
            }
        }
        Err(error) => Err(error.to_string()),
    };
    if let Err(reason) = current {
        return PublicationStatus::Unpublished(reason);
    }
    if let Some((package, _)) = builds.iter().find(|(_, build)| {
        !build
            .root
            .join(D::ENVELOPE_DIR)
            .join("plugin.json")
            .is_file()
    }) {
        return PublicationStatus::Unpublished(format!(
            "the {noun} lists `{package}`, whose build is gone; {rerun}"
        ));
    }
    PublicationStatus::Published
}

/// Whether a package receipt was made through the marketplace this build
/// registers. Matched by the marketplace's *name*: a root that moved is
/// repointed, never uninstalled and installed again, while one installed
/// through [`RETIRED_MARKETPLACE`] is retired and installed again through
/// [`MARKETPLACE`].
pub(crate) fn receipt_serves<D: MarketplaceDialect>(receipt: &AttachmentReceipt) -> bool {
    let ManagedArtifact::IntegrationOwned { kind, selector, .. } = &receipt.artifact else {
        return true;
    };
    if receipt_origin::<D>(kind).is_none() {
        return true;
    }
    selector
        .rsplit_once('@')
        .is_some_and(|(_, marketplace)| marketplace == MARKETPLACE)
}

/// Makes the harness's `uze-store` the vendor store: registered when the
/// harness does not know it, moved there when it is registered elsewhere (an
/// earlier release rooted it under the runtime tree), and left alone when it
/// is already there.
pub(crate) fn ensure_registered<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    root: &Path,
) -> Result<()> {
    match D::registered_root(executable, command_home, MARKETPLACE) {
        Some(registered) if registered == root => Ok(()),
        None => D::add_marketplace(executable, command_home, root),
        Some(_) if D::ADD_REPOINTS => D::add_marketplace(executable, command_home, root),
        Some(previous) => {
            D::remove_marketplace(executable, command_home, MARKETPLACE)?;
            D::add_marketplace(executable, command_home, root).map_err(|error| {
                // The previous root still exists: registered again, the
                // plugins it served keep working, and the failure is said.
                let again = D::add_marketplace(executable, command_home, &previous);
                UzeError::DeliveryFailed(match again {
                    Ok(()) => format!(
                        "the {} could not be moved to {} ({error}); it is registered at {} again",
                        D::CATALOGUE_NOUN,
                        root.display(),
                        previous.display()
                    ),
                    Err(again) => format!(
                        "the {} could not be moved to {} ({error}), nor registered at {} again \
                         ({again})",
                        D::CATALOGUE_NOUN,
                        root.display(),
                        previous.display()
                    ),
                })
            })
        }
    }
}

/// The harness's `uze-store` registration, read as `read` allows, beside
/// the vendor store it should name; `None` when the read cannot observe it.
pub(crate) fn registration<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
    read: RegistrationRead,
) -> Option<VendorRegistration> {
    let registered = match read {
        RegistrationRead::Recorded => D::recorded_root(command_home, MARKETPLACE)?,
        RegistrationRead::Live => D::registered_root(executable, command_home, MARKETPLACE),
    };
    Some(VendorRegistration {
        kind: VendorRegistrationKind::Marketplace,
        name: MARKETPLACE.to_owned(),
        expected: marketplace_root::<D>(uze_home),
        registered,
    })
}

/// Moves the harness's `uze-store` registration to its vendor store, the
/// way delivery does ([`ensure_registered`]): only when it names another
/// root, and with the old root registered again when the move fails.
pub(crate) fn repoint<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
    registration: &VendorRegistration,
) -> Result<()> {
    if registration.kind != VendorRegistrationKind::Marketplace || registration.name != MARKETPLACE
    {
        return Err(UzeError::ExposureUnavailable(format!(
            "the {} keeps no registration called `{}`",
            D::CATALOGUE_NOUN,
            registration.name
        )));
    }
    ensure_registered::<D>(executable, command_home, &marketplace_root::<D>(uze_home))
}

/// Installs a package's build through the marketplace and returns the
/// receipt. The build is published first (its receipt is in the ledger),
/// and the catalogue is written to list it before the harness is asked.
pub(crate) fn attach_package<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
    integration_id: &str,
    package: &StoredPackage,
) -> Result<AttachmentReceipt> {
    let build =
        crate::shared::build::recorded_build(uze_home, integration_id, package.id.as_str())?
            .ok_or_else(|| {
                UzeError::DeliveryFailed(format!(
                    "`{}` has no build in the {} to install",
                    package.id.as_str(),
                    D::CATALOGUE_NOUN
                ))
            })?;
    let Some(origin) = build.origin else {
        return Err(UzeError::DeliveryFailed(format!(
            "`{}` carries no plugin for the {}",
            package.id.as_str(),
            D::CATALOGUE_NOUN
        )));
    };
    write_catalogue::<D>(uze_home, &listed_builds(uze_home, integration_id, None)?)?;
    let root = marketplace_root::<D>(uze_home);
    let selector = format!("{}@{MARKETPLACE}", package.active_name.as_str());
    // Two independent questions to one CLI, each a process start of its
    // own: asked at once, they cost the slower of the two.
    let parent = tracing::Span::current();
    let (registered, installed) = std::thread::scope(|scope| {
        let registered = scope
            .spawn(|| parent.in_scope(|| ensure_registered::<D>(executable, command_home, &root)));
        let installed = D::plugin_installed(executable, command_home, &selector);
        let registered = registered.join().unwrap_or_else(|_| {
            Err(UzeError::HarnessCommand(
                "registering the marketplace stopped unexpectedly".to_owned(),
            ))
        });
        (registered, installed)
    });
    registered?;
    if !installed {
        D::install_plugin(executable, command_home, &selector)?;
    }
    let detail: BTreeMap<String, serde_json::Value> = [
        ("marketplace_root".to_owned(), serde_json::json!(root)),
        ("package_root".to_owned(), serde_json::json!(build.root)),
    ]
    .into_iter()
    .collect();
    let kind = match origin {
        Origin::Explicit => D::EXPLICIT_KIND,
        Origin::Generated => D::GENERATED_KIND,
    };
    Ok(AttachmentReceipt {
        package_id: package.id.as_str().to_owned(),
        resource_identity: None,
        integration: integration_id.to_owned(),
        artifact: ManagedArtifact::IntegrationOwned {
            kind: kind.to_owned(),
            selector,
            detail,
        },
    })
}

/// Whether a receipt `kind` is one of this dialect's plugin receipts, and
/// which envelope it installed.
pub(crate) fn receipt_origin<D: MarketplaceDialect>(kind: &str) -> Option<Origin> {
    if kind == D::EXPLICIT_KIND {
        Some(Origin::Explicit)
    } else if kind == D::GENERATED_KIND {
        Some(Origin::Generated)
    } else {
        None
    }
}

pub(crate) fn inspect_package<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    selector: &str,
    detail: &BTreeMap<String, serde_json::Value>,
) -> AttachmentInspection {
    let Some(marketplace_root) = detail_path(detail, "marketplace_root") else {
        return blocked("plugin receipt has no marketplace root");
    };
    D::inspect_plugin(
        executable,
        command_home,
        selector,
        &marketplace_root,
        detail,
    )
}

/// Uninstalls the plugin. Its build is the build receipt's, and leaves with
/// that receipt; what an earlier release generated for it under the
/// runtime tree goes now, nothing naming it any more. Once no receipt names
/// [`RETIRED_MARKETPLACE`], it is unregistered too: `detach` never removes
/// the marketplace a plugin came from otherwise.
pub(crate) fn detach_package<D: MarketplaceDialect>(
    executable: &Path,
    command_home: &Path,
    uze_home: &UzeHome,
    receipt: &AttachmentReceipt,
    selector: &str,
) -> Result<()> {
    D::remove_plugin(executable, command_home, selector)?;
    if let ManagedArtifact::IntegrationOwned { detail, .. } = &receipt.artifact
        && let Some(earlier) = detail_path(detail, "package_root")
    {
        remove_runtime_leftover(uze_home, &earlier)?;
    }
    if selector.ends_with(&format!("@{RETIRED_MARKETPLACE}"))
        && !names_retired_marketplace(uze_home, receipt)?
    {
        D::remove_marketplace(executable, command_home, RETIRED_MARKETPLACE)?;
    }
    Ok(())
}

/// Reads one path out of an opaque receipt payload.
pub(crate) fn detail_path(
    detail: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<PathBuf> {
    detail
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
}

/// The `plugin marketplace list --json` entries, in either shape a vendor
/// has answered with: a bare array or `{"marketplaces": [...]}`.
pub(crate) fn marketplace_entries(listing: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    listing.as_array().or_else(|| {
        listing
            .get("marketplaces")
            .and_then(serde_json::Value::as_array)
    })
}
