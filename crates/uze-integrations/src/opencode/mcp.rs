//! OpenCode V2 MCP exposure — UZE writes the standard stdio command/args
//! into the global `mcp.servers.<name>` entry of `opencode.json` directly,
//! the same file inspection and detach read; the OpenCode MCP runtime
//! remains native.

use std::path::Path;

use uze_core::{
    Result, UzeError,
    capability::Resource,
    exposure::ExposurePlan,
    integration::{AttachmentInspection, AttachmentState, IntegrationPort},
    router::CompatibilityRoute,
    state,
};

use super::OpenCodeIntegration;
use crate::shared::json_config;
use crate::shared::mcp::{McpEntry, managed_stdio_plan};
use crate::shared::plan::unsupported;

impl OpenCodeIntegration {
    pub(super) fn mcp_plan(&self, resource: &Resource) -> ExposurePlan {
        if !state::is_installed(&self.uze_home, self.id()) {
            return unsupported(
                "OpenCode has not completed `uze setup`; its managed global MCP config is not yet enabled.",
            );
        }
        let Some(entry_name) = resource
            .resolved_exposure_name
            .clone()
            .or_else(|| self.exposure_name_candidates(resource).into_iter().next())
        else {
            return unsupported("Resource has no derivable attachment entry name.");
        };
        managed_stdio_plan(
            resource,
            &super::STORE.root_of(&resource.package_root),
            entry_name,
            CompatibilityRoute::Native,
            Some(true),
            "UZE writes the store-owned MCP server into opencode.json's mcp.servers.<name> entry (type local, command array) — the one file attach, inspection and detach all read, with foreign entries left untouched; OpenCode MCP runtime remains native.",
        )
        .unwrap_or_else(|| unsupported("mcp.json server entry is missing a usable `command` field."))
    }
}

/// Where the server lives in `opencode.json`.
fn entry_path(entry_name: &str) -> [&str; 3] {
    ["mcp", "servers", entry_name]
}

/// Writes the managed entry, leaving an identical one alone and refusing to
/// replace one UZE did not write.
pub(super) fn attach_mcp_config(
    config_path: &Path,
    entry_name: &str,
    command: &Path,
    args: &[String],
) -> Result<()> {
    let mut config = if config_path.exists() {
        json_config::read_object(config_path).map_err(UzeError::HarnessConfig)?
    } else {
        serde_json::json!({ "$schema": "https://opencode.ai/config.json" })
    };
    let command_values: Vec<serde_json::Value> =
        std::iter::once(command.to_string_lossy().into_owned())
            .chain(args.iter().cloned())
            .map(serde_json::Value::String)
            .collect();
    let desired = serde_json::json!({ "type": "local", "command": command_values });
    match json_config::get_path(&config, &entry_path(entry_name)) {
        Some(current) if current == &desired => return Ok(()),
        Some(_) => {
            return Err(UzeError::ExposureUnavailable(format!(
                "OpenCode MCP entry `{entry_name}` already exists and is not owned by this UZE plan"
            )));
        }
        None => {}
    }
    json_config::set_path(&mut config, &entry_path(entry_name), desired).map_err(|reason| {
        UzeError::HarnessConfig(format!("cannot attach OpenCode MCP entry: {reason}"))
    })?;
    json_config::write_object(config_path, &config)
}

/// The managed entry's state in a read `opencode.json`.
pub(super) fn inspect_mcp_entry(
    config: &serde_json::Value,
    entry: &McpEntry,
) -> AttachmentInspection {
    match json_config::get_path(config, &entry_path(entry.name)) {
        Some(current) => inspect_opencode_mcp_value(current, entry),
        None => AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "OpenCode MCP entry is missing".to_owned(),
        },
    }
}

/// Removes exactly the managed entry once the file it is read from still
/// matches the receipt.
pub(super) fn detach_mcp_config(
    config_path: &Path,
    entry: &McpEntry,
) -> Result<AttachmentInspection> {
    let mut config = json_config::read_object(config_path).map_err(UzeError::HarnessConfig)?;
    let inspection = inspect_mcp_entry(&config, entry);
    if inspection.state != AttachmentState::Matched {
        return Ok(inspection);
    }
    json_config::remove_path(&mut config, &entry_path(entry.name));
    json_config::write_object(config_path, &config)?;
    Ok(AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "OpenCode managed MCP entry detached".to_owned(),
    })
}

fn inspect_opencode_mcp_value(
    current: &serde_json::Value,
    entry: &McpEntry,
) -> AttachmentInspection {
    let McpEntry {
        transport,
        command,
        args,
        cwd,
        environment,
        enabled,
        ..
    } = *entry;
    let expected_command = std::iter::once(command.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let matches = transport == "stdio"
        && current.get("type").and_then(serde_json::Value::as_str) == Some("local")
        && enabled.is_none_or(|expected| {
            current
                .get("disabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                == !expected
        })
        && current
            .get("command")
            .and_then(serde_json::Value::as_array)
            .and_then(|values| {
                values
                    .iter()
                    .map(serde_json::Value::as_str)
                    .collect::<Option<Vec<_>>>()
            })
            .is_some_and(|actual| actual == expected_command)
        && cwd.is_none_or(|expected| {
            current.get("cwd").and_then(serde_json::Value::as_str)
                == Some(expected.to_string_lossy().as_ref())
        })
        && (environment.is_empty()
            || current
                .get("environment")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|env| {
                    environment
                        .iter()
                        .all(|reference| env.contains_key(&reference.name))
                }));
    if matches {
        AttachmentInspection {
            state: AttachmentState::Matched,
            reason: "OpenCode V2 MCP entry matches receipt".to_owned(),
        }
    } else {
        AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "OpenCode V2 MCP entry differs from receipt".to_owned(),
        }
    }
}

#[cfg(test)]
mod mcp_tests {
    use std::path::{Path, PathBuf};

    use uze_core::integration::AttachmentState;

    use super::{McpEntry, attach_mcp_config, inspect_opencode_mcp_value};

    #[test]
    fn managed_cwd_and_environment_reference_drift_are_detected() {
        let expected = PathBuf::from("/bin/example");
        let args = vec!["--serve".to_owned()];
        let current = serde_json::json!({
            "type": "local",
            "command": ["/bin/example", "--serve"],
            "disabled": false,
            "cwd": "/other",
            "environment": {"OTHER": "opaque"}
        });
        assert_eq!(
            inspect_opencode_mcp_value(
                &current,
                &McpEntry {
                    cwd: Some(Path::new("/expected")),
                    environment: &[uze_core::exposure::McpEnvironmentReference {
                        name: "TOKEN".to_owned()
                    }],
                    enabled: Some(true),
                    ..McpEntry::planned("uze-example", &expected, &args)
                },
            )
            .state,
            AttachmentState::Drifted
        );
    }

    /// The entry's working directory is part of what UZE wrote: the same
    /// command run from somewhere else is somebody's edit.
    #[test]
    fn a_working_directory_moved_alone_is_drift() {
        let expected = PathBuf::from("/bin/example");
        let args = vec!["--serve".to_owned()];
        let entry = McpEntry {
            cwd: Some(Path::new("/expected")),
            ..McpEntry::planned("uze-example", &expected, &args)
        };
        let at = |cwd: &str| {
            serde_json::json!({
                "type": "local",
                "command": ["/bin/example", "--serve"],
                "cwd": cwd,
            })
        };
        assert_eq!(
            inspect_opencode_mcp_value(&at("/expected"), &entry).state,
            AttachmentState::Matched
        );
        assert_eq!(
            inspect_opencode_mcp_value(&at("/other"), &entry).state,
            AttachmentState::Drifted
        );
    }

    /// An entry of that name UZE did not write is refused, never replaced.
    #[test]
    fn an_entry_of_the_same_name_uze_did_not_write_is_left_alone() {
        let root = uze_testkit::temp::scratch("opencode-mcp-foreign-entry");
        let config = root.join("opencode.json");
        std::fs::create_dir_all(&root).unwrap();
        let operators =
            r#"{"mcp":{"servers":{"uze-example":{"type":"local","command":["/bin/theirs"]}}}}"#;
        std::fs::write(&config, operators).unwrap();

        let attached = attach_mcp_config(
            &config,
            "uze-example",
            Path::new("/bin/example"),
            &["--serve".to_owned()],
        );
        assert!(
            matches!(attached, Err(uze_core::UzeError::ExposureUnavailable(_))),
            "{attached:?}"
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), operators);
        let _ = std::fs::remove_dir_all(root);
    }
}
