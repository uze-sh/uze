//! Codex MCP server registration and inspection — the `codex mcp <verb>`
//! CLI surface. No `--scope` flag exists for Codex; global is the only
//! destination.

use std::path::Path;

use uze_core::{
    Result,
    capability::Resource,
    exposure::ExposurePlan,
    integration::{AttachmentInspection, AttachmentState, IntegrationPort},
    router::CompatibilityRoute,
    state,
};

use super::CodexIntegration;
use crate::shared::mcp::{
    McpEntry, claim_existing, cli_add, cli_exists, cli_remove, managed_stdio_plan,
};
use crate::shared::plan::{blocked, unsupported};
use crate::shared::process::{capture, is_cli_safe_token};

impl CodexIntegration {
    pub(super) fn mcp_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        if !state::is_installed(&self.uze_home, self.id()) {
            return unsupported(
                "Codex has not completed `uze setup`; run `uze setup` so UZE can attach this MCP server (see ADR-007).",
            );
        }
        let Some(entry_name) = resource
            .resolved_exposure_name
            .clone()
            .or_else(|| self.exposure_name_candidates(resource).into_iter().next())
        else {
            return unsupported("Resource has no derivable attachment entry name.");
        };
        if !is_cli_safe_token(&entry_name) {
            return unsupported(
                "MCP server name would be parsed as a flag by `codex mcp add`, not a name; refusing to attach.",
            );
        }
        managed_stdio_plan(
            resource,
            &super::STORE.root_of(&resource.package_root),
            entry_name,
            CompatibilityRoute::Adaptable,
            None,
            "UZE registers the store-owned MCP server once via `codex mcp add`, writing to ~/.codex/config.toml's [mcp_servers.*] (no --scope flag exists; global is the only destination). Available to every future session in any project.",
        )
        .unwrap_or_else(|| unsupported("mcp.json server entry is missing a usable `command` field."))
    }
}

/// Registers the server globally — Codex has no scope flag. A name Codex
/// already knows is claimed only when it is exactly the planned entry.
pub(super) fn attach_mcp_entry(
    executable: &Path,
    command_home: &Path,
    entry_name: &str,
    command: &Path,
    args: &[String],
) -> Result<()> {
    if cli_exists(executable, command_home, entry_name) {
        return claim_existing(
            inspect_codex_mcp(
                executable,
                command_home,
                &McpEntry::planned(entry_name, command, args),
            ),
            &command_home.join(".codex").join("config.toml"),
        );
    }
    cli_add(
        executable,
        command_home,
        "codex",
        &["mcp", "add"],
        entry_name,
        command,
        args,
    )
}

/// Inspects `codex mcp get --json`, the documented structured Codex surface.
/// No TOML is read or written by UZE; an unavailable or malformed response is
/// deliberately BLOCKED rather than interpreted as absence.
pub(super) fn inspect_codex_mcp(
    executable: &Path,
    command_home: &Path,
    entry: &McpEntry,
) -> AttachmentInspection {
    let entry_name = entry.name;
    let output = match capture(
        executable,
        command_home,
        &["mcp", "get", entry_name, "--json"],
    ) {
        Ok(output) => output,
        Err(error) => {
            return blocked(format!("failed to run `codex mcp get --json`: {error}"));
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Current Codex identifies an absent name with exit 1 and this
        // stable diagnostic. Any other non-zero result is not positive
        // absence evidence, so removal must remain blocked.
        if output.status.code() == Some(1)
            && stderr.contains("No MCP server named")
            && stderr.contains(entry_name)
        {
            return AttachmentInspection {
                state: AttachmentState::Missing,
                reason: "Codex MCP entry is absent".to_owned(),
            };
        }
        return blocked(format!(
            "`codex mcp get --json` could not verify entry: {}",
            stderr.trim()
        ));
    }
    let value = match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
        Ok(value) => value,
        Err(error) => return blocked(format!("Codex MCP JSON is invalid: {error}")),
    };
    inspect_codex_mcp_value(&value, entry)
}

pub(super) fn inspect_codex_mcp_value(
    value: &serde_json::Value,
    entry: &McpEntry,
) -> AttachmentInspection {
    let McpEntry {
        name: entry_name,
        transport: expected_transport,
        command,
        args,
        cwd: expected_cwd,
        environment: expected_environment,
        enabled: expected_enabled,
    } = *entry;
    let object = value
        .as_object()
        .or_else(|| value.get("server")?.as_object());
    let Some(object) = object else {
        return blocked("Codex MCP JSON has no server object");
    };
    if let Some(name) = object
        .get("name")
        .or_else(|| object.get("id"))
        .and_then(serde_json::Value::as_str)
        && name != entry_name
    {
        return AttachmentInspection {
            state: AttachmentState::Conflict,
            reason: "Codex MCP JSON identifies a different entry".to_owned(),
        };
    }
    if expected_enabled.is_some()
        && object.get("enabled").and_then(serde_json::Value::as_bool) != expected_enabled
    {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Codex MCP entry is disabled".to_owned(),
        };
    }
    let transport = object
        .get("transport")
        .and_then(serde_json::Value::as_object)
        .unwrap_or(object);
    if transport
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|actual| actual != expected_transport)
    {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Codex MCP transport differs from stdio receipt".to_owned(),
        };
    }
    let Some(actual_command) = transport.get("command").and_then(serde_json::Value::as_str) else {
        return blocked("Codex MCP JSON has no stdio command");
    };
    let Some(actual_args) = transport.get("args").and_then(serde_json::Value::as_array) else {
        return blocked("Codex MCP JSON has no args array");
    };
    let actual_args = actual_args
        .iter()
        .map(serde_json::Value::as_str)
        .collect::<Option<Vec<_>>>();
    let Some(actual_args) = actual_args else {
        return blocked("Codex MCP JSON args are not strings");
    };
    if let Some(expected_cwd) = expected_cwd
        && transport.get("cwd").and_then(serde_json::Value::as_str)
            != Some(expected_cwd.to_string_lossy().as_ref())
    {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Codex MCP cwd differs from receipt".to_owned(),
        };
    }
    if !expected_environment.is_empty() {
        let actual = transport
            .get("env_vars")
            .and_then(serde_json::Value::as_array)
            .and_then(|values| {
                values
                    .iter()
                    .map(serde_json::Value::as_str)
                    .collect::<Option<Vec<_>>>()
            });
        let expected = expected_environment
            .iter()
            .map(|reference| reference.name.as_str())
            .collect::<Vec<_>>();
        if actual.as_deref() != Some(expected.as_slice()) {
            return AttachmentInspection {
                state: AttachmentState::Drifted,
                reason: "Codex MCP environment references differ from receipt".to_owned(),
            };
        }
    }
    if actual_command == command.to_string_lossy() && actual_args == args {
        AttachmentInspection {
            state: AttachmentState::Matched,
            reason: "Codex MCP entry matches receipt".to_owned(),
        }
    } else {
        AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Codex MCP command or args differ from receipt".to_owned(),
        }
    }
}

/// Removes a UZE-registered MCP entry. Wired to the remove lifecycle
/// (`detach_receipt`) and exercised directly by `tests/integrations/
/// contract.rs`; `command_home` is always set as `HOME`, never inherited.
pub fn detach_mcp_entry(executable: &Path, command_home: &Path, entry_name: &str) -> Result<()> {
    cli_remove(executable, command_home, "codex", entry_name)
}

#[cfg(test)]
mod mcp_tests {
    use std::path::{Path, PathBuf};

    use uze_core::integration::AttachmentState;

    use super::{McpEntry, attach_mcp_entry, inspect_codex_mcp, inspect_codex_mcp_value};

    /// A `codex` that logs every invocation under its `HOME` and answers
    /// `mcp get` with `stdout`, `stderr` and `code`.
    fn fake_codex(label: &str, stdout: &str, stderr: &str, code: u8) -> (PathBuf, PathBuf) {
        let home = uze_testkit::temp::scratch(label);
        std::fs::create_dir_all(&home).unwrap();
        let codex = home.join("codex");
        uze_testkit::process::install_executable(
            &codex,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/codex.log\"\nif [ \"$1 $2\" = \"mcp get\" ]; then\n  printf '%s' '{stdout}'\n  printf '%s' '{stderr}' >&2\n  exit {code}\nfi\nexit 0\n"
            )
            .as_bytes(),
        );
        (codex, home)
    }

    fn invocations(home: &Path) -> Vec<String> {
        std::fs::read_to_string(home.join("codex.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn inspected(label: &str, stdout: &str, stderr: &str, code: u8) -> AttachmentState {
        let (codex, home) = fake_codex(label, stdout, stderr, code);
        let command = PathBuf::from("/bin/serve");
        let args = vec!["--x".to_owned()];
        let state =
            inspect_codex_mcp(&codex, &home, &McpEntry::planned("uze-x", &command, &args)).state;
        let _ = std::fs::remove_dir_all(home);
        state
    }

    /// Codex says a name is absent with exit 1 and its "No MCP server
    /// named" diagnostic naming it. Nothing else is evidence of absence, so
    /// any other failure stays Blocked and a removal never drops a receipt
    /// whose server may still be registered.
    // Unix: the stand-in harness is a `#!/bin/sh` script.
    #[cfg(unix)]
    #[test]
    fn only_codexs_own_absence_answer_reads_as_missing() {
        let planned = r#"{"name":"uze-x","command":"/bin/serve","args":["--x"]}"#;
        assert_eq!(
            inspected("codex-mcp-get-match", planned, "", 0),
            AttachmentState::Matched
        );
        assert_eq!(
            inspected(
                "codex-mcp-get-absent",
                "",
                "Error: No MCP server named 'uze-x' found.",
                1
            ),
            AttachmentState::Missing
        );
        for (label, stderr, code) in [
            ("codex-mcp-get-invalid", "error: config.toml invalid", 1),
            ("codex-mcp-get-names-it", "error: could not parse uze-x", 1),
            (
                "codex-mcp-get-other-code",
                "Error: No MCP server named 'uze-x' found.",
                2,
            ),
        ] {
            assert_eq!(
                inspected(label, "", stderr, code),
                AttachmentState::Blocked,
                "{stderr} (exit {code})"
            );
        }
    }

    // Unix: the stand-in harness is a `#!/bin/sh` script.
    #[cfg(unix)]
    #[test]
    fn a_server_codex_does_not_know_is_added_with_its_command() {
        let (codex, home) = fake_codex(
            "codex-mcp-attach",
            "",
            "Error: No MCP server named 'uze-x' found.",
            1,
        );
        attach_mcp_entry(
            &codex,
            &home,
            "uze-x",
            Path::new("/bin/serve"),
            &["--x".to_owned()],
        )
        .unwrap();
        assert!(
            invocations(&home).contains(&"mcp add uze-x -- /bin/serve --x".to_owned()),
            "{:?}",
            invocations(&home)
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// A receipt that records whether the server is enabled, where it runs
    /// and which environment it names matches only the entry carrying the
    /// same, and drifts from one that differs in any of them.
    #[test]
    fn a_recorded_enabled_state_cwd_and_environment_decide_the_match() {
        let command = PathBuf::from("/bin/example");
        let args = vec!["--serve".to_owned()];
        let cwd = PathBuf::from("/srv/example");
        let environment = [uze_core::exposure::McpEnvironmentReference {
            name: "TOKEN".to_owned(),
        }];
        let entry = McpEntry {
            cwd: Some(&cwd),
            environment: &environment,
            enabled: Some(true),
            ..McpEntry::planned("uze-example", &command, &args)
        };
        let exact = serde_json::json!({
            "name": "uze-example", "enabled": true,
            "transport": {"type": "stdio", "command": "/bin/example", "args": ["--serve"],
                          "cwd": "/srv/example", "env_vars": ["TOKEN"]}
        });
        assert_eq!(
            inspect_codex_mcp_value(&exact, &entry).state,
            AttachmentState::Matched
        );
        for (pointer, value) in [
            ("/enabled", serde_json::json!(false)),
            ("/transport/cwd", serde_json::json!("/elsewhere")),
            ("/transport/env_vars", serde_json::json!(["OTHER"])),
        ] {
            let mut differing = exact.clone();
            *differing.pointer_mut(pointer).unwrap() = value;
            assert_eq!(
                inspect_codex_mcp_value(&differing, &entry).state,
                AttachmentState::Drifted,
                "{pointer}"
            );
        }
    }

    #[test]
    fn structured_mcp_inspection_distinguishes_match_drift_and_conflict() {
        let expected_command = PathBuf::from("/bin/example");
        let expected_args = vec!["--serve".to_owned()];
        let exact = serde_json::json!({
            "name": "uze-example",
            "command": "/bin/example",
            "args": ["--serve"],
            "unrelated": {"future": true}
        });
        assert_eq!(
            inspect_codex_mcp_value(
                &exact,
                &McpEntry::planned("uze-example", &expected_command, &expected_args),
            )
            .state,
            AttachmentState::Matched
        );
        let official_shape = serde_json::json!({
            "name": "uze-example", "enabled": true,
            "transport": {"type":"stdio", "command":"/bin/example", "args":["--serve"], "env":null}
        });
        assert_eq!(
            inspect_codex_mcp_value(
                &official_shape,
                &McpEntry::planned("uze-example", &expected_command, &expected_args),
            )
            .state,
            AttachmentState::Matched
        );
        let changed =
            serde_json::json!({"name":"uze-example", "command":"/bin/changed", "args":["--serve"]});
        assert_eq!(
            inspect_codex_mcp_value(
                &changed,
                &McpEntry::planned("uze-example", &expected_command, &expected_args),
            )
            .state,
            AttachmentState::Drifted
        );
        let foreign =
            serde_json::json!({"name":"foreign", "command":"/bin/example", "args":["--serve"]});
        assert_eq!(
            inspect_codex_mcp_value(
                &foreign,
                &McpEntry::planned("uze-example", &expected_command, &expected_args),
            )
            .state,
            AttachmentState::Conflict
        );
        assert_eq!(
            inspect_codex_mcp_value(
                &serde_json::json!({}),
                &McpEntry::planned("uze-example", &expected_command, &expected_args),
            )
            .state,
            AttachmentState::Blocked
        );
    }
}
