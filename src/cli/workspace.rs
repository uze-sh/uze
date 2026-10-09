//! `uze workspace allow|revoke`: a person's answer about the commands a
//! project's `agents.yaml` asks the workspace to run (ADR-055).
//!
//! Deliberately not under `uze agent`: the agents a project launches are
//! exactly who must not be able to approve the commands it declares.

use crate::*;

pub(crate) fn allow_project_commands(app: &UzeApplication, path: &Path) -> Result<()> {
    let project = app.workspace().root(path);
    let Some(awaiting) = app.workspace().commands_awaiting_approval(&project) else {
        progress::success(&format!(
            "nothing to approve: {} declares no command this machine runs that waits for an \
             answer",
            progress::path(&project)
        ));
        return Ok(());
    };
    // Either stamp is enough to refuse: the key proves which agent is asking,
    // but any process carrying an agent's identity is one a project launched.
    let stamped = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if stamped(uze_terminal::launch::AGENT_IDENTITY_VARIABLE)
        || stamped(uze_terminal::launch::AGENT_KEY_VARIABLE)
    {
        return Err(refusal(
            "an agent cannot approve the commands its project runs; the operator approves \
             them, in the workspace or with `uze workspace allow` from their own terminal",
        ));
    }
    if !prompt::interactive() {
        return Err(refusal(
            "approving a project's commands asks a person at a terminal, and this one cannot \
             answer; run `uze workspace allow` interactively",
        ));
    }
    let approved = progress::uninterrupted(|| {
        eprint!("{}", approval_evidence(&awaiting));
        prompt::confirm("Approve and run these commands?", false)
    });
    if approved != Some(true) {
        return Err(refusal("not approved; nothing will run until it is"));
    }
    let warnings = app.workspace().approve_commands(&awaiting)?;
    for warning in &warnings {
        progress::warn(warning);
    }
    progress::success(&format!(
        "approved {} for {}",
        progress::count(awaiting.commands.len(), "command"),
        progress::path(&awaiting.project)
    ));
    Ok(())
}

pub(crate) fn revoke_project_commands(app: &UzeApplication, path: &Path) -> Result<()> {
    let project = app.workspace().root(path);
    if app.workspace().revoke_commands(&project)? {
        progress::success(&format!(
            "approval withdrawn for {}; its commands wait for you again",
            progress::path(&project)
        ));
    } else {
        progress::success(&format!(
            "nothing to withdraw: {} has no approval",
            progress::path(&project)
        ));
    }
    Ok(())
}

/// What the question is about, above the question, on the stream it is
/// asked on. Every line is the escaped form: what is approved is what was
/// read.
pub(crate) fn approval_evidence(awaiting: &uze_application::CommandsAwaitingApproval) -> String {
    let mut text = String::from("\n");
    text.push_str(if awaiting.changed {
        "This project's commands changed since you approved them.\n"
    } else {
        "This project asks uze to run commands on your machine, with your permissions.\n"
    });
    text.push_str(&format!(
        "\nProject\n  {}\n",
        progress::path(&awaiting.project)
    ));
    for (step, heading, when) in [
        (
            uze_application::PolicyStep::Setup,
            "Setup",
            "in every checkout the workspace prepares",
        ),
        (
            uze_application::PolicyStep::Gate,
            "Gate",
            "before any work is delivered",
        ),
    ] {
        let lines: Vec<&str> = awaiting
            .commands
            .iter()
            .filter(|command| command.step == step)
            .map(|command| command.line.as_str())
            .collect();
        if lines.is_empty() {
            continue;
        }
        text.push_str(&format!("\n{heading} {}\n", progress::label(when)));
        for line in lines {
            text.push_str(&format!("  {line}\n"));
        }
    }
    text.push('\n');
    text
}

fn refusal(reason: &str) -> uze_application::UzeError {
    uze_application::UzeError::CommandApproval(reason.to_owned())
}
