//! Antigravity's GENERATED native plugin envelope: every package whose
//! plugin.json is a valid Antigravity manifest is installed through a plugin
//! UZE synthesizes into a UZE-owned derived directory, never the Store tree
//! itself. `agy plugin install` stages a byte copy of what it is given, so a
//! Store tree would reach it with `${PLUGIN_ROOT}` unresolved in every
//! skill, and with the package's `agents/`, which agy loads from a plugin
//! under their bare names beside the labelled ones UZE delivers (measured on
//! 1.2.12). The plugin carries the manifest, the skills mirrored with the
//! package root resolved, and the MCP servers in the vendor's
//! `mcp_config.json` — translated from canonical `mcp.json`, or the author's
//! own `mcp_config.json` when the package ships one.
//!
//! Generated Native Plugin sits between Explicit Native Plugin and Native
//! Capability (ADR-013 §3), mirroring the other
//! integrations' generated-envelope discipline.
//! Simpler than either in one way (no catalogue — `plugin install` points
//! straight at the directory) and costlier in another (the vendor stages a
//! byte copy; see [`super::plugin`]'s module doc for why that stays a
//! Derived Artifact).

use std::{collections::BTreeSet, fs, path::PathBuf};

use uze_core::{Result, UzeError, capability::Resource, home::UzeHome, store::StoredPackage};

use crate::shared::marketplace::remove_generated_dir;
use crate::shared::mcp::delivered_mcp_servers;
use crate::shared::skill::write_file;
use crate::shared::tree::mirror_tree;

/// Root of every package's generated plugin directory. Lives under
/// `$UZE_HOME/runtime/attachments/antigravity/plugins/` — the same convention
/// every other integration's generated envelopes use, never under the Store.
pub(super) fn generated_root(uze_home: &UzeHome) -> PathBuf {
    crate::shared::path::attachment_root(uze_home, "antigravity").join("plugins")
}

fn generated_package_dir_for_id(uze_home: &UzeHome, package_id: &str) -> PathBuf {
    generated_root(uze_home).join(sanitize_for_agy_path(package_id))
}

/// `agy plugin install <path>` parses a final path segment shaped like
/// `name@marketplace` as a marketplace-qualified selector rather than a
/// literal filesystem path, and fails with "unknown marketplace: ..." for
/// any marketplace name not registered with AGY itself (verified against
/// real agy 1.1.22). `PackageId`s are always marketplace-qualified this way
/// (ADR-036), so the `@` is replaced before it ever reaches the vendor CLI;
/// `remove_generated_plugin_by_id` uses the same helper so create/remove
/// always agree on the directory name.
fn sanitize_for_agy_path(package_id: &str) -> String {
    package_id.replace('@', "--")
}

/// The canonical `mcp.json`'s declared server names, `Some` only when the
/// file exists and carries a non-empty `mcpServers` object. `None` for an
/// absent, malformed, or server-less file — in which case nothing needs
/// translating and the explicit route handles the package.
pub(super) fn canonical_mcp_servers(package: &StoredPackage) -> Option<BTreeSet<String>> {
    let entries: BTreeSet<String> = fs::read(package.root.join("mcp.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("mcpServers")
                .and_then(serde_json::Value::as_object)
                .map(|servers| servers.keys().cloned().collect())
        })
        .unwrap_or_default();
    (!entries.is_empty()).then_some(entries)
}

/// The intersection ADR-013 §2 requires, computed against the SEMANTIC
/// surface a generated plugin preserves: canonical `skills/` are carried
/// verbatim, and the MCP servers declared in canonical `mcp.json` are
/// translated into the generated `mcp_config.json`. The `skills/` tree is
/// carried unchanged, so only a package whose Skills all carry the default
/// policy reaches here (`package_exposure_plan` decomposes any other).
/// Coverage and generation agree by construction.
pub(super) fn generated_exact_coverage(
    package: &StoredPackage,
    resources: &[&Resource],
) -> BTreeSet<String> {
    let declared_mcp = delivered_mcp_names(package);
    let mut provided = BTreeSet::new();
    for resource in resources {
        match resource.capability.kind {
            uze_core::capability::CapabilityKind::AgentSkill => {
                if super::plugin::under_skills_dir(package, &resource.capability.path) {
                    provided.insert(resource.identity());
                }
            }
            uze_core::capability::CapabilityKind::Mcp => {
                if let Some(name) = &resource.resource_name
                    && declared_mcp.contains(name)
                {
                    provided.insert(resource.identity());
                }
            }
            // Hooks are deliberately absent: the plugin carries no
            // `hooks.json` any more, because the vendor never reads one from
            // a plugin directory. They are delivered capability-level, as
            // named entries merged into the shared `~/.gemini/config/hooks.json`.
            _ => {}
        }
    }
    provided
}

/// The servers the generated plugin's `mcp_config.json` declares: the
/// author's own file when the package ships one, else canonical `mcp.json`.
fn delivered_mcp_names(package: &StoredPackage) -> BTreeSet<String> {
    let author = super::plugin::author_mcp_config_servers(package);
    if author.is_empty() {
        canonical_mcp_servers(package).unwrap_or_default()
    } else {
        author
    }
}

/// The generated `plugin.json` document. Name is the canonical manifest's
/// own (the plan guaranteed it satisfies the vendor pattern); description
/// comes from the package's own canonical manifest, never invented.
fn generated_plugin_document(package: &StoredPackage) -> serde_json::Value {
    let (name, description) = super::plugin::plugin_manifest_name(package)
        .map(|name| {
            let description = fs::read(&package.manifest)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|value| {
                    value
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| {
                    "UZE-managed Antigravity plugin, generated from a vendor-neutral package."
                        .to_owned()
                });
            (name, description)
        })
        .expect("package_exposure_plan gates generation on a valid plugin name");
    serde_json::json!({
        "name": name,
        "description": description,
    })
}

/// Translates canonical `mcp.json` servers into the vendor's
/// `mcp_config.json` form: every server entry passes through as-is, with
/// the legacy remote keys `url`/`httpUrl` rewritten to the modern
/// `serverUrl` — the exact mapping Antigravity's own official
/// legacy-migration path performs (official docs: "Legacy schema keys:
/// `url` or `httpUrl`; Modern schema key: `serverUrl`"). Nothing is
/// dropped; stdio `command`, `args`, `env`, `cwd` carry only the package
/// root resolved.
fn translated_mcp_config(package: &StoredPackage) -> serde_json::Value {
    let mut document = serde_json::json!({ "mcpServers": {} });
    if let Some(serde_json::Value::Object(servers)) = delivered_mcp_servers(package) {
        for (name, mut server) in servers {
            if let Some(value) = server.as_object_mut()
                && let Some(url) = value.remove("url").or_else(|| value.remove("httpUrl"))
            {
                value.insert("serverUrl".to_owned(), url);
            }
            document["mcpServers"][name] = server;
        }
    }
    document
}

/// Materializes (or refreshes) one package's generated plugin directory.
/// Idempotent and deterministic: recreated wholesale from the Store package
/// on every call — the directory is entirely UZE-owned and
/// non-authoritative (ADR-013 §5). `skills/` is mirrored as real files with
/// the package root resolved in every `SKILL.md`; the MCP servers go into
/// the vendor `mcp_config.json`. `commands/` is no longer a
/// canonical surface (ADR-030): a vendor-authored `commands/` directory is
/// only ever delivered through an explicit plugin the author shipped.
pub(super) fn materialize_generated_plugin(
    uze_home: &UzeHome,
    package: &StoredPackage,
) -> Result<PathBuf> {
    let dir = generated_package_dir_for_id(uze_home, package.id.as_str());
    uze_core::persistence::replace_dir(&dir, |staging| {
        let manifest = generated_plugin_document(package);
        fs::write(
            staging.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).expect("generated manifest is serializable"),
        )
        .map_err(|source| UzeError::Write {
            path: staging.join("plugin.json"),
            source,
        })?;

        let skills_source = package.root.join("skills");
        if skills_source.is_dir() {
            let package_root =
                fs::canonicalize(&package.root).map_err(|source| UzeError::Read {
                    path: package.root.clone(),
                    source,
                })?;
            mirror_tree(&skills_source, &staging.join("skills"), &package_root, &[])?;
            for resource in uze_core::engine::package_resources_at(&package.id, &package.root)? {
                if resource.capability.kind != uze_core::capability::CapabilityKind::AgentSkill {
                    continue;
                }
                if let std::borrow::Cow::Owned(resolved) = crate::shared::dialect::delivered_skill(
                    &resource.capability.payload,
                    &resource.package_root,
                    super::skills::ANTIGRAVITY_KEYS,
                ) && let Ok(relative) = resource.capability.path.strip_prefix(&package.root)
                {
                    write_file(&staging.join(relative), &resolved)?;
                }
            }
        }
        let author_mcp = package.root.join("mcp_config.json");
        if author_mcp.is_file() {
            fs::copy(&author_mcp, staging.join("mcp_config.json")).map_err(|source| {
                UzeError::Write {
                    path: staging.join("mcp_config.json"),
                    source,
                }
            })?;
        } else if canonical_mcp_servers(package).is_some() {
            let mcp = translated_mcp_config(package);
            fs::write(
                staging.join("mcp_config.json"),
                serde_json::to_vec_pretty(&mcp).expect("generated MCP config is serializable"),
            )
            .map_err(|source| UzeError::Write {
                path: staging.join("mcp_config.json"),
                source,
            })?;
        }
        // No `hooks.json` is written here. AGY 1.1.24 reads hooks from its
        // shared customization roots and never opens a plugin's `hooks.json`,
        // whatever its own plugin guide says (measured in the Conformance Lab:
        // `agy plugin validate` counts the file's hooks while the session
        // reports `loaded 0 named hooks from 0 hooks.json file(s)`). A file the
        // vendor never reads is not a delivery, so hooks go to
        // `~/.gemini/config/hooks.json` as receipt-owned named entries instead
        // (see `AntigravityIntegration::hook_exposure_plan`).
        Ok(())
    })?;
    Ok(dir)
}

/// Removes one package's generated plugin directory by id alone — used at
/// detach time, when only the receipt's `package_id` (not a full
/// `StoredPackage`) is available. Safe unconditionally: this directory is
/// never anything but a Derived Artifact (ADR-013 §5).
pub(super) fn remove_generated_plugin_by_id(uze_home: &UzeHome, package_id: &str) -> Result<()> {
    remove_generated_dir(package_id, |id| generated_package_dir_for_id(uze_home, id))
}

#[cfg(test)]
mod generated_native_tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;

    use uze_core::capability::Resource;
    use uze_core::capability::{Capability, CapabilityKind};
    use uze_core::home::UzeHome;
    use uze_core::integration::IntegrationPort;

    use super::super::AntigravityIntegration;
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        uze_testkit::temp::scratch(label)
    }

    fn make_package_with_mcp(label: &str) -> (PathBuf, StoredPackage) {
        let root = temp_root(label);
        let pkg_root = root.join("pkg");
        fs::create_dir_all(pkg_root.join("skills/commit")).unwrap();
        fs::write(
            pkg_root.join("skills/commit/SKILL.md"),
            "---\nname: commit\n---\n",
        )
        .unwrap();
        fs::write(
            pkg_root.join("plugin.json"),
            r#"{"name":"flow","version":"1.2.0","description":"Vendor-neutral flow package"}"#,
        )
        .unwrap();
        fs::write(
            pkg_root.join("mcp.json"),
            r#"{"mcpServers":{"mcp-a":{"command":"a"},"remote-b":{"url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();
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

    fn make_package_with_hooks(label: &str) -> (PathBuf, StoredPackage) {
        let (root, pkg) = make_package_with_mcp(label);
        // Hooks replace the MCP surface for this fixture's purpose.
        fs::remove_file(pkg.root.join("mcp.json")).unwrap();
        fs::write(
            pkg.root.join("hooks.json"),
            r#"{"hooks":{"PreToolUse":[{"id":"protect-env","matcher":"shell","effect":"deny","hooks":[{"type":"command","command":"${PLUGIN_ROOT}/check"}]}],"Stop":[{"hooks":[{"type":"command","command":"archive"}]}]}}"#,
        )
        .unwrap();
        (root, pkg)
    }

    fn skill_resource(pkg: &StoredPackage) -> Resource {
        let path = pkg.root.join("skills/commit/SKILL.md");
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
        let path = pkg.root.join("mcp.json");
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

    #[test]
    fn canonical_mcp_is_detected_only_for_a_real_declaration() {
        let (root, pkg) = make_package_with_mcp("canonical-mcp");
        assert_eq!(
            canonical_mcp_servers(&pkg),
            Some(BTreeSet::from(["mcp-a".to_owned(), "remote-b".to_owned()]))
        );
        fs::remove_file(pkg.root.join("mcp.json")).unwrap();
        assert!(canonical_mcp_servers(&pkg).is_none());
        fs::write(pkg.root.join("mcp.json"), "{not json").unwrap();
        assert!(canonical_mcp_servers(&pkg).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_with_canonical_mcp_takes_the_generated_route() {
        let (root, pkg) = make_package_with_mcp("plan-generated");
        let r_skill = skill_resource(&pkg);
        let r_mcp = mcp_resource(&pkg, "mcp-a");
        let resources = vec![&r_skill, &r_mcp];
        let uze_home = UzeHome::at(root.join("uze"));
        let integration = AntigravityIntegration::new(root.join("agents"), uze_home.clone());
        let plan = integration
            .package_exposure_plan(&pkg, &resources)
            .expect("generated route applies");
        assert_eq!(plan.route, uze_core::router::CompatibilityRoute::Native);
        assert_eq!(
            plan.provided_resource_identities,
            BTreeSet::from([r_skill.identity(), r_mcp.identity()])
        );
        assert!(
            !generated_root(&uze_home).join(pkg.id.as_str()).exists(),
            "planning must stay read-only"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// ADR-030 §13: a non-default invoke policy must not enter an unchanged
    /// Antigravity plugin tree. The package is decomposed so every resource
    /// follows its own policy-aware capability route.
    #[test]
    fn non_default_policy_skill_disables_the_generated_package_route() {
        let (root, pkg) = make_package_with_mcp("plan-policy");
        let user_only = Resource::from_package(
            pkg.id.clone(),
            pkg.root.clone(),
            Capability {
                kind: CapabilityKind::AgentSkill,
                path: pkg.root.join("skills/commit/SKILL.md"),
                payload: b"---\nname: commit\ninvoke:\n  model: false\n  user: true\n---\n"
                    .to_vec(),
            },
        );
        let resources = vec![&user_only];
        let uze_home = UzeHome::at(root.join("uze"));
        let integration = AntigravityIntegration::new(root.join("agents"), uze_home);
        assert!(
            integration
                .package_exposure_plan(&pkg, &resources)
                .is_none(),
            "the unchanged generated plugin must not carry the user-only Skill"
        );
        let fallback = integration.exposure_plan(&user_only);
        assert_eq!(
            fallback.route,
            uze_core::router::CompatibilityRoute::Adaptable,
            "the capability-level fallback reports the degradation honestly"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialize_translates_and_never_writes_into_the_store() {
        let (root, pkg) = make_package_with_mcp("materialize");
        let uze_home = UzeHome::at(root.join("uze"));
        let before: BTreeSet<PathBuf> = walk(&pkg.root);
        let dir = materialize_generated_plugin(&uze_home, &pkg).unwrap();
        let after: BTreeSet<PathBuf> = walk(&pkg.root);
        assert_eq!(
            before, after,
            "Store package tree must be byte-for-byte unchanged"
        );
        assert!(
            dir.starts_with(uze_home.runtime_dir()),
            "a generated package is produced again from the Store and the \
             Engine alone, so it belongs with what UZE generates rather than \
             with the records"
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("plugin.json")).unwrap()).unwrap();
        assert_eq!(manifest["name"], "flow");
        assert!(!dir.join("skills").is_symlink() && dir.join("skills").is_dir());
        assert!(
            !dir.join("commands").exists(),
            "commands/ is no longer a canonical surface; the generated plugin never carries it"
        );
        let mcp: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("mcp_config.json")).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["mcp-a"]["command"], "a");
        assert_eq!(
            mcp["mcpServers"]["remote-b"]["serverUrl"],
            "https://example.com/mcp"
        );
        assert!(
            mcp["mcpServers"]["remote-b"].get("url").is_none(),
            "legacy url key must be rewritten"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// agy stages a byte copy of what it installs and loads a plugin's
    /// `agents/` under their bare names, so the plugin carries resolved
    /// skills and no agents — those arrive as labelled files on their own.
    #[test]
    fn the_plugin_resolves_the_package_root_and_leaves_agents_out() {
        let (root, pkg) = make_package_with_mcp("resolved-no-agents");
        fs::write(
            pkg.root.join("skills/commit/SKILL.md"),
            "---\nname: commit\ndescription: d\n---\nRun ${PLUGIN_ROOT}/scripts/x.sh\n",
        )
        .unwrap();
        fs::create_dir_all(pkg.root.join("agents")).unwrap();
        fs::write(
            pkg.root.join("agents/reviewer.md"),
            "---\ndescription: d\n---\nb\n",
        )
        .unwrap();
        let uze_home = UzeHome::at(root.join("uze"));
        let dir = materialize_generated_plugin(&uze_home, &pkg).unwrap();
        let skill = fs::read_to_string(dir.join("skills/commit/SKILL.md")).unwrap();
        assert!(
            skill.contains(&format!("Run {}/scripts/x.sh", pkg.root.display())),
            "{skill}"
        );
        assert!(!dir.join("agents").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generated_dir_never_carries_an_at_sign() {
        // `agy plugin install <path>` parses a final path segment shaped
        // like `name@marketplace` as a marketplace selector, not a literal
        // path, and fails with "unknown marketplace: ..." — regression
        // coverage for that real-CLI quirk (verified against agy 1.1.22).
        let (root, pkg) = make_package_with_mcp("no-at-sign");
        assert!(
            pkg.id.as_str().contains('@'),
            "fixture must be marketplace-qualified"
        );
        let uze_home = UzeHome::at(root.join("uze"));
        let dir = materialize_generated_plugin(&uze_home, &pkg).unwrap();
        assert!(
            !dir.file_name().unwrap().to_str().unwrap().contains('@'),
            "generated dir name must not contain '@': {dir:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialize_is_deterministic_across_rebuilds() {
        let (root, pkg) = make_package_with_mcp("deterministic");
        let uze_home = UzeHome::at(root.join("uze"));
        materialize_generated_plugin(&uze_home, &pkg).unwrap();
        let first = fs::read(
            generated_package_dir_for_id(&uze_home, pkg.id.as_str()).join("mcp_config.json"),
        )
        .unwrap();
        materialize_generated_plugin(&uze_home, &pkg).unwrap();
        let second = fs::read(
            generated_package_dir_for_id(&uze_home, pkg.id.as_str()).join("mcp_config.json"),
        )
        .unwrap();
        assert_eq!(first, second);
        let _ = fs::remove_dir_all(root);
    }

    /// The vendor's plugin guide says a plugin's `hooks.json` is
    /// "registered and run during the agent's lifecycle". On 1.1.24 it is
    /// not: `agy plugin validate` counts the file's hooks while the session
    /// reports `loaded 0 named hooks from 0 hooks.json file(s)` and never
    /// opens it (Conformance Lab, `hooks > delivery`). So the generated
    /// plugin writes none, and hooks are delivered capability-level into the
    /// shared `~/.gemini/config/hooks.json` instead.
    #[test]
    fn the_generated_plugin_carries_no_hooks_and_claims_none() {
        let (root, pkg) = make_package_with_hooks("plugin-hooks");
        let uze_home = UzeHome::at(root.join("uze"));
        let dir = materialize_generated_plugin(&uze_home, &pkg).unwrap();
        assert!(
            !dir.join("hooks.json").exists(),
            "a file the harness never reads is not a delivery"
        );
        assert!(
            !dir.join("hooks").exists(),
            "no wrapper is vendored where nothing would run it"
        );
        assert!(
            pkg.root.join("hooks.json").is_file(),
            "Store bytes stay untouched"
        );

        let resources = uze_core::engine::package_resources_at(&pkg.id, &pkg.root).unwrap();
        let references: Vec<&Resource> = resources.iter().collect();
        let hook_identities: BTreeSet<String> = references
            .iter()
            .filter(|resource| {
                resource.capability.kind == uze_core::capability::CapabilityKind::Hook
            })
            .map(|resource| resource.identity())
            .collect();
        assert!(!hook_identities.is_empty(), "the fixture declares hooks");
        let covered = generated_exact_coverage(&pkg, &references);
        assert!(
            hook_identities.is_disjoint(&covered),
            "package-level coverage must not claim a hook the plugin does not carry"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remove_generated_plugin_deletes_only_the_derived_directory() {
        let (root, pkg) = make_package_with_mcp("removal");
        let uze_home = UzeHome::at(root.join("uze"));
        materialize_generated_plugin(&uze_home, &pkg).unwrap();
        assert!(generated_package_dir_for_id(&uze_home, pkg.id.as_str()).exists());
        remove_generated_plugin_by_id(&uze_home, pkg.id.as_str()).unwrap();
        assert!(!generated_package_dir_for_id(&uze_home, pkg.id.as_str()).exists());
        assert!(
            pkg.root.join("skills/commit/SKILL.md").is_file(),
            "Store bytes untouched"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn walk(root: &std::path::Path) -> BTreeSet<PathBuf> {
        let mut out = BTreeSet::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                out.insert(path);
            }
        }
        out
    }

    #[test]
    fn the_generated_plugin_resolves_the_package_root_its_servers_name() {
        let (root, pkg) = make_package_with_mcp("mcp-package-root");
        fs::write(
            pkg.root.join("mcp.json"),
            r#"{"mcpServers":{"srv":{"command":"python3","args":["${PLUGIN_ROOT}/scripts/server.py"]}}}"#,
        )
        .unwrap();
        let uze_home = UzeHome::at(root.join("uze"));
        let dir = materialize_generated_plugin(&uze_home, &pkg).unwrap();
        let delivered: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("mcp_config.json")).unwrap()).unwrap();
        assert_eq!(
            delivered["mcpServers"]["srv"]["args"][0],
            pkg.root
                .join("scripts/server.py")
                .to_string_lossy()
                .as_ref()
        );
        let _ = fs::remove_dir_all(root);
    }
}
