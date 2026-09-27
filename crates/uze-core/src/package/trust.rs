//! Core trust boundary: the one question acquisition made necessary: *should this package be
//! allowed to introduce code a harness will execute?*
//!
//! `uze add ./plugin` has always been able to register an MCP server, but the
//! operator had the directory in front of them. `uze add <url>` removes that,
//! so installing stops meaning "copy some files" and starts meaning
//! "authorize execution of something you have not read". UZE is what performs
//! the introduction, so UZE is where the question belongs.
//!
//! Two things this deliberately is **not**:
//!
//! - It is not a permission system. There is no policy language, no stored
//!   grant history, no per-capability rules. It makes one boundary visible.
//! - It is not a check for executable files. A repository full of scripts
//!   nobody invokes is inert; a single declared MCP `command` is not. The
//!   boundary is *a capability that introduces process execution*, which for
//!   M2 means MCP.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{capability::CapabilityKind, capability::Resource};

/// One capability that will cause a process to run once a harness picks it
/// up. Carries what a person needs to judge it, and nothing else.
///
/// The environment and working directory are part of it: `LD_PRELOAD` or a
/// different `cwd` changes what the same command runs as much as a new
/// argument does.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutableCapability {
    pub name: String,
    pub command: String,
    pub arguments: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

/// What the operator is being asked to authorize.
///
/// Structured rather than pre-rendered so every front end — CLI now, TUI
/// later — asks the same question from the same facts instead of
/// reimplementing the judgement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TrustRequest {
    pub package_id: String,
    /// What the operator asked for.
    pub requested_source: String,
    /// What it resolved to — the immutable revision, when there is one.
    pub resolved_source: String,
    pub executable: Vec<ExecutableCapability>,
    /// Set when this package is already installed and the update introduces
    /// execution it did not previously have. Consent is not inherited just
    /// because the package id is unchanged.
    pub previously_trusted: bool,
}

/// The answer, and the reason it is three-valued.
///
/// `Unavailable` is not `Denied`: a CI run that cannot ask must fail with a
/// signal a pipeline can act on, not look like an operator who said no.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustOutcome {
    Granted,
    Denied,
    Unavailable,
}

/// Whoever can answer the question. The Application asks; it never decides.
pub trait TrustAuthority {
    fn authorize(&self, request: &TrustRequest) -> TrustOutcome;
}

/// Grants without asking. For a caller that has already obtained consent out
/// of band — an explicit `--trust` flag, or a package with nothing executable
/// to authorize.
pub struct AlwaysTrust;

impl TrustAuthority for AlwaysTrust {
    fn authorize(&self, _request: &TrustRequest) -> TrustOutcome {
        TrustOutcome::Granted
    }
}

/// Cannot ask. The correct authority for a non-interactive process: it turns
/// an unanswerable question into a structured failure rather than a silent
/// yes.
pub struct NoTrustAuthority;

impl TrustAuthority for NoTrustAuthority {
    fn authorize(&self, _request: &TrustRequest) -> TrustOutcome {
        TrustOutcome::Unavailable
    }
}

/// Extracts the capabilities that introduce process execution.
///
/// MCP servers and portable Hooks both introduce declared process execution.
/// A Skill is text a model reads; it carries no execution of its own.
pub fn executable_capabilities(resources: &[&Resource]) -> Vec<ExecutableCapability> {
    resources
        .iter()
        .flat_map(|resource| match resource.capability.kind {
            CapabilityKind::Mcp => mcp_execution(resource).into_iter().collect(),
            CapabilityKind::Hook => hook_executions(resource),
            _ => Vec::new(),
        })
        .collect()
}

fn mcp_execution(resource: &Resource) -> Option<ExecutableCapability> {
    let config: serde_json::Value = serde_json::from_slice(&resource.capability.payload).ok()?;
    let command = config.get("command")?.as_str()?.to_owned();
    let arguments = config
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let environment = config
        .get("env")
        .and_then(serde_json::Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(key, value)| {
                    let value = value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned);
                    (key.clone(), value)
                })
                .collect()
        })
        .unwrap_or_default();
    let working_directory = config
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Some(ExecutableCapability {
        name: resource.name(),
        command,
        arguments,
        environment,
        working_directory,
    })
}

fn hook_executions(resource: &Resource) -> Vec<ExecutableCapability> {
    serde_json::from_slice::<crate::hook::PortableHook>(&resource.capability.payload)
        .map(|hook| {
            hook.handlers
                .into_iter()
                .enumerate()
                .map(|(index, handler)| ExecutableCapability {
                    name: format!("{}#{index}", hook.id),
                    command: handler.command,
                    arguments: Vec::new(),
                    environment: BTreeMap::new(),
                    working_directory: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether an update introduces execution the installed package did not
/// already have.
///
/// Compares the whole invocation, not just presence: a server whose command,
/// arguments, environment or working directory changed is a new thing to
/// authorize, even though
/// the package id and the capability name are unchanged. This is not a
/// permission history — it exists so a change cannot pass unseen.
pub fn introduces_new_execution(
    previous: &[ExecutableCapability],
    next: &[ExecutableCapability],
) -> bool {
    next.iter().any(|candidate| !previous.contains(candidate))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{capability::Capability, store::PackageId};

    fn mcp_resource(name: &str, command: &str, args: &[&str]) -> Resource {
        let payload = serde_json::to_vec(&serde_json::json!({
            "command": command,
            "args": args,
        }))
        .unwrap();
        Resource::from_package_named(
            PackageId::from_plugin_name("demo", &PathBuf::from("plugin.json")).unwrap(),
            PathBuf::from("/store/demo"),
            Capability {
                kind: CapabilityKind::Mcp,
                path: PathBuf::from("/store/demo/mcp.json"),
                payload,
            },
            name.to_owned(),
        )
    }

    fn without_context() -> ExecutableCapability {
        ExecutableCapability {
            name: String::new(),
            command: String::new(),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            working_directory: None,
        }
    }

    fn skill_resource() -> Resource {
        Resource::from_package(
            PackageId::from_plugin_name("demo", &PathBuf::from("plugin.json")).unwrap(),
            PathBuf::from("/store/demo"),
            Capability {
                kind: CapabilityKind::AgentSkill,
                path: PathBuf::from("/store/demo/skills/a/SKILL.md"),
                payload: b"body".to_vec(),
            },
        )
    }

    /// A declarative package asks nothing of the operator.
    #[test]
    fn a_skill_only_package_declares_no_executable_capability() {
        let skill = skill_resource();
        assert!(executable_capabilities(&[&skill]).is_empty());
    }

    #[test]
    fn an_mcp_server_with_a_command_is_executable() {
        let mcp = mcp_resource("files", "./bin/server", &["--stdio"]);
        assert_eq!(
            executable_capabilities(&[&mcp]),
            vec![ExecutableCapability {
                name: "files".to_owned(),
                command: "./bin/server".to_owned(),
                arguments: vec!["--stdio".to_owned()],
                ..without_context()
            }]
        );
    }

    #[test]
    fn hook_commands_require_the_same_explicit_trust() {
        let hook = Resource::from_package_named(
            PackageId::from_plugin_name("demo", &PathBuf::from("plugin.json")).unwrap(),
            PathBuf::from("/store/demo"),
            Capability {
                kind: CapabilityKind::Hook,
                path: PathBuf::from("/store/demo/hooks.json"),
                payload: serde_json::to_vec(&crate::hook::PortableHook {
                    id: "protect-env".to_owned(),
                    event: crate::hook::HookEvent::PreToolUse,
                    matchers: Vec::new(),
                    handlers: vec![crate::hook::CommandHook {
                        handler_type: crate::hook::CommandHandlerType::Command,
                        command: "scripts/check".to_owned(),
                        timeout: 10,
                    }],
                    effect: crate::hook::HookEffect::Deny,
                    order: 0,
                })
                .unwrap(),
            },
            "protect-env".to_owned(),
        );
        assert_eq!(
            executable_capabilities(&[&hook])[0].command,
            "scripts/check"
        );
    }

    #[test]
    fn an_unchanged_invocation_introduces_nothing_new() {
        let existing = vec![ExecutableCapability {
            name: "files".to_owned(),
            command: "./bin/server".to_owned(),
            arguments: vec!["--stdio".to_owned()],
            ..without_context()
        }];
        assert!(!introduces_new_execution(&existing, &existing.clone()));
    }

    /// The same capability name pointing at a different command is a new
    /// thing to authorize — this is exactly the case consent must not be
    /// inherited through.
    #[test]
    fn a_changed_command_or_argument_introduces_new_execution() {
        let previous = vec![ExecutableCapability {
            name: "files".to_owned(),
            command: "./bin/server".to_owned(),
            arguments: vec!["--stdio".to_owned()],
            ..without_context()
        }];
        for (command, argument) in [("./bin/other", "--stdio"), ("./bin/server", "--elevated")] {
            let next = vec![ExecutableCapability {
                name: "files".to_owned(),
                command: command.to_owned(),
                arguments: vec![argument.to_owned()],
                ..without_context()
            }];
            assert!(introduces_new_execution(&previous, &next));
        }
    }

    /// `LD_PRELOAD` or a new `cwd` changes what an unchanged command runs,
    /// so it is asked about like a changed argument.
    #[test]
    fn a_changed_environment_or_working_directory_introduces_new_execution() {
        let configured = |env: serde_json::Value, cwd: Option<&str>| {
            let mut config = serde_json::json!({ "command": "./bin/server", "env": env });
            if let Some(cwd) = cwd {
                config["cwd"] = serde_json::Value::from(cwd);
            }
            let mut resource = mcp_resource("files", "./bin/server", &[]);
            resource.capability.payload = serde_json::to_vec(&config).unwrap();
            executable_capabilities(&[&resource])
        };
        let previous = configured(serde_json::json!({ "MODE": "safe" }), None);
        assert!(!introduces_new_execution(
            &previous,
            &configured(serde_json::json!({ "MODE": "safe" }), None)
        ));
        assert!(introduces_new_execution(
            &previous,
            &configured(
                serde_json::json!({ "MODE": "safe", "LD_PRELOAD": "./evil.so" }),
                None
            )
        ));
        assert!(introduces_new_execution(
            &previous,
            &configured(serde_json::json!({ "MODE": "safe" }), Some("/"))
        ));
    }

    #[test]
    fn removing_execution_never_requires_fresh_consent() {
        let previous = vec![ExecutableCapability {
            name: "files".to_owned(),
            command: "./bin/server".to_owned(),
            arguments: Vec::new(),
            ..without_context()
        }];
        assert!(!introduces_new_execution(&previous, &[]));
    }
}
