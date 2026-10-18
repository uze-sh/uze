//! Asking the operator: a project's commands and a proposed machine fact.

use super::WorkspaceHit;
use super::WorkspaceModel;
use super::support::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uze_core::UzeHome;

/// A project whose `agents.yaml` declares commands, with an escape
/// sequence hidden in one of them, and the question about it as the
/// application would ask it.
fn project_awaiting_approval(
    label: &str,
) -> (UzeHome, PathBuf, uze_application::CommandsAwaitingApproval) {
    let home = UzeHome::at(uze_testkit::temp::scratch(&format!("{label}-home")));
    let project = uze_testkit::temp::scratch(label);
    std::fs::write(
        project.join("agents.yaml"),
        "workspace:\n  \
         setup:\n    posix: \"make\\e[2J\"\n    windows: \"make\\e[2J\"\n  \
         gate:\n    posix: make test\n    windows: make test\n",
    )
    .unwrap();
    let awaiting = uze_application::UzeApplication::new(home.clone(), Vec::new())
        .workspace()
        .commands_awaiting_approval(&project)
        .expect("an unapproved project waits");
    (home, project, awaiting)
}

/// The question is said once a session, stays until answered, and the
/// dialog it opens shows every line with its controls written out —
/// a command that clears the screen must not be able to hide itself
/// from the person approving it.
#[test]
fn a_projects_commands_are_asked_about_once_and_shown_escaped() {
    let (home, _project, awaiting) = project_awaiting_approval("orchestrator-approval-ask");
    let mut driven = driven(WorkspaceModel::default(), &home);

    driven.attach.model.commands_await(awaiting.clone(), false);
    driven.attach.model.commands_await(awaiting, false);
    let stack = driven.attach.model.toast_stack();
    assert_eq!(stack.len(), 1, "said once: {stack:?}");
    let said = format!("{stack:?}");
    assert!(said.contains("Review"), "it offers the question: {said}");
    assert_eq!(
        driven.attach.model.toast_offer(0),
        Some(WorkspaceHit::ReviewCommands)
    );

    driven.attach.model.review_commands();
    let screen = frame_rows(&mut driven.attach.model).join("\n");
    assert!(screen.contains("Run this project"), "{screen}");
    assert!(screen.contains("make test"), "{screen}");
    assert!(
        screen.contains("\\u{1b}[2J"),
        "the escape is written out: {screen}"
    );
    assert!(!screen.contains('\u{1b}'), "and never drawn raw");
}

/// Only the operator's yes records anything: the way out leaves the
/// project waiting, and approving records exactly the lines shown.
#[test]
fn only_the_operators_yes_approves_a_projects_commands() {
    let (home, project, awaiting) = project_awaiting_approval("orchestrator-approval-answer");
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let mut driven = driven(WorkspaceModel::default(), &home);
    driven.attach.model.commands_await(awaiting, false);

    // Through the keymap, as the operator presses them: the dialog's
    // own scope is what answers, and it opens on the way out.
    let enter = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    let right = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    );
    driven.attach.model.review_commands();
    driven.press_key(enter);
    assert!(driven.attach.model.approval.is_none(), "the dialog closed");
    assert!(
        app.workspace()
            .commands_awaiting_approval(&project)
            .is_some(),
        "enter on the way out approves nothing"
    );

    driven.attach.model.review_commands();
    driven.press_key(right);
    driven.press_key(enter);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app
        .workspace()
        .commands_awaiting_approval(&project)
        .is_some()
        || !format!("{:?}", driven.attach.model.toast_stack()).contains("approved")
    {
        assert!(Instant::now() < deadline, "the approval must be answered");
        std::thread::sleep(Duration::from_millis(10));
        driven.pump();
    }
    assert!(driven.attach.model.remembered.commands_awaiting.is_empty());
}

fn proposed(home: &UzeHome, name: &str) -> uze_application::application::memory::Entry {
    let project = home.root().parent().unwrap().join("repo");
    std::fs::create_dir_all(&project).unwrap();
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    app.memory()
        .remember(
            uze_application::application::memory::MemoryCaller::Agent(uze_application::Claim {
                id: "not-recorded",
                key: "k",
                cwd: &project,
            }),
            uze_application::application::memory::MemoryScope::Machine,
            name,
            uze_application::application::memory::Kind::Feedback,
            "reply in pt-BR",
            "The operator reads Portuguese.",
        )
        .unwrap();
    app.memory().report(None).proposals.remove(0)
}

/// A machine fact an agent proposed is said once a session, offers the
/// question, and the question opens on the way out.
#[test]
fn a_proposed_machine_fact_is_asked_about_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-proposal-ask").join("uze"));
    let fact = proposed(&home, "reply-ptbr");
    let mut driven = driven(WorkspaceModel::default(), &home);

    for _ in 0..2 {
        driven
            .attach
            .model
            .absorb_proposals(crate::ui::orchestrator::ProposalsResolution {
                proposals: vec![fact.clone()],
            });
    }
    let stack = driven.attach.model.toast_stack();
    assert_eq!(stack.len(), 1, "said once: {stack:?}");
    assert_eq!(
        driven.attach.model.toast_offer(0),
        Some(WorkspaceHit::ReviewProposal)
    );

    driven.attach.model.review_proposal();
    let screen = frame_rows(&mut driven.attach.model).join("\n");
    assert!(
        screen.contains("Keep this fact for every project?"),
        "{screen}"
    );
    assert!(screen.contains("reply-ptbr"), "{screen}");
    assert!(
        screen.contains("Discard") && screen.contains("Accept"),
        "{screen}"
    );
    assert_eq!(
        driven.attach.model.proposal.as_ref().map(|open| open.focus),
        Some(0)
    );
}

/// Enter on the way out keeps the proposal; moving to Accept keeps the
/// fact for every project.
#[test]
fn only_the_operators_accept_keeps_a_proposed_fact() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-proposal-answer").join("uze"));
    let fact = proposed(&home, "reply-ptbr");
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let mut driven = driven(WorkspaceModel::default(), &home);
    driven
        .attach
        .model
        .absorb_proposals(crate::ui::orchestrator::ProposalsResolution {
            proposals: vec![fact],
        });
    let key = |code| crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);

    driven.attach.model.review_proposal();
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    assert!(driven.attach.model.proposal.is_none());
    assert_eq!(
        app.memory().report(None).proposals.len(),
        1,
        "nothing accepted"
    );

    driven.attach.model.review_proposal();
    driven.press_key(key(crossterm::event::KeyCode::Right));
    driven.press_key(key(crossterm::event::KeyCode::Right));
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !format!("{:?}", driven.attach.model.toast_stack()).contains("machine fact kept") {
        assert!(Instant::now() < deadline, "the answer must arrive");
        std::thread::sleep(Duration::from_millis(10));
        driven.pump();
    }
    let report = app.memory().report(None);
    assert!(report.proposals.is_empty());
    assert!(
        report
            .entries
            .iter()
            .any(|entry| entry.name == "reply-ptbr")
    );
}
