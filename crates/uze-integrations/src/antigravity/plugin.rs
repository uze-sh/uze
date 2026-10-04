//! Antigravity native package delivery: `agy plugin install
//! <package-directory>`, pointing straight at a package directory (the
//! Store's, or a UZE-owned generated one), never publishing a catalogue
//! (Antigravity needs none — see the module doc on
//! [`super::AntigravityIntegration`]).
//!
//! Unlike link-based installers, there is no link route: `plugin
//! install` stages a **byte copy** at `~/.gemini/config/plugins/<name>/`
//! (symlinks are dereferenced — verified against 1.1.19), so the staged
//! tree is deliberately treated as a Derived Artifact (ADR-013 §5): UZE
//! rebuilds it from the Store on attach, records a content fingerprint as
//! its ownership proof, and removes it through `agy plugin uninstall` on
//! detach. No Store bytes are ever read from the staged copy.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    Result, UzeError,
    integration::{AttachmentInspection, AttachmentReceipt, AttachmentState, IntegrationPort},
    store::StoredPackage,
};

use super::AntigravityIntegration;
use crate::shared::process::{capture, json, run_quiet};

/// The `kind` stamped on explicit-plugin receipts. Only this module and its
/// composition root interpret it.
pub(super) const PLUGIN_KIND: &str = "antigravity-plugin";
/// The `kind` stamped on generated-plugin receipts (see `generate.rs`).
pub(super) const GENERATED_PLUGIN_KIND: &str = "antigravity-plugin-generated";

/// The vendor's plugin-name pattern (`plugin.json` docs + `invalid plugin
/// name` error): alphanumerics, hyphens, underscores.
pub(super) fn valid_plugin_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// The canonical manifest's `name` field, when it is a usable Antigravity
/// plugin name. The canonical UZE `plugin.json` doubles as the vendor
/// manifest, so this — not a separate vendor-specific file — is what
/// decides explicitness. A missing or unparseable manifest, or a name that
/// does not satisfy the vendor pattern, yields `None` (the package falls
/// back to capability-level delivery; no generated name is ever invented).
pub(super) fn plugin_manifest_name(package: &StoredPackage) -> Option<String> {
    let bytes = fs::read(&package.manifest).ok()?;
    let manifest: serde_json::Value = uze_core::authored::json(&bytes).ok()?;
    let name = manifest.get("name")?.as_str()?;
    valid_plugin_name(name).then(|| name.to_owned())
}

/// MCP server names declared by an author-shipped `mcp_config.json` at the
/// package root — the one Antigravity-specific file an author may provide.
/// Shared with `generate.rs`'s coverage logic.
pub(super) fn author_mcp_config_servers(package: &StoredPackage) -> BTreeSet<String> {
    declared_servers(&package.root.join("mcp_config.json"))
}

pub(super) fn declared_servers(path: &Path) -> BTreeSet<String> {
    fs::read(path)
        .ok()
        .and_then(|bytes| uze_core::authored::json::<serde_json::Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("mcpServers")
                .and_then(serde_json::Value::as_object)
                .map(|servers| servers.keys().cloned().collect())
        })
        .unwrap_or_default()
}

/// Whether a resource lives under the package's conventional `skills/`
/// directory, compared by component so a `skills-extra` sibling is never
/// mistaken for inside it.
pub(super) fn under_skills_dir(package: &StoredPackage, path: &Path) -> bool {
    path.strip_prefix(&package.root)
        .ok()
        .and_then(Path::parent)
        .is_some_and(|parent| parent.starts_with("skills"))
}

// --- CLI verbs ---------------------------------------------------------------

pub(super) fn run_agy(
    executable: &str,
    command_home: &Path,
    args: &[&str],
    label: &str,
) -> Result<()> {
    run_quiet(Path::new(executable), command_home, label, args)
}

/// The full `agy plugin list` document, which the CLI writes as JSON
/// (verified against 1.1.19; `{"imports":[...]}`). An unreadable listing is
/// an error — inspection must never guess about ownership from silence.
///
/// `agy` prints its own `import_manifest.json`, so that is read first: an
/// `agy` start is a quarter of a second, paid three times by a removal.
/// Trusted only in the shape the CLI prints; anything else asks the CLI.
///
/// Nothing imported is an answer too, spelled two ways that are not a
/// listing (1.2.11): the manifest keeps `"imports": null`, and the CLI
/// prints a sentence instead of JSON. Read as unreadable, uninstalling the
/// last plugin — what every update of the only one does — could never be
/// confirmed, and blocked the update.
pub(super) fn installed_plugins(
    executable: &str,
    command_home: &Path,
) -> std::result::Result<serde_json::Value, String> {
    let recorded = std::fs::read(command_home.join(".gemini/config/import_manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|manifest| match manifest.get("imports") {
            Some(serde_json::Value::Null) => Some(no_imports()),
            Some(serde_json::Value::Array(imports))
                if imports.iter().all(|entry| entry.get("name").is_some()) =>
            {
                Some(manifest)
            }
            _ => None,
        });
    if let Some(recorded) = recorded {
        return Ok(recorded);
    }
    json(
        Path::new(executable),
        command_home,
        &["plugin", "list"],
        "agy",
    )
    .or_else(|unreadable| {
        let said = capture(Path::new(executable), command_home, &["plugin", "list"])
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
        match said.as_deref() {
            Some(NOTHING_IMPORTED) => Ok(no_imports()),
            _ => Err(unreadable),
        }
    })
}

/// What `agy plugin list` prints in place of a listing when nothing is
/// imported.
const NOTHING_IMPORTED: &str = "No imported plugins.";

fn no_imports() -> serde_json::Value {
    serde_json::json!({ "imports": [] })
}

/// The ownership decision for one installed plugin, separated from the
/// process call so every branch is testable without an `agy` binary.
/// Ownership is proven by registration + staged identity + content
/// fingerprint — the vendor's manifest has no source path, so the
/// fingerprint is the only thing that distinguishes UZE's copy from a
/// user-imported same-name plugin.
pub(super) fn inspect_installed_plugin(
    listing: &serde_json::Value,
    name: &str,
    staged_dir: &Path,
    expected_fingerprint: &str,
) -> AttachmentInspection {
    let Some(entries) = listing.get("imports").and_then(serde_json::Value::as_array) else {
        return AttachmentInspection {
            state: AttachmentState::Blocked,
            reason: "agy plugin list has no imports array".to_owned(),
        };
    };
    let matching: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some(name))
        .collect();
    match matching.as_slice() {
        [] => {
            return AttachmentInspection {
                state: AttachmentState::Missing,
                reason: "Antigravity plugin is not imported".to_owned(),
            };
        }
        [_] => {}
        _ => {
            return AttachmentInspection {
                state: AttachmentState::Conflict,
                reason: "more than one Antigravity plugin answers to this name".to_owned(),
            };
        }
    }
    if !staged_dir.is_dir() {
        return AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "Antigravity plugin staging directory is missing".to_owned(),
        };
    }
    if !staged_dir.join("plugin.json").is_file() {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Antigravity plugin staging directory no longer holds a plugin.json".to_owned(),
        };
    }
    let actual_fingerprint = match fingerprint_dir(staged_dir) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return AttachmentInspection {
                state: AttachmentState::Blocked,
                reason: error.to_string(),
            };
        }
    };
    if actual_fingerprint != expected_fingerprint {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Antigravity plugin staged content differs from the receipt".to_owned(),
        };
    }
    // Enablement is deliberately not part of this ownership proof: `disable`
    // is a user preference on an artifact UZE still demonstrably created
    // (the same rationale every other integration applies to its vendor's
    // enablement signal).
    AttachmentInspection {
        state: AttachmentState::Matched,
        reason: "Antigravity plugin staged copy matches receipt (enablement is a user preference, not an ownership signal)"
            .to_owned(),
    }
}

// --- Attachment --------------------------------------------------------------

/// Attaches a package through the plugin UZE generates for it in a
/// UZE-owned derived directory.
pub(super) fn attach_generated_plugin(
    executable: &str,
    integration: &AntigravityIntegration,
    package: &StoredPackage,
) -> Result<Option<AttachmentReceipt>> {
    let name = plugin_manifest_name(package).ok_or_else(|| {
        UzeError::ExposureUnavailable("package has no usable plugin name".to_owned())
    })?;
    if !preflight_name_free(executable, integration, &name) {
        return Ok(None);
    }
    let derived_dir =
        super::generate::materialize_generated_plugin(&integration.uze_home, package)?;
    let args: Vec<&Path> = vec![Path::new("plugin"), Path::new("install"), &derived_dir];
    run_quiet(
        Path::new(executable),
        &integration.command_home,
        &format!("agy plugin install `{name}`"),
        &args,
    )?;
    // Antigravity stages a copy named after the plugin's own declared
    // manifest name, not the source directory it was given (verified
    // against real agy 1.1.22) — the same convention the Store-tree route
    // above already relies on. `derived_dir`'s own basename is the qualified
    // Store id, never `name` (`generated_package_dir_for_id`), so it cannot
    // be used to predict the staged path.
    let staged_dir = integration.plugins_dir.join(&name);
    let fingerprint = fingerprint_dir(&staged_dir)?;
    Ok(Some(AttachmentReceipt {
        package_id: package.id.as_str().to_owned(),
        resource_identity: None,
        integration: integration.id().to_owned(),
        artifact: uze_core::integration::ManagedArtifact::IntegrationOwned {
            kind: GENERATED_PLUGIN_KIND.to_owned(),
            selector: name,
            detail: [
                ("source_path".to_owned(), serde_json::json!(derived_dir)),
                ("staged_path".to_owned(), serde_json::json!(staged_dir)),
                ("package_root".to_owned(), serde_json::json!(package.root)),
                ("fingerprint".to_owned(), serde_json::json!(fingerprint)),
                ("origin".to_owned(), serde_json::json!("generated")),
            ]
            .into_iter()
            .collect(),
        },
    }))
}

/// UZE never overwrites an import it does not own. The vendor's install
/// verb merges over an existing same-name plugin (verified: stale files
/// survive a re-install), so a name already registered — with no receipt in
/// the ledger — is foreign state, not something to clobber or silently
/// resume. It is nevertheless a successful no-op: the harness already
/// exposes a native plugin under the requested name, and UZE must neither
/// replace it nor present ordinary setup as failed.
fn preflight_name_free(executable: &str, integration: &AntigravityIntegration, name: &str) -> bool {
    // If the listing itself is unreadable (agy absent, malformed output),
    // let the install verb speak for itself rather than double-guessing here.
    if let Ok(listing) = installed_plugins(executable, &integration.command_home) {
        let already = listing
            .get("imports")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("name").and_then(serde_json::Value::as_str) == Some(name)
                })
            });
        if already {
            return false;
        }
    }
    true
}

/// Deterministic content fingerprint of a directory tree (relative path +
/// bytes, files only, sorted). FNV-1a 64 — stable across Rust releases and
/// platforms, which receipts persisted across versions require. The staged
/// copy is byte-identical to the installed source (verified against 1.1.19:
/// `diff -r` on the staged tree), so the fingerprint recorded at attach time
/// and the one recomputed at inspection time agree by construction.
pub(super) fn fingerprint_dir(dir: &Path) -> Result<String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current).map_err(UzeError::read(&current))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(UzeError::read(&path))?;
            if metadata.is_symlink() {
                // Never follow symlinks out of the tree; a symlink adds no
                // content of its own and the install verb dereferences it.
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut digest = String::new();
    for path in files {
        let relative = path.strip_prefix(dir).map_err(|_| {
            UzeError::ExposureUnavailable("fingerprint path escaped its root".to_owned())
        })?;
        let bytes = fs::read(&path).map_err(UzeError::read(&path))?;
        digest.push_str(&relative.to_string_lossy());
        digest.push('\0');
        digest.push_str(&bytes.len().to_string());
        digest.push('\0');
        digest.push_str(&fnv1a64(&bytes));
        digest.push('\n');
    }
    Ok(digest)
}

/// FNV-1a 64-bit digest, implemented locally (no hash-crate dependency) so
/// it is stable across Rust releases and platforms — receipts persisted
/// across versions require that, and `DefaultHasher`'s algorithm is not
/// guaranteed stable.
fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod plugin_tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use uze_core::capability::Resource;
    use uze_core::capability::{Capability, CapabilityKind};
    use uze_core::home::UzeHome;
    use uze_core::integration::{AttachmentState, IntegrationPort};
    use uze_core::store::StoredPackage;

    use super::*;
    use crate::antigravity::AntigravityIntegration;

    fn temp_root(label: &str) -> PathBuf {
        uze_testkit::temp::scratch(label)
    }

    fn make_package(label: &str, manifest: &str) -> (PathBuf, StoredPackage) {
        let root = temp_root(label);
        let pkg_root = root.join("pkg");
        fs::create_dir_all(&pkg_root).unwrap();
        fs::write(pkg_root.join("plugin.json"), manifest).unwrap();
        let id =
            uze_core::store::PackageId::from_plugin_name("flow", &pkg_root.join("plugin.json"))
                .unwrap();
        let pkg = StoredPackage {
            active_name: id.plugin_name().to_owned(),
            id,
            root: pkg_root.clone(),
            manifest: pkg_root.join("plugin.json"),
            provenance: uze_core::acquisition::Provenance {
                requested: uze_core::acquisition::PackageSource::Local {
                    path: PathBuf::from("/tmp/fake"),
                },
                resolved: uze_core::acquisition::ResolvedSource::Local {
                    path: PathBuf::from("/tmp/fake"),
                },
            },
        };
        (root, pkg)
    }

    fn skill_resource(pkg: &StoredPackage, dir: &str, name: &str) -> Resource {
        let path = pkg.root.join(dir).join(name).join("SKILL.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("---\nname: {name}\n---\n")).unwrap();
        Resource::from_package(
            pkg.id.clone(),
            pkg.root.clone(),
            Capability {
                kind: CapabilityKind::AgentSkill,
                path,
                payload: Vec::new(),
            },
        )
    }

    fn mcp_resource(pkg: &StoredPackage, name: &str) -> Resource {
        let path = pkg.root.join("mcp_config.json");
        Resource::from_package_named(
            pkg.id.clone(),
            pkg.root.clone(),
            Capability {
                kind: CapabilityKind::Mcp,
                path,
                payload: Vec::new(),
            },
            name.to_owned(),
        )
    }

    // --- Manifest/name rules ----------------------------------------------

    #[test]
    fn valid_plugin_names_follow_the_vendor_pattern() {
        assert!(valid_plugin_name("flow"));
        assert!(valid_plugin_name("my-plugin"));
        assert!(valid_plugin_name("my_plugin"));
        assert!(!valid_plugin_name("Bad Name!"));
        assert!(!valid_plugin_name("flow:review"));
        assert!(!valid_plugin_name(""));
    }

    #[test]
    fn a_valid_manifest_name_decides_the_explicit_route() {
        let (_root, pkg) = make_package("name-ok", r#"{"name":"flow","version":"1.0.0"}"#);
        assert_eq!(plugin_manifest_name(&pkg).as_deref(), Some("flow"));
        let (_root2, pkg2) = make_package("name-bad", r#"{"name":"Bad Name!","version":"1.0.0"}"#);
        assert_eq!(plugin_manifest_name(&pkg2), None);
        let (_root3, pkg3) = make_package("name-missing", r#"{"version":"1.0.0"}"#);
        assert_eq!(plugin_manifest_name(&pkg3), None);
    }

    // --- Listing --------------------------------------------------------------

    /// Uninstalling the last plugin leaves `"imports": null` behind (agy
    /// 1.2.11). That is a listing with nothing in it, so the plugin just
    /// taken off reads as gone — never as an unreadable listing that
    /// blocks the removal an update is made of.
    #[test]
    fn a_manifest_emptied_to_null_lists_nothing_imported() {
        let home = temp_root("agy-null-imports");
        fs::create_dir_all(home.join(".gemini/config")).unwrap();
        fs::write(
            home.join(".gemini/config/import_manifest.json"),
            r#"{"imports": null}"#,
        )
        .unwrap();

        let listing = installed_plugins("/nonexistent/agy", &home).unwrap();

        let inspection = inspect_installed_plugin(&listing, "uze", &home.join("uze"), "digest");
        assert_eq!(inspection.state, AttachmentState::Missing);
        let _ = fs::remove_dir_all(home);
    }

    /// With no manifest to read, the CLI is asked, and it says there is
    /// nothing imported in a sentence rather than in JSON.
    #[cfg(unix)]
    #[test]
    fn the_cli_saying_nothing_is_imported_lists_nothing_imported() {
        use std::os::unix::fs::PermissionsExt;

        let home = temp_root("agy-nothing-imported");
        let agy = home.join("agy");
        fs::write(&agy, "#!/bin/sh\necho 'No imported plugins.'\n").unwrap();
        fs::set_permissions(&agy, fs::Permissions::from_mode(0o755)).unwrap();

        let listing = installed_plugins(&agy.to_string_lossy(), &home).unwrap();

        assert_eq!(listing, serde_json::json!({ "imports": [] }));
        let _ = fs::remove_dir_all(home);
    }

    // --- Exact coverage -----------------------------------------------------

    fn listing_with(name: &str) -> serde_json::Value {
        serde_json::json!({"imports":[{"name": name, "source":"antigravity", "components":["skills"]}]})
    }

    /// A. Conventional skill + declared MCP → covered; skill outside
    /// `skills/` → not covered.
    #[test]
    fn explicit_coverage_covers_exactly_the_conventional_and_declared_surface() {
        let (_root, pkg) = make_package("explicit-full", r#"{"name":"flow"}"#);
        fs::write(
            pkg.root.join("mcp_config.json"),
            r#"{"mcpServers":{"mcp-a":{"command":"a"}}}"#,
        )
        .unwrap();
        let r_skill = skill_resource(&pkg, "skills", "commit");
        let r_out = skill_resource(&pkg, "extra", "outside");
        let r_mcp = mcp_resource(&pkg, "mcp-a");
        let resources = vec![&r_skill, &r_out, &r_mcp];
        let covered = crate::antigravity::generate::generated_exact_coverage(&pkg, &resources);
        assert_eq!(
            covered,
            BTreeSet::from([r_skill.identity(), r_mcp.identity()])
        );
        assert!(!covered.contains(&r_out.identity()));
        let _ = fs::remove_dir_all(_root);
    }

    /// B. A missing mcp_config.json contributes no MCP coverage, never an error.
    #[test]
    fn missing_author_mcp_config_yields_no_mcp_coverage() {
        let (_root, pkg) = make_package("explicit-no-mcp", r#"{"name":"flow"}"#);
        let r_m = mcp_resource(&pkg, "mcp-a");
        let resources = vec![&r_m];
        assert!(
            crate::antigravity::generate::generated_exact_coverage(&pkg, &resources).is_empty()
        );
        let _ = fs::remove_dir_all(_root);
    }

    /// C. A malformed author mcp_config.json is tolerated as no declaration.
    #[test]
    fn malformed_author_mcp_config_is_tolerated_as_no_declaration() {
        let (_root, pkg) = make_package("explicit-malformed", r#"{"name":"flow"}"#);
        fs::write(pkg.root.join("mcp_config.json"), "{not json").unwrap();
        let r_m = mcp_resource(&pkg, "mcp-a");
        let covered = crate::antigravity::generate::generated_exact_coverage(&pkg, &[&r_m]);
        assert!(covered.is_empty());
        let _ = fs::remove_dir_all(_root);
    }

    // --- Fingerprint ---------------------------------------------------------

    #[test]
    fn fingerprint_is_deterministic_and_content_sensitive() {
        let root = temp_root("fingerprint");
        fs::create_dir_all(root.join("a/skills/x")).unwrap();
        fs::write(root.join("a/plugin.json"), r#"{"name":"x"}"#).unwrap();
        fs::write(root.join("a/skills/x/SKILL.md"), "body").unwrap();
        let one = fingerprint_dir(&root.join("a")).unwrap();
        let two = fingerprint_dir(&root.join("a")).unwrap();
        assert_eq!(one, two);
        fs::write(root.join("a/skills/x/SKILL.md"), "different").unwrap();
        assert_ne!(one, fingerprint_dir(&root.join("a")).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fingerprint_is_order_independent() {
        let root = temp_root("fingerprint-order");
        let a = root.join("a");
        let b = root.join("b");
        for dir in [&a, &b] {
            fs::create_dir_all(dir.join("skills")).unwrap();
        }
        // Same final content, different creation order (a: plugin.json first;
        // b: skill first) — the digest must not depend on insertion order.
        fs::write(a.join("plugin.json"), r#"{"name":"x"}"#).unwrap();
        fs::write(b.join("plugin.json"), r#"{"name":"x"}"#).unwrap();
        fs::write(b.join("skills/SKILL.md"), "same").unwrap();
        fs::write(a.join("skills/SKILL.md"), "same").unwrap();
        assert_eq!(fingerprint_dir(&a).unwrap(), fingerprint_dir(&b).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    // --- Inspection (pure, no binary) ---------------------------------------

    fn staged_tree(root: &Path, name: &str, content: &str) -> PathBuf {
        let dir = root.join("config/plugins").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.json"), content).unwrap();
        dir
    }

    #[test]
    fn an_absent_plugin_is_missing_not_blocked() {
        let listing = serde_json::json!({"imports":[]});
        let inspection =
            inspect_installed_plugin(&listing, "flow", Path::new("/nope/flow"), "fingerprint");
        assert_eq!(inspection.state, AttachmentState::Missing);
    }

    #[test]
    fn a_registered_matching_plugin_is_matched() {
        let root = temp_root("inspect-matched");
        let staged = staged_tree(&root, "flow", r#"{"name":"flow"}"#);
        let fingerprint = fingerprint_dir(&staged).unwrap();
        let listing = listing_with("flow");
        let inspection = inspect_installed_plugin(&listing, "flow", &staged, &fingerprint);
        assert_eq!(inspection.state, AttachmentState::Matched);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn changed_staged_content_is_drift() {
        let root = temp_root("inspect-drift");
        let staged = staged_tree(&root, "flow", r#"{"name":"flow"}"#);
        let fingerprint = fingerprint_dir(&staged).unwrap();
        fs::write(
            staged.join("plugin.json"),
            r#"{"name":"flow","description":"tampered"}"#,
        )
        .unwrap();
        let inspection =
            inspect_installed_plugin(&listing_with("flow"), "flow", &staged, &fingerprint);
        assert_eq!(inspection.state, AttachmentState::Drifted);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_registered_plugin_with_a_missing_staging_dir_is_missing() {
        let root = temp_root("inspect-missing-dir");
        let inspection = inspect_installed_plugin(
            &listing_with("flow"),
            "flow",
            &root.join("config/plugins/flow"),
            "fingerprint",
        );
        assert_eq!(inspection.state, AttachmentState::Missing);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn two_entries_answering_to_one_name_are_ambiguous() {
        let listing = serde_json::json!({"imports":[
            {"name":"flow","source":"antigravity"},
            {"name":"flow","source":"other"}
        ]});
        let inspection =
            inspect_installed_plugin(&listing, "flow", Path::new("/nope/flow"), "fingerprint");
        assert_eq!(inspection.state, AttachmentState::Conflict);
    }

    /// Disabling is a user preference on an artifact UZE still owns, so it
    /// must stay MATCHED — otherwise `uze remove` could never detach it.
    #[test]
    fn a_registered_matching_plugin_is_matched_when_no_enablement_field_exists() {
        let root = temp_root("inspect-enablement");
        let staged = staged_tree(&root, "flow", r#"{"name":"flow"}"#);
        let fingerprint = fingerprint_dir(&staged).unwrap();
        // The vendor's list carries no enablement signal for imported
        // plugins (only installs do); identity + fingerprint prove ownership.
        let inspection =
            inspect_installed_plugin(&listing_with("flow"), "flow", &staged, &fingerprint);
        assert_eq!(inspection.state, AttachmentState::Matched);
        let _ = fs::remove_dir_all(root);
    }

    // --- Plan-level precedence (no binary needed) ---------------------------

    #[test]
    fn a_canonical_package_with_no_mcp_takes_the_explicit_route() {
        let (_root, pkg) = make_package("plan-explicit", r#"{"name":"flow","description":"d"}"#);
        let r_a = skill_resource(&pkg, "skills", "commit");
        let uze_home = UzeHome::at(_root.join("uze"));
        let integration = AntigravityIntegration::new(_root.join("agents"), uze_home);
        let plan = integration
            .package_exposure_plan(&pkg, &[&r_a])
            .expect("explicit route applies");
        assert_eq!(plan.route, uze_core::router::CompatibilityRoute::Native);
        assert_eq!(
            plan.provided_resource_identities,
            BTreeSet::from([r_a.identity()])
        );
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn an_invalid_plugin_name_takes_no_native_package_route() {
        let (_root, pkg) = make_package("plan-invalid", r#"{"name":"Bad Name!"}"#);
        let r_a = skill_resource(&pkg, "skills", "commit");
        let uze_home = UzeHome::at(_root.join("uze"));
        let integration = AntigravityIntegration::new(_root.join("agents"), uze_home);
        assert!(
            integration.package_exposure_plan(&pkg, &[&r_a]).is_none(),
            "a package whose canonical name is not a valid Antigravity plugin name must fall back to capability-level delivery"
        );
        let _ = fs::remove_dir_all(_root);
    }
}
