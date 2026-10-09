//! Codex's launch-time delivery of a project's own agents.
//!
//! Codex 0.158 reads `./.agents/skills` itself but never `./.agents/agents`:
//! its project root for agents is `.codex/agents`, read in a trusted project
//! only, and UZE writes nothing into the checkout. What a launch can add is a
//! `-c` override, a configuration layer Codex merges over the user's own. It
//! carries an `agents` table naming each project agent, its description,
//! and a `config_file` outside the repository holding what the role runs
//! with. Measured: the user's own roles, `[agents]` table included, stay
//! offered beside them; a role's instructions reach the model when it is
//! dispatched; and an interactive session served by an app-server daemon
//! started without the flag gets them too (`experiments/codex/project-agents`,
//! contract `context-project-agent-reaches-model`). `CODEX_HOME` would
//! reach the same roots by replacing the user's whole configuration, which
//! is why it is not the mechanism.
//!
//! A `-c` layer is the operator's own configuration as far as Codex can
//! tell, so it would hand a cloned repository's roles over without the
//! folder trust Codex asks before it reads `.codex/agents` itself. They are
//! handed over only once the person trusted the project in Codex, and a
//! role never leaves the sandbox the operator runs Codex in
//! (`sandbox_mode` is lowered to `workspace-write`, the dialect's ceiling).

use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use uze_core::{
    harness_runtime::{self, HarnessRuntimeContribution, RuntimeContext},
    project_context::{self, AgentsDirectoryResource},
};

use super::{codex_agent_description, codex_role_config, toml_string};
use crate::shared::agent::project_agents;

/// The launch's `-c agents={...}` override, or passthrough when the project
/// authored no agents. Fails open like every runtime contribution: a role
/// file that cannot be written costs this launch its project agents, never
/// the launch.
pub(super) fn runtime_contribution(
    ctx: &RuntimeContext,
    keys: &[&str],
    config_toml: &Path,
) -> HarnessRuntimeContribution {
    match project_agents_override(ctx, keys, config_toml) {
        Ok(Some(value)) => HarnessRuntimeContribution {
            extra_args: vec![OsString::from("-c"), OsString::from(value)],
            ..HarnessRuntimeContribution::default()
        },
        Ok(None) => HarnessRuntimeContribution::passthrough(),
        Err(reason) => HarnessRuntimeContribution::passthrough_with_note(reason),
    }
}

/// Whether a launch from `ctx.cwd` is handed project agents, answered
/// without writing the role files the launch itself would refresh.
pub(super) fn projection_would_activate(ctx: &RuntimeContext, config_toml: &Path) -> bool {
    project_context::resolve(ctx.cwd)
        .resource_directory(AgentsDirectoryResource::Agents)
        .is_some()
        && trusted(ctx.cwd, config_toml)
}

fn trusted(cwd: &Path, config_toml: &Path) -> bool {
    super::trust::project_trusted(config_toml, cwd, || primary_checkout(cwd))
}

/// The primary checkout of the repository `cwd` is in: the parent of its
/// common directory, which is `<primary>/.git` for a linked worktree too.
fn primary_checkout(cwd: &Path) -> Option<PathBuf> {
    let common = uze_git::repository::common_dir(cwd).ok()?;
    (common.file_name()? == ".git").then(|| common.parent().map(Path::to_path_buf))?
}

fn project_agents_override(
    ctx: &RuntimeContext,
    keys: &[&str],
    config_toml: &Path,
) -> std::result::Result<Option<String>, String> {
    let context = project_context::resolve(ctx.cwd);
    let Some(directory) = context.resource_directory(AgentsDirectoryResource::Agents) else {
        return Ok(None);
    };
    let agents = project_agents(&directory);
    if agents.is_empty() {
        return Ok(None);
    }
    if !trusted(ctx.cwd, config_toml) {
        return Err(format!(
            "the project's {} are not handed to Codex until you trust this folder in Codex",
            directory.display()
        ));
    }
    let roles = harness_runtime::prepare_projection(ctx.home, "codex", &context.root)
        .map_err(|error| error.to_string())?
        .join("agents");
    fs::create_dir_all(&roles).map_err(|error| error.to_string())?;

    let mut taken = HashSet::new();
    let mut written = Vec::new();
    let mut entries = Vec::new();
    for agent in &agents {
        let file = roles.join(role_file_name(&agent.label, &mut taken));
        refresh(
            &file,
            &codex_role_config(&agent.document, &agent.document.body, keys),
        )?;
        entries.push(format!(
            "{} = {{ description = {}, config_file = {} }}",
            toml_string(&agent.label),
            toml_string(codex_agent_description(&agent.document)),
            toml_string(&file.to_string_lossy()),
        ));
        written.push(file);
    }
    sweep(&roles, &written);
    Ok(Some(format!("agents={{ {} }}", entries.join(", "))))
}

/// A file name for `label`'s role that no other role of this launch has:
/// a label may carry characters a file name should not.
fn role_file_name(label: &str, taken: &mut HashSet<String>) -> String {
    let stem: String = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let mut name = format!("{stem}.toml");
    let mut suffix = 2;
    while !taken.insert(name.clone()) {
        name = format!("{stem}-{suffix}.toml");
        suffix += 1;
    }
    name
}

/// Writes `content` unless the file already holds it, so a second launch of
/// the same project never touches the file a running session reads.
fn refresh(file: &Path, content: &str) -> std::result::Result<(), String> {
    if fs::read_to_string(file).is_ok_and(|current| current == content) {
        return Ok(());
    }
    uze_core::persistence::write_atomic(file, content.as_bytes()).map_err(|error| error.to_string())
}

/// Removes the role files of agents the project no longer has. Best effort:
/// nothing names a stale file any more, so one left behind costs nothing.
fn sweep(roles: &Path, kept: &[PathBuf]) {
    let Ok(entries) = fs::read_dir(roles) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
            && !kept.contains(&path)
        {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uze_core::harness_runtime::RuntimeContext;
    use uze_core::home::UzeHome;

    use super::{projection_would_activate, role_file_name, runtime_contribution};

    /// A Codex `config.toml` holding `level` for `project`.
    fn codex_config(
        root: &std::path::Path,
        project: &std::path::Path,
        level: &str,
    ) -> std::path::PathBuf {
        let config = root.join("codex/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            format!(
                "[projects.{:?}]\ntrust_level = \"{level}\"\n",
                project.display().to_string()
            ),
        )
        .unwrap();
        config
    }

    fn project_with_agents(root: &std::path::Path) -> std::path::PathBuf {
        let project = root.join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join(".agents/agents/checks")).unwrap();
        fs::write(
            project.join(".agents/agents/reviewer.md"),
            "---\nname: house-reviewer\ndescription: Reviews \"it\"\nmodel: haiku\nharness:\n  codex:\n    model_reasoning_effort: low\n    nickname: x\n---\nReview it.\n",
        )
        .unwrap();
        fs::write(
            project.join(".agents/agents/checks/security.md"),
            "---\ndescription: Checks security\n---\nCheck it.\n",
        )
        .unwrap();
        project
    }

    #[test]
    fn project_agents_are_handed_over_as_one_configuration_layer_outside_the_checkout() {
        let root = uze_testkit::temp::scratch("codex-project-agents");
        let project = project_with_agents(&root);
        let home = UzeHome::at(root.join("uze-home"));
        let ctx = RuntimeContext {
            cwd: &project,
            home: &home,
        };

        let config = codex_config(&root, &project, "trusted");

        let contribution = runtime_contribution(&ctx, &["codex"], &config);
        assert!(contribution.note.is_none(), "{:?}", contribution.note);
        assert_eq!(contribution.extra_args[0], "-c");
        let value = contribution.extra_args[1].to_string_lossy().into_owned();
        let parsed: toml_edit::DocumentMut = value.parse().expect("the override is TOML");
        let agents = parsed["agents"].as_inline_table().unwrap();
        assert_eq!(
            agents.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["security", "house-reviewer"],
            "labelled by logical name, no plugin prefix"
        );
        let reviewer = agents["house-reviewer"].as_inline_table().unwrap();
        assert_eq!(reviewer["description"].as_str(), Some("Reviews \"it\""));
        let role = std::path::PathBuf::from(reviewer["config_file"].as_str().unwrap());
        assert!(role.starts_with(home.runtime_dir()));
        assert_eq!(
            fs::read_to_string(&role).unwrap(),
            "developer_instructions = \"Review it.\"\nmodel_reasoning_effort = \"low\"\n",
            "the instructions and the fields Codex reads under `harness.codex`, nothing it refuses"
        );

        fs::remove_file(project.join(".agents/agents/reviewer.md")).unwrap();
        let _ = runtime_contribution(&ctx, &["codex"], &config);
        assert!(!role.exists(), "a role the project dropped is swept");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_without_agents_is_left_alone() {
        let root = uze_testkit::temp::scratch("codex-no-project-agents");
        let project = root.join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join(".agents/skills/demo")).unwrap();
        let home = UzeHome::at(root.join("uze-home"));
        let contribution = runtime_contribution(
            &RuntimeContext {
                cwd: &project,
                home: &home,
            },
            &["codex"],
            &codex_config(&root, &project, "trusted"),
        );
        assert!(contribution.is_passthrough());
        assert!(!home.runtime_dir().exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A `-c` layer reads to Codex as the operator's own configuration, so a
    /// cloned repository's roles wait for the folder trust Codex itself
    /// asks before reading a project's agents: unset is not trusted.
    #[test]
    fn project_agents_wait_for_the_person_to_trust_the_project_in_codex() {
        let root = uze_testkit::temp::scratch("codex-project-agents-trust");
        let project = project_with_agents(&root);
        let home = UzeHome::at(root.join("uze-home"));
        let ctx = RuntimeContext {
            cwd: &project,
            home: &home,
        };
        for config in [
            root.join("codex/absent.toml"),
            codex_config(&root, &root.join("elsewhere"), "trusted"),
            codex_config(&root, &project, "untrusted"),
        ] {
            let contribution = runtime_contribution(&ctx, &["codex"], &config);
            assert!(
                contribution.extra_args.is_empty(),
                "{:?}",
                contribution.extra_args
            );
            assert!(
                contribution
                    .note
                    .as_deref()
                    .is_some_and(|note| note.contains("trust")),
                "{:?}",
                contribution.note
            );
            assert!(!projection_would_activate(&ctx, &config));
            assert!(!home.runtime_dir().exists(), "no role file is written");
        }
        assert!(projection_would_activate(
            &ctx,
            &codex_config(&root, &project, "trusted")
        ));
        let _ = fs::remove_dir_all(&root);
    }

    /// Trusting a project hands its roles over, never a sandbox wider than
    /// the one the operator runs Codex in.
    #[test]
    fn a_project_role_never_leaves_the_workspace_sandbox() {
        let root = uze_testkit::temp::scratch("codex-project-agents-sandbox");
        let project = project_with_agents(&root);
        fs::write(
            project.join(".agents/agents/reviewer.md"),
            "---\nname: wide\ndescription: d\nharness:\n  codex:\n    sandbox_mode: danger-full-access\n    approval_policy: never\n---\nGo.\n",
        )
        .unwrap();
        let home = UzeHome::at(root.join("uze-home"));
        let contribution = runtime_contribution(
            &RuntimeContext {
                cwd: &project,
                home: &home,
            },
            &["codex"],
            &codex_config(&root, &project, "trusted"),
        );
        let value = contribution.extra_args[1].to_string_lossy().into_owned();
        let parsed: toml_edit::DocumentMut = value.parse().unwrap();
        let role = parsed["agents"]["wide"]["config_file"]
            .as_str()
            .unwrap()
            .to_owned();
        let role = fs::read_to_string(role).unwrap();
        assert!(
            role.contains("sandbox_mode = \"workspace-write\""),
            "{role}"
        );
        assert!(!role.contains("danger-full-access"), "{role}");
        assert!(!role.contains("approval_policy"), "{role}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn role_files_never_share_a_name() {
        let mut taken = std::collections::HashSet::new();
        assert_eq!(role_file_name("a:b", &mut taken), "a_b.toml");
        assert_eq!(role_file_name("a_b", &mut taken), "a_b-2.toml");
    }
}
