//! Claude Code MCP server registration, inspection, and detachment — the
//! `claude mcp <verb>` CLI surface, plus `~/.claude.json`'s `mcpServers`
//! read path used for read-only inspection.

use std::path::Path;

use uze_core::{
    Result,
    capability::Resource,
    exposure::ExposurePlan,
    integration::{AttachmentInspection, AttachmentState, IntegrationPort},
    router::CompatibilityRoute,
    state,
};

use super::ClaudeIntegration;
use crate::shared::json_config;
use crate::shared::mcp::{
    McpEntry, claim_existing, cli_add, cli_exists, cli_remove, managed_stdio_plan,
};
use crate::shared::plan::{blocked, unsupported};
use crate::shared::process::is_cli_safe_token;

impl ClaudeIntegration {
    pub(super) fn mcp_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        if !state::is_installed(&self.uze_home, self.id()) {
            return unsupported(
                "Claude Code has not completed `uze setup`; run `uze setup` so UZE can attach this MCP server (see ADR-007).",
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
                "MCP server name would be parsed as a flag by `claude mcp add`, not a name; refusing to attach.",
            );
        }
        managed_stdio_plan(
            resource,
            entry_name,
            CompatibilityRoute::Adaptable,
            None,
            "UZE registers the store-owned MCP server once via `claude mcp add --scope user --transport stdio`, writing to ~/.claude.json's mcpServers. Available to every future session in any project with no --plugin-dir-style flag.",
        )
        .unwrap_or_else(|| unsupported("mcp.json server entry is missing a usable `command` field."))
    }
}

/// Registers the server at user scope (`--scope user`), where every future
/// session in any project reads it. A name Claude already knows, in any
/// scope, is claimed only when it is exactly the planned user-scope entry.
pub(super) fn attach_mcp_entry(
    executable: &Path,
    command_home: &Path,
    entry_name: &str,
    command: &Path,
    args: &[String],
) -> Result<()> {
    if cli_exists(executable, command_home, entry_name) {
        let config = command_home.join(".claude.json");
        return claim_existing(
            inspect_claude_mcp(&config, &McpEntry::planned(entry_name, command, args)),
            &config,
        );
    }
    cli_add(
        executable,
        command_home,
        "claude",
        &["mcp", "add", "--scope", "user", "--transport", "stdio"],
        entry_name,
        command,
        args,
    )
}

/// Claude has no structured `mcp get` output. This is deliberately read-only:
/// attachment/removal still go through the official CLI, while inspection
/// reads only the one expected `mcpServers.<name>` entry.
pub(super) fn inspect_claude_mcp(path: &Path, entry: &McpEntry) -> AttachmentInspection {
    if !entry.is_plain_stdio() {
        return blocked("Claude MCP receipt requests state this integration cannot verify safely");
    }
    let config = match json_config::read_object(path) {
        Ok(config) => config,
        Err(reason) => return blocked(reason),
    };
    let Some(server) = json_config::get_path(&config, &["mcpServers", entry.name]) else {
        return AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "Claude MCP entry is missing".to_owned(),
        };
    };
    if entry.runs_as(server) {
        AttachmentInspection {
            state: AttachmentState::Matched,
            reason: "Claude MCP entry matches receipt".to_owned(),
        }
    } else {
        AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Claude MCP command or args differ from receipt".to_owned(),
        }
    }
}

/// Removes a UZE-registered MCP entry. Wired to the remove lifecycle
/// (`detach_receipt`) and exercised directly by `tests/integrations/
/// contract.rs`; `command_home` is always set as `HOME`, never inherited.
pub fn detach_mcp_entry(executable: &Path, command_home: &Path, entry_name: &str) -> Result<()> {
    cli_remove(executable, command_home, "claude", entry_name)
}

// Its stand-in programs are POSIX shell scripts.
#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use uze_core::UzeError;

    use super::attach_mcp_entry;

    /// `claude mcp get` knows every name; the log says whether `mcp add`
    /// ever ran.
    fn knowing_claude(home: &Path) -> std::path::PathBuf {
        let executable = home.join("claude");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\n[ \"$2\" = add ] && echo added >> '{}'\nexit 0\n",
                home.join("calls").display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        executable
    }

    #[test]
    fn a_name_claude_already_knows_is_claimed_only_when_it_is_the_planned_entry() {
        let home = uze_testkit::temp::scratch("claude-mcp-existing");
        fs::create_dir_all(&home).unwrap();
        let claude = knowing_claude(&home);
        let args = ["--serve".to_owned()];
        let command = Path::new("/bin/server");

        fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{"uze-x":{"command":"/bin/server","args":["--serve"]}}}"#,
        )
        .unwrap();
        attach_mcp_entry(&claude, &home, "uze-x", command, &args).unwrap();

        fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{"uze-x":{"command":"/bin/foreign","args":[]}}}"#,
        )
        .unwrap();
        assert!(matches!(
            attach_mcp_entry(&claude, &home, "uze-x", command, &args),
            Err(UzeError::ManagedEntryConflict(_))
        ));
        assert!(
            !home.join("calls").exists(),
            "an existing name is never re-added over"
        );
        let _ = fs::remove_dir_all(home);
    }
}
