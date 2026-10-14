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
use crate::shared::process::{json, run_quiet};

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

/// One plugin Antigravity holds, as its own records name it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Installed {
    pub(super) name: String,
}

/// The plugins Antigravity holds, read for inspection, which runs on
/// budgeted paths: its own `import_manifest.json` first — an `agy` start is
/// a quarter of a second, paid three times by a removal — and the CLI only
/// when the manifest is absent or in a shape this build does not know.
///
/// The manifest knows imports only; a plugin installed from the vendor's
/// marketplace is seen by [`listed_plugins`], which install and detach ask.
/// Nothing imported is an answer too: the manifest keeps `"imports": null`
/// after the last uninstall (1.2.11), and reading that as unreadable would
/// block the removal every update of the only plugin is made of.
pub(super) fn installed_plugins(
    executable: &str,
    command_home: &Path,
) -> std::result::Result<Vec<Installed>, String> {
    let recorded = std::fs::read(command_home.join(".gemini/config/import_manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|manifest| match manifest.get("imports") {
            Some(serde_json::Value::Null) => Some(Vec::new()),
            Some(serde_json::Value::Array(imports)) => named(imports),
            _ => None,
        });
    match recorded {
        Some(recorded) => Ok(recorded),
        None => listed_plugins(executable, command_home),
    }
}

/// Every plugin Antigravity holds, imported or installed from its
/// marketplace, as `agy plugin list --json` prints them (an array of
/// `{name, enabled, version, source, path, skills}` since 1.3.4). A shape
/// this build does not know is an error: ownership is never guessed from
/// silence.
pub(super) fn listed_plugins(
    executable: &str,
    command_home: &Path,
) -> std::result::Result<Vec<Installed>, String> {
    let listing = json(
        Path::new(executable),
        command_home,
        &["plugin", "list", "--json"],
        "agy",
    )?;
    listing
        .as_array()
        .and_then(|entries| named(entries))
        .ok_or_else(|| "`agy plugin list --json` printed a listing this UZE cannot read".to_owned())
}

/// The names in a list of plugin entries; `None` when any entry has none.
fn named(entries: &[serde_json::Value]) -> Option<Vec<Installed>> {
    entries
        .iter()
        .map(|entry| {
            entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(|name| Installed {
                    name: name.to_owned(),
                })
        })
        .collect()
}

/// How many plugins Antigravity holds under `name`, asked of the CLI, which
/// sees marketplace installs the manifest does not.
pub(super) fn holders_of(
    executable: &str,
    command_home: &Path,
    name: &str,
) -> std::result::Result<usize, String> {
    Ok(listed_plugins(executable, command_home)?
        .iter()
        .filter(|installed| installed.name == name)
        .count())
}

/// Why `agy plugin uninstall <name>` must not run, from how many plugins
/// the CLI says hold the name; `None` when exactly one does, UZE's. With
/// none, the import manifest that called UZE's plugin present and the CLI
/// disagree, and removing by name is a guess.
pub(super) fn uninstall_refused(
    holders: std::result::Result<usize, String>,
) -> Option<AttachmentInspection> {
    match holders {
        Ok(1) => None,
        Ok(0) => Some(crate::shared::plan::blocked(
            "Antigravity's import manifest lists this plugin and `agy plugin list` does not",
        )),
        Ok(_) => Some(name_held_twice()),
        Err(unreadable) => Some(crate::shared::plan::blocked(unreadable)),
    }
}

/// Two plugins answer to one name: `uninstall` takes a name, so neither is
/// touched.
fn name_held_twice() -> AttachmentInspection {
    AttachmentInspection {
        state: AttachmentState::Conflict,
        reason: "more than one Antigravity plugin answers to this name".to_owned(),
    }
}

/// The ownership decision for one installed plugin, separated from the
/// process call so every branch is testable without an `agy` binary.
/// Ownership is proven by registration + staged identity + content
/// fingerprint — the vendor's manifest has no source path, so the
/// fingerprint is the only thing that distinguishes UZE's copy from a
/// user-imported same-name plugin.
pub(super) fn inspect_installed_plugin(
    listing: &[Installed],
    name: &str,
    staged_dir: &Path,
    expected_fingerprint: &str,
) -> AttachmentInspection {
    let matching: Vec<&Installed> = listing
        .iter()
        .filter(|installed| installed.name == name)
        .collect();
    match matching.as_slice() {
        // Nothing of UZE's is there; a directory under the name is a plugin
        // installed from the vendor's marketplace, which the manifest omits.
        [] if staged_dir.is_dir() => {
            return AttachmentInspection {
                state: AttachmentState::Missing,
                reason: "Antigravity holds another plugin under this name, one UZE did not install"
                    .to_owned(),
            };
        }
        [] => {
            return AttachmentInspection {
                state: AttachmentState::Missing,
                reason: "Antigravity plugin is not imported".to_owned(),
            };
        }
        [_] => {}
        _ => return name_held_twice(),
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
) -> Result<AttachmentReceipt> {
    let name = plugin_manifest_name(package).ok_or_else(|| {
        UzeError::ExposureUnavailable("package has no usable plugin name".to_owned())
    })?;
    preflight_name_free(executable, integration, &name)?;
    let super::PluginContents { hooks, agents } = integration.plugin_contents(package)?;
    if !hooks.is_empty()
        && let Some(source) = crate::hooks::wrapper_source(super::HOOKS)
    {
        crate::hooks::materialize_wrapper(
            &super::HOOKS.wrapper_path(&integration.uze_home),
            &source,
        )?;
    }
    let derived_dir = super::generate::materialize_generated_plugin(
        &integration.uze_home,
        package,
        &hooks,
        &agents,
    )?;
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
    Ok(AttachmentReceipt {
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
                (
                    crate::hooks::RUNS_HOOK_WRAPPER.to_owned(),
                    serde_json::json!(!hooks.is_empty()),
                ),
            ]
            .into_iter()
            .collect(),
        },
    })
}

/// UZE never overwrites a plugin it does not own. The vendor's install
/// verb replaces an existing same-name plugin's directory exactly (measured
/// on 1.2.17; it merged until 1.1.28), so installing over a name already
/// held — imported by hand or installed from the vendor's marketplace, with
/// no receipt in the ledger — would destroy someone else's plugin. The
/// delivery is refused and says so: reported as a success, the package
/// looked installed while none of it reached Antigravity (measured on
/// 1.3.5 against a marketplace plugin, where only `uze doctor` told). A
/// census UZE cannot read refuses too: the name might be held, and finding
/// out by overwriting is not an option.
fn preflight_name_free(
    executable: &str,
    integration: &AntigravityIntegration,
    name: &str,
) -> Result<()> {
    let holders = holders_of(executable, &integration.command_home, name).map_err(|unreadable| {
        UzeError::DeliveryFailed(format!(
            "cannot tell whether Antigravity already holds a plugin named `{name}`: {unreadable}"
        ))
    })?;
    if holders == 0 {
        return Ok(());
    }
    Err(UzeError::DeliveryFailed(format!(
        "Antigravity already holds a plugin named `{name}` that UZE did not install, and UZE \
         never replaces it; `agy plugin uninstall {name}` frees the name for this package"
    )))
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

    /// A stand-in `agy` that prints `listing` for `plugin list --json`.
    // It is a POSIX shell script.
    #[cfg(unix)]
    fn agy_listing(home: &Path, listing: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let agy = home.join("agy");
        fs::create_dir_all(home).unwrap();
        fs::write(&agy, format!("#!/bin/sh\ncat <<'EOF'\n{listing}\nEOF\n")).unwrap();
        fs::set_permissions(&agy, fs::Permissions::from_mode(0o755)).unwrap();
        agy.to_string_lossy().into_owned()
    }

    /// With no manifest to read, the CLI's JSON listing (1.3.4) is asked,
    /// and a plugin from the marketplace is in it like an import.
    // Its stand-in `agy` is a POSIX shell script.
    #[cfg(unix)]
    #[test]
    fn the_cli_listing_names_imports_and_marketplace_plugins_alike() {
        let home = temp_root("agy-cli-listing");
        let agy = agy_listing(
            &home,
            r#"[{"name":"flow","enabled":true,"source":"antigravity","path":"/h/.gemini/config/plugins/flow","skills":[]},
               {"name":"gdocs","enabled":true,"version":"1.0.0","source":"antigravity-plugins-official","path":"/h/x","skills":[]}]"#,
        );

        let listed = installed_plugins(&agy, &home).unwrap();

        assert_eq!(
            listed.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            ["flow", "gdocs"]
        );
        assert_eq!(holders_of(&agy, &home, "gdocs"), Ok(1));
        assert_eq!(holders_of(&agy, &home, "free"), Ok(0));
        let _ = fs::remove_dir_all(home);
    }

    /// A listing in a shape this build does not know, such as the
    /// `{"imports": [...]}` object of the 1.1 CLI or its sentence for an
    /// empty one, is an answer UZE cannot read: never a free name.
    // Its stand-in `agy` is a POSIX shell script.
    #[cfg(unix)]
    #[test]
    fn a_listing_this_build_cannot_read_is_an_error() {
        for listing in [
            r#"{"imports":[]}"#,
            "No imported plugins.",
            r#"[{"enabled":true}]"#,
        ] {
            let home = temp_root("agy-cli-unknown");
            let agy = agy_listing(&home, listing);
            assert!(holders_of(&agy, &home, "flow").is_err(), "{listing}");
            assert!(installed_plugins(&agy, &home).is_err(), "{listing}");
            let _ = fs::remove_dir_all(home);
        }
    }

    /// The manifest, which inspection reads first, knows imports only; the
    /// CLI, which install and detach ask, sees the marketplace twin.
    // Its stand-in `agy` is a POSIX shell script.
    #[cfg(unix)]
    #[test]
    fn a_marketplace_twin_is_seen_by_the_census_install_and_detach_ask() {
        let home = temp_root("agy-twin");
        fs::create_dir_all(home.join(".gemini/config")).unwrap();
        fs::write(
            home.join(".gemini/config/import_manifest.json"),
            r#"{"imports":[{"name":"flow","source":"antigravity"}]}"#,
        )
        .unwrap();
        let agy = agy_listing(
            &home,
            r#"[{"name":"flow","source":"antigravity","path":"/a"},{"name":"flow","source":"antigravity-plugins-official","path":"/b"}]"#,
        );

        assert_eq!(installed_plugins(&agy, &home).unwrap().len(), 1);
        assert_eq!(holders_of(&agy, &home, "flow"), Ok(2));
        let _ = fs::remove_dir_all(home);
    }

    /// Uninstalling by name runs only when the CLI names exactly one holder.
    #[test]
    fn uninstall_runs_only_for_a_name_one_plugin_holds() {
        assert_eq!(uninstall_refused(Ok(1)), None);
        for (holders, state) in [
            (Ok(0), AttachmentState::Blocked),
            (Ok(2), AttachmentState::Conflict),
            (Err("unreadable".to_owned()), AttachmentState::Blocked),
        ] {
            assert_eq!(
                uninstall_refused(holders).map(|refused| refused.state),
                Some(state)
            );
        }
    }

    /// A name a marketplace plugin already holds is refused aloud: nothing
    /// is installed over it, and the delivery says why rather than passing
    /// for one that happened.
    // Its stand-in `agy` is a POSIX shell script.
    #[cfg(unix)]
    #[test]
    fn delivering_over_a_held_name_is_refused_and_says_so() {
        let (root, pkg) = make_package("agy-held", r#"{"name":"flow","description":"d"}"#);
        let integration =
            AntigravityIntegration::new(root.join("agents"), UzeHome::at(root.join("uze")));
        let agy = agy_listing(
            &root,
            r#"[{"name":"flow","source":"antigravity-plugins-official","path":"/b"}]"#,
        );
        let refused = attach_generated_plugin(&agy, &integration, &pkg).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("already holds a plugin named `flow`"),
            "{refused}"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// With UZE's plugin gone and another one in its folder, inspection says
    /// the name is taken rather than that nothing is there.
    #[test]
    fn a_name_another_plugin_took_is_named_on_inspection() {
        let root = temp_root("agy-taken");
        let staged = root.join("plugins/flow");
        fs::create_dir_all(&staged).unwrap();
        let inspection = inspect_installed_plugin(&[], "flow", &staged, "fp");
        assert_eq!(inspection.state, AttachmentState::Missing);
        assert!(
            inspection.reason.contains("another plugin"),
            "{}",
            inspection.reason
        );
        let _ = fs::remove_dir_all(root);
    }

    // --- Exact coverage -----------------------------------------------------

    fn listing_with(name: &str) -> Vec<Installed> {
        vec![Installed {
            name: name.to_owned(),
        }]
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
        let listing: Vec<Installed> = Vec::new();
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
        let listing = [listing_with("flow"), listing_with("flow")].concat();
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
