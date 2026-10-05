//! The commands a project's worktree policy runs that this machine cannot:
//! a setup step or a gate written only for the other platform's shell.

use std::path::Path;

use serde::Serialize;

use uze_core::shell::ShellCommand;
use uze_workspace::{
    declaration,
    worktree::{PolicyStep, isolated_checkout},
};

use super::Workspace;

/// One project command this machine cannot run, as `agents.yaml` spells it.
#[derive(Clone, Debug, Serialize)]
pub struct StepNotSpelledHere {
    pub step: PolicyStep,
    pub command: String,
    /// The spelling it lacks: `windows` or `posix`.
    pub platform: &'static str,
}

impl Workspace<'_> {
    /// The policy's commands this machine's shell has no spelling for: a
    /// setup step skipped here, a gate that refuses every delivery here.
    /// The policy is the primary checkout's, found without asking Git: a
    /// slot's own manifest is never read, and this answers `uze status`,
    /// which reads only what is cached or on disk.
    #[tracing::instrument(name = "workspace.steps_not_spelled_here", skip_all, fields(project_root = %project_root.display()))]
    pub fn steps_not_spelled_here(&self, project_root: &Path) -> Vec<StepNotSpelledHere> {
        let primary =
            isolated_checkout(project_root).map_or(project_root, |checkout| checkout.primary);
        let Ok(Some(policy)) = declaration::declared(primary) else {
            return Vec::new();
        };
        policy
            .steps_not_spelled_here()
            .into_iter()
            .map(|(step, command)| StepNotSpelledHere {
                step,
                command: command.to_string(),
                platform: ShellCommand::platform(),
            })
            .collect()
    }
}
