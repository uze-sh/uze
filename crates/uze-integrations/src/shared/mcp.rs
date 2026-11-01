//! MCP delivery every harness states the same way: one stdio server read
//! from its canonical payload, the managed config entry planned for it, and
//! — for the harnesses whose own CLI registers servers — the `mcp`
//! add/probe/remove verbs.

use std::{ffi::OsString, fs, path::Path, path::PathBuf};

use uze_core::{
    Result, UzeError,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, McpEnvironmentReference},
    integration::{AttachmentInspection, AttachmentState, ManagedArtifact},
    router::CompatibilityRoute,
    store::StoredPackage,
};

use crate::shared::package_root::{PluginRoot, resolve_json};
use crate::shared::plan::unsupported;
use crate::shared::process::{capture, failed_message, is_cli_safe_token, succeeds};

/// The `mcpServers` value of the package's canonical `mcp.json`, resolved
/// into the grammar every harness runs: what an envelope carries.
pub(crate) fn delivered_mcp_servers(
    package: &StoredPackage,
    root: &PluginRoot,
) -> Option<serde_json::Value> {
    fs::read(package.root.join("mcp.json"))
        .ok()
        .and_then(|bytes| uze_core::authored::json::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.get("mcpServers").cloned())
        .map(|servers| match servers {
            serde_json::Value::Object(servers) => serde_json::Value::Object(
                servers
                    .into_iter()
                    .map(|(name, server)| (name, resolve_server(&server, root)))
                    .collect(),
            ),
            other => resolve_json(&other, root),
        })
}

/// One server with the package root resolved: `${PLUGIN_ROOT}` wherever it
/// is written, and the `./` form Agent Plugins 1.0 gives `command` and
/// `cwd`, which a harness would otherwise resolve against a directory of
/// its own choosing. Its command is then named as a harness starting it
/// directly reaches it (`npx` through `cmd /c` on Windows, where it is a
/// batch launcher).
fn resolve_server(server: &serde_json::Value, package_root: &PluginRoot) -> serde_json::Value {
    let mut server = resolve_json(server, package_root);
    if let Some(entries) = server.as_object_mut() {
        for key in ["command", "cwd"] {
            if let Some(serde_json::Value::String(value)) = entries.get_mut(key)
                && let Some(relative) = value.strip_prefix("./")
            {
                *value = package_root.join(relative).to_string_lossy().into_owned();
            }
        }
        launched_directly(entries);
    }
    server
}

/// Rewrites a server's `command` and `args` into the form
/// [`uze_platform::executable::direct_launch`] gives them. A server whose
/// arguments are not all strings is left as written: it is refused as
/// undeliverable before it would run.
fn launched_directly(server: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(command) = server.get("command").and_then(serde_json::Value::as_str) else {
        return;
    };
    let arguments = match server.get("args") {
        None | Some(serde_json::Value::Null) => Some(Vec::new()),
        Some(serde_json::Value::Array(arguments)) => arguments
            .iter()
            .map(|argument| argument.as_str().map(str::to_owned))
            .collect(),
        Some(_) => None,
    };
    let Some(arguments) = arguments else {
        return;
    };
    let had_arguments = server.contains_key("args");
    let (command, arguments) = uze_platform::executable::direct_launch(command, arguments);
    server.insert("command".to_owned(), serde_json::json!(command));
    if had_arguments || !arguments.is_empty() {
        server.insert("args".to_owned(), serde_json::json!(arguments));
    }
}

/// `{"command": "...", "args": [...]}` from one server's canonical config
/// object, as MCP resource discovery extracts it from `mcp.json`, with the
/// package root resolved to `package_root`. `None` without a usable `command`, or with an
/// argument that is not a string: an entry that runs something other than
/// what the author declared is not a delivery of it.
pub(crate) fn stdio_command(
    payload: &[u8],
    package_root: &PluginRoot,
) -> Option<(PathBuf, Vec<String>)> {
    let value = resolve_server(&uze_core::authored::json(payload).ok()?, package_root);
    let command = value.get("command")?.as_str()?;
    let args = match value.get("args") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(args) => args
            .as_array()?
            .iter()
            .map(|arg| arg.as_str().map(str::to_owned))
            .collect::<Option<_>>()?,
    };
    Some((PathBuf::from(command), args))
}

/// How the stdio server `server` of `package` is started, exactly as a
/// harness is handed it: its command and arguments with the package root
/// resolved, or why it is not one a harness would be handed
/// (`extension-mcp` D3).
pub fn stdio_server_launch(
    package: &StoredPackage,
    server: &str,
) -> std::result::Result<(PathBuf, Vec<String>), String> {
    let plugin = package.id.as_str();
    let bytes = fs::read(package.root.join("mcp.json"))
        .map_err(|_| format!("{plugin} delivers no MCP server"))?;
    let servers = uze_core::authored::json::<serde_json::Value>(&bytes)
        .map_err(|error| format!("{plugin}'s mcp.json is not read: {error}"))?;
    let declared = servers
        .get("mcpServers")
        .and_then(|servers| servers.get(server))
        .ok_or_else(|| format!("{plugin} delivers no MCP server `{server}`"))?;
    let payload = serde_json::to_vec(declared).expect("a JSON value serializes");
    let transport = declared.get("type").and_then(serde_json::Value::as_str);
    if !matches!(transport, None | Some("stdio")) {
        return Err(format!(
            "`{server}` is a `{}` server; only a stdio server is started",
            transport.unwrap_or_default()
        ));
    }
    if let Some(why) = undeliverable(&payload) {
        return Err(why.to_owned());
    }
    // UZE starts this server itself: `${PLUGIN_ROOT}` names its own copy of
    // the package, never a harness's build.
    let root = uze_core::home::UzeHome::installed_package_of(&package.root)
        .map(|(home, id)| home.delivered_package_dir(&id))
        .unwrap_or_else(|| package.root.clone());
    stdio_command(&payload, &PluginRoot::whole(root))
        .ok_or_else(|| format!("`{server}` declares no command a harness could start"))
}

/// What a server declares that the managed stdio entry has no place for.
/// The entry carries a command and its arguments; a server that also needs
/// an environment or a working directory would start without them.
fn undeliverable(payload: &[u8]) -> Option<&'static str> {
    let value = uze_core::authored::json::<serde_json::Value>(payload).ok()?;
    let present = |key: &str| {
        value.get(key).is_some_and(|declared| match declared {
            serde_json::Value::Null => false,
            serde_json::Value::Object(entries) => !entries.is_empty(),
            _ => true,
        })
    };
    if present("env") {
        Some(
            "mcp.json server declares `env`, which the managed entry cannot carry; delivering it would start the server without its environment.",
        )
    } else if present("cwd") {
        Some(
            "mcp.json server declares `cwd`, which the managed entry cannot carry; delivering it would start the server in the wrong directory.",
        )
    } else if value
        .get("args")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|args| args.iter().any(|arg| !arg.is_string()))
    {
        Some(
            "mcp.json server declares an argument that is not a string, which the managed entry cannot carry faithfully.",
        )
    } else {
        None
    }
}

/// The managed stdio entry for `resource` under `entry_name`: `None` when
/// its payload carries no usable command, an Unsupported plan when it
/// declares something the entry cannot carry.
pub(crate) fn managed_stdio_plan(
    resource: &Resource,
    root: &PluginRoot,
    entry_name: String,
    route: CompatibilityRoute,
    enabled: Option<bool>,
    evidence: &str,
) -> Option<ExposurePlan> {
    if let Some(reason) = undeliverable(&resource.capability.payload) {
        return Some(unsupported(reason));
    }
    let (command, args) = stdio_command(&resource.capability.payload, root)?;
    Some(ExposurePlan {
        route,
        mechanism: ExposureMechanism::Managed(ManagedArtifact::VendorConfigEntry {
            entry_name,
            transport: "stdio".to_owned(),
            command,
            args,
            cwd: None,
            environment: Vec::new(),
            enabled,
        }),
        evidence: evidence.to_owned(),
    })
}

/// The answer for a server the vendor already knows by the planned name:
/// one that inspects as exactly the planned entry is this attach done
/// already, and anything else is somebody else's server, which neither an
/// add-or-update verb nor a receipt may claim.
pub(crate) fn claim_existing(existing: AttachmentInspection, config: &Path) -> Result<()> {
    if existing.state == AttachmentState::Matched {
        Ok(())
    } else {
        Err(UzeError::ManagedEntryConflict(config.to_path_buf()))
    }
}

/// A managed stdio server as its receipt records it — the one shape every
/// harness's MCP inspection reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct McpEntry<'a> {
    pub name: &'a str,
    pub transport: &'a str,
    pub command: &'a Path,
    pub args: &'a [String],
    pub cwd: Option<&'a Path>,
    pub environment: &'a [McpEnvironmentReference],
    pub enabled: Option<bool>,
}

impl<'a> McpEntry<'a> {
    /// The entry a `VendorConfigEntry` receipt records; `None` for any
    /// other artifact.
    pub(crate) fn recorded(artifact: &'a ManagedArtifact) -> Option<Self> {
        let ManagedArtifact::VendorConfigEntry {
            entry_name,
            transport,
            command,
            args,
            cwd,
            environment,
            enabled,
        } = artifact
        else {
            return None;
        };
        Some(Self {
            name: entry_name,
            transport,
            command,
            args,
            cwd: cwd.as_deref(),
            environment,
            enabled: *enabled,
        })
    }

    /// What an attach plans ([`managed_stdio_plan`]): stdio, a command and
    /// its arguments, nothing else.
    pub(crate) fn planned(name: &'a str, command: &'a Path, args: &'a [String]) -> Self {
        Self {
            name,
            transport: "stdio",
            command,
            args,
            cwd: None,
            environment: &[],
            enabled: None,
        }
    }

    /// Whether the receipt asks for nothing but a stdio command and its
    /// arguments — all a vendor entry read by command and args can prove.
    pub(crate) fn is_plain_stdio(&self) -> bool {
        self.transport == "stdio"
            && self.cwd.is_none()
            && self.environment.is_empty()
            && self.enabled.is_none()
    }

    /// Whether a vendor's `{"command": ..., "args": [...]}` object runs
    /// this entry's command with its arguments. Absent `args` is none.
    pub(crate) fn runs_as(&self, server: &serde_json::Value) -> bool {
        let command = server.get("command").and_then(serde_json::Value::as_str);
        let args: Vec<&str> = server
            .get("args")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect()
            })
            .unwrap_or_default();
        command == Some(self.command.to_string_lossy().as_ref())
            && args == self.args.iter().map(String::as_str).collect::<Vec<_>>()
    }
}

/// Whether the vendor already knows a server by this name (`mcp get`).
pub(crate) fn cli_exists(executable: &Path, home: &Path, entry_name: &str) -> bool {
    succeeds(executable, home, &["mcp", "get", entry_name])
}

/// Registers `entry_name` through `<add_verb> <entry_name> -- <command>
/// [args...]`. The caller settles a name the vendor already knows first
/// ([`claim_existing`]): neither vendor's overwrite behavior for a
/// colliding, differently-configured name was confirmed, so UZE never
/// relies on it (ADR-007).
pub(crate) fn cli_add(
    executable: &Path,
    home: &Path,
    vendor: &str,
    add_verb: &[&str],
    entry_name: &str,
    command: &Path,
    args: &[String],
) -> Result<()> {
    let arguments: Vec<OsString> = add_verb
        .iter()
        .map(OsString::from)
        .chain([OsString::from(entry_name), OsString::from("--")])
        .chain(std::iter::once(command.as_os_str().to_owned()))
        .chain(args.iter().map(OsString::from))
        .collect();
    let output = capture(executable, home, &arguments).map_err(|error| {
        UzeError::HarnessCommand(format!(
            "failed to run `{vendor} mcp add` for entry `{entry_name}`: {error}"
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(UzeError::HarnessCommand(failed_message(
        &format!("{vendor} mcp add `{entry_name}`"),
        &output,
    )))
}

/// Removes `entry_name` through `mcp remove`. An entry already absent is not
/// an error — removal is idempotent — and a flag-shaped name is refused
/// before any process is spawned.
pub(crate) fn cli_remove(
    executable: &Path,
    home: &Path,
    vendor: &str,
    entry_name: &str,
) -> Result<()> {
    if !is_cli_safe_token(entry_name) {
        return Err(UzeError::ExposureUnavailable(format!(
            "MCP server name `{entry_name}` would be parsed as a flag by `{vendor} mcp remove`, not a name; refusing to detach."
        )));
    }
    let output = capture(executable, home, &["mcp", "remove", entry_name]).map_err(|error| {
        UzeError::HarnessCommand(format!(
            "failed to run `{vendor} mcp remove` for entry `{entry_name}`: {error}"
        ))
    })?;
    if output.status.success() || !cli_exists(executable, home, entry_name) {
        return Ok(());
    }
    Err(UzeError::HarnessCommand(failed_message(
        &format!("{vendor} mcp remove `{entry_name}`"),
        &output,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uze_core::home::UzeHome;

    #[test]
    fn the_package_root_resolves_in_every_string_a_server_carries_to_the_build() {
        let home = UzeHome::at("/home/u/.uze");
        let id = uze_core::store::PackageId::from_marketplace_plugin(
            "mk",
            "pm",
            Path::new("plugin.json"),
        )
        .unwrap();
        let sources = home.plugin_dir(&id);
        let declared = serde_json::json!({
            "command": "${PLUGIN_ROOT}/bin/server",
            "args": ["--data", "${PLUGIN_ROOT}/data", 3],
            "env": { "HOME_OF": "${PLUGIN_ROOT}" },
        });
        // The placeholder becomes the harness's build, spelled as this
        // platform spells it; the rest of each string is the author's. A
        // `bin/` Claude would put on its `PATH` is carried, not at the root.
        let build = crate::claude::STORE.root_of(&sources);
        assert_eq!(build.path(), home.build_dir("claude", "pm@mk"));
        let shown = build.path().display();
        assert_eq!(
            resolve_json(&declared, &build),
            serde_json::json!({
                "command": format!("{shown}/.uze-carried/bin/server"),
                "args": ["--data", format!("{shown}/data"), 3],
                "env": { "HOME_OF": shown.to_string() },
            })
        );
    }

    #[test]
    fn an_agent_plugins_relative_command_runs_from_the_package() {
        let root = Path::new("/store/claude/pm@mk");
        let declared = br#"{"type":"stdio","command":"./bin/server","args":["./not-a-path"]}"#;
        assert_eq!(
            stdio_command(declared, &PluginRoot::whole(root)),
            Some((root.join("bin/server"), vec!["./not-a-path".to_owned()]))
        );
        assert_eq!(
            resolve_server(
                &serde_json::json!({"command": "./bin/s", "cwd": "./data"}),
                &PluginRoot::whole(root)
            ),
            serde_json::json!({
                "command": root.join("bin/s"),
                "cwd": root.join("data"),
            })
        );
    }

    #[test]
    fn a_server_the_entry_cannot_carry_is_unsupported_not_trimmed() {
        for declared in [
            r#"{"command":"server","env":{"TOKEN":"x"}}"#,
            r#"{"command":"server","cwd":"/srv"}"#,
            r#"{"command":"server","args":["--port",8080]}"#,
        ] {
            assert!(undeliverable(declared.as_bytes()).is_some(), "{declared}");
            assert_eq!(
                stdio_command(declared.as_bytes(), &PluginRoot::whole("/store/pm")).is_some(),
                !declared.contains("8080")
            );
        }
        for declared in [
            r#"{"command":"server"}"#,
            r#"{"command":"server","env":{},"args":["--serve"]}"#,
        ] {
            assert_eq!(undeliverable(declared.as_bytes()), None, "{declared}");
        }
    }

    #[test]
    fn only_the_planned_entry_is_claimed() {
        let config = Path::new("/home/.vendor.json");
        let inspection = |state| AttachmentInspection {
            state,
            reason: String::new(),
        };
        assert!(claim_existing(inspection(AttachmentState::Matched), config).is_ok());
        for state in [
            AttachmentState::Drifted,
            AttachmentState::Missing,
            AttachmentState::Conflict,
            AttachmentState::Blocked,
        ] {
            assert!(matches!(
                claim_existing(inspection(state), config),
                Err(UzeError::ManagedEntryConflict(_))
            ));
        }
    }

    #[test]
    fn a_managed_entry_runs_the_resolved_command() {
        let (command, args) = stdio_command(
            br#"{"command":"python3","args":["${PLUGIN_ROOT}/scripts/server.py"]}"#,
            &PluginRoot::whole("/store/pm"),
        )
        .unwrap();
        assert_eq!(command, PathBuf::from("python3"));
        assert_eq!(args, vec!["/store/pm/scripts/server.py".to_owned()]);
    }

    /// A server whose command is a launcher the package carries for both
    /// platforms (a script, and a batch file on Windows) runs when started
    /// the way a harness starts it: directly, by the entry's command.
    #[test]
    fn a_launcher_server_runs_when_started_directly() {
        let root = uze_testkit::temp::scratch("mcp-launcher");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        uze_testkit::process::install_executable(
            &root.join("bin").join("serve"),
            b"#!/bin/sh\nexit \"$1\"\n",
        );
        std::fs::write(root.join("bin").join("serve.cmd"), "@exit /b %1\r\n").unwrap();
        let (command, args) = stdio_command(
            br#"{"command":"./bin/serve","args":["6"]}"#,
            &PluginRoot::whole(&root),
        )
        .unwrap();
        let status = std::process::Command::new(command)
            .args(args)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(6));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_extension_starts_a_server_as_a_harness_is_handed_it_or_says_why_not() {
        let scratch = uze_testkit::temp::TempDir::new("mcp-launch");
        let root = scratch.path().to_path_buf();
        std::fs::write(
            root.join("mcp.json"),
            r#"{"mcpServers":{
                "rtc":{"type":"stdio","command":"bun","args":["run","${PLUGIN_ROOT}/mcp/rtc/src/index.ts"]},
                "remote":{"type":"http","url":"https://example.com/mcp"},
                "tokened":{"command":"server","env":{"TOKEN":"x"}}
            }}"#,
        )
        .unwrap();
        let id =
            uze_core::store::PackageId::from_plugin_name("std", &root.join("plugin.json")).unwrap();
        let package = StoredPackage {
            active_name: id.plugin_name().to_owned(),
            id,
            root: root.clone(),
            manifest: root.join("plugin.json"),
            provenance: uze_core::acquisition::Provenance {
                requested: uze_core::acquisition::PackageSource::Local { path: root.clone() },
                resolved: uze_core::acquisition::ResolvedSource::Local { path: root.clone() },
            },
        };
        let (command, args) = stdio_server_launch(&package, "rtc").unwrap();
        assert_eq!(
            (command, args),
            stdio_command(
                br#"{"command":"bun","args":["run","${PLUGIN_ROOT}/mcp/rtc/src/index.ts"]}"#,
                &PluginRoot::whole(&root)
            )
            .unwrap()
        );
        assert!(
            stdio_server_launch(&package, "remote")
                .unwrap_err()
                .contains("only a stdio server")
        );
        assert!(
            stdio_server_launch(&package, "tokened")
                .unwrap_err()
                .contains("`env`")
        );
        assert!(
            stdio_server_launch(&package, "absent")
                .unwrap_err()
                .contains("no MCP server `absent`")
        );
    }
}
