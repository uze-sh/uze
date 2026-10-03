//! Package and capability delivery plans.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::{capability::Resource, store::PackageId};

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, UzeError},
    hook::HookEvent,
    integration::{AttachmentInspection, AttachmentState},
    router::CompatibilityRoute,
};

/// Secret-free declaration of a process environment value UZE may pass
/// through to an MCP server. Values are intentionally never persisted in an
/// attachment receipt; integrations can only report MATCHED when their
/// vendor surface exposes an equivalent reference rather than an opaque
/// literal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct McpEnvironmentReference {
    pub name: String,
}

/// How an integration makes a resource available: the artifact UZE will own
/// once attached — described exactly as its receipt records it — or why
/// there is none.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExposureMechanism {
    Managed(ManagedArtifact),
    Unsupported { rationale: String },
}

/// One harness-owned side effect UZE is responsible for. As a plan it says
/// what attaching will create; inside a receipt it is the ownership proof
/// every later inspection and detach is judged against.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ManagedArtifact {
    /// A persistent reference placed once in a harness's user-scope
    /// discovery directory (e.g. `~/.claude/skills/<name>`), pointing at
    /// content inside the UZE store. Tied to no project or session
    /// (ADR-006).
    SymlinkReference { path: PathBuf, target: PathBuf },
    /// A generated entry in a harness's own user-scope vendor configuration
    /// (e.g. `~/.claude.json`'s `mcpServers`), registered through that
    /// harness's management CLI. The registration command differs per
    /// harness, so the owning integration attaches it (ADR-007).
    VendorConfigEntry {
        entry_name: String,
        transport: String,
        command: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        environment: Vec<McpEnvironmentReference>,
        enabled: Option<bool>,
    },
    /// A delimited region inside a shared text file UZE does not otherwise
    /// own. Every safety rule lives in `crate::text_region`, which knows
    /// nothing about what `target_file` is or why `region_identity` was
    /// chosen.
    ManagedTextRegion {
        target_file: PathBuf,
        region_identity: String,
        expected_content: String,
    },
    /// A delivery whose ownership proof only the owning integration can
    /// interpret. The Core routes it by `receipt.integration`, never reads
    /// `detail`, and refuses to inspect or detach it generically.
    IntegrationOwned {
        kind: String,
        selector: String,
        #[serde(flatten, default)]
        detail: BTreeMap<String, serde_json::Value>,
    },
    /// A UZE-namespaced entry inside the harness's shared hook
    /// configuration (ADR-033). `entry_name` is the stable UZE identity,
    /// `event` the manifest group's semantic event where the target shape
    /// is event-keyed, and `expected` the exact serialized entry content.
    /// Inspection and detach are integration-owned: the Core knows the
    /// identity, never the file's shape.
    HookConfigEntry {
        config_file: PathBuf,
        entry_name: String,
        event: HookEvent,
        expected: String,
        /// The generated wrapper this entry runs: materialized on attach,
        /// verified by content identity, removed once no entry needs it.
        wrapper: PathBuf,
    },
    /// A whole derived file the harness loads from its own discovery
    /// directory (the OpenCode hook bridge): no configuration entry exists
    /// to merge, so `path` is the entire artifact. The owning integration
    /// attaches, inspects and detaches it.
    ManagedHookFile { path: PathBuf },
    /// A whole file UZE writes into a harness's own discovery directory,
    /// whose full content the receipt carries: a harness that reads a
    /// definition file by file (an agent) is handed the bytes themselves
    /// rather than a link, since not every harness follows one (one lists a
    /// linked agent file and refuses to run it).
    GeneratedFile { path: PathBuf, content: String },
    /// A whole directory UZE writes into a harness's own discovery root (a
    /// skill), proven by the `tree_sha256` of what was put there. Rendered
    /// files and the supporting files beside them are regular files: one
    /// harness does not walk a linked skill root, another does not list a
    /// linked `SKILL.md`. The bytes stay out of the receipt — a skill can
    /// carry megabytes of assets, all reproducible from the Store — so a
    /// missing tree is restored by attaching again, never from here.
    ///
    /// In a plan `digest` is empty: the tree is rendered at attach time, and
    /// what the receipt records is the digest of the tree attached.
    GeneratedTree { path: PathBuf, digest: String },
}

impl ManagedArtifact {
    /// Creates or verifies an artifact whose full desired state the artifact
    /// itself carries — a symlink reference or a text region. Never touches
    /// an entry UZE does not already own. Every other artifact is attached
    /// by its owning integration.
    pub fn attach_standard(&self) -> Result<()> {
        match self {
            Self::SymlinkReference { path, target } => attach_symlink(path, target),
            Self::ManagedTextRegion {
                target_file,
                region_identity,
                expected_content,
            } => crate::text_region::attach(target_file, region_identity, expected_content),
            Self::GeneratedFile { path, content } => attach_generated_file(path, content),
            _ => Err(UzeError::ExposureUnavailable(
                "this artifact is attached by its owning integration".to_owned(),
            )),
        }
    }

    /// Inspection for artifacts whose ownership proof does not depend on a
    /// harness schema. Vendor integrations call this explicitly rather than
    /// redispatching through `IntegrationPort` from an override.
    pub fn inspect_standard(&self) -> AttachmentInspection {
        match self {
            Self::SymlinkReference { path, target } => inspect_symlink(path, target),
            Self::ManagedTextRegion {
                target_file,
                region_identity,
                expected_content,
            } => crate::text_region::inspect(target_file, region_identity, expected_content),
            Self::GeneratedFile { path, content } => inspect_generated_file(path, content),
            Self::GeneratedTree { path, digest } => inspect_generated_tree(path, digest),
            _ => AttachmentInspection {
                state: AttachmentState::Blocked,
                reason: "integration must inspect this vendor artifact".to_owned(),
            },
        }
    }

    /// Removes only a currently matched standard artifact. Any non-matched
    /// inspection is returned unchanged, so drift never turns into a
    /// destructive operation.
    pub fn detach_standard(&self) -> Result<AttachmentInspection> {
        match self {
            // `text_region::detach` inspects the region in the same read it
            // removes it from, so drift never becomes a removal (ADR-009).
            Self::ManagedTextRegion {
                target_file,
                region_identity,
                expected_content,
            } => crate::text_region::detach(target_file, region_identity, expected_content),
            Self::SymlinkReference { path, target } => {
                let inspection = inspect_symlink(path, target);
                if inspection.state != AttachmentState::Matched {
                    return Ok(inspection);
                }
                fs::remove_file(path).map_err(UzeError::write(&path))?;
                Ok(AttachmentInspection {
                    state: AttachmentState::Missing,
                    reason: "managed artifact detached".to_owned(),
                })
            }
            Self::GeneratedFile { path, content } => {
                let inspection = inspect_generated_file(path, content);
                if inspection.state != AttachmentState::Matched {
                    return Ok(inspection);
                }
                fs::remove_file(path).map_err(UzeError::write(&path))?;
                Ok(AttachmentInspection {
                    state: AttachmentState::Missing,
                    reason: "managed artifact detached".to_owned(),
                })
            }
            Self::GeneratedTree { path, digest } => {
                let inspection = inspect_generated_tree(path, digest);
                if inspection.state != AttachmentState::Matched {
                    return Ok(inspection);
                }
                fs::remove_dir_all(path).map_err(UzeError::write(&path))?;
                Ok(AttachmentInspection {
                    state: AttachmentState::Missing,
                    reason: "managed artifact detached".to_owned(),
                })
            }
            _ => Ok(self.inspect_standard()),
        }
    }

    /// The physical exposure name this artifact claims. `None` for a shape
    /// with no single physical name of its own: a text region spans a
    /// portion of a shared file, and an integration-owned artifact's naming
    /// is opaque to the Core by design.
    pub fn exposure_name(&self) -> Option<String> {
        match self {
            Self::SymlinkReference { path, .. }
            | Self::ManagedHookFile { path }
            | Self::GeneratedTree { path, .. } => path.file_name()?.to_str().map(str::to_owned),
            Self::VendorConfigEntry { entry_name, .. }
            | Self::HookConfigEntry { entry_name, .. } => Some(entry_name.clone()),
            // The name a harness gives a file it reads one by one is the
            // file's, without its format's extension.
            Self::GeneratedFile { path, .. } => path.file_stem()?.to_str().map(str::to_owned),
            Self::ManagedTextRegion { .. } | Self::IntegrationOwned { .. } => None,
        }
    }

    /// A human-readable locator. Display-only: the artifact itself remains
    /// the source of truth.
    pub fn location(&self) -> PathBuf {
        match self {
            Self::SymlinkReference { path, .. }
            | Self::ManagedHookFile { path }
            | Self::GeneratedFile { path, .. }
            | Self::GeneratedTree { path, .. } => path.clone(),
            Self::VendorConfigEntry { entry_name, .. } => {
                PathBuf::from(format!("mcp:{entry_name}"))
            }
            Self::HookConfigEntry {
                config_file,
                entry_name,
                ..
            } => PathBuf::from(format!("{}#{entry_name}", config_file.display())),
            Self::ManagedTextRegion {
                target_file,
                region_identity,
                ..
            } => PathBuf::from(format!("{}#{region_identity}", target_file.display())),
            Self::IntegrationOwned { kind, selector, .. } => {
                PathBuf::from(format!("{kind}:{selector}"))
            }
        }
    }

    /// A cheap, vendor-neutral fingerprint of the filesystem surface the
    /// artifact lives on — the freshness half of the inspection cache
    /// (ADR 018).
    ///
    /// Only a symlink reference has a directly stat-able presence, and it
    /// always produces one: a missing link is a real, checkable state, not
    /// the absence of a fingerprint. Everything else lives inside vendor
    /// files whose locations this layer deliberately does not know, so its
    /// verdicts are bounded by TTL and mutation invalidation alone.
    pub fn fingerprint(&self) -> Option<String> {
        if let Self::GeneratedTree { path, digest } = self {
            return Some(format!("{}:{digest}", tree_stat_signature(path)));
        }
        if let Self::GeneratedFile { path, content } = self {
            // A file UZE wrote is stat-able like a link: an edit moves its
            // modification time or its length, a removal both.
            let state = fs::metadata(path)
                .ok()
                .and_then(|metadata| {
                    let modified = metadata.modified().ok()?;
                    let nanos = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()?
                        .as_nanos();
                    Some(format!("{nanos}:{}", metadata.len()))
                })
                .unwrap_or_else(|| "absent".to_owned());
            return Some(format!("{state}:{}", content.len()));
        }
        let Self::SymlinkReference { path, target } = self else {
            return None;
        };
        let state = fs::symlink_metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or(u128::MAX); // absent/untimed: a state, never "no info"
        let link = fs::read_link(path)
            .map(|resolved| resolved.to_string_lossy().into_owned())
            .unwrap_or_default();
        Some(format!("{state}:{link}:{}", target.display()))
    }

    /// Whether the artifact is still physically in place, answered only for
    /// what carries its whole desired state: a symlink reference (the link
    /// exists and points where it should) and a generated file (it says
    /// exactly what the receipt does). Everything else is the owning
    /// integration's verdict, so the answer is `false` and callers fall back
    /// to receipt existence.
    pub fn is_in_place(&self) -> bool {
        match self {
            Self::SymlinkReference { path, target } => {
                fs::read_link(path).is_ok_and(|resolved| resolved == *target)
            }
            Self::GeneratedFile { path, content } => {
                fs::read(path).is_ok_and(|existing| existing == content.as_bytes())
            }
            Self::GeneratedTree { path, digest } => {
                inspect_generated_tree(path, digest).state == AttachmentState::Matched
            }
            _ => false,
        }
    }
}

/// Writes the file when it is absent and accepts it when it already says
/// exactly this; anything else at the path is not UZE's to replace.
fn attach_generated_file(path: &Path, content: &str) -> Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == content.as_bytes() => Ok(()),
        Ok(_) => Err(UzeError::ManagedEntryConflict(path.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
            }
            fs::write(path, content).map_err(UzeError::write(path))
        }
        Err(source) => Err(UzeError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Writes the tree `build` produces at `path`, replacing it whole, and
/// returns the digest a receipt records for it.
///
/// `owned` is the digest of the tree UZE's receipt says it put at `path`,
/// if one does. A directory already there is replaced only when it is still
/// exactly that tree; an edited one is drift, and one with no receipt is
/// accepted only when it already holds exactly what this build writes, so
/// attaching twice is idempotent and nobody's directory is overwritten.
pub fn attach_generated_tree(
    path: &Path,
    owned: Option<&str>,
    build: impl FnOnce(&Path) -> Result<()>,
) -> Result<String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(UzeError::ManagedEntryConflict(path.to_path_buf()));
        }
        Ok(_) => match owned {
            None => return adopt_identical_tree(path, build),
            Some(digest)
                if inspect_generated_tree(path, digest).state != AttachmentState::Matched =>
            {
                return Err(UzeError::ManagedEntryDrift(path.to_path_buf()));
            }
            Some(_) => {}
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(UzeError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    crate::persistence::replace_dir(path, build)?;
    crate::digest::tree_sha256(path).map_err(UzeError::read(path))
}

/// The digest of the directory at `path` when it holds exactly the tree
/// `build` writes, built aside to compare and then discarded.
fn adopt_identical_tree(path: &Path, build: impl FnOnce(&Path) -> Result<()>) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tree");
    let probe = path.with_file_name(format!(".{name}.uze-probe-{}", std::process::id()));
    let _ = fs::remove_dir_all(&probe);
    fs::create_dir_all(&probe).map_err(UzeError::write(&probe))?;
    let rendered = build(&probe)
        .and_then(|()| crate::digest::tree_sha256(&probe).map_err(UzeError::read(&probe)));
    let _ = fs::remove_dir_all(&probe);
    let rendered = rendered?;
    match crate::digest::tree_sha256(path) {
        Ok(existing) if existing == rendered => Ok(rendered),
        _ => Err(UzeError::ManagedEntryConflict(path.to_path_buf())),
    }
}

fn inspect_generated_tree(path: &Path, digest: &str) -> AttachmentInspection {
    let (state, reason) = match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_dir() => (
            AttachmentState::Conflict,
            "managed path is occupied by something other than a directory".to_owned(),
        ),
        Ok(_) => match crate::digest::tree_sha256(path) {
            Ok(actual) if actual == digest => (
                AttachmentState::Matched,
                "managed directory content matches receipt".to_owned(),
            ),
            Ok(_) => (
                AttachmentState::Drifted,
                "managed directory content differs from receipt".to_owned(),
            ),
            Err(error) => (AttachmentState::Blocked, error.to_string()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            AttachmentState::Missing,
            "managed directory is missing".to_owned(),
        ),
        Err(error) => (AttachmentState::Blocked, error.to_string()),
    };
    AttachmentInspection { state, reason }
}

/// Every entry's relative path, length and modification time: an edit,
/// an addition or a removal anywhere in the tree moves it, without reading
/// a byte of content.
fn tree_stat_signature(root: &Path) -> String {
    let mut entries = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(children) = fs::read_dir(&dir) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(path.clone());
            }
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_nanos());
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            entries.push(format!(
                "{}:{}:{modified}",
                relative.display(),
                metadata.len()
            ));
        }
    }
    if entries.is_empty() && fs::symlink_metadata(root).is_err() {
        return "absent".to_owned();
    }
    entries.sort();
    crate::digest::short_hex(entries.join("\n").as_bytes())
}

fn inspect_generated_file(path: &Path, content: &str) -> AttachmentInspection {
    let (state, reason) = match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => (
            AttachmentState::Conflict,
            "managed path is occupied by something other than a file".to_owned(),
        ),
        Ok(_) => match fs::read(path) {
            Ok(existing) if existing == content.as_bytes() => (
                AttachmentState::Matched,
                "managed file content matches receipt".to_owned(),
            ),
            Ok(_) => (
                AttachmentState::Drifted,
                "managed file content differs from receipt".to_owned(),
            ),
            Err(error) => (AttachmentState::Blocked, error.to_string()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            AttachmentState::Missing,
            "managed file is missing".to_owned(),
        ),
        Err(error) => (AttachmentState::Blocked, error.to_string()),
    };
    AttachmentInspection { state, reason }
}

fn attach_symlink(path: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let current = fs::read_link(path).map_err(UzeError::read(path))?;
            if current == target {
                return Ok(());
            }
            // A reference that resolves to nothing is holding nothing: the
            // harness reads no capability through it, and refusing it
            // preserves no work. That is exactly what a UZE-owned target
            // which moved leaves behind — the generated tier is rebuilt at
            // its new path while the reference into it stays where the
            // harness looks — so the old reference is adopted rather than
            // defended against its own owner.
            //
            // The question is asked as "is the target absent", never as
            // "can the target be reached": a volume not mounted, a
            // directory this user may not traverse, a dead network path —
            // each answers "no" to the second question while the content
            // is still there, and adopting on that answer would delete a
            // live reference. Only `NotFound` is absence; every other
            // error means UZE cannot tell, and what it cannot tell about
            // it does not touch.
            match fs::metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::remove_file(path).map_err(UzeError::write(path))?;
                    crate::persistence::create_symlink(target, path)
                }
                // Resolves, or cannot be told apart from one that does: a
                // managed-looking name is not ownership proof, and users
                // may repoint an earlier UZE reference. Preserve it.
                _ => Err(UzeError::ManagedEntryDrift(path.to_path_buf())),
            }
        }
        Ok(_) => Err(UzeError::ManagedEntryConflict(path.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::persistence::create_symlink(target, path)
        }
        Err(source) => Err(UzeError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn inspect_symlink(path: &Path, target: &Path) -> AttachmentInspection {
    let (state, reason) = match fs::read_link(path) {
        Ok(actual) if actual == target => (
            AttachmentState::Matched,
            "managed symlink target matches receipt".to_owned(),
        ),
        Ok(_) => (
            AttachmentState::Drifted,
            "symlink target differs from receipt".to_owned(),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            AttachmentState::Missing,
            "managed symlink is missing".to_owned(),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => (
            AttachmentState::Conflict,
            "managed path is occupied by a non-symlink".to_owned(),
        ),
        Err(error) => (AttachmentState::Blocked, error.to_string()),
    };
    AttachmentInspection { state, reason }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExposurePlan {
    pub route: CompatibilityRoute,
    pub mechanism: ExposureMechanism,
    pub evidence: String,
}

/// Package-level planning is intentionally separate from `ExposurePlan`:
/// Plugin is the distribution unit; resources/capabilities remain the unit
/// of compatibility. A native package plan declares exactly which resources
/// it consumes so callers never also attach them individually.
///
/// It deliberately carries no mechanism. The Core needs to know only *that*
/// a harness consumes this package as a native unit and which resources that
/// covers — never how. Every value an earlier `PackageExposureMechanism`
/// held (catalog root, catalog name, plugin selector) was derivable by the
/// owning integration from the package and UZE's own layout, so the enum
/// carried vendor vocabulary rather than information.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackageExposurePlan {
    pub package_id: PackageId,
    pub route: CompatibilityRoute,
    pub envelope: PackageEnvelope,
    pub provided_resource_identities: BTreeSet<String>,
    pub evidence: String,
}

/// Whose manifest a package-level delivery hands the harness: the one the
/// author shipped, or one the integration wrote because the author shipped
/// none. Both reach the harness as a package, and an author debugging a
/// release needs to know which one it read.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageEnvelope {
    Own,
    Generated,
}

impl PackageExposurePlan {
    pub fn provides(&self, resource: &Resource) -> bool {
        self.provided_resource_identities
            .contains(&resource.identity())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn write_skill(body: &'static str) -> impl FnOnce(&Path) -> Result<()> {
        move |dir: &Path| {
            fs::write(dir.join("SKILL.md"), body).unwrap();
            Ok(())
        }
    }

    #[test]
    fn a_tree_is_written_replaced_while_owned_and_its_digest_proves_it() {
        let root = uze_testkit::temp::scratch("generated-tree");
        let path = root.join("skills/flow:review");
        let first = attach_generated_tree(&path, None, write_skill("one")).unwrap();
        let artifact = ManagedArtifact::GeneratedTree {
            path: path.clone(),
            digest: first.clone(),
        };
        assert_eq!(artifact.inspect_standard().state, AttachmentState::Matched);

        let second = attach_generated_tree(&path, Some(&first), write_skill("two")).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "two");
        assert_eq!(artifact.inspect_standard().state, AttachmentState::Drifted);
    }

    #[test]
    fn an_edited_tree_is_drift_and_is_not_replaced() {
        let root = uze_testkit::temp::scratch("generated-tree-drift");
        let path = root.join("flow:review");
        let digest = attach_generated_tree(&path, None, write_skill("one")).unwrap();
        fs::write(path.join("SKILL.md"), "edited").unwrap();
        let outcome = attach_generated_tree(&path, Some(&digest), write_skill("two"));
        assert!(matches!(outcome, Err(UzeError::ManagedEntryDrift(_))));
        assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "edited");
    }

    #[test]
    fn an_unowned_directory_is_adopted_only_when_it_already_holds_the_same_tree() {
        let root = uze_testkit::temp::scratch("generated-tree-foreign");
        let path = root.join("flow:review");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), "theirs").unwrap();
        let outcome = attach_generated_tree(&path, None, write_skill("ours"));
        assert!(matches!(outcome, Err(UzeError::ManagedEntryConflict(_))));
        assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "theirs");

        let same = attach_generated_tree(&path, None, write_skill("theirs")).unwrap();
        assert_eq!(same, crate::digest::tree_sha256(&path).unwrap());
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1,
            "no probe left behind"
        );
    }

    #[test]
    fn a_matched_tree_is_detached_and_a_drifted_one_is_kept() {
        let root = uze_testkit::temp::scratch("generated-tree-detach");
        let path = root.join("flow:review");
        let digest = attach_generated_tree(&path, None, write_skill("one")).unwrap();
        let artifact = ManagedArtifact::GeneratedTree {
            path: path.clone(),
            digest,
        };
        fs::write(path.join("SKILL.md"), "edited").unwrap();
        assert_eq!(
            artifact.detach_standard().unwrap().state,
            AttachmentState::Drifted
        );
        assert!(path.exists());
        fs::write(path.join("SKILL.md"), "one").unwrap();
        assert_eq!(
            artifact.detach_standard().unwrap().state,
            AttachmentState::Missing
        );
        assert!(!path.exists());
    }

    fn managed_reference(discovery_root: &Path, target: &Path) -> ManagedArtifact {
        ManagedArtifact::SymlinkReference {
            path: discovery_root.join("uze-example"),
            target: target.to_path_buf(),
        }
    }

    #[test]
    fn attach_creates_a_symlink_and_is_idempotent() {
        let root = uze_testkit::temp::scratch("attach");
        let discovery_root = root.join("skills");
        let source = root.join("store-entry");
        fs::create_dir_all(&source).unwrap();

        let artifact = managed_reference(&discovery_root, &source);
        artifact.attach_standard().unwrap();
        let link = discovery_root.join("uze-example");
        assert!(link.is_symlink());
        assert_eq!(fs::read_link(&link).unwrap(), source);

        // Second attach is a no-op, not an error and not a re-link.
        artifact.attach_standard().unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), source);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn attach_adopts_a_reference_whose_target_no_longer_exists() {
        // The shape UZE's own move of its generated tier left on every
        // machine that had one: the reference is where the harness looks,
        // and points at a directory that is gone.
        let root = uze_testkit::temp::scratch("adopt");
        let discovery_root = root.join("skills");
        fs::create_dir_all(&discovery_root).unwrap();
        let link = discovery_root.join("uze-example");
        crate::persistence::create_symlink(&root.join("where-it-used-to-be"), &link).unwrap();

        let source = root.join("where-it-is-now");
        fs::create_dir_all(&source).unwrap();
        managed_reference(&discovery_root, &source)
            .attach_standard()
            .unwrap();

        assert_eq!(fs::read_link(&link).unwrap(), source);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn attach_preserves_a_reference_somebody_repointed_at_their_own_content() {
        let root = uze_testkit::temp::scratch("repointed");
        let discovery_root = root.join("skills");
        fs::create_dir_all(&discovery_root).unwrap();
        let theirs = root.join("their-own-skill");
        fs::create_dir_all(&theirs).unwrap();
        let link = discovery_root.join("uze-example");
        crate::persistence::create_symlink(&theirs, &link).unwrap();

        let source = root.join("store-entry");
        fs::create_dir_all(&source).unwrap();
        let error = managed_reference(&discovery_root, &source)
            .attach_standard()
            .unwrap_err();

        assert!(matches!(error, UzeError::ManagedEntryDrift(_)));
        assert_eq!(fs::read_link(&link).unwrap(), theirs);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn attach_preserves_a_reference_whose_target_cannot_be_read() {
        // Absence and unreadability are different answers. A target behind
        // a directory this user may not traverse is still there, and
        // adopting on "could not stat" would delete a live reference.
        let root = uze_testkit::temp::scratch("unreadable");
        let discovery_root = root.join("skills");
        fs::create_dir_all(&discovery_root).unwrap();
        let vault = root.join("vault");
        let theirs = vault.join("their-own-skill");
        fs::create_dir_all(&theirs).unwrap();
        let link = discovery_root.join("uze-example");
        crate::persistence::create_symlink(&theirs, &link).unwrap();

        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&vault, fs::Permissions::from_mode(0o000)).unwrap();

        let source = root.join("store-entry");
        fs::create_dir_all(&source).unwrap();
        let outcome = managed_reference(&discovery_root, &source).attach_standard();

        fs::set_permissions(&vault, fs::Permissions::from_mode(0o755)).unwrap();

        // Running as root defeats the permission bit entirely; there the
        // target simply resolves, which this same branch must also refuse.
        assert!(matches!(outcome, Err(UzeError::ManagedEntryDrift(_))));
        assert_eq!(fs::read_link(&link).unwrap(), theirs);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn attach_never_overwrites_an_entry_it_does_not_own() {
        let root = uze_testkit::temp::scratch("conflict");
        let discovery_root = root.join("skills");
        fs::create_dir_all(&discovery_root).unwrap();
        fs::create_dir_all(discovery_root.join("uze-example")).unwrap();
        let source = root.join("store-entry");
        fs::create_dir_all(&source).unwrap();

        let error = managed_reference(&discovery_root, &source)
            .attach_standard()
            .unwrap_err();
        assert!(matches!(error, UzeError::ManagedEntryConflict(_)));

        fs::remove_dir_all(&root).unwrap();
    }
}

#[cfg(test)]
mod generated_file_tests {
    use super::*;

    #[test]
    fn a_generated_file_is_written_recognised_and_never_taken_from_someone_else() {
        let root = uze_testkit::temp::scratch("generated-file");
        let path = root.join("agents/flow:reviewer.md");
        let artifact = ManagedArtifact::GeneratedFile {
            path: path.clone(),
            content: "---\nname: flow:reviewer\n---\nReview.\n".to_owned(),
        };
        assert_eq!(artifact.inspect_standard().state, AttachmentState::Missing);
        artifact.attach_standard().unwrap();
        assert!(!path.is_symlink());
        assert_eq!(artifact.inspect_standard().state, AttachmentState::Matched);
        assert_eq!(artifact.exposure_name().as_deref(), Some("flow:reviewer"));
        artifact.attach_standard().unwrap();

        fs::write(&path, "edited by hand").unwrap();
        assert_eq!(artifact.inspect_standard().state, AttachmentState::Drifted);
        assert!(matches!(
            artifact.attach_standard(),
            Err(UzeError::ManagedEntryConflict(_))
        ));
        assert_eq!(
            artifact.detach_standard().unwrap().state,
            AttachmentState::Drifted,
            "a drifted file is never removed"
        );
        assert!(path.is_file());

        fs::write(&path, "---\nname: flow:reviewer\n---\nReview.\n").unwrap();
        assert_eq!(
            artifact.detach_standard().unwrap().state,
            AttachmentState::Missing
        );
        assert!(!path.exists());
    }
}
