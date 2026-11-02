//! Install-once package Store.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    acquisition::{MaterializedPackage, Provenance},
    error::{Result, UzeError},
    home::UzeHome,
};

/// What UZE reads of a package's `plugin.json`: the name it declares, the
/// executables it needs, and where the manifest sits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginManifest {
    pub name: String,
    pub path: PathBuf,
    pub requirements: Vec<crate::requirement::Requirement>,
}

/// Reads the Agent Plugins manifest at `root`, refusing one that is absent,
/// unnamed, that references a path outside the package, or whose
/// requirements are malformed: the check every acquisition passes before a
/// byte is stored.
pub fn read_plugin_manifest(root: &Path) -> Result<PluginManifest> {
    let path = root.join("plugin.json");
    if !path.is_file() {
        return Err(UzeError::MissingManifest(root.to_path_buf()));
    }
    let bytes = read_package_file(&path)?;
    let value: serde_json::Value =
        crate::authored::json(&bytes).map_err(|source| UzeError::Json {
            path: path.clone(),
            source,
        })?;
    validate_references(&value, &path)?;
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| UzeError::MissingPackageName(path.clone()))?
        .to_owned();
    let requirements = crate::requirement::declared_in(&value, &path)?;
    Ok(PluginManifest {
        name,
        path,
        requirements,
    })
}

/// Every file UZE reads out of a package is a declaration it parses or text
/// an agent loads into its context, and a megabyte is far past what either
/// can use. The bound is what keeps a package from answering a read with an
/// endless one.
pub(crate) const MAX_PACKAGE_FILE_BYTES: u64 = 1024 * 1024;

/// Reads a file a package supplies, refusing anything but a regular file of
/// at most [`MAX_PACKAGE_FILE_BYTES`].
///
/// Opened non-blocking and judged by the descriptor it got, not by a path
/// asked about beforehand: a FIFO would otherwise hold the open until
/// somebody writes to it, and a device would answer forever.
pub(crate) fn read_package_file(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;

    let failed = |source: std::io::Error| UzeError::Read {
        path: path.to_path_buf(),
        source,
    };
    let file = uze_platform::fs::open_without_blocking(path).map_err(failed)?;
    if !file.metadata().map_err(failed)?.is_file() {
        return Err(failed(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        )));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PACKAGE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(failed)?;
    if bytes.len() as u64 > MAX_PACKAGE_FILE_BYTES {
        return Err(failed(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("larger than {MAX_PACKAGE_FILE_BYTES} bytes"),
        )));
    }
    Ok(bytes)
}

fn validate_references(value: &serde_json::Value, manifest: &Path) -> Result<()> {
    match value {
        serde_json::Value::Object(entries) => {
            for (key, value) in entries {
                let key = key.to_ascii_lowercase();
                if (key.contains("path") || key.contains("file"))
                    && let serde_json::Value::String(reference) = value
                {
                    validate_reference(reference, manifest)?;
                }
                validate_references(value, manifest)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                validate_references(value, manifest)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_reference(reference: &str, manifest: &Path) -> Result<()> {
    let path = Path::new(reference);
    if crate::path::is_anchored(path)
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(UzeError::UnsafePathReference {
            path: manifest.to_path_buf(),
            reference: reference.to_owned(),
        });
    }
    Ok(())
}

/// An installed plugin identity, qualified by its marketplace.
///
/// A plugin name is only unique inside a marketplace. The qualified form is
/// deliberately the state/receipt identity so `git@one` and `git@two` can
/// coexist without sharing bytes or lifecycle records.
///
/// `Deserialize` is hand-implemented (not derived) so a ledger entry can
/// never introduce an id the constructors would have rejected: the id is
/// later joined verbatim into filesystem paths (`plugin_dir`) and passed as
/// a bare argument to vendor CLIs, so every byte that enters through
/// `packages.json` must pass the same charset/shape rule as one constructed
/// from a manifest.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PackageId(String);

impl<'de> Deserialize<'de> for PackageId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if !is_valid_qualified_id(&value) {
            return Err(serde::de::Error::custom(format!(
                "package id `{value}` is not a valid `name@marketplace` id"
            )));
        }
        Ok(Self(value))
    }
}

impl PackageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_plugin_name(name: &str, manifest: &Path) -> Result<Self> {
        Self::from_marketplace_plugin("local", name, manifest)
    }

    pub fn from_marketplace_plugin(marketplace: &str, name: &str, manifest: &Path) -> Result<Self> {
        // The one chokepoint every package id is constructed through, so
        // the name rule (see `is_valid_name_component`) is enforced here
        // rather than re-checked at each call site.
        let valid = is_valid_name_component(name);
        let valid_marketplace = is_valid_name_component(marketplace);
        if !valid_marketplace {
            return Err(UzeError::InvalidPackageName {
                path: manifest.to_path_buf(),
                name: marketplace.to_owned(),
            });
        }
        if valid {
            Ok(Self(format!("{name}@{marketplace}")))
        } else {
            Err(UzeError::InvalidPackageName {
                path: manifest.to_path_buf(),
                name: name.to_owned(),
            })
        }
    }

    pub fn from_qualified(value: &str, manifest: &Path) -> Result<Self> {
        let (name, marketplace) = value.rsplit_once('@').ok_or_else(|| {
            UzeError::InvalidPluginSpec(format!("`{value}` must be `name@marketplace`"))
        })?;
        Self::from_marketplace_plugin(marketplace, name, manifest)
    }

    pub fn plugin_name(&self) -> &str {
        self.0.rsplit_once('@').map_or(&self.0, |(name, _)| name)
    }

    pub fn marketplace(&self) -> &str {
        self.0
            .rsplit_once('@')
            .map_or("local", |(_, marketplace)| marketplace)
    }
}

/// The longest name any harness accepts: the Agent Skills specification and
/// OpenCode both cap a skill name at 64, and a plugin's name becomes one.
pub const NAME_MAX_LEN: usize = 64;

/// The one rule every plugin name, marketplace name, and install alias is
/// held to: lowercase kebab-case, `^[a-z0-9]+(-[a-z0-9]+)*$`, at most
/// [`NAME_MAX_LEN`] characters.
///
/// It is the intersection of what every harness UZE delivers to accepts —
/// the Agent Skills specification, Claude Code's and Codex's plugin names,
/// Gemini's extension names — so a name UZE accepts is never one a harness
/// rejects later. It also subsumes the older leading-`-` rule (ADR-036): a
/// name a vendor CLI takes as a bare positional argument cannot be parsed
/// as a flag. And because it is lowercase-only, two names that differ by
/// case cannot become two packages sharing one directory on a
/// case-insensitive filesystem.
fn is_valid_name_component(value: &str) -> bool {
    value.len() <= NAME_MAX_LEN
        && !value.is_empty()
        && value
            .split('-')
            .all(|segment| !segment.is_empty() && segment.bytes().all(is_name_byte))
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

/// The name [`is_valid_package_name`] would accept for `value`, when one
/// can be derived: lowercased, `_`, spaces and `.` read as `-`, anything
/// else outside the rule dropped, hyphens collapsed and trimmed, cut to
/// [`NAME_MAX_LEN`]. `None` when nothing is left, or when `value` already
/// holds.
pub fn suggested_name(value: &str) -> Option<String> {
    if is_valid_name_component(value) {
        return None;
    }
    let mut suggestion = String::with_capacity(value.len());
    for character in value.chars() {
        let character = character.to_ascii_lowercase();
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            suggestion.push(character);
        } else if matches!(character, '-' | '_' | ' ' | '.') && !suggestion.ends_with('-') {
            suggestion.push('-');
        }
    }
    suggestion.truncate(NAME_MAX_LEN);
    let suggestion = suggestion.trim_matches('-');
    (!suggestion.is_empty()).then(|| suggestion.to_owned())
}

/// What a refused name is told: the rule, and the corrected name when one
/// can be derived.
pub fn name_rule(value: &str) -> String {
    let rule = format!(
        "names are lowercase kebab-case: `a-z`, `0-9` and single `-` between them, at most \
         {NAME_MAX_LEN} characters"
    );
    match suggested_name(value) {
        Some(suggestion) => format!("{rule} — try `{suggestion}`"),
        None => rule,
    }
}

/// The same rule a [`PackageId`] is held to, asked before one is built — the
/// authoring surface validates the name the author chose rather than letting
/// the failure surface from a constructed id.
pub fn is_valid_package_name(value: &str) -> bool {
    is_valid_name_component(value)
}

/// Parses the `plugin@marketplace` spelling an operator types. Both halves
/// are required and held to the same rule a [`PackageId`] is, after
/// [`typed_name`] forgives their case.
pub fn parse_plugin_marketplace_spec(spec: &str) -> Result<(String, String)> {
    let typed = typed_name(spec);
    // The message echoes what was typed, and a URL typed where a
    // marketplace name belongs may carry a token.
    let shown = uze_platform::redact::redact;
    let (plugin, marketplace) = typed.split_once('@').ok_or_else(|| {
        UzeError::InvalidPluginSpec(format!("`{}` must be `name@marketplace`", shown(spec)))
    })?;
    for part in [plugin, marketplace] {
        if !is_valid_name_component(part) {
            let part = shown(part);
            return Err(UzeError::InvalidPluginSpec(format!(
                "`{}` must be `name@marketplace`, and `{part}` is not a valid name: {}",
                shown(spec),
                name_rule(&part)
            )));
        }
    }
    Ok((plugin.to_owned(), marketplace.to_owned()))
}

/// A name as a person typed it, in the only case a name can be spelled in.
/// Every name on record is lowercase, so `Flow@AI` can only ever have meant
/// `flow@ai`; resolution after this stays exact.
pub fn typed_name(value: &str) -> String {
    value.to_ascii_lowercase()
}

/// Whether `value` is a valid qualified `name@marketplace` package id — the
/// same rule the constructors and the ledger deserializer enforce. Public so
/// integrations can re-check an id that arrives from state (a receipt's
/// `package_id`) before turning it into a filesystem path.
pub fn is_valid_qualified_id(value: &str) -> bool {
    value.rsplit_once('@').is_some_and(|(name, marketplace)| {
        is_valid_name_component(name) && is_valid_name_component(marketplace)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPackage {
    pub id: PackageId,
    pub root: PathBuf,
    pub manifest: PathBuf,
    /// Where this package came from. Carried for reporting and for a later
    /// reinstall; the Store itself never reads inside it.
    pub provenance: Provenance,
    /// The local token this plugin currently invokes under — `id.plugin_name()`
    /// unless an install-time alias resolved a collision with another
    /// marketplace's same-named plugin (ADR-036). This is what a harness's
    /// generated manifest/catalog and every Skill/Command label use; `id`
    /// remains the real, marketplace-qualified identity everywhere else
    /// (Store paths, receipts, removal, update).
    pub active_name: String,
}

impl StoredPackage {
    /// When these bytes were put in the Store, as seconds since the epoch:
    /// the package's directory is cleared and written afresh by every
    /// install and every update, so its modification time is the last of
    /// either. `None` where the filesystem cannot say.
    pub fn written_at_unix(&self) -> Option<u64> {
        modified_unix(&self.root)
    }
}

fn modified_unix(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
}

#[derive(Clone, Debug)]
pub struct UzeStore {
    home: UzeHome,
}

#[derive(Clone, Debug, Serialize)]
struct PackageRegistry {
    schema_version: u32,
    packages: BTreeMap<PackageId, Registration>,
}

impl PackageRegistry {
    fn empty() -> Self {
        Self {
            schema_version: <RawPackageRegistry as uze_document::Shaped>::SHAPE,
            packages: BTreeMap::new(),
        }
    }
}

/// The on-disk shape of `packages.json`, read with plain `String` keys and
/// undecided values so `load_registry` can validate each entry
/// independently — see its doc comment for why deserializing straight into
/// `PackageRegistry` (keyed by the strict `PackageId`, valued by the strict
/// `Registration`) is the wrong tool here: one bad entry would fail the
/// whole map instead of just that entry.
#[derive(Default, Deserialize)]
struct RawPackageRegistry {
    packages: BTreeMap<String, serde_json::Value>,
}

/// Shape 2 is shape 1 written by a build that keeps the package bytes in
/// `sources/` and caches each package's digest. A shape-1 build would
/// otherwise go on recreating `store/plugins` and installing into it, bytes
/// this build never reads, so it must refuse the registry as newer than
/// itself; the step up carries every entry unchanged, and the digests are
/// backfilled by the move (`package::sources`), which can read the disk.
impl uze_document::Shaped for RawPackageRegistry {
    const SHAPE: u32 = 2;
    const KIND: &'static str = "packages";

    fn ladder() -> uze_document::Ladder {
        &[uze_document::Step {
            from: 1,
            to: 2,
            lasts: uze_document::earlier::Lasts::Unbounded,
            climb: Ok,
        }]
    }
}

/// Which digest bytes put back by [`UzeStore::restore_bytes`] must have.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordedDigest {
    /// The one recorded at ingest, which stays.
    Kept,
    /// Any: the one they have is recorded instead.
    Replaced,
}

/// What [`UzeStore::restore_bytes`] did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Restored {
    Restored,
    /// The bytes were there already; nothing was acquired.
    AlreadyThere,
    /// The source now yields other bytes, which were not kept.
    DigestDiffers {
        found: String,
    },
    /// No digest was recorded to compare them with, so they were not kept.
    NothingToCompare,
}

/// A `packages.json` entry this UZE cannot read, and why.
///
/// Quarantined rather than fatal: the registry is a ledger of independent
/// registrations, and the entries that *do* read are still the truth about
/// the packages they name. A quarantined entry answers to nothing — it is
/// not listed, not resolvable, not removable — and the next `save_registry`
/// drops it, which is exactly what the remedy asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuarantinedRegistration {
    pub id: String,
    pub reason: String,
}

impl QuarantinedRegistration {
    /// What to do about it, whatever made the entry unreadable: the registry
    /// is rebuilt from what is installed, so losing an entry costs a
    /// re-register and nothing else.
    pub const REMEDY: &'static str = "remove it and run `uze install` to re-register";
}

/// One registry entry.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Registration {
    provenance: Provenance,
    /// `None` means "no alias was ever chosen" — the local name defaults to
    /// `id.plugin_name()`. `Some(alias)` is only ever written by an explicit
    /// `alias` collision resolution at install time.
    active_name: Option<String>,
    /// The symbolic links the package holds that its checkout could not make,
    /// which stand in its directory as copies of what they point at. Read by
    /// [`UzeStore::digest`]; absent from every entry that has none.
    #[serde(default, skip_serializing_if = "crate::digest::Links::is_empty")]
    links: crate::digest::Links,
    /// The digest of the package's bytes as ingested, so delivery can tell
    /// a changed package from an unchanged one without walking its tree,
    /// and a restore can tell the bytes it fetched are the ones it lost.
    /// Absent only on an entry a build before shape 2 wrote and the move
    /// has not reached yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
}

impl UzeStore {
    pub fn new(home: UzeHome) -> Self {
        Self { home }
    }

    pub fn home(&self) -> &UzeHome {
        &self.home
    }

    /// Ingests an already-materialized Agent Plugins 1.0 package once.
    ///
    /// The Store preserves the complete external package tree (including any
    /// vendor-native envelope) and never creates a UZE plugin manifest or
    /// rewrites payloads. It writes nothing a harness reads: a harness-owned
    /// view of the installed set belongs to that harness's integration.
    ///
    /// It also knows nothing about where the bytes came from. Provenance
    /// arrives attached to the materialized package, is persisted verbatim,
    /// and is compared only through `Provenance::same_origin` — this module
    /// never reads a field of it or matches a source mechanism.
    ///
    /// The plugin is recorded under the marketplace that resolved it, active
    /// under its own bare name unless `active_name` gives an alias — the
    /// `alias` collision resolution (ADR-036). Fails with
    /// `PluginNameCollision` when the name it would answer to is already
    /// active under a different marketplace-qualified identity.
    pub fn ingest(
        &self,
        package: &MaterializedPackage,
        marketplace: &str,
        active_name: Option<&str>,
    ) -> Result<StoredPackage> {
        let _span = tracing::info_span!("store.ingest", root = %package.root().display()).entered();
        let source = package.root();
        // Every source passes through this one check, so a local package and
        // a remote one are held to the same rule. It runs before any byte is
        // read or written, so nothing is read through a link that leaves the
        // package and a rejected package leaves nothing behind.
        assert_self_contained(source)?;
        assert_no_reserved_path(source)?;
        let PluginManifest {
            name,
            path: manifest,
            ..
        } = read_plugin_manifest(source)?;
        let name = name.as_str();
        let id = PackageId::from_marketplace_plugin(marketplace, name, &manifest)?;
        let requested_active = active_name.unwrap_or(name);
        if let Some(alias) = active_name
            && !is_valid_name_component(alias)
        {
            return Err(UzeError::InvalidPackageName {
                path: manifest.clone(),
                name: alias.to_owned(),
            });
        }

        self.home.ensure_layout()?;
        let mut registry = self.load_registry()?;
        if let Some(existing) = registry.packages.get(&id) {
            if existing.provenance.same_origin(package.provenance()) {
                return self.package(&id);
            }
            return Err(UzeError::PackageConflict {
                id: id.as_str().to_owned(),
                existing: existing.provenance.requested.display(),
                requested: package.provenance().requested.display(),
            });
        }
        // A plugin name is only reserved once actively claimed: two
        // packages coexist fine in the Store (ADR-036, bytes never share
        // state), but only one of them may answer to a given invocation
        // name at a time — the other would silently shadow it in every
        // harness (verified against real Claude Code: whichever loads
        // first wins `/name:capability`, with zero indication the other
        // exists). This is the one place every install path passes
        // through, so the check cannot be bypassed by a different entry
        // point (ADR-036).
        if let Some(holder) = Self::active_name_holder(&registry, requested_active)
            && holder != &id
        {
            return Err(UzeError::PluginNameCollision {
                name: requested_active.to_owned(),
                existing: holder.as_str().to_owned(),
                requested: id.as_str().to_owned(),
            });
        }

        let destination = self.home.plugin_dir(&id);
        fs::create_dir_all(
            destination
                .parent()
                .expect("plugin directory has a marketplace parent"),
        )
        .map_err(|source_error| UzeError::Write {
            path: destination
                .parent()
                .expect("plugin directory has a marketplace parent")
                .to_path_buf(),
            source: source_error,
        })?;
        // Nothing in the registry claims this id — the checks above returned
        // for every id that does — so a directory already sitting here is
        // debris from an install that was interrupted between the copy and
        // the registration. Clearing it is what keeps `create_dir` from
        // refusing this id forever; leaving it was a dead end with no
        // command to escape it, since `remove` answers only to registered
        // ids.
        if destination.exists() {
            fs::remove_dir_all(&destination).map_err(|source_error| UzeError::Write {
                path: destination.clone(),
                source: source_error,
            })?;
        }
        fs::create_dir(&destination).map_err(|source_error| UzeError::Write {
            path: destination.clone(),
            source: source_error,
        })?;

        let ingested = (|| {
            copy_tree(source, &destination)?;
            let digest = crate::digest::tree_sha256_with_links(&destination, package.links())
                .map_err(UzeError::read(&destination))?;
            registry.packages.insert(
                id.clone(),
                Registration {
                    provenance: package.provenance().clone(),
                    active_name: active_name
                        .filter(|alias| *alias != name)
                        .map(str::to_owned),
                    links: package.links().clone(),
                    digest: Some(digest),
                },
            );
            self.save_registry(&registry)
        })();
        if let Err(error) = ingested {
            // A half-copied, unregistered tree is debris the next attempt
            // would have to clear anyway; clearing it here is what makes a
            // failed install leave the Store as it found it.
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        self.package(&id)
    }

    pub fn package(&self, id: &PackageId) -> Result<StoredPackage> {
        let registry = self.load_registry()?;
        let registration = registry
            .packages
            .get(id)
            .ok_or_else(|| UzeError::UnknownPackage(id.as_str().to_owned()))?;
        Ok(self.stored(id, registration))
    }

    /// Every installed package, in the order [`package_ids`](Self::package_ids)
    /// lists them, from one read of the registry.
    pub fn packages(&self) -> Result<Vec<StoredPackage>> {
        let registry = self.load_registry()?;
        Ok(registry
            .packages
            .iter()
            .map(|(id, registration)| self.stored(id, registration))
            .collect())
    }

    /// The digest of `package`'s bytes: links its checkout could not make
    /// count as the links they are (see [`crate::digest::tree_sha256_with_links`]),
    /// so a package digests to one value on every platform.
    pub fn digest(&self, package: &StoredPackage) -> Result<String> {
        let links = self
            .load_registry()?
            .packages
            .get(&package.id)
            .map(|registration| registration.links.clone())
            .unwrap_or_default();
        crate::digest::tree_sha256_with_links(&package.root, &links)
            .map_err(UzeError::read(&package.root))
    }

    fn stored(&self, id: &PackageId, registration: &Registration) -> StoredPackage {
        let root = self.home.plugin_dir(id);
        let active_name = registration
            .active_name
            .clone()
            .unwrap_or_else(|| id.plugin_name().to_owned());
        StoredPackage {
            id: id.clone(),
            manifest: root.join("plugin.json"),
            root,
            provenance: registration.provenance.clone(),
            active_name,
        }
    }

    /// The local invocation name `id` currently answers to, without paying
    /// for a full [`package`] resolution (no root/manifest path building).
    /// Falls back to the bare plugin name for an id this Store does not
    /// recognize — permissive, since this is a naming convenience read, not
    /// an existence check; callers that need existence use [`package`].
    pub fn active_name_for(&self, id: &PackageId) -> String {
        self.load_registry()
            .ok()
            .and_then(|registry| {
                registry
                    .packages
                    .get(id)
                    .and_then(|r| r.active_name.clone())
            })
            .unwrap_or_else(|| id.plugin_name().to_owned())
    }

    /// The installed package currently active under local name `name`, if
    /// any — either because it is its own bare plugin name and holds no
    /// alias, or because an `alias` resolution explicitly claimed `name`
    /// for it. At most one package ever holds a given active name (enforced
    /// at ingest time), so this never needs to report ambiguity.
    pub fn find_by_active_name(&self, name: &str) -> Result<Option<PackageId>> {
        let registry = self.load_registry()?;
        Ok(Self::active_name_holder(&registry, name).cloned())
    }

    fn active_name_holder<'a>(registry: &'a PackageRegistry, name: &str) -> Option<&'a PackageId> {
        registry.packages.iter().find_map(|(id, registration)| {
            let active = registration
                .active_name
                .as_deref()
                .unwrap_or(id.plugin_name());
            (active == name).then_some(id)
        })
    }

    /// Lists installed package identities in deterministic order. Package
    /// selection and dependency resolution are intentionally out of scope for
    /// this PoC; the composed local environment currently includes every
    /// locally installed package.
    pub fn package_ids(&self) -> Result<Vec<PackageId>> {
        Ok(self.load_registry()?.packages.into_keys().collect())
    }

    /// Package directories under the Store that its registry does not
    /// list: bytes an interrupted install or a hand copy left, which
    /// nothing installs, updates or removes. Reported, never deleted: a
    /// package's bytes are the one thing UZE cannot always acquire again.
    pub fn unregistered_directories(&self) -> Result<Vec<PathBuf>> {
        let registered: std::collections::BTreeSet<PathBuf> = self
            .package_ids()?
            .iter()
            .map(|id| self.home.plugin_dir(id))
            .collect();
        let mut found = Vec::new();
        let Ok(markets) = fs::read_dir(self.home.plugins_dir()) else {
            return Ok(found);
        };
        for market in markets.flatten() {
            if !market.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Ok(plugins) = fs::read_dir(market.path()) else {
                continue;
            };
            for plugin in plugins.flatten() {
                let path = plugin.path();
                let hidden = plugin.file_name().to_string_lossy().starts_with('.');
                if !hidden
                    && plugin.file_type().is_ok_and(|kind| kind.is_dir())
                    && !registered.contains(&path)
                {
                    found.push(path);
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// Removes registry entries whose backing directory is gone — a
    /// registration that survived whatever stopped writing its bytes (an
    /// interrupted install, manual cleanup).
    ///
    /// A registry entry is the Store's sole claim that a package is
    /// installed; once its directory is gone, that claim is simply false,
    /// not merely unhealthy — so this prunes rather than reports it, the
    /// same way a `Missing` receipt is either repaired or forgotten, never
    /// left to keep asserting something false. Returns the ids pruned, so
    /// a caller can also clean up anything that still references them
    /// (receipts, project locks).
    ///
    /// `still_delivered` answers whether something a harness reads was built
    /// from an entry: such an entry is missing its bytes, not a ghost, and
    /// keeps its place (vendor-stores D3). The Store never asks a harness
    /// itself; its caller does.
    pub fn prune_ghost_registrations(
        &self,
        still_delivered: impl Fn(&PackageId) -> bool,
    ) -> Result<Vec<PackageId>> {
        let mut registry = self.load_registry()?;
        let ghosts: Vec<PackageId> = registry
            .packages
            .keys()
            .filter(|id| !self.home.plugin_dir(id).is_dir() && !still_delivered(id))
            .cloned()
            .collect();
        if ghosts.is_empty() {
            return Ok(ghosts);
        }
        for id in &ghosts {
            registry.packages.remove(id);
        }
        self.save_registry(&registry)?;
        Ok(ghosts)
    }

    /// Copies a package's stored bytes to `destination` — symlinks, modes
    /// and all, exactly as [`ingest`](Self::ingest)
    /// wrote them, so the copy is itself a materialized package.
    ///
    /// What an update keeps aside while the package replacing it installs:
    /// the removal is what makes an update destructive, and after it only a
    /// copy of the bytes can put the previous one back.
    pub fn copy_package_to(&self, id: &PackageId, destination: &Path) -> Result<()> {
        copy_tree(&self.home.plugin_dir(id), destination)
    }

    /// Removes only UZE-owned package bytes and its registry entry. Callers
    /// must complete attachment reconciliation first; the Store deliberately
    /// knows nothing about harness artifacts or their ownership.
    pub fn remove_package(&self, id: &PackageId) -> Result<()> {
        let mut registry = self.load_registry()?;
        if registry.packages.remove(id).is_none() {
            return Err(UzeError::UnknownPackage(id.as_str().to_owned()));
        }
        for root in [
            self.home.plugin_dir(id),
            self.home.delivered_package_dir(id),
        ] {
            if let Err(source) = fs::remove_dir_all(&root)
                && source.kind() != std::io::ErrorKind::NotFound
            {
                return Err(UzeError::Write { path: root, source });
            }
        }
        self.save_registry(&registry)
    }

    /// The entries of `packages.json` this UZE cannot read, each with the
    /// reason and [`QuarantinedRegistration::REMEDY`] — what `doctor` reports
    /// instead of leaving the operator to guess why a package vanished.
    pub fn quarantined_registrations(&self) -> Result<Vec<QuarantinedRegistration>> {
        Ok(self.read_registry()?.1)
    }

    fn load_registry(&self) -> Result<PackageRegistry> {
        Ok(self.read_registry()?.0)
    }

    /// `packages.json` is a ledger of independent registrations, so one
    /// unreadable entry — a key that fails `PackageId`'s validation, or a
    /// value whose fields this UZE no longer knows (an install by an older
    /// UZE, a hand edit, corruption) — must not make every *other*,
    /// still-valid package unreachable through `list`/`remove`/`doctor`.
    /// Deserializing the whole map through the strict types would do exactly
    /// that: one bad entry fails the entire parse. So keys are read as plain
    /// `String` and values left undecided, then each is validated on its own
    /// and the failures are quarantined rather than trusted — they answer to
    /// nothing, and the next save drops them.
    ///
    /// What is *not* tolerated is a file that is not JSON at all, or one
    /// whose top level is not a registry: there are no independent entries
    /// to salvage, and silently reading it as empty would invite the next
    /// mutation to overwrite it.
    fn read_registry(&self) -> Result<(PackageRegistry, Vec<QuarantinedRegistration>)> {
        let path = self.home.registry_path();
        if !path.exists() {
            return Ok((PackageRegistry::empty(), Vec::new()));
        }
        // A record: what the operator registered, which nothing else on the
        // machine knows. Read through the one rule, so a shape this build
        // understands is carried across rather than read as a registry with
        // no packages in it.
        let raw: RawPackageRegistry = uze_document::read::<RawPackageRegistry>(&path)?
            .record()
            .unwrap_or_default();
        let mut packages = BTreeMap::new();
        let mut quarantined = Vec::new();
        for (key, value) in raw.packages {
            if !is_valid_qualified_id(&key) {
                quarantined.push(QuarantinedRegistration {
                    id: key,
                    reason: "not a valid marketplace-qualified package id".to_owned(),
                });
                continue;
            }
            match serde_json::from_value::<Registration>(value) {
                Ok(registration) => {
                    packages.insert(PackageId(key), registration);
                }
                Err(source) => quarantined.push(QuarantinedRegistration {
                    id: key,
                    reason: source.to_string(),
                }),
            }
        }
        Ok((
            PackageRegistry {
                packages,
                ..PackageRegistry::empty()
            },
            quarantined,
        ))
    }

    /// Puts the bytes of `id` back from `package`, a fresh acquisition of
    /// the source the registry recorded, when they are gone: kept only when
    /// their digest is the one recorded at ingest, so what comes back is
    /// exactly what was authorized. `RecordedDigest::Replaced` takes them
    /// whatever their digest and records it instead — the bundled plugin's
    /// snapshot, which is this binary's own version of it.
    ///
    /// Staged beside the sources and moved into place whole, so a reader
    /// never meets half of them, and a mismatch leaves nothing behind.
    pub fn restore_bytes(
        &self,
        id: &PackageId,
        package: &MaterializedPackage,
        digest: RecordedDigest,
    ) -> Result<Restored> {
        let _span = tracing::info_span!("store.restore", id = %id.as_str()).entered();
        let destination = self.home.plugin_dir(id);
        if destination.is_dir() {
            return Ok(Restored::AlreadyThere);
        }
        let mut registry = self.load_registry()?;
        let recorded = registry
            .packages
            .get(id)
            .ok_or_else(|| UzeError::UnknownPackage(id.as_str().to_owned()))?
            .digest
            .clone();
        let source = package.root();
        assert_self_contained(source)?;
        assert_no_reserved_path(source)?;
        let staging = self.home.restoring_dir(id);
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(UzeError::write(&staging))?;
        }
        fs::create_dir_all(&staging).map_err(UzeError::write(&staging))?;
        let staged = (|| {
            copy_tree(source, &staging)?;
            crate::digest::tree_sha256_with_links(&staging, package.links())
                .map_err(UzeError::read(&staging))
        })();
        let found = match staged {
            Ok(found) => found,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        };
        let keep = match digest {
            RecordedDigest::Kept => recorded.as_deref() == Some(found.as_str()),
            RecordedDigest::Replaced => true,
        };
        if !keep {
            let _ = fs::remove_dir_all(&staging);
            return Ok(match recorded {
                Some(_) => Restored::DigestDiffers { found },
                None => Restored::NothingToCompare,
            });
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
        }
        fs::rename(&staging, &destination).map_err(UzeError::write(&destination))?;
        if let Some(emptied) = staging.parent() {
            let _ = fs::remove_dir(emptied);
        }
        if digest == RecordedDigest::Replaced
            && let Some(registration) = registry.packages.get_mut(id)
        {
            registration.digest = Some(found);
            registration.links = package.links().clone();
            self.save_registry(&registry)?;
        }
        Ok(Restored::Restored)
    }

    /// The digest recorded for `id` at ingest, without walking its bytes;
    /// `None` for an id that is not registered or whose entry predates the
    /// cache.
    pub fn recorded_digest(&self, id: &PackageId) -> Result<Option<String>> {
        Ok(self
            .load_registry()?
            .packages
            .get(id)
            .and_then(|registration| registration.digest.clone()))
    }

    /// Writes the registry back in the shape this build writes, recording a
    /// digest for every entry that has bytes and none yet. The fence and the
    /// backfill of the move to `sources/`: an older build refuses a registry
    /// it finds in this shape before it can prune or install anything.
    ///
    /// Writes nothing when the file already holds exactly that, compared
    /// with the bytes on disk: the move runs on every mutation while
    /// anything is left on the old layout, and must not touch the home each
    /// time.
    pub fn rewrite_registry_with_digests(&self) -> Result<()> {
        if !self.home.registry_path().exists() {
            return Ok(());
        }
        let mut registry = self.load_registry()?;
        for (id, registration) in &mut registry.packages {
            let root = self.home.plugin_dir(id);
            if registration.digest.is_none() && root.is_dir() {
                registration.digest = Some(
                    crate::digest::tree_sha256_with_links(&root, &registration.links)
                        .map_err(UzeError::read(&root))?,
                );
            }
        }
        let on_disk = fs::read(self.home.registry_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        if on_disk.is_some() && on_disk == serde_json::to_value(&registry).ok() {
            return Ok(());
        }
        self.save_registry(&registry)
    }

    fn save_registry(&self, registry: &PackageRegistry) -> Result<()> {
        let path = self.home.registry_path();
        let payload =
            serde_json::to_vec_pretty(registry).expect("registry serialization is infallible");
        crate::persistence::write_atomic(&path, &payload)
    }
}

/// Where a build keeps, under their own relative paths, the parts of the
/// package its harness would misread at the plugin root (Claude's `bin/`,
/// `commands/`, ...): out of the harness's reach as components, still
/// there for the `${PLUGIN_ROOT}` references that name them.
pub const CARRIED_DIR: &str = ".uze-carried";

/// Paths, relative to a package, that UZE writes into the package's build on
/// every harness — the hook wrapper, the hook document a runtime plugin
/// reads, the carried parts ([`CARRIED_DIR`]) — and so that no package may
/// ship itself: its file would collide with UZE's own in every build.
pub const RESERVED_BUILD_PATHS: &[&str] = &[
    "hooks/uze-exec",
    "hooks/uze-exec.ps1",
    "hooks/uze-groups.json",
    CARRIED_DIR,
];

/// Refuses a package that ships a file at one of the
/// [`RESERVED_BUILD_PATHS`].
pub(crate) fn assert_no_reserved_path(root: &Path) -> Result<()> {
    match RESERVED_BUILD_PATHS
        .iter()
        .find(|reserved| fs::symlink_metadata(root.join(reserved)).is_ok())
    {
        Some(reserved) => Err(UzeError::PackageShipsReservedPath((*reserved).to_owned())),
        None => Ok(()),
    }
}

/// Enforces the invariant that an installed package is **self-contained**:
/// no symlink the Store persists may resolve outside the package root.
///
/// This is not a rule about where a package came from. A local directory and
/// a cloned repository are held to it identically, because it protects what
/// happens *after* installation: an integration later points a harness at a
/// path inside the store, and the harness follows whatever it finds there.
///
/// Every symlink is checked on its own and none is followed. That is
/// deliberate and it is also what makes chains and cycles harmless: a chain
/// can only leave the root if some individual link leaves it, and that link
/// is checked like any other. Nothing here traverses a link, so there is no
/// cycle to guard against.
///
/// The one place a textual reading and the kernel disagree is `..` stepping
/// back out of a link: `s -> .` makes `s/s/../..` two levels up on disk and
/// none on paper. A target that does that is refused rather than resolved,
/// so each link's own check stays the whole answer.
pub(crate) fn assert_self_contained(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(UzeError::read(&directory))?;
        // Two names in one directory that a case-insensitive filesystem
        // cannot tell apart. macOS and Windows are both such filesystems by
        // default, and the copy below writes entry by entry, so the second
        // of a colliding pair silently overwrites the first — deterministic
        // by sort order, and therefore choosable. A package shipping both
        // `SKILL.md` and `skill.md` reads as two files in review, in `git
        // show`, and to this very walk, while exactly one lands and it is
        // the one whose name sorts last. Refused before a byte is written,
        // like every other rule here, and refused on Linux too: what is
        // rejected must not depend on where the install happens to run.
        let mut folded: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in entries {
            let entry = entry.map_err(UzeError::read(&directory))?;
            let path = entry.path();
            let folded_name = entry.file_name().to_string_lossy().to_lowercase();
            if let Some(first) = folded.insert(folded_name, path.clone()) {
                return Err(UzeError::PackageNameCollides {
                    first,
                    second: path,
                });
            }
            let metadata = fs::symlink_metadata(&path).map_err(UzeError::read(&path))?;
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path).map_err(UzeError::read(&path))?;
                // An absolute target is refused whatever it names, including a
                // path inside the source being read right now. Containment is
                // judged here against the *source* root, but `copy_symlink`
                // writes the target verbatim: a relative link keeps pointing
                // inside the package once copied, while an absolute one keeps
                // pointing at the source — a store entry aimed at a directory
                // UZE does not own and the user may repoint afterwards.
                if crate::path::is_anchored(&target) {
                    return Err(UzeError::PackageEscapesRoot { link: path, target });
                }
                let Some(resolved) = resolve_lexically(&path, &target) else {
                    return Err(UzeError::PackageEscapesRoot { link: path, target });
                };
                if !resolved.starts_with(root) {
                    return Err(UzeError::PackageEscapesRoot {
                        link: path,
                        target: resolved,
                    });
                }
            } else if metadata.is_dir() {
                // A real directory only. Symlinked directories were rejected
                // or accepted above and are never descended into, so this
                // walk cannot be led outside the root either.
                pending.push(path);
            }
        }
    }
    Ok(())
}

/// Resolves a symlink target against its own location **without following
/// it**, so `..` is normalized textually rather than by following whatever
/// it currently points at. Only relative targets reach here — an absolute
/// one is refused before the call, because no copy of the package can keep
/// it inside the root.
///
/// `None` when a `..` would pop a component that is itself a symlink: that
/// is the only case where the textual answer differs from the kernel's, and
/// the kernel's is the one a harness gets.
fn resolve_lexically(link: &Path, target: &Path) -> Option<PathBuf> {
    let base = if target.is_absolute() {
        PathBuf::new()
    } else {
        link.parent().unwrap_or_else(|| Path::new("")).to_path_buf()
    };
    let mut resolved = base;
    for component in target.components() {
        match component {
            std::path::Component::ParentDir => {
                if fs::symlink_metadata(&resolved)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    return None;
                }
                resolved.pop();
            }
            std::path::Component::CurDir => {}
            other => resolved.push(other.as_os_str()),
        }
    }
    Some(resolved)
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination).map_err(|source_error| UzeError::Write {
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    let mut entries = fs::read_dir(source)
        .map_err(|source_error| UzeError::Read {
            path: source.to_path_buf(),
            source: source_error,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source_error| UzeError::Read {
            path: source.to_path_buf(),
            source: source_error,
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata =
            fs::symlink_metadata(&source_path).map_err(|source_error| UzeError::Read {
                path: source_path.clone(),
                source: source_error,
            })?;
        if metadata.file_type().is_symlink() {
            copy_symlink(&source_path, &destination_path)?;
        } else if metadata.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            copy_file(&source_path, &destination_path)?;
        } else {
            return Err(UzeError::UnpreservableEntry(source_path));
        }
    }
    Ok(())
}

/// `fs::copy` carries the permission bits itself, set on the open descriptor
/// rather than through the umask, so an executable stays executable.
fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    fs::copy(source, destination).map_err(|source_error| UzeError::Write {
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    Ok(())
}

fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target = fs::read_link(source).map_err(|source_error| UzeError::Read {
        path: source.to_path_buf(),
        source: source_error,
    })?;
    crate::persistence::create_symlink(&target, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UzeHome;

    fn local_package(root: &Path, body: &str) -> MaterializedPackage {
        fs::create_dir_all(root.join("skills/flow")).unwrap();
        fs::write(root.join("plugin.json"), "{\"name\":\"flow\"}\n").unwrap();
        fs::write(root.join("skills/flow/SKILL.md"), body).unwrap();
        MaterializedPackage::borrowed(root.to_path_buf(), Provenance::of_local_path(root))
    }

    /// Bytes acquired again are kept only when they are the ones recorded
    /// at ingest; a source that moved on leaves the sources as they were,
    /// and the bundled snapshot records its own digest instead.
    #[test]
    fn restored_bytes_are_kept_only_with_the_digest_recorded_at_ingest() {
        let scratch = uze_testkit::temp::scratch("restore-bytes");
        let store = UzeStore::new(UzeHome::at(scratch.join("home")));
        let source = scratch.join("flow");
        let stored = store
            .ingest(&local_package(&source, "# Flow\n"), "local", None)
            .unwrap();
        let recorded = store.recorded_digest(&stored.id).unwrap();
        fs::remove_dir_all(&stored.root).unwrap();

        let moved_on = local_package(&scratch.join("flow-edited"), "# Flow, edited\n");
        assert!(matches!(
            store
                .restore_bytes(&stored.id, &moved_on, RecordedDigest::Kept)
                .unwrap(),
            Restored::DigestDiffers { .. }
        ));
        assert!(!stored.root.exists());
        assert!(!store.home().restoring_dir(&stored.id).exists());

        let as_ingested = local_package(&source, "# Flow\n");
        assert_eq!(
            store
                .restore_bytes(&stored.id, &as_ingested, RecordedDigest::Kept)
                .unwrap(),
            Restored::Restored
        );
        assert_eq!(store.digest(&stored).unwrap(), recorded.clone().unwrap());
        assert_eq!(
            store
                .restore_bytes(&stored.id, &as_ingested, RecordedDigest::Kept)
                .unwrap(),
            Restored::AlreadyThere
        );

        fs::remove_dir_all(&stored.root).unwrap();
        assert_eq!(
            store
                .restore_bytes(&stored.id, &moved_on, RecordedDigest::Replaced)
                .unwrap(),
            Restored::Restored
        );
        assert_ne!(store.recorded_digest(&stored.id).unwrap(), recorded);
        assert_eq!(
            store.recorded_digest(&stored.id).unwrap(),
            Some(store.digest(&stored).unwrap())
        );
        fs::remove_dir_all(scratch).ok();
    }

    #[test]
    fn a_package_shipping_the_carried_directory_is_refused_by_name() {
        let root = uze_testkit::temp::scratch("reserved-carried");
        fs::create_dir_all(root.join(CARRIED_DIR).join("bin")).unwrap();
        fs::write(root.join(CARRIED_DIR).join("bin/server"), "#!/bin/sh\n").unwrap();
        assert!(matches!(
            assert_no_reserved_path(&root),
            Err(UzeError::PackageShipsReservedPath(path)) if path == CARRIED_DIR
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_the_declared_name_of_a_plugin_manifest() {
        let root = uze_testkit::temp::scratch("manifest-name");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("plugin.json"), "{\"name\":\"demo\"}\n").unwrap();
        let manifest = read_plugin_manifest(&root).unwrap();
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.path, root.join("plugin.json"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_package_file_past_the_bound_is_refused_rather_than_read_whole() {
        let root = uze_testkit::temp::scratch("package-file-bound");
        fs::create_dir_all(&root).unwrap();
        let manifest = root.join("plugin.json");
        fs::write(&manifest, vec![b' '; MAX_PACKAGE_FILE_BYTES as usize + 1]).unwrap();
        assert!(matches!(
            read_plugin_manifest(&root),
            Err(UzeError::Read { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    // A FIFO, which Windows has no file for.
    #[cfg(unix)]
    #[test]
    fn a_package_file_that_is_a_fifo_is_refused_without_waiting_for_a_writer() {
        let root = uze_testkit::temp::scratch("package-file-fifo");
        fs::create_dir_all(&root).unwrap();
        let fifo = root.join("SKILL.md");
        let spelled = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path; mkfifo touches nothing else.
        assert_eq!(unsafe { libc::mkfifo(spelled.as_ptr(), 0o600) }, 0);
        assert!(matches!(
            read_package_file(&fifo),
            Err(UzeError::Read { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_parent_directory_manifest_reference() {
        let root = uze_testkit::temp::scratch("manifest-unsafe");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("plugin.json"),
            "{\"name\":\"demo\",\"scriptPath\":\"../outside.sh\"}",
        )
        .unwrap();
        assert!(matches!(
            read_plugin_manifest(&root),
            Err(UzeError::UnsafePathReference { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_plugin_spec_requires_both_valid_halves() {
        assert!(parse_plugin_marketplace_spec("flow").is_err());
        assert!(parse_plugin_marketplace_spec("flow@").is_err());
        assert!(parse_plugin_marketplace_spec("@ai").is_err());
        assert!(parse_plugin_marketplace_spec("fl/ow@ai").is_err());
        assert!(parse_plugin_marketplace_spec("flow@-ai").is_err());
        let (plugin, marketplace) = parse_plugin_marketplace_spec("flow@ai").unwrap();
        assert_eq!(plugin, "flow");
        assert_eq!(marketplace, "ai");
    }

    #[test]
    fn a_typed_spec_resolves_in_the_one_case_a_name_has() {
        let (plugin, marketplace) = parse_plugin_marketplace_spec("Flow@AI").unwrap();
        assert_eq!((plugin.as_str(), marketplace.as_str()), ("flow", "ai"));
        let refused = parse_plugin_marketplace_spec("my_plugin@ai")
            .unwrap_err()
            .to_string();
        assert!(refused.contains("try `my-plugin`"), "{refused}");
    }

    #[test]
    fn a_name_is_lowercase_kebab_case_of_at_most_64_characters() {
        for valid in ["a", "git", "uze", "x2", "2x", "a-b-c", &"a".repeat(64)] {
            assert!(is_valid_package_name(valid), "{valid} should hold");
        }
        for invalid in [
            "",
            "Flow",
            "PDF-Processing",
            "my_plugin",
            "double--hyphen",
            "-leading",
            "trailing-",
            "-",
            "has space",
            "has.dot",
            "has/slash",
            "ünicode",
            &"a".repeat(65),
        ] {
            assert!(
                !is_valid_package_name(invalid),
                "{invalid} should be refused"
            );
        }
    }

    #[test]
    fn a_refused_name_is_told_the_name_it_meant() {
        for (typed, meant) in [
            ("Flow", "flow"),
            ("PDF-Processing", "pdf-processing"),
            ("my_plugin", "my-plugin"),
            ("My Plugin", "my-plugin"),
            ("double--hyphen", "double-hyphen"),
            ("-leading", "leading"),
            ("trailing-", "trailing"),
            ("v1.2", "v1-2"),
            ("__a__b__", "a-b"),
        ] {
            assert_eq!(suggested_name(typed).as_deref(), Some(meant), "{typed}");
            assert!(is_valid_package_name(meant));
        }
        assert_eq!(suggested_name(&"a".repeat(65)), Some("a".repeat(64)));
        assert_eq!(suggested_name("git"), None);
        assert_eq!(suggested_name("---"), None);
        assert!(!name_rule("---").contains("try"));
    }

    #[test]
    fn package_id_rejects_invalid_names() {
        let manifest = PathBuf::from("/tmp/plugin.json");
        assert!(PackageId::from_plugin_name("valid-name-123", &manifest).is_ok());
        assert!(PackageId::from_plugin_name("valid_name", &manifest).is_err());
        assert!(PackageId::from_plugin_name("", &manifest).is_err());
        assert!(PackageId::from_plugin_name("has space", &manifest).is_err());
        assert!(PackageId::from_plugin_name("has/slash", &manifest).is_err());
        assert!(PackageId::from_plugin_name("has.dot", &manifest).is_err());
        assert!(PackageId::from_plugin_name("has:colon", &manifest).is_err());
    }

    #[test]
    fn package_id_rejects_a_leading_dash() {
        // Package ids are used as bare positional/selector arguments to
        // vendor CLIs (e.g. `codex plugin remove <id>@marketplace`); a
        // leading `-` would let the vendor CLI parse the id as a flag
        // instead, so it must be rejected before it ever becomes an id.
        let manifest = PathBuf::from("/tmp/plugin.json");
        assert!(PackageId::from_plugin_name("-force", &manifest).is_err());
        assert!(PackageId::from_plugin_name("--force", &manifest).is_err());
        // A dash elsewhere in the name remains fine.
        assert!(PackageId::from_plugin_name("my-plugin", &manifest).is_ok());
    }

    #[test]
    fn bytes_the_registry_does_not_list_are_reported_and_kept() {
        let root = uze_testkit::temp::scratch("store-unregistered");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        let stray = home.plugins_dir().join("local/stray");
        fs::create_dir_all(&stray).unwrap();
        fs::create_dir_all(home.plugins_dir().join("local/.staging")).unwrap();
        assert_eq!(
            store.unregistered_directories().unwrap(),
            vec![stray.clone()]
        );
        assert!(stray.is_dir(), "an audit never deletes bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_registry_returns_empty_when_missing_and_survives_corrupt_json() {
        let root = uze_testkit::temp::scratch("registry-missing");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        // No registry yet — should be empty, not error.
        assert_eq!(store.package_ids().unwrap().len(), 0);
        // Corrupt JSON should surface as error, not panic.
        home.ensure_layout().unwrap();
        fs::write(home.registry_path(), "bad json").unwrap();
        assert!(store.package_ids().is_err());
        let _ = fs::remove_dir_all(root);
    }

    /// An entry whose bytes are gone but a harness still reads a build of
    /// is missing its bytes, not a ghost: only an entry with neither goes.
    #[test]
    fn an_entry_still_delivered_is_missing_its_bytes_and_never_pruned() {
        let root = uze_testkit::temp::scratch("registry-bytes-missing");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        home.ensure_layout().unwrap();
        let shape = <RawPackageRegistry as uze_document::Shaped>::SHAPE;
        let entry = r#"{"provenance":{"requested":{"LOCAL":{"path":"/x"}},"resolved":{"LOCAL":{"path":"/x"}}},"active_name":null}"#;
        fs::write(
            home.registry_path(),
            format!(
                r#"{{"schema_version":{shape},"packages":{{"built@ai":{entry},"ghost@ai":{entry}}}}}"#
            ),
        )
        .unwrap();

        let pruned = store
            .prune_ghost_registrations(|id| id.as_str() == "built@ai")
            .unwrap();
        assert_eq!(
            pruned.iter().map(PackageId::as_str).collect::<Vec<_>>(),
            ["ghost@ai"]
        );
        assert_eq!(
            store
                .package_ids()
                .unwrap()
                .iter()
                .map(PackageId::as_str)
                .collect::<Vec<_>>(),
            ["built@ai"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_registry_from_a_newer_build_is_refused_and_nothing_is_pruned() {
        // The fence of the move to `sources/`, seen from the other side: a
        // build meeting a registry in a shape it does not know refuses it by
        // name instead of reading it as empty and pruning every entry as a
        // ghost.
        let root = uze_testkit::temp::scratch("registry-newer");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        home.ensure_layout().unwrap();
        let newer = <RawPackageRegistry as uze_document::Shaped>::SHAPE + 1;
        let written = format!(r#"{{"schema_version":{newer},"packages":{{"flow@ai":{{}}}}}}"#);
        fs::write(home.registry_path(), &written).unwrap();

        let refused = store.package_ids().unwrap_err().to_string();
        assert!(refused.contains("packages"), "{refused}");
        assert!(store.prune_ghost_registrations(|_| false).is_err());
        assert_eq!(fs::read_to_string(home.registry_path()).unwrap(), written);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_id_deserializes_only_valid_qualified_ids() {
        // The registry's `packages.json` is the one path through which an id
        // can reach `plugin_dir` and `remove_dir_all` without passing a
        // constructor, so deserialization must hold the same charset/shape
        // rule the constructors do.
        let manifest = PathBuf::from("/tmp/plugin.json");
        let valid = PackageId::from_marketplace_plugin("local", "flow", &manifest).unwrap();
        let round_trip: PackageId =
            serde_json::from_str(&format!("\"{}\"", valid.as_str())).unwrap();
        assert_eq!(round_trip, valid);
        assert!(
            serde_json::from_str::<PackageId>("\"flow\"").is_err(),
            "unqualified ids are not valid state"
        );
        assert!(
            serde_json::from_str::<PackageId>("\"../escape@local\"").is_err(),
            "path traversal must never deserialize"
        );
        assert!(
            serde_json::from_str::<PackageId>("\"../../x@local\"").is_err(),
            "multi-component traversal must never deserialize"
        );
        assert!(
            serde_json::from_str::<PackageId>("\"-force@local\"").is_err(),
            "a leading dash must never deserialize"
        );
        assert!(
            serde_json::from_str::<PackageId>("\"flow@../market\"").is_err(),
            "the marketplace component is held to the same rule"
        );
    }

    #[test]
    fn tampered_registry_entry_never_resolves_to_a_removable_package() {
        // End-to-end guard: even with a hand-edited registry carrying a
        // traversal id, nothing about that id is ever actionable — it never
        // loads into a `PackageId` an operation could reach, so it can never
        // be looked up, listed, or removed.
        let root = uze_testkit::temp::scratch("registry-tamper");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        home.ensure_layout().unwrap();
        let victim = uze_testkit::temp::scratch("registry-tamper-victim");
        fs::create_dir_all(&victim).unwrap();
        let state = serde_json::json!({
            "packages": {
                "../../..": {
                    "provenance": {
                        "requested": { "LOCAL": { "path": "/tmp/plugin" } },
                        "resolved": { "LOCAL": { "path": "/tmp/plugin" } }
                    },
                    "active_name": ".."
                }
            }
        });
        fs::write(home.registry_path(), state.to_string()).unwrap();
        let ids = store
            .package_ids()
            .expect("a tampered entry must be quarantined, not fail the whole load");
        assert!(
            ids.is_empty(),
            "the tampered id must never resolve to a usable PackageId"
        );
        assert!(victim.exists(), "nothing outside the store may be removed");
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(victim);
    }

    #[test]
    fn load_registry_quarantines_a_tampered_entry_without_losing_valid_ones() {
        // The whole point of the per-entry-tolerant parse: a single
        // corrupted or hand-edited key must not make every *other*,
        // still-valid package unreachable through `list`/`remove`/`doctor`.
        let root = uze_testkit::temp::scratch("registry-mixed-tamper");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        home.ensure_layout().unwrap();
        let state = serde_json::json!({
            "packages": {
                "../../escape@local": {
                    "provenance": {
                        "requested": { "LOCAL": { "path": "/tmp/escape" } },
                        "resolved": { "LOCAL": { "path": "/tmp/escape" } }
                    },
                    "active_name": "escape"
                },
                "flow@local": {
                    "provenance": {
                        "requested": { "LOCAL": { "path": "/tmp/flow" } },
                        "resolved": { "LOCAL": { "path": "/tmp/flow" } }
                    }
                }
            }
        });
        fs::write(home.registry_path(), state.to_string()).unwrap();
        let ids = store
            .package_ids()
            .expect("one tampered entry must not fail the whole registry load");
        assert_eq!(
            ids,
            vec![
                PackageId::from_marketplace_plugin(
                    "local",
                    "flow",
                    &PathBuf::from("/tmp/plugin.json")
                )
                .unwrap()
            ],
            "the valid entry must still load even though its sibling is quarantined"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// An entry whose fields this UZE cannot read must not fail the whole
    /// file: the commands that would clear it are the ones that would stop
    /// running. It is quarantined like an unreadable key, and named with
    /// what to do about it.
    #[test]
    fn an_entry_with_unreadable_fields_is_quarantined_and_named() {
        let root = uze_testkit::temp::scratch("registry-unreadable-fields");
        let home = UzeHome::at(&root);
        let store = UzeStore::new(home.clone());
        home.ensure_layout().unwrap();
        let state = serde_json::json!({
            "packages": {
                "old@local": {
                    "source": {
                        "requested": { "LOCAL": { "path": "/tmp/old" } },
                        "resolved": { "LOCAL": { "path": "/tmp/old" } }
                    },
                    "active_name": null
                },
                "flow@local": {
                    "provenance": {
                        "requested": { "LOCAL": { "path": "/tmp/flow" } },
                        "resolved": { "LOCAL": { "path": "/tmp/flow" } }
                    }
                }
            }
        });
        fs::write(home.registry_path(), state.to_string()).unwrap();

        let ids = store
            .package_ids()
            .expect("an unreadable entry must not fail the whole registry load");
        assert_eq!(
            ids.iter().map(PackageId::as_str).collect::<Vec<_>>(),
            vec!["flow@local"],
            "the readable entry must survive its unreadable sibling"
        );

        let quarantined = store.quarantined_registrations().unwrap();
        assert_eq!(
            quarantined
                .iter()
                .map(|entry| &entry.id)
                .collect::<Vec<_>>(),
            vec!["old@local"]
        );
        assert!(
            quarantined[0].reason.contains("provenance"),
            "the reason must name the field that could not be read: {}",
            quarantined[0].reason
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_package_is_dated_by_when_its_bytes_were_written() {
        let root = uze_testkit::temp::scratch("store-written-at");
        fs::create_dir_all(&root).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let written = modified_unix(&root).expect("a directory has a time");
        assert!(now.abs_diff(written) < 60, "{written} against {now}");
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(modified_unix(&root), None, "nothing there, no date");
    }
}
