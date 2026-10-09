//! The commands a project's worktree policy runs: the ones this machine
//! cannot (a setup step or a gate written only for the other platform's
//! shell), and the ones waiting for the operator's approval before they run
//! at all (ADR-055).

use std::path::{Path, PathBuf};

use serde::Serialize;

use uze_core::{Result, shell::ShellCommand};
use uze_workspace::{
    approval::{self, Awaiting, ProjectCommands},
    checkout, declaration,
    worktree::{PolicyStep, WorktreePolicy, isolated_checkout},
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

/// A project's commands that no approval covers, as a person is shown
/// them, with what approving them would record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommandsAwaitingApproval {
    /// The primary checkout: the project the approval is recorded for.
    pub project: PathBuf,
    /// The project had approved commands, and they have changed since.
    pub changed: bool,
    pub commands: Vec<ShownCommand>,
    /// The checkout placed without its setup, which approving prepares.
    #[serde(skip)]
    pub checkout: Option<PathBuf>,
    /// Exactly the lines shown: approving records these, never whatever
    /// `agents.yaml` says by the time the answer comes.
    #[serde(skip)]
    approving: ProjectCommands,
}

/// One command line as the operator reads it, controls written out.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ShownCommand {
    pub step: PolicyStep,
    pub line: String,
}

impl CommandsAwaitingApproval {
    pub(super) fn of(project: &Path, awaiting: Awaiting, checkout: Option<PathBuf>) -> Self {
        Self {
            project: project.to_path_buf(),
            changed: awaiting.changed,
            commands: awaiting
                .commands
                .iter()
                .map(|command| ShownCommand {
                    step: command.step,
                    line: command.shown(),
                })
                .collect(),
            checkout,
            approving: awaiting.commands,
        }
    }

    /// The same question, about a checkout placed without its setup.
    pub fn for_checkout(mut self, checkout: PathBuf) -> Self {
        self.checkout = Some(checkout);
        self
    }
}

impl Workspace<'_> {
    /// The project's commands no approval covers, or `None` when every
    /// command it declares for this machine is approved, or it declares
    /// none. Read from the primary checkout's manifest, as everything that
    /// runs them reads it.
    #[tracing::instrument(name = "workspace.commands_awaiting_approval", skip_all, fields(project_root = %project_root.display()))]
    pub fn commands_awaiting_approval(
        &self,
        project_root: &Path,
    ) -> Option<CommandsAwaitingApproval> {
        let primary = primary_of(project_root);
        let policy = declaration::declared(&primary).ok()??;
        let awaiting = approval::consent(&self.0.home, &primary, &policy).awaiting()?;
        Some(CommandsAwaitingApproval::of(&primary, awaiting, None))
    }

    /// Records the operator's approval of exactly the commands they were
    /// shown, then prepares the checkout that was placed without them.
    /// Answers what preparing it could not do.
    ///
    /// Only for a person: an agent asking to run the commands it declared
    /// is the thing this approval exists to stop, and the surfaces that
    /// call this are the CLI verb a person types and the workspace's own
    /// dialog — never `uze agent`.
    #[tracing::instrument(name = "workspace.approve_commands", skip_all, fields(project = %awaiting.project.display()), err)]
    pub fn approve_commands(&self, awaiting: &CommandsAwaitingApproval) -> Result<Vec<String>> {
        approval::approve(&self.0.home, &awaiting.project, &awaiting.approving)?;
        let Some(slot) = awaiting.checkout.as_ref().filter(|slot| slot.is_dir()) else {
            return Ok(Vec::new());
        };
        let policy = declaration::policy(&awaiting.project)?;
        Ok(checkout::materialize(
            &awaiting.project,
            slot,
            &approval::consent(&self.0.home, &awaiting.project, &policy),
        ))
    }

    /// Withdraws the project's approval; the next checkout is placed
    /// without its setup and nothing is delivered until it is approved
    /// again. Says whether there was one to withdraw.
    #[tracing::instrument(name = "workspace.revoke_commands", skip_all, fields(project_root = %project_root.display()), err)]
    pub fn revoke_commands(&self, project_root: &Path) -> Result<bool> {
        approval::revoke(&self.0.home, &primary_of(project_root))
    }

    /// What the operator has said about `policy`'s commands here, for the
    /// places that run them.
    pub(super) fn consent<'p>(
        &self,
        primary: &Path,
        policy: &'p WorktreePolicy,
    ) -> approval::Consent<'p> {
        approval::consent(&self.0.home, primary, policy)
    }
}

/// The primary checkout a directory's commands are declared in. Found
/// without asking Git, as `uze status` reads only what is on disk.
fn primary_of(project_root: &Path) -> PathBuf {
    isolated_checkout(project_root).map_or_else(
        || project_root.to_path_buf(),
        |checkout| checkout.primary.to_path_buf(),
    )
}
