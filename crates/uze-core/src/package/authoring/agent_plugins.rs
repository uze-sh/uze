//! Whether a uze package is also a valid [Agent Plugins 1.0] plugin.
//!
//! uze's own format stays authoritative: everything here is advice, and
//! nothing it finds keeps a package from installing. It answers a
//! different question from the rest of `check`: not "what would uze do",
//! but "would a client of the standard (Codex, Copilot, Cursor, Kiro, VS
//! Code) load this directory unchanged".
//!
//! The standard covers `plugin.json`, `skills/` and `mcp.json`. What uze
//! adds beyond it (`agents/`, `hooks.json`, `AGENTS.md`, a skill's
//! `invoke:` and `harness:` blocks) are component types and fields the
//! standard leaves to each client, so they never count against it; uze's
//! manifest namespace is `extensions["sh.uze"]`.
//!
//! [Agent Plugins 1.0]: https://github.com/agentplugins/agent-plugins-spec/blob/main/spec/1.0.0.md

use std::path::{Component, Path};

use noyalib::{ParserConfig, compat::serde_yaml, from_str_with_config};

/// The standard this module judges against, as a reader names it.
pub const STANDARD: &str = "Agent Plugins 1.0";

/// The `$schema` a 1.0 `plugin.json` must carry, exactly.
pub const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";

/// The `$schema` a 1.0 `mcp.json` must carry, exactly.
pub const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";

/// uze's reverse-domain namespace under the manifest's `extensions`, from
/// the domain it publishes under (`uze.sh`).
pub const UZE_NAMESPACE: &str = "sh.uze";

const MANIFEST_FIELDS: [&str; 10] = [
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];
const AUTHOR_FIELDS: [&str; 3] = ["name", "email", "url"];
const STDIO_FIELDS: [&str; 5] = ["type", "command", "args", "env", "cwd"];
const REMOTE_FIELDS: [&str; 3] = ["type", "url", "headers"];
const RESERVED_ENVIRONMENT: [&str; 2] = ["PLUGIN_ROOT", "PLUGIN_DATA"];
const PLUGIN_ROOT: &str = "${PLUGIN_ROOT}";
const PLUGIN_DATA: &str = "${PLUGIN_DATA}";

/// The Agent Skills specification's bound on `description`.
const SKILL_DESCRIPTION_MAX_CHARS: usize = 1024;

/// The answer about one standard: conformant when nothing diverges.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StandardConformance {
    pub standard: &'static str,
    pub conformant: bool,
    /// What keeps the package from being a valid plugin of the standard,
    /// located like a finding. Empty is conformant.
    pub divergences: Vec<String>,
}

impl StandardConformance {
    pub(super) fn from_divergences(divergences: Vec<String>) -> Self {
        Self {
            standard: STANDARD,
            conformant: divergences.is_empty(),
            divergences,
        }
    }
}

/// What a standard client would refuse, skip or report in the package at
/// `root`, and — separately — what uze itself reads differently from how
/// the standard says (returned as the second list, for the report's
/// warnings).
pub(super) fn judge(root: &Path) -> (StandardConformance, Vec<String>) {
    let mut divergences = Vec::new();
    let mut warnings = Vec::new();
    manifest_divergences(root, &mut divergences, &mut warnings);
    skill_divergences(root, &mut divergences);
    mcp_divergences(root, &mut divergences, &mut warnings);
    (StandardConformance::from_divergences(divergences), warnings)
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    crate::store::read_package_file(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn manifest_divergences(root: &Path, divergences: &mut Vec<String>, warnings: &mut Vec<String>) {
    let path = root.join("plugin.json");
    let located = path.display();
    let Some(value) = read_json(&path) else {
        divergences.push(format!(
            "{located}: the standard requires a JSON `plugin.json` at the plugin root"
        ));
        return;
    };
    let Some(manifest) = value.as_object() else {
        divergences.push(format!("{located}: the manifest is not a JSON object"));
        return;
    };
    match manifest.get("$schema").and_then(serde_json::Value::as_str) {
        Some(PLUGIN_SCHEMA) => {}
        Some(other) => divergences.push(format!(
            "{located}: `$schema` is `{other}`, not the {STANDARD} identifier `{PLUGIN_SCHEMA}`"
        )),
        None => divergences.push(format!(
            "{located}: no `$schema`; the standard requires `\"$schema\": \"{PLUGIN_SCHEMA}\"`"
        )),
    }
    for (field, value) in manifest {
        let field = field.as_str();
        if !MANIFEST_FIELDS.contains(&field) {
            divergences.push(format!(
                "{located}: `{field}` is not a manifest field of the standard, whose clients \
                 report and ignore it; data for one client belongs under `extensions`"
            ));
            continue;
        }
        if let Some(reason) = manifest_field_fault(field, value) {
            divergences.push(format!(
                "{located}: `{field}` {reason}, which makes the standard's clients reject the \
                 plugin"
            ));
        }
    }
    if let Some(ours) = manifest
        .get("extensions")
        .and_then(|extensions| extensions.get(UZE_NAMESPACE))
        .and_then(serde_json::Value::as_object)
    {
        for key in ours.keys() {
            warnings.push(format!(
                "{located}: `extensions[\"{UZE_NAMESPACE}\"].{key}` is not a setting uze \
                 reads; it is ignored"
            ));
        }
    }
}

fn manifest_field_fault(field: &str, value: &serde_json::Value) -> Option<&'static str> {
    use serde_json::Value;
    match field {
        "version" | "description" | "homepage" | "repository" | "license" => {
            (!value.is_string()).then_some("must be a string")
        }
        "keywords" => match value {
            Value::Array(items) if items.iter().all(Value::is_string) => None,
            _ => Some("must be a list of strings"),
        },
        "author" => match value {
            Value::Object(author)
                if author.iter().all(|(key, value)| {
                    AUTHOR_FIELDS.contains(&key.as_str()) && value.is_string()
                }) =>
            {
                None
            }
            _ => Some("must be an object of `name`, `email` and `url` strings only"),
        },
        "extensions" => match value {
            Value::Object(namespaces) if namespaces.values().all(Value::is_object) => None,
            Value::Object(_) => Some("must map each namespace to an object"),
            _ => Some("must be an object keyed by namespace"),
        },
        _ => None,
    }
}

/// The standard discovers `skills/<name>/SKILL.md` one level deep and holds
/// each to the Agent Skills specification; uze walks deeper.
fn skill_divergences(root: &Path, divergences: &mut Vec<String>) {
    let skills = root.join("skills");
    if !skills.is_dir() {
        return;
    }
    let Ok(found) = crate::engine::discover_files(&skills, |path| path.ends_with("SKILL.md"))
    else {
        return;
    };
    for path in found {
        let depth = path
            .strip_prefix(&skills)
            .map(|relative| relative.components().count())
            .unwrap_or_default();
        if depth != 2 {
            divergences.push(format!(
                "{}: the standard discovers only `skills/<name>/SKILL.md`, so its clients do \
                 not see a skill nested deeper",
                path.display()
            ));
            continue;
        }
        if let Some(length) = description_length(&path)
            && length > SKILL_DESCRIPTION_MAX_CHARS
        {
            divergences.push(format!(
                "{}: `description` is {length} characters; the Agent Skills specification the \
                 standard holds skills to allows {SKILL_DESCRIPTION_MAX_CHARS}",
                path.display()
            ));
        }
    }
}

fn description_length(path: &Path) -> Option<usize> {
    let bytes = crate::store::read_package_file(path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let (head, _) = crate::skill::split_frontmatter(text.strip_prefix('\u{feff}').unwrap_or(text))?;
    let frontmatter: serde_yaml::Value =
        from_str_with_config(head, &ParserConfig::serde_yaml_compat()).ok()?;
    frontmatter
        .get("description")
        .and_then(serde_yaml::Value::as_str)
        .map(|description| description.chars().count())
}

fn mcp_divergences(root: &Path, divergences: &mut Vec<String>, warnings: &mut Vec<String>) {
    let path = root.join("mcp.json");
    if !path.is_file() {
        return;
    }
    let located = path.display();
    let Some(serde_json::Value::Object(document)) = read_json(&path) else {
        divergences.push(format!("{located}: not a JSON object"));
        return;
    };
    match document.get("$schema").and_then(serde_json::Value::as_str) {
        Some(MCP_SCHEMA) => {}
        Some(other) => divergences.push(format!(
            "{located}: `$schema` is `{other}`, not the {STANDARD} identifier `{MCP_SCHEMA}`"
        )),
        None => divergences.push(format!(
            "{located}: no `$schema`; the standard requires `\"$schema\": \"{MCP_SCHEMA}\"`"
        )),
    }
    for field in document.keys() {
        if field != "$schema" && field != "mcpServers" {
            divergences.push(format!(
                "{located}: `{field}` is not allowed beside `$schema` and `mcpServers`"
            ));
        }
    }
    let Some(servers) = document
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
    else {
        divergences.push(format!("{located}: `mcpServers` must be an object"));
        return;
    };
    for (name, server) in servers {
        for reason in server_faults(server) {
            divergences.push(format!("{located}: server `{name}` {reason}"));
        }
    }
    if crate::store::read_package_file(&path)
        .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(PLUGIN_DATA))
    {
        warnings.push(format!(
            "{located}: uze does not provide `{PLUGIN_DATA}` yet, so it reaches the harness \
             unexpanded"
        ));
    }
}

/// Why a standard client would skip this server entry; empty when it would
/// load it.
fn server_faults(server: &serde_json::Value) -> Vec<String> {
    let Some(server) = server.as_object() else {
        return vec!["is not an object".to_owned()];
    };
    let text = |key: &str| server.get(key).and_then(serde_json::Value::as_str);
    let mut faults = Vec::new();
    let remote = match text("type") {
        Some("stdio") => false,
        Some("streamable-http" | "sse") => true,
        Some(other) => {
            return vec![format!(
                "has `type: {other}`; the standard knows `stdio`, `streamable-http` and `sse`"
            )];
        }
        None => {
            let remote = server.contains_key("url");
            let suggested = if remote { "streamable-http" } else { "stdio" };
            faults.push(format!(
                "has no `type`; the standard requires one, here `\"type\": \"{suggested}\"`"
            ));
            remote
        }
    };
    let allowed: &[&str] = if remote {
        &REMOTE_FIELDS
    } else {
        &STDIO_FIELDS
    };
    faults.extend(
        server
            .keys()
            .filter(|key| !allowed.contains(&key.as_str()) && key.as_str() != "type")
            .map(|key| format!("carries `{key}`, which its `type` does not allow")),
    );
    if remote {
        faults.extend(remote_fault(text("url")));
    } else {
        faults.extend(stdio_faults(
            text("command"),
            text("cwd"),
            server.get("env"),
        ));
    }
    faults
}

fn stdio_faults(
    command: Option<&str>,
    cwd: Option<&str>,
    env: Option<&serde_json::Value>,
) -> Vec<String> {
    let mut faults = Vec::new();
    match command {
        None => faults.push("has no `command`".to_owned()),
        Some(command) if command.contains("${") => faults.push(format!(
            "names `{command}`, but the standard never expands a placeholder in `command`; \
             name a bundled executable as `./path` instead"
        )),
        Some(command) if command.split_whitespace().nth(1).is_some() => faults.push(format!(
            "runs `{command}`, but `command` is one executable, its arguments go in `args`"
        )),
        Some(command) if command.contains('/') && !command.starts_with("./") => {
            faults.push(format!(
                "names `{command}`; `command` is a bare name found on PATH or a `./` path \
                 inside the plugin"
            ));
        }
        Some(_) => {}
    }
    if let Some(cwd) = cwd {
        let rooted = cwd.starts_with("./")
            || [PLUGIN_ROOT, PLUGIN_DATA]
                .iter()
                .any(|base| cwd == *base || cwd.starts_with(&format!("{base}/")));
        let escapes = Path::new(cwd)
            .components()
            .any(|component| component == Component::ParentDir);
        if !rooted || escapes {
            faults.push(format!(
                "has `cwd: {cwd}`; the standard takes a `./` path, or one under \
                 `{PLUGIN_ROOT}` or `{PLUGIN_DATA}`, that stays inside it"
            ));
        }
    }
    if let Some(serde_json::Value::Object(env)) = env {
        for reserved in RESERVED_ENVIRONMENT {
            if env.contains_key(reserved) {
                faults.push(format!(
                    "sets `{reserved}` in `env`, which the standard reserves to the client"
                ));
            }
        }
    }
    faults
}

fn remote_fault(url: Option<&str>) -> Option<String> {
    let Some(url) = url else {
        return Some("has no `url`".to_owned());
    };
    let loopback = ["http://localhost", "http://127.", "http://[::1]"]
        .iter()
        .any(|prefix| url.starts_with(prefix));
    (!url.starts_with("https://") && !loopback)
        .then(|| format!("has `url: {url}`; the standard requires HTTPS for anything but loopback"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stdio_server_in_the_standard_shape_has_no_fault() {
        let server = serde_json::json!({
            "type": "stdio",
            "command": "./bin/server",
            "args": ["${PLUGIN_ROOT}/config.json"],
            "env": { "DATA": "${PLUGIN_DATA}/db" },
            "cwd": "${PLUGIN_ROOT}"
        });
        assert!(
            server_faults(&server).is_empty(),
            "{:?}",
            server_faults(&server)
        );
    }

    #[test]
    fn each_server_fault_is_named() {
        for (server, expected) in [
            (
                serde_json::json!({ "command": "x" }),
                "`\"type\": \"stdio\"`",
            ),
            (
                serde_json::json!({ "url": "https://x" }),
                "`\"type\": \"streamable-http\"`",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "${PLUGIN_ROOT}/bin/s" }),
                "never expands a placeholder",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "node server.js" }),
                "its arguments go in `args`",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "bin/s" }),
                "a `./` path",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "s", "cwd": "data" }),
                "`cwd: data`",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "s", "cwd": "./../up" }),
                "`cwd: ./../up`",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "s", "env": { "PLUGIN_ROOT": "/" } }),
                "reserves to the client",
            ),
            (
                serde_json::json!({ "type": "stdio", "command": "s", "url": "https://x" }),
                "carries `url`",
            ),
            (
                serde_json::json!({ "type": "sse", "url": "http://example.com" }),
                "requires HTTPS",
            ),
            (
                serde_json::json!({ "type": "http", "url": "https://x" }),
                "`type: http`",
            ),
        ] {
            let faults = server_faults(&server);
            assert!(
                faults.iter().any(|fault| fault.contains(expected)),
                "{server}: {faults:?}"
            );
        }
        assert!(
            server_faults(&serde_json::json!({ "type": "streamable-http", "url": "http://localhost:8080/mcp" }))
                .is_empty()
        );
    }

    #[test]
    fn manifest_field_types_follow_the_standard() {
        assert!(manifest_field_fault("version", &serde_json::json!("1.0.0")).is_none());
        assert!(manifest_field_fault("version", &serde_json::json!(1)).is_some());
        assert!(manifest_field_fault("author", &serde_json::json!({ "name": "a" })).is_none());
        assert!(manifest_field_fault("author", &serde_json::json!("a")).is_some());
        assert!(
            manifest_field_fault("author", &serde_json::json!({ "name": "a", "x": "b" })).is_some()
        );
        assert!(manifest_field_fault("keywords", &serde_json::json!(["a"])).is_none());
        assert!(manifest_field_fault("keywords", &serde_json::json!([1])).is_some());
        assert!(manifest_field_fault("extensions", &serde_json::json!({ "sh.uze": {} })).is_none());
        assert!(
            manifest_field_fault("extensions", &serde_json::json!({ "sh.uze": true })).is_some()
        );
    }
}
