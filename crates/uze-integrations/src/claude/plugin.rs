//! Claude Code's native plugin marketplaces: the dialect UZE's derived
//! `.claude-plugin/marketplace.json` catalogues are written in, the
//! exact-coverage computation that tells UZE which resources a package's
//! own envelope already accounts for, and the `claude plugin` verbs.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::Path,
    path::PathBuf,
};

use uze_core::{
    Result,
    capability::{CapabilityKind, Resource},
    integration::{AttachmentInspection, AttachmentState, UnreadableDelivery},
    skill::SkillInvocationPolicy,
    store::StoredPackage,
};

use crate::shared::marketplace::{
    MarketplaceDialect, Origin, manifest_fields, marketplace_entries,
};
use crate::shared::path::normalize_declared_relative_path;
use crate::shared::plan::blocked;
use crate::shared::process::{json, run_quiet};

/// The owner every catalogue UZE writes into Claude's marketplace UI
/// declares. Named once so the two documents that carry it cannot drift
/// into attributing UZE's local marketplace to someone else.
const MARKETPLACE_OWNER_URL: &str = "https://github.com/uze-sh/uze";

pub(super) struct ClaudeMarketplace;

impl MarketplaceDialect for ClaudeMarketplace {
    const VENDOR: &'static str = "claude";
    const SETUP_NAME: &'static str = "claude";
    const CATALOGUE_NOUN: &'static str = "Claude marketplace";
    const ENVELOPE_DIR: &'static str = ".claude-plugin";
    const CATALOGUE_PATH: &'static str = ".claude-plugin/marketplace.json";
    const EXPLICIT_KIND: &'static str = "claude-plugin";
    const GENERATED_KIND: &'static str = "claude-plugin-generated";
    const EXPLICIT_EVIDENCE: &'static str = "The preserved external .claude-plugin/plugin.json is exposed through UZE's derived Claude marketplace. Claude Code owns Skill and MCP loading for this plugin, so UZE must not attach them a second time.";
    const GENERATED_EVIDENCE: &'static str = "No .claude-plugin/plugin.json was provided. UZE synthesizes one deterministically into a UZE-owned derived directory (never the Store): a copy of the package carrying its skills, agents and mcp.json-declared servers, published through a second, generated-only Claude marketplace, so Claude names every skill and agent under the plugin.";
    const ENVELOPE_CARRIES_AGENTS: bool = true;

    fn catalogue_document(
        name: &str,
        display_name: &str,
        plugins: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "owner": { "name": display_name, "url": MARKETPLACE_OWNER_URL },
            "plugins": plugins
        })
    }

    fn catalogue_entry(
        package: &StoredPackage,
        source: String,
        origin: Origin,
    ) -> serde_json::Value {
        let (description, version) = match origin {
            Origin::Explicit => manifest_fields(
                &package.root.join(".claude-plugin/plugin.json"),
                "UZE-managed Claude plugin",
            ),
            Origin::Generated => {
                manifest_fields(&package.manifest, super::generate::GENERATED_DESCRIPTION)
            }
        };
        serde_json::json!({
            "name": package.active_name.as_str(),
            "source": source,
            "description": description,
            "version": version
        })
    }

    fn explicit_coverage(package: &StoredPackage, resources: &[&Resource]) -> BTreeSet<String> {
        claude_exact_coverage(package, resources)
    }

    /// Claude's own frontmatter markers carry every valid combination, so
    /// only the invalid policy is left out.
    fn envelope_preserves(policy: SkillInvocationPolicy) -> bool {
        !policy.is_invalid()
    }

    fn materialize_envelope(package: &StoredPackage, dir: &Path) -> Result<()> {
        super::generate::materialize_envelope(package, dir)
    }

    fn generated_receipt_root(package: &StoredPackage, _envelope_dir: &Path) -> PathBuf {
        package.root.clone()
    }

    fn marketplace_exists(executable: &Path, home: &Path, root: &Path) -> bool {
        if let Some(known) = recorded_marketplaces(home) {
            return known.iter().any(|entry| {
                [
                    entry.get("installLocation"),
                    entry.get("source").and_then(|source| source.get("path")),
                ]
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .any(|candidate| Path::new(candidate) == root)
            });
        }
        let Ok(listing) = json(
            executable,
            home,
            &["plugin", "marketplace", "list", "--json"],
            "claude",
        ) else {
            return false;
        };
        marketplace_entries(&listing).is_some_and(|entries| {
            entries.iter().any(|entry| {
                ["path", "installLocation"].iter().any(|key| {
                    entry
                        .get(*key)
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|candidate| Path::new(candidate) == root)
                })
            })
        })
    }

    fn add_marketplace(executable: &Path, home: &Path, root: &Path) -> Result<()> {
        let label = format!("claude plugin marketplace add {}", root.display());
        let args: Vec<&OsStr> = vec![
            OsStr::new("plugin"),
            OsStr::new("marketplace"),
            OsStr::new("add"),
            root.as_os_str(),
        ];
        run_quiet(executable, home, &label, &args)
    }

    /// Installed only when absent: Claude's behavior for re-installing an
    /// installed plugin is not relied on.
    fn plugin_installed(executable: &Path, home: &Path, selector: &str) -> bool {
        if let Some(installed) = recorded_user_install(home, selector) {
            return installed;
        }
        json(executable, home, &["plugin", "list", "--json"], "claude")
            .is_ok_and(|listing| installed_entry(&listing, selector).is_some())
    }

    fn install_plugin(executable: &Path, home: &Path, selector: &str) -> Result<()> {
        run_quiet(
            executable,
            home,
            &format!("claude plugin install `{selector}`"),
            &["plugin", "install", selector],
        )
    }

    fn inspect_plugin(
        executable: &Path,
        home: &Path,
        selector: &str,
        marketplace_root: &Path,
        _detail: &BTreeMap<String, serde_json::Value>,
    ) -> AttachmentInspection {
        inspect_claude_plugin(executable, home, selector, marketplace_root)
    }

    fn remove_plugin(executable: &Path, home: &Path, selector: &str) -> Result<()> {
        run_quiet(
            executable,
            home,
            &format!("claude plugin uninstall {selector}"),
            &["plugin", "uninstall", selector],
        )
    }

    fn remove_marketplace(executable: &Path, home: &Path, name: &str) -> Result<()> {
        run_quiet(
            executable,
            home,
            &format!("claude plugin marketplace remove {name}"),
            &["plugin", "marketplace", "remove", name],
        )
    }
}

/// What Claude would not load of a plugin UZE delivered and its receipt
/// still matches: the plugin the marketplace points at gone from where UZE
/// wrote it, or Claude's cached copy (the `installPath` its record names,
/// which is what a session actually reads) missing a skill or an agent the
/// package carries. A hollow cache is what an install that raced Claude's
/// copy, or a copy made before the files existed, leaves behind.
pub(super) fn plugin_unreadable(
    command_home: &Path,
    uze_home: &uze_core::home::UzeHome,
    package: &StoredPackage,
    kind: &str,
    selector: &str,
    served: &[&Resource],
) -> Vec<UnreadableDelivery> {
    let every = |reason: String| {
        served
            .iter()
            .map(|resource| UnreadableDelivery {
                capability: resource.identity(),
                reason: reason.clone(),
            })
            .collect::<Vec<_>>()
    };
    let source = match crate::shared::marketplace::receipt_origin::<ClaudeMarketplace>(kind) {
        Some(Origin::Explicit) => package.root.clone(),
        Some(Origin::Generated) => crate::shared::marketplace::generated_package_dir::<
            ClaudeMarketplace,
        >(uze_home, package.id.as_str()),
        None => return Vec::new(),
    };
    if !source.join(".claude-plugin/plugin.json").is_file() {
        return every(format!(
            "the plugin Claude's marketplace points at is gone from {}",
            source.display()
        ));
    }
    let Some(install_path) = recorded_install_path(command_home, selector) else {
        return Vec::new();
    };
    if !install_path.is_dir() {
        return every(format!(
            "Claude's cached copy of the plugin at {} does not exist",
            install_path.display()
        ));
    }
    let mut files = Vec::new();
    collect_files(&install_path, 0, &mut files);
    served
        .iter()
        .filter_map(|resource| {
            let found = match resource.capability.kind {
                CapabilityKind::AgentSkill => {
                    let skill = resource.capability.path.parent()?.file_name()?;
                    files.iter().any(|file| {
                        file.file_name() == Some(OsStr::new("SKILL.md"))
                            && file.parent().and_then(Path::file_name) == Some(skill)
                    })
                }
                CapabilityKind::Agent => {
                    let agent = resource.capability.path.file_name()?;
                    files.iter().any(|file| {
                        file.file_name() == Some(agent)
                            && file.components().any(|part| part.as_os_str() == "agents")
                    })
                }
                _ => return None,
            };
            (!found).then(|| UnreadableDelivery {
                capability: resource.identity(),
                reason: format!(
                    "not in Claude's cached copy of the plugin at {}",
                    install_path.display()
                ),
            })
        })
        .collect()
}

/// Where Claude's record says its user-scope copy of `selector` lives.
fn recorded_install_path(home: &Path, selector: &str) -> Option<PathBuf> {
    let installed = recorded(home, "installed_plugins.json")?;
    installed
        .get("plugins")?
        .get(selector)?
        .as_array()?
        .iter()
        .find(|install| install.get("scope").and_then(serde_json::Value::as_str) == Some("user"))?
        .get("installPath")?
        .as_str()
        .map(PathBuf::from)
}

/// Every file under `dir`, to a depth no plugin layout reaches.
fn collect_files(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    const DEEPEST: usize = 8;
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < DEEPEST {
                collect_files(&path, depth + 1, files);
            }
        } else if path.is_file() {
            files.push(path);
        }
    }
}

/// One of Claude's own plugin records, read where it keeps them.
///
/// A `claude` process start costs a quarter of a second — more with every
/// other harness starting beside it — to answer what these files already
/// say. So they are read first, and trusted only in the shape this knows:
/// a file that is missing, unreadable or shaped otherwise answers nothing,
/// and the CLI is asked as before.
fn recorded(home: &Path, file: &str) -> Option<serde_json::Value> {
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    serde_json::from_slice(&fs::read(config.join("plugins").join(file)).ok()?).ok()
}

/// The inspection [`inspect_claude_plugin`] would give, answered from
/// Claude's own records — `None` whenever one of them is missing or shaped
/// otherwise, or the plugin has no explicit enablement, so the CLI is asked.
/// Removing a plugin inspects every receipt before, during and after; three
/// times two `claude` starts was three seconds of a five-second removal.
fn inspect_from_records(
    home: &Path,
    selector: &str,
    marketplace_root: &Path,
) -> Option<AttachmentInspection> {
    let (plugin, marketplace_name) = selector.rsplit_once('@')?;
    let known = recorded(home, "known_marketplaces.json")?;
    let known = known.as_object()?;
    let installed = recorded_user_install(home, selector)?;
    let found = |state: AttachmentState, reason: &str| {
        Some(AttachmentInspection {
            state,
            reason: reason.to_owned(),
        })
    };
    let Some(marketplace) = known.get(marketplace_name) else {
        return found(AttachmentState::Missing, "Claude marketplace is absent");
    };
    let root = marketplace
        .get("installLocation")
        .or_else(|| {
            marketplace
                .get("source")
                .and_then(|source| source.get("path"))
        })
        .and_then(serde_json::Value::as_str)?;
    if Path::new(root) != marketplace_root {
        return found(
            AttachmentState::Drifted,
            "Claude marketplace root differs from receipt",
        );
    }
    if !installed {
        return found(AttachmentState::Missing, "Claude plugin is not installed");
    }
    let settings = recorded_settings(home)?;
    let enabled = settings
        .get("enabledPlugins")?
        .get(format!("{plugin}@{marketplace_name}"))?
        .as_bool()?;
    if enabled {
        found(
            AttachmentState::Matched,
            "Claude native plugin matches receipt",
        )
    } else {
        found(AttachmentState::Drifted, "Claude plugin is disabled")
    }
}

/// Claude's user settings, where it records which plugins are enabled.
fn recorded_settings(home: &Path) -> Option<serde_json::Value> {
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    serde_json::from_slice(&fs::read(config.join("settings.json")).ok()?).ok()
}

/// Every marketplace Claude has registered, when its record reads as one
/// entry per marketplace, each with a source.
fn recorded_marketplaces(home: &Path) -> Option<Vec<serde_json::Value>> {
    let known = recorded(home, "known_marketplaces.json")?;
    let entries: Vec<serde_json::Value> = known.as_object()?.values().cloned().collect();
    entries
        .iter()
        .all(|entry| {
            entry
                .get("source")
                .is_some_and(serde_json::Value::is_object)
        })
        .then_some(entries)
}

/// Whether Claude records `selector` installed at user scope — the scope
/// UZE installs at — when its record is the version-2 shape.
fn recorded_user_install(home: &Path, selector: &str) -> Option<bool> {
    let installed = recorded(home, "installed_plugins.json")?;
    if installed.get("version").and_then(serde_json::Value::as_u64) != Some(2) {
        return None;
    }
    let plugins = installed.get("plugins")?.as_object()?;
    Some(
        plugins
            .get(selector)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|installs| {
                installs.iter().any(|install| {
                    install.get("scope").and_then(serde_json::Value::as_str) == Some("user")
                })
            }),
    )
}

fn installed_entry<'a>(
    listing: &'a serde_json::Value,
    selector: &str,
) -> Option<&'a serde_json::Value> {
    listing.as_array()?.iter().find(|entry| {
        entry
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id == selector)
    })
}

/// What the package's own `.claude-plugin/plugin.json` makes Claude load,
/// by Claude's documented discovery (plugins reference, "How each key
/// combines with its default location"), so nothing it loads is delivered
/// a second time beside it:
///
/// - Skills: `skills/` is always scanned; a `skills` field only adds
///   directories to it.
/// - Agents: `agents/`, recursively, unless an `agents` field lists the
///   files to load instead.
/// - MCP servers: `.mcp.json`, merged with `mcpServers` given inline, as a
///   path to a JSON file, or as a list of either.
/// - Hooks: an envelope that declares hooks of its own (`hooks/hooks.json`
///   or a `hooks` field) is the author's Claude hook surface, and the
///   canonical `hooks.json` is not delivered beside it.
///
/// A Skill is covered only when the bytes Claude reads already carry its
/// canonical invocation policy, since UZE never rewrites an author's
/// envelope (ADR-030 §13).
pub(super) fn claude_exact_coverage(
    package: &StoredPackage,
    resources: &[&Resource],
) -> BTreeSet<String> {
    let Some(manifest) = read_json(&package.root.join(".claude-plugin/plugin.json")) else {
        return BTreeSet::new();
    };
    let skill_dirs = declared_paths(manifest.get("skills"));
    let listed_agents = manifest
        .get("agents")
        .map(|agents| declared_paths(Some(agents)));
    let mcp_servers = envelope_mcp_servers(&package.root, &manifest);
    let has_hooks =
        manifest.get("hooks").is_some() || package.root.join("hooks/hooks.json").is_file();

    resources
        .iter()
        .filter(|resource| {
            let relative = resource
                .capability
                .path
                .strip_prefix(&package.root)
                .unwrap_or(&resource.capability.path);
            match resource.capability.kind {
                CapabilityKind::AgentSkill => {
                    let parent = relative.parent().unwrap_or(Path::new(""));
                    let loaded = parent.parent() == Some(Path::new("skills"))
                        || skill_dirs
                            .iter()
                            .any(|dir| parent == dir || parent.parent() == Some(dir));
                    loaded && skill_policy_preserved(resource)
                }
                CapabilityKind::Agent => match &listed_agents {
                    None => relative.starts_with("agents"),
                    Some(files) => files.iter().any(|file| file == relative),
                },
                CapabilityKind::Mcp => resource
                    .resource_name
                    .as_ref()
                    .is_some_and(|name| mcp_servers.contains(name)),
                CapabilityKind::Hook => has_hooks,
                CapabilityKind::Instruction => false,
            }
        })
        .map(|resource| resource.identity())
        .collect()
}

fn skill_policy_preserved(resource: &Resource) -> bool {
    let policy = resource.skill_invocation();
    if policy.is_invalid() {
        false
    } else if !policy.model {
        crate::shared::skill::has_disable_model_invocation(&resource.capability.payload)
    } else if !policy.user {
        crate::shared::skill::has_user_invocable_false(&resource.capability.payload)
    } else {
        true
    }
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// A path field's declarations — one path or a list of them — normalized,
/// with anything absolute or escaping the plugin dropped (see
/// `crate::shared::path`: such a path never loads in Claude either).
fn declared_paths(field: Option<&serde_json::Value>) -> Vec<std::path::PathBuf> {
    let raw: Vec<&str> = match field {
        Some(serde_json::Value::String(path)) => vec![path.as_str()],
        Some(serde_json::Value::Array(items)) => {
            items.iter().filter_map(serde_json::Value::as_str).collect()
        }
        _ => Vec::new(),
    };
    raw.into_iter()
        .filter_map(normalize_declared_relative_path)
        .collect()
}

/// Every server name Claude loads from the envelope.
fn envelope_mcp_servers(root: &Path, manifest: &serde_json::Value) -> BTreeSet<String> {
    fn add_document(names: &mut BTreeSet<String>, document: &serde_json::Value) {
        if let Some(servers) = document
            .get("mcpServers")
            .and_then(serde_json::Value::as_object)
        {
            names.extend(servers.keys().cloned());
        }
    }
    let mut names = BTreeSet::new();
    if let Some(document) = read_json(&root.join(".mcp.json")) {
        add_document(&mut names, &document);
    }
    let declared = match manifest.get("mcpServers") {
        Some(serde_json::Value::Array(items)) => items.iter().collect(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    for entry in declared {
        match entry {
            serde_json::Value::Object(servers) => names.extend(servers.keys().cloned()),
            serde_json::Value::String(path) => {
                if let Some(document) = normalize_declared_relative_path(path)
                    .and_then(|relative| read_json(&root.join(relative)))
                {
                    add_document(&mut names, &document);
                }
            }
            _ => {}
        }
    }
    names
}

fn inspect_claude_plugin(
    executable: &Path,
    command_home: &Path,
    selector: &str,
    marketplace_root: &Path,
) -> AttachmentInspection {
    if let Some(inspection) = inspect_from_records(command_home, selector, marketplace_root) {
        return inspection;
    }
    // Verify marketplace still points at expected root.
    let marketplace_list = match json(
        executable,
        command_home,
        &["plugin", "marketplace", "list", "--json"],
        "claude",
    ) {
        Ok(value) => value,
        Err(reason) => return blocked(reason),
    };
    let marketplace_name = selector.rsplit_once('@').map(|(_, name)| name);
    let Some(marketplace_name) = marketplace_name else {
        return blocked("plugin receipt selector has no marketplace identity");
    };
    let Some(entries) = marketplace_entries(&marketplace_list) else {
        return blocked("Claude marketplace JSON has no marketplaces array");
    };
    let matching = entries.iter().find(|entry| {
        entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| name == marketplace_name)
    });
    let Some(matching) = matching else {
        return AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "Claude marketplace is absent".to_owned(),
        };
    };
    let actual_root = matching
        .get("path")
        .or_else(|| matching.get("installLocation"))
        .or_else(|| matching.get("root"))
        .and_then(serde_json::Value::as_str);
    if actual_root.is_none_or(|root| Path::new(root) != marketplace_root) {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Claude marketplace root differs from receipt".to_owned(),
        };
    }
    // Verify plugin installed.
    let plugins = match json(
        executable,
        command_home,
        &["plugin", "list", "--json"],
        "claude",
    ) {
        Ok(value) => value,
        Err(reason) => return blocked(reason),
    };
    if !plugins.is_array() {
        return blocked("Claude plugin JSON is not an array");
    }
    let Some(plugin) = installed_entry(&plugins, selector) else {
        return AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "Claude plugin is not installed".to_owned(),
        };
    };
    let enabled = plugin.get("enabled").and_then(serde_json::Value::as_bool);
    if enabled == Some(false) {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Claude plugin is disabled".to_owned(),
        };
    }
    // Existence of the cached installPath is deliberately not checked: the
    // cache is a Derived Artifact, not a source of truth. Marketplace root
    // identity + enabled + selector is sufficient for Matched; any
    // marketplace/selector mismatch already returned Drifted above.
    AttachmentInspection {
        state: AttachmentState::Matched,
        reason: "Claude native plugin matches receipt".to_owned(),
    }
}

#[cfg(test)]
mod claude_native_coverage_tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;

    use uze_core::capability::Resource;
    use uze_core::capability::{Capability, CapabilityKind};
    use uze_core::home::UzeHome;
    use uze_core::integration::IntegrationPort;

    use super::ClaudeMarketplace;
    use crate::claude::ClaudeIntegration;
    use crate::shared::marketplace::{Origin, catalogue_document};

    fn claude_catalogue_document(packages: &[uze_core::store::StoredPackage]) -> serde_json::Value {
        catalogue_document::<ClaudeMarketplace>(packages, Origin::Explicit)
    }
    use super::claude_exact_coverage;

    fn temp_root(label: &str) -> PathBuf {
        uze_testkit::temp::scratch(label)
    }

    fn make_package_with_plugin(
        label: &str,
        plugin_json: &str,
    ) -> (PathBuf, uze_core::store::StoredPackage) {
        let root = temp_root(label);
        let pkg_root = root.join("pkg");
        fs::create_dir_all(pkg_root.join(".claude-plugin")).unwrap();
        fs::write(pkg_root.join(".claude-plugin/plugin.json"), plugin_json).unwrap();
        fs::write(pkg_root.join("plugin.json"), r#"{"name":"test-pkg"}"#).unwrap();
        let id =
            uze_core::store::PackageId::from_plugin_name("test-pkg", &pkg_root.join("plugin.json"))
                .unwrap();
        let pkg = uze_core::store::StoredPackage {
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

    fn skill_resource(pkg: &uze_core::store::StoredPackage, skill: &str) -> Resource {
        skill_resource_with(pkg, skill, &format!("---\nname: {skill}\n---\n"))
    }

    fn skill_resource_with(
        pkg: &uze_core::store::StoredPackage,
        skill: &str,
        body: &str,
    ) -> Resource {
        let path = pkg.root.join(format!("skills/{skill}/SKILL.md"));
        skill_at(pkg, path, body)
    }

    fn skill_at(pkg: &uze_core::store::StoredPackage, path: PathBuf, body: &str) -> Resource {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        Resource::from_package(
            pkg.id.clone(),
            pkg.root.clone(),
            Capability {
                kind: CapabilityKind::AgentSkill,
                path,
                payload: body.as_bytes().to_vec(),
            },
        )
    }

    fn mcp_resource(pkg: &uze_core::store::StoredPackage, name: &str) -> Resource {
        let path = pkg.root.join("mcp.json");
        let payload = serde_json::json!({"command":"node","args":[name]})
            .to_string()
            .into_bytes();
        Resource::from_package_named(
            pkg.id.clone(),
            pkg.root.clone(),
            Capability {
                kind: CapabilityKind::Mcp,
                path,
                payload,
            },
            name.to_owned(),
        )
    }

    /// ADR-030 §13: an explicit envelope Skill is only claimed as covered
    /// when the canonical `invoke:` policy is actually preserved by the
    /// vendor bytes the author shipped — UZE never rewrites
    /// explicit-envelope content, and Claude's defaults are model+user.
    #[test]
    fn explicit_user_only_skill_with_the_vendor_marker_is_covered() {
        let (_root, pkg) = make_package_with_plugin(
            "policy-marker",
            r#"{"name":"test-pkg","skills":["./skills/review"]}"#,
        );
        let r = skill_resource_with(
            &pkg,
            "review",
            "---\ndescription: Review\ndisable-model-invocation: true\ninvoke:\n  model: false\n  user: true\n---\nBody.\n",
        );
        let covered = claude_exact_coverage(&pkg, &[&r]);
        assert_eq!(covered, BTreeSet::from([r.identity()]));
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn explicit_user_only_skill_without_the_vendor_marker_is_not_covered() {
        let (_root, pkg) = make_package_with_plugin(
            "policy-no-marker",
            r#"{"name":"test-pkg","skills":["./skills/review"]}"#,
        );
        let r = skill_resource_with(
            &pkg,
            "review",
            "---\ndescription: Review\ninvoke:\n  model: false\n  user: true\n---\nBody.\n",
        );
        let covered = claude_exact_coverage(&pkg, &[&r]);
        assert!(
            covered.is_empty(),
            "a path-matched user-only Skill must not be claimed as covered without its own disable-model-invocation marker"
        );
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn explicit_model_only_skill_requires_user_invocable_false() {
        let (_root, pkg) = make_package_with_plugin(
            "policy-model-only",
            r#"{"name":"test-pkg","skills":["./skills/legacy"]}"#,
        );
        let covered_with = skill_resource_with(
            &pkg,
            "legacy",
            "---\nuser-invocable: false\ninvoke:\n  model: true\n  user: false\n---\nBody.\n",
        );
        assert_eq!(
            claude_exact_coverage(&pkg, &[&covered_with]),
            BTreeSet::from([covered_with.identity()])
        );
        let covered_without = skill_resource_with(
            &pkg,
            "legacy",
            "---\ninvoke:\n  model: true\n  user: false\n---\nBody.\n",
        );
        assert!(
            claude_exact_coverage(&pkg, &[&covered_without]).is_empty(),
            "model-only semantics degrade without the vendor's own user-invocable: false marker"
        );
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn explicit_invalid_policy_skill_is_never_covered() {
        let (_root, pkg) = make_package_with_plugin(
            "policy-invalid",
            r#"{"name":"test-pkg","skills":["./skills/dead"]}"#,
        );
        let r = skill_resource_with(
            &pkg,
            "dead",
            "---\ninvoke:\n  model: false\n  user: false\n---\nBody.\n",
        );
        let covered = claude_exact_coverage(&pkg, &[&r]);
        assert!(covered.is_empty());
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn envelope_declares_all_is_fully_covered() {
        let (_root, pkg) = make_package_with_plugin(
            "all",
            r#"{"name":"test-pkg","version":"0.1.0","skills":["./skills/a","./skills/b"],"mcpServers":{"mcp-x":{"command":"x"}}}"#,
        );
        let r_a = skill_resource(&pkg, "a");
        let r_b = skill_resource(&pkg, "b");
        let r_m = mcp_resource(&pkg, "mcp-x");
        let resources = vec![&r_a, &r_b, &r_m];
        let covered = claude_exact_coverage(&pkg, &resources);
        let expected: BTreeSet<String> = resources.iter().map(|r| r.identity()).collect();
        assert_eq!(covered, expected);
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn every_skill_under_skills_is_loaded_whatever_the_skills_field_lists() {
        let (_root, pkg) = make_package_with_plugin(
            "subset",
            r#"{"name":"test-pkg","version":"0.1.0","skills":["./skills/a"]}"#,
        );
        let r_a = skill_resource(&pkg, "a");
        let r_b = skill_resource(&pkg, "b");
        let r_m = mcp_resource(&pkg, "mcp-x");
        let resources = vec![&r_a, &r_b, &r_m];
        let covered = claude_exact_coverage(&pkg, &resources);
        // Claude always scans `skills/`; the field only adds directories.
        assert_eq!(covered, BTreeSet::from([r_a.identity(), r_b.identity()]));
        assert!(!covered.contains(&r_m.identity()), "no server was declared");
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn a_declared_directory_that_does_not_exist_changes_nothing() {
        let (_root, pkg) = make_package_with_plugin(
            "ghost",
            r#"{"name":"test-pkg","skills":["./skills/ghost"]}"#,
        );
        let r_a = skill_resource(&pkg, "a");
        let covered = claude_exact_coverage(&pkg, &[&r_a]);
        assert_eq!(covered, BTreeSet::from([r_a.identity()]));
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn a_nested_skill_is_loaded_only_when_its_directory_is_declared() {
        let (_root, pkg) = make_package_with_plugin(
            "nested",
            r#"{"name":"test-pkg","skills":["./skills/extra"]}"#,
        );
        let declared = skill_at(
            &pkg,
            pkg.root.join("skills/extra/deploy/SKILL.md"),
            "---\nname: deploy\n---\n",
        );
        let undeclared = skill_at(
            &pkg,
            pkg.root.join("skills/other/audit/SKILL.md"),
            "---\nname: audit\n---\n",
        );
        let covered = claude_exact_coverage(&pkg, &[&declared, &undeclared]);
        assert_eq!(covered, BTreeSet::from([declared.identity()]));
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn malformed_envelope_yields_empty_coverage_but_package_still_deliverable() {
        let (_root, pkg) = make_package_with_plugin("malformed", r#"{"name":"bad", "skills": [}"#);
        let r_a = skill_resource(&pkg, "a");
        let resources = vec![&r_a];
        let covered = claude_exact_coverage(&pkg, &resources);
        assert!(covered.is_empty());
        // package_exposure_plan still returns Some even with empty coverage
        let integration =
            ClaudeIntegration::new(_root.join("claude"), UzeHome::at(_root.join("uze")));
        let plan = integration.package_exposure_plan(&pkg, &resources);
        assert!(plan.is_some());
        assert!(plan.unwrap().provided_resource_identities.is_empty());
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn an_envelope_without_fields_still_loads_its_default_skills() {
        let (_root, pkg) =
            make_package_with_plugin("empty", r#"{"name":"test-pkg","version":"0.1.0"}"#);
        let r_a = skill_resource(&pkg, "a");
        let covered = claude_exact_coverage(&pkg, &[&r_a]);
        // This is the report's duplicate delivery: every skill was also
        // linked loose because an envelope with no `skills` field was read
        // as covering nothing.
        assert_eq!(covered, BTreeSet::from([r_a.identity()]));
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn agents_are_loaded_from_agents_unless_the_field_lists_files() {
        let agent = |pkg: &uze_core::store::StoredPackage, relative: &str| {
            let path = pkg.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "---\ndescription: d\n---\nbody\n").unwrap();
            Resource::from_package(
                pkg.id.clone(),
                pkg.root.clone(),
                Capability {
                    kind: CapabilityKind::Agent,
                    path,
                    payload: Vec::new(),
                },
            )
        };
        let (_root, pkg) = make_package_with_plugin("agents-default", r#"{"name":"test-pkg"}"#);
        let flat = agent(&pkg, "agents/reviewer.md");
        let nested = agent(&pkg, "agents/review/security.md");
        assert_eq!(
            claude_exact_coverage(&pkg, &[&flat, &nested]),
            BTreeSet::from([flat.identity(), nested.identity()])
        );
        let _ = fs::remove_dir_all(_root);

        let (_root, pkg) = make_package_with_plugin(
            "agents-listed",
            r#"{"name":"test-pkg","agents":["./agents/reviewer.md"]}"#,
        );
        let flat = agent(&pkg, "agents/reviewer.md");
        let nested = agent(&pkg, "agents/review/security.md");
        assert_eq!(
            claude_exact_coverage(&pkg, &[&flat, &nested]),
            BTreeSet::from([flat.identity()])
        );
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn servers_are_read_from_mcp_json_and_from_a_declared_file() {
        let (_root, pkg) = make_package_with_plugin(
            "mcp-files",
            r#"{"name":"test-pkg","mcpServers":"./mcp/servers.json"}"#,
        );
        fs::write(
            pkg.root.join(".mcp.json"),
            r#"{"mcpServers":{"rtc":{"command":"x"}}}"#,
        )
        .unwrap();
        fs::create_dir_all(pkg.root.join("mcp")).unwrap();
        fs::write(
            pkg.root.join("mcp/servers.json"),
            r#"{"mcpServers":{"gitlab":{"command":"y"}}}"#,
        )
        .unwrap();
        let rtc = mcp_resource(&pkg, "rtc");
        let gitlab = mcp_resource(&pkg, "gitlab");
        let other = mcp_resource(&pkg, "other");
        assert_eq!(
            claude_exact_coverage(&pkg, &[&rtc, &gitlab, &other]),
            BTreeSet::from([rtc.identity(), gitlab.identity()])
        );
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn an_envelope_with_hooks_of_its_own_shadows_the_canonical_ones() {
        let hook = |pkg: &uze_core::store::StoredPackage| {
            Resource::from_package_named(
                pkg.id.clone(),
                pkg.root.clone(),
                Capability {
                    kind: CapabilityKind::Hook,
                    path: pkg.root.join("hooks.json"),
                    payload: Vec::new(),
                },
                "guard".to_owned(),
            )
        };
        let (_root, pkg) = make_package_with_plugin("no-hooks", r#"{"name":"test-pkg"}"#);
        assert!(claude_exact_coverage(&pkg, &[&hook(&pkg)]).is_empty());
        let _ = fs::remove_dir_all(_root);

        let (_root, pkg) = make_package_with_plugin("own-hooks", r#"{"name":"test-pkg"}"#);
        fs::create_dir_all(pkg.root.join("hooks")).unwrap();
        fs::write(pkg.root.join("hooks/hooks.json"), r#"{"hooks":{}}"#).unwrap();
        let guard = hook(&pkg);
        assert_eq!(
            claude_exact_coverage(&pkg, &[&guard]),
            BTreeSet::from([guard.identity()])
        );
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn duplicate_declarations_are_deduplicated() {
        let (_root, pkg) = make_package_with_plugin(
            "dup",
            r#"{"name":"test-pkg","skills":["./skills/a","./skills/a","./skills/a"]}"#,
        );
        let r_a = skill_resource(&pkg, "a");
        let resources = vec![&r_a];
        let covered = claude_exact_coverage(&pkg, &resources);
        assert_eq!(covered.len(), 1);
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn path_normalization_and_escape_attempts_are_ignored() {
        let (_root, pkg) = make_package_with_plugin(
            "escape",
            r#"{"name":"test-pkg","skills":["./skills/a","../escape","/absolute","./skills/b/../c"]}"#,
        );
        let r_a = skill_resource(&pkg, "a");
        let resources = vec![&r_a];
        let covered = claude_exact_coverage(&pkg, &resources);
        // Only ./skills/a is valid; others contain .. or absolute or are normalized with ..
        assert_eq!(covered, BTreeSet::from([r_a.identity()]));
        let _ = fs::remove_dir_all(_root);
    }

    /// Regression test for a real bug (see `crate::shared::path`'s own doc
    /// comment): the previous per-entry normalization stripped a leading
    /// `/` before checking `is_absolute()`, so `/skills/extra` silently
    /// became the relative declaration `skills/extra` and was ACCEPTED —
    /// wrongly covering a real skill that lives at exactly that path.
    #[test]
    fn leading_slash_declaration_is_rejected_even_when_it_would_collide_with_a_real_skill() {
        let (_root, pkg) = make_package_with_plugin(
            "leading-slash-collision",
            r#"{"name":"test-pkg","skills":["/skills/extra"]}"#,
        );
        let nested = skill_at(
            &pkg,
            pkg.root.join("skills/extra/commit/SKILL.md"),
            "---\nname: commit\n---\n",
        );
        let covered = claude_exact_coverage(&pkg, &[&nested]);
        assert!(
            covered.is_empty(),
            "`/skills/extra` is an absolute declaration and must never be silently \
             relativized into covering the real `skills/extra` directory: got {covered:?}"
        );
        let _ = fs::remove_dir_all(_root);
    }

    /// Same regression, guarding against whitespace padding defeating the
    /// absolute check the same way a bare leading `/` would.
    #[test]
    fn whitespace_padded_absolute_declaration_is_also_rejected() {
        let (_root, pkg) = make_package_with_plugin(
            "whitespace-padded-absolute",
            r#"{"name":"test-pkg","skills":["  /skills/extra  "]}"#,
        );
        let nested = skill_at(
            &pkg,
            pkg.root.join("skills/extra/commit/SKILL.md"),
            "---\nname: commit\n---\n",
        );
        let covered = claude_exact_coverage(&pkg, &[&nested]);
        assert!(covered.is_empty(), "got {covered:?}");
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn mcp_coverage_exact_by_name() {
        let (_root, pkg) = make_package_with_plugin(
            "mcp",
            r#"{"name":"test-pkg","mcpServers":{"mcp-a":{"command":"a"},"mcp-b":{"command":"b"}}}"#,
        );
        let r_a = mcp_resource(&pkg, "mcp-a");
        let r_b = mcp_resource(&pkg, "mcp-b");
        let r_c = mcp_resource(&pkg, "mcp-c");
        let resources = vec![&r_a, &r_b, &r_c];
        let covered = claude_exact_coverage(&pkg, &resources);
        assert_eq!(covered, BTreeSet::from([r_a.identity(), r_b.identity()]));
        let _ = fs::remove_dir_all(_root);
    }

    #[test]
    fn marketplace_republish_is_deterministic() {
        let (_root, pkg1) = make_package_with_plugin(
            "pub1",
            r#"{"name":"test-pkg","version":"1.2.3","description":"desc"}"#,
        );
        // Need distinct package ids for two packages
        let pkg1_id = pkg1.id.clone();
        let pkg1_root = pkg1.root.clone();
        // Second package
        let root2 = temp_root("pub2");
        let pkg2_root = root2.join("pkg2");
        fs::create_dir_all(pkg2_root.join(".claude-plugin")).unwrap();
        fs::write(
            pkg2_root.join(".claude-plugin/plugin.json"),
            r#"{"name":"pkg-two","version":"0.1.0"}"#,
        )
        .unwrap();
        fs::write(pkg2_root.join("plugin.json"), r#"{"name":"pkg-two"}"#).unwrap();
        let pkg2 = uze_core::store::StoredPackage {
            id: uze_core::store::PackageId::from_plugin_name(
                "pkg-two",
                &pkg2_root.join("plugin.json"),
            )
            .unwrap(),
            active_name: "pkg-two".to_owned(),
            root: pkg2_root.clone(),
            manifest: pkg2_root.join("plugin.json"),
            provenance: uze_core::acquisition::Provenance {
                requested: uze_core::acquisition::PackageSource::Local {
                    path: PathBuf::from("/tmp/fake2"),
                },
                resolved: uze_core::acquisition::ResolvedSource::Local {
                    path: PathBuf::from("/tmp/fake2"),
                },
            },
        };
        let doc1 = claude_catalogue_document(&[pkg1.clone(), pkg2.clone()]);
        let doc1_again = claude_catalogue_document(&[pkg1, pkg2]);
        assert_eq!(doc1, doc1_again);
        assert_eq!(doc1["name"], "uze-local");
        assert_eq!(
            doc1["owner"]["url"], "https://github.com/uze-sh/uze",
            "the owner Claude's marketplace UI shows is this project, not another"
        );
        assert_eq!(doc1["plugins"].as_array().unwrap().len(), 2);
        let _ = fs::remove_dir_all(_root);
        let _ = fs::remove_dir_all(root2);
        let _ = pkg1_id;
        let _ = pkg1_root;
    }

    #[test]
    fn fallback_without_envelope_returns_none() {
        let root = temp_root("fallback");
        let pkg_root = root.join("pkg");
        fs::create_dir_all(&pkg_root).unwrap();
        fs::write(pkg_root.join("plugin.json"), r#"{"name":"no-claude"}"#).unwrap();
        let pkg = uze_core::store::StoredPackage {
            id: uze_core::store::PackageId::from_plugin_name(
                "no-claude",
                &pkg_root.join("plugin.json"),
            )
            .unwrap(),
            active_name: "no-claude".to_owned(),
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
        let integration =
            ClaudeIntegration::new(root.join("claude"), UzeHome::at(root.join("uze")));
        let resources: Vec<&Resource> = vec![];
        assert!(
            integration
                .package_exposure_plan(&pkg, &resources)
                .is_none()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_receipt_is_integration_owned_and_uncovered_fallback_remains() {
        let (_root, pkg) =
            make_package_with_plugin("receipt", r#"{"name":"test-pkg","skills":["./skills/a"]}"#);
        let r_a = skill_resource(&pkg, "a");
        // A server the envelope does not load stays with capability-level
        // delivery.
        let r_b = mcp_resource(&pkg, "undeclared");
        let resources = vec![&r_a, &r_b];
        let integration =
            ClaudeIntegration::new(_root.join("claude"), UzeHome::at(_root.join("uze")));
        let plan = integration
            .package_exposure_plan(&pkg, &resources)
            .expect("should have plan");
        assert_eq!(plan.provided_resource_identities.len(), 1);
        assert!(plan.provided_resource_identities.contains(&r_a.identity()));
        assert!(!plan.provided_resource_identities.contains(&r_b.identity()));
        // r_b should still be attachable via capability fallback
        uze_core::state::record(
            &UzeHome::at(_root.join("uze")),
            integration.id(),
            uze_core::state::IntegrationRecord::default(),
        )
        .unwrap();
        let plan_b = integration.exposure_plan(&r_b);
        assert!(!matches!(
            plan_b.mechanism,
            uze_core::exposure::ExposureMechanism::Unsupported { .. }
        ));
        let _ = fs::remove_dir_all(_root);
    }
}

#[cfg(test)]
mod explicit_marketplace_tests {
    use std::{collections::BTreeMap, fs, path::PathBuf};

    use uze_core::{
        home::UzeHome,
        integration::{AttachmentReceipt, ManagedArtifact},
        store::{PackageId, StoredPackage},
    };

    use super::ClaudeMarketplace;
    use crate::shared::marketplace::{self, MarketplaceDialect};

    fn explicit_package(root: &std::path::Path) -> StoredPackage {
        let pkg_root = root.join("store/plugins/local/kit");
        fs::create_dir_all(pkg_root.join(".claude-plugin")).unwrap();
        fs::create_dir_all(pkg_root.join("skills/a/scripts")).unwrap();
        fs::write(
            pkg_root.join(".claude-plugin/plugin.json"),
            r#"{"name":"kit"}"#,
        )
        .unwrap();
        fs::write(pkg_root.join("plugin.json"), r#"{"name":"kit"}"#).unwrap();
        fs::write(pkg_root.join("skills/a/SKILL.md"), "---\nname: a\n---\n").unwrap();
        fs::write(pkg_root.join("skills/a/scripts/run.sh"), "#!/bin/sh\n").unwrap();
        let id = PackageId::from_plugin_name("kit", &pkg_root.join("plugin.json")).unwrap();
        StoredPackage {
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
        }
    }

    fn explicit_receipt(package: &StoredPackage, marketplace_root: PathBuf) -> AttachmentReceipt {
        let detail: BTreeMap<String, serde_json::Value> = [(
            "marketplace_root".to_owned(),
            serde_json::json!(marketplace_root),
        )]
        .into_iter()
        .collect();
        AttachmentReceipt {
            package_id: package.id.as_str().to_owned(),
            resource_identity: None,
            integration: "claude-code".to_owned(),
            artifact: ManagedArtifact::IntegrationOwned {
                kind: ClaudeMarketplace::EXPLICIT_KIND.to_owned(),
                selector: "kit@uze-local".to_owned(),
                detail,
            },
        }
    }

    #[test]
    fn an_explicit_envelope_is_mirrored_out_of_the_store() {
        let root = uze_testkit::temp::scratch("explicit-mirror");
        let home = UzeHome::at(root.join("uze"));
        let package = explicit_package(&root);
        let stale = home.store_dir().join(ClaudeMarketplace::CATALOGUE_PATH);
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "{}").unwrap();

        marketplace::republish::<ClaudeMarketplace>(&home, std::slice::from_ref(&package)).unwrap();

        let mirror = marketplace::explicit_package_dir::<ClaudeMarketplace>(&home, &package);
        assert!(mirror.starts_with(home.runtime_dir()));
        assert!(mirror.join(".claude-plugin/plugin.json").is_file());
        assert!(mirror.join("skills/a/scripts/run.sh").is_file());
        assert!(!mirror.join("skills").is_symlink());
        let catalogue = marketplace::marketplace_root::<ClaudeMarketplace>(
            &home,
            marketplace::Origin::Explicit,
        )
        .join(ClaudeMarketplace::CATALOGUE_PATH);
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(catalogue).unwrap()).unwrap();
        assert_eq!(document["plugins"][0]["source"], "./plugins/local/kit");
        assert!(!stale.exists(), "no catalogue is left in the Store");
    }

    #[test]
    fn a_receipt_made_through_the_store_rooted_marketplace_no_longer_serves() {
        let root = uze_testkit::temp::scratch("explicit-serves");
        let home = UzeHome::at(root.join("uze"));
        let package = explicit_package(&root);
        let old = explicit_receipt(&package, home.store_dir());
        let current = explicit_receipt(
            &package,
            marketplace::marketplace_root::<ClaudeMarketplace>(
                &home,
                marketplace::Origin::Explicit,
            ),
        );
        assert!(!marketplace::receipt_serves::<ClaudeMarketplace>(
            &home, &old
        ));
        assert!(marketplace::receipt_serves::<ClaudeMarketplace>(
            &home, &current
        ));
    }

    #[test]
    fn the_generated_tier_is_pruned_by_reference() {
        let root = uze_testkit::temp::scratch("explicit-prune");
        let home = UzeHome::at(root.join("uze"));
        let generated = marketplace::generated_root::<ClaudeMarketplace>(&home);
        let orphan = generated.join("gone@local");
        let named = generated.join("kept@local");
        let explicit_orphan =
            marketplace::explicit_root::<ClaudeMarketplace>(&home).join("plugins/local/gone");
        for dir in [&orphan, &named, &explicit_orphan] {
            fs::create_dir_all(dir).unwrap();
        }
        let mut receipt = explicit_receipt(&explicit_package(&root), generated.clone());
        if let ManagedArtifact::IntegrationOwned { detail, .. } = &mut receipt.artifact {
            detail.insert("package_root".to_owned(), serde_json::json!(named));
        }
        uze_core::state::record_receipt(&home, receipt).unwrap();

        marketplace::republish::<ClaudeMarketplace>(&home, &[]).unwrap();

        assert!(!orphan.exists() && !explicit_orphan.exists());
        assert!(
            named.exists(),
            "a directory a receipt names waits for its detach"
        );
    }
}

#[cfg(test)]
mod recorded_state_tests {
    use std::{fs, path::Path};

    use super::{ClaudeMarketplace, recorded_marketplaces, recorded_user_install};
    use crate::shared::marketplace::MarketplaceDialect;

    fn home_with(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let home = uze_testkit::temp::scratch(label);
        let plugins = home.join(".claude/plugins");
        fs::create_dir_all(&plugins).unwrap();
        for (name, body) in files {
            fs::write(plugins.join(name), body).unwrap();
        }
        home
    }

    /// Answered from Claude's own records, without starting it: the program
    /// named here does not exist, so any answer came from the files.
    #[test]
    fn claudes_records_answer_without_its_cli() {
        let mut env = uze_testkit::env::scope();
        env.remove("CLAUDE_CONFIG_DIR");
        let home = home_with(
            "claude-recorded",
            &[
                (
                    "known_marketplaces.json",
                    r#"{"uze-store":{"source":{"source":"directory","path":"/uze/generated"},"installLocation":"/uze/generated"}}"#,
                ),
                (
                    "installed_plugins.json",
                    r#"{"version":2,"plugins":{"git@uze-store":[{"scope":"user"}],"other@uze-store":[{"scope":"project"}]}}"#,
                ),
            ],
        );
        let absent = Path::new("/nonexistent/claude");

        assert!(ClaudeMarketplace::marketplace_exists(
            absent,
            &home,
            Path::new("/uze/generated")
        ));
        assert!(!ClaudeMarketplace::marketplace_exists(
            absent,
            &home,
            Path::new("/elsewhere")
        ));
        assert!(ClaudeMarketplace::plugin_installed(
            absent,
            &home,
            "git@uze-store"
        ));
        assert!(
            !ClaudeMarketplace::plugin_installed(absent, &home, "other@uze-store"),
            "an install at project scope is not the user-scope one UZE makes"
        );
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn an_installed_enabled_plugin_inspects_as_matched_from_the_records() {
        let mut env = uze_testkit::env::scope();
        env.remove("CLAUDE_CONFIG_DIR");
        let home = home_with(
            "claude-recorded-inspect",
            &[
                (
                    "known_marketplaces.json",
                    r#"{"uze-store":{"source":{"source":"directory","path":"/uze/generated"},"installLocation":"/uze/generated"}}"#,
                ),
                (
                    "installed_plugins.json",
                    r#"{"version":2,"plugins":{"git@uze-store":[{"scope":"user"}]}}"#,
                ),
            ],
        );
        let settings = home.join(".claude/settings.json");
        let absent = Path::new("/nonexistent/claude");
        let inspect = |selector: &str| {
            super::inspect_claude_plugin(absent, &home, selector, Path::new("/uze/generated")).state
        };

        fs::write(&settings, r#"{"enabledPlugins":{"git@uze-store":true}}"#).unwrap();
        assert_eq!(
            inspect("git@uze-store"),
            uze_core::integration::AttachmentState::Matched
        );
        assert_eq!(
            inspect("other@uze-store"),
            uze_core::integration::AttachmentState::Missing
        );

        fs::write(&settings, r#"{"enabledPlugins":{"git@uze-store":false}}"#).unwrap();
        assert_eq!(
            inspect("git@uze-store"),
            uze_core::integration::AttachmentState::Drifted
        );
        let _ = fs::remove_dir_all(home);
    }

    /// A record in a shape this does not know answers nothing, so the CLI
    /// is asked as before.
    #[test]
    fn an_unknown_shape_is_not_trusted() {
        let mut env = uze_testkit::env::scope();
        env.remove("CLAUDE_CONFIG_DIR");
        let home = home_with(
            "claude-recorded-unknown",
            &[
                ("known_marketplaces.json", r#"{"uze-store":{"where":"/x"}}"#),
                ("installed_plugins.json", r#"{"version":3,"plugins":{}}"#),
            ],
        );
        assert!(recorded_marketplaces(&home).is_none());
        assert!(recorded_user_install(&home, "git@uze-store").is_none());
        let _ = fs::remove_dir_all(home);
    }
}
