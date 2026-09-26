//! Antigravity CLI MCP server registration and inspection — `agy mcp add
//! <name> <command> [args...]` (global scope; flags must precede the name
//! in 1.1.19), with `agy mcp list` being human-readable only, so inspection
//! reads the one expected `mcpServers.<name>` entry directly out of
//! `~/.gemini/config/mcp_config.json` — the vendor's dedicated, sparse
//! MCP profile (Antigravity separately MCP from settings.json; legacy
//! inline declarations are gone).

use std::{path::Path, path::PathBuf};

use uze_core::{
    Result, UzeError,
    capability::Resource,
    exposure::ExposurePlan,
    integration::{AttachmentInspection, AttachmentState, IntegrationPort},
    router::CompatibilityRoute,
    state,
};

use super::AntigravityIntegration;
use crate::shared::json_config;
use crate::shared::mcp::{McpEntry, claim_existing, managed_stdio_plan};
use crate::shared::plan::{blocked, unsupported};
use crate::shared::process::{capture, failed_message, is_cli_safe_token};

impl AntigravityIntegration {
    pub(super) fn mcp_exposure_plan(&self, resource: &Resource) -> ExposurePlan {
        let Some(entry_name) = resource
            .resolved_exposure_name
            .clone()
            .or_else(|| self.exposure_name_candidates(resource).into_iter().next())
        else {
            return unsupported("MCP resource has no derivable entry name.");
        };
        if !is_cli_safe_token(&entry_name) {
            return unsupported(
                "MCP server name would be parsed as a flag by `agy mcp add`, not a name; refusing to attach.",
            );
        }
        if !state::is_installed(&self.uze_home, self.id()) {
            return unsupported(
                "Antigravity setup has not completed, so no managed MCP entry exists yet.",
            );
        }
        managed_stdio_plan(
            resource,
            entry_name,
            CompatibilityRoute::Adaptable,
            None,
            "UZE registers the store-owned MCP server once via `agy mcp add <name> <command> [args...]`, writing to ~/.gemini/config/mcp_config.json's mcpServers. The Antigravity MCP runtime remains native.",
        )
        .unwrap_or_else(|| {
            unsupported("Antigravity MCP attachment is only modeled for a stdio command/args server.")
        })
    }
}

pub(super) fn attach_mcp_entry(
    executable: &str,
    command_home: &Path,
    entry_name: &str,
    command: &Path,
    args: &[String],
) -> Result<()> {
    // Checked before ever calling `mcp add`: Antigravity's verb is
    // add-or-update (help text: "Add or update an MCP server
    // configuration"), so a colliding, differently-configured name would be
    // silently overwritten — UZE never relies on that (same discipline as
    // ADR-007 for the other peers). Exactly the planned entry is this
    // attach already done.
    let config_path = mcp_config_path(command_home);
    if let Ok(config) = json_config::read_object(&config_path)
        && let Some(existing) = json_config::get_path(&config, &["mcpServers", entry_name])
    {
        return claim_existing(
            inspect_antigravity_mcp_value(existing, &McpEntry::planned(entry_name, command, args)),
            &config_path,
        );
    }
    let mut mcp_args: Vec<std::ffi::OsString> = vec![
        std::ffi::OsString::from("mcp"),
        std::ffi::OsString::from("add"),
        std::ffi::OsString::from(entry_name),
    ];
    mcp_args.push(command.as_os_str().to_owned());
    mcp_args.extend(args.iter().map(std::ffi::OsString::from));
    let output = capture(Path::new(executable), command_home, &mcp_args).map_err(|error| {
        UzeError::HarnessCommand(format!(
            "failed to run `agy mcp add` for entry `{entry_name}`: {error}"
        ))
    })?;
    if !output.status.success() {
        return Err(UzeError::HarnessCommand(failed_message(
            &format!("agy mcp add `{entry_name}`"),
            &output,
        )));
    }
    Ok(())
}

fn mcp_config_path(command_home: &Path) -> PathBuf {
    command_home.join(".gemini/config/mcp_config.json")
}

pub(super) fn inspect_antigravity_mcp(path: &Path, entry: &McpEntry) -> AttachmentInspection {
    if !entry.is_plain_stdio() {
        return blocked(
            "Antigravity MCP receipt requests state this integration cannot verify safely",
        );
    }
    let config = match json_config::read_object(path) {
        Ok(config) => config,
        Err(reason) => return blocked(reason),
    };
    let Some(server) = json_config::get_path(&config, &["mcpServers", entry.name]) else {
        return AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "Antigravity MCP entry is absent".to_owned(),
        };
    };
    inspect_antigravity_mcp_value(server, entry)
}

fn inspect_antigravity_mcp_value(
    entry: &serde_json::Value,
    planned: &McpEntry,
) -> AttachmentInspection {
    if !planned.runs_as(entry) {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "Antigravity MCP command or args differ from receipt".to_owned(),
        };
    }
    // The receipt declares no env or cwd, so any of them present is state
    // UZE did not create and cannot claim.
    for unexpected in ["env", "cwd", "headers"] {
        if entry.get(unexpected).is_some_and(|value| !value.is_null()) {
            return AttachmentInspection {
                state: AttachmentState::Drifted,
                reason: format!("Antigravity MCP entry carries an unexpected `{unexpected}`"),
            };
        }
    }
    // `disabled: true` is a user preference on an entry UZE still owns
    // (same rationale as every vendor's enablement field); a non-boolean
    // `disabled` is state this integration cannot interpret.
    if let Some(disabled) = entry.get("disabled") {
        match disabled.as_bool() {
            Some(_) => {}
            None => {
                return AttachmentInspection {
                    state: AttachmentState::Blocked,
                    reason: "Antigravity MCP entry has a non-boolean `disabled`".to_owned(),
                };
            }
        }
    }
    AttachmentInspection {
        state: AttachmentState::Matched,
        reason: "Antigravity MCP entry matches receipt".to_owned(),
    }
}

#[cfg(test)]
mod mcp_tests {
    use std::{fs, path::Path};

    use uze_core::{UzeError, integration::AttachmentState};

    use super::{McpEntry, attach_mcp_entry, inspect_antigravity_mcp_value, mcp_config_path};

    /// A second install finds its own entry from the first; a differing
    /// one under the same name is somebody else's, and `agy mcp add` would
    /// overwrite it. Neither case reaches the vendor binary, which does not
    /// exist here.
    #[test]
    fn an_existing_entry_is_claimed_only_when_it_is_the_planned_one() {
        let home = uze_testkit::temp::scratch("agy-mcp-existing");
        let config = mcp_config_path(&home);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        let args = ["--serve".to_owned()];
        fs::write(
            &config,
            r#"{"mcpServers":{"uze-x":{"command":"/bin/server","args":["--serve"]}}}"#,
        )
        .unwrap();
        attach_mcp_entry(
            "/nonexistent/agy",
            &home,
            "uze-x",
            Path::new("/bin/server"),
            &args,
        )
        .expect("the planned entry is this attach done already");

        fs::write(
            &config,
            r#"{"mcpServers":{"uze-x":{"command":"/bin/other","args":["--serve"]}}}"#,
        )
        .unwrap();
        assert!(matches!(
            attach_mcp_entry(
                "/nonexistent/agy",
                &home,
                "uze-x",
                Path::new("/bin/server"),
                &args
            ),
            Err(UzeError::ManagedEntryConflict(_))
        ));
        let _ = fs::remove_dir_all(home);
    }

    fn entry(command: &str, args: &[&str]) -> serde_json::Value {
        serde_json::json!({ "command": command, "args": args })
    }

    #[test]
    fn a_matching_mcp_entry_is_matched() {
        assert_eq!(
            inspect_antigravity_mcp_value(
                &entry("/bin/server", &["--serve"]),
                &McpEntry::planned("uze-x", Path::new("/bin/server"), &["--serve".to_owned()]),
            )
            .state,
            AttachmentState::Matched
        );
    }

    #[test]
    fn a_changed_command_or_args_is_drift_not_a_match() {
        assert_eq!(
            inspect_antigravity_mcp_value(
                &entry("/bin/other", &["--serve"]),
                &McpEntry::planned("uze-x", Path::new("/bin/server"), &["--serve".to_owned()]),
            )
            .state,
            AttachmentState::Drifted
        );
        assert_eq!(
            inspect_antigravity_mcp_value(
                &entry("/bin/server", &["--different"]),
                &McpEntry::planned("uze-x", Path::new("/bin/server"), &["--serve".to_owned()]),
            )
            .state,
            AttachmentState::Drifted
        );
    }

    #[test]
    fn state_uze_never_created_is_drift() {
        let mut value = entry("/bin/server", &[]);
        value["env"] = serde_json::json!({ "TOKEN": "x" });
        assert_eq!(
            inspect_antigravity_mcp_value(
                &value,
                &McpEntry::planned("uze-x", Path::new("/bin/server"), &[])
            )
            .state,
            AttachmentState::Drifted
        );
    }

    #[test]
    fn a_user_disable_is_a_preference_not_an_ownership_signal() {
        let mut value = entry("/bin/server", &[]);
        value["disabled"] = serde_json::json!(true);
        assert_eq!(
            inspect_antigravity_mcp_value(
                &value,
                &McpEntry::planned("uze-x", Path::new("/bin/server"), &[])
            )
            .state,
            AttachmentState::Matched
        );
    }

    #[test]
    fn a_non_boolean_disabled_is_blocked_not_guessed() {
        let mut value = entry("/bin/server", &[]);
        value["disabled"] = serde_json::json!("yes");
        assert_eq!(
            inspect_antigravity_mcp_value(
                &value,
                &McpEntry::planned("uze-x", Path::new("/bin/server"), &[])
            )
            .state,
            AttachmentState::Blocked
        );
    }
}
