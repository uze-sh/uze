//! The space overview: its drawer, the agent section, the workspace
//! settings form and the declaration it edits.

use super::WorkspaceHit;
use super::support::*;
use super::{AgentSupportDropdown, SupportResolution, WorkspaceModel, space_prompts_panel};
use crate::ui::theme::{self};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use std::path::PathBuf;
use uze_core::UzeHome;
use uze_terminal::ClientEvent;

fn drawer_over(model: &WorkspaceModel) -> AgentSupportDropdown {
    space_prompts_panel(model, &identities_fixture()).expect("a space is in front")
}

/// The Agent section reads the agent in front when there is one —
/// what its support is read for — and the space's prompts either way.
#[test]
fn the_agent_section_reads_the_agent_in_front_or_the_space_alone() {
    let model = agent_session_in("/repo/.worktrees/a");
    let panel = drawer_over(&model);
    assert!(panel.key.is_some(), "the agent in front is read");

    let shells = model_of(session("/repo"));
    let panel = drawer_over(&shells);
    assert_eq!(panel.key, None, "a shell has no support to read");
    assert_eq!(panel.space_root, PathBuf::from("/repo"));
}

fn support_fixture() -> crate::ui::agent_support::AgentSupport {
    use uze_application::application::{
        AgentContextStatus, ContextMechanism, HarnessContextSupport, HarnessHealth,
        ResourceDelivery,
    };
    let health = HarnessHealth {
        below_minimum: None,
        integration: "claude-code".to_owned(),
        display_name: "Claude Code".to_owned(),
        description: String::new(),
        detection: uze_core::integration::HarnessDetection {
            present: true,
            version: None,
        },
        setup: "installed".to_owned(),
        strategy: None,
        provisioning: None,
        publication: uze_core::integration::PublicationStatus::NotApplicable,
        capabilities: Default::default(),
        runtime_shim_active: true,
        agent_context: None,
        context_support: HarnessContextSupport {
            instructions: ContextMechanism::RuntimeShim,
            project_skills: ContextMechanism::RuntimeShim,
            project_agents: ContextMechanism::RuntimeShim,
        },
    };
    let context = AgentContextStatus {
        integration: "claude-code".to_owned(),
        display_name: "Claude Code".to_owned(),
        present: true,
        root: PathBuf::from("/repo"),
        instructions: ResourceDelivery::Projected,
        project_skills: ResourceDelivery::Projected,
        project_agents: ResourceDelivery::AbsentFromProject,
    };
    crate::ui::agent_support::AgentSupport::resolve(health, &context)
}

/// The Agent section holds the support answer read for its agent: the
/// client's one shared answer moves on with whatever agent comes to the
/// front, and the section stopped saying what reaches its own agent
/// the moment that happened.
#[test]
fn the_agent_section_keeps_its_own_agents_support() {
    let mut model = agent_session_in("/repo/.worktrees/a");
    let mut agent = drawer_over(&model);
    agent.support = Some(support_fixture());
    let (space, behind) = {
        let space = model.session.as_ref().expect("session").selected_space();
        (space.id, space.selected_tab)
    };
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.agent = Some(agent);
    overview.answer = Some(Ok(project_space(
        Default::default(),
        Err("not read".to_owned()),
    )));
    model.remembered.agent_support = Some(SupportResolution {
        key: ("codex".to_owned(), PathBuf::from("/elsewhere")),
        support: None,
    });
    // Wide enough that the drawer, where what reaches the agent is
    // read, stands beside the prompts.
    let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, 45));
    assert!(
        rows.iter().any(|row| row.contains("AGENTS.md")),
        "{}",
        rows.join("\n")
    );
}

/// Opening the overview reads what reaches the agent in front and the
/// space's prompts once: opened again before they answer, it waits for
/// the reads already out, and their answers free it to read again.
#[test]
fn opening_the_overview_again_waits_for_the_reads_already_out() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-reads-once",
    ));
    let mut driven = driven(agent_session_in("/repo/.worktrees/a"), &home);
    driven.attach.toggle_space_overview();
    let pending = |driven: &Driven<'_>| {
        let remembered = &driven.attach.model.remembered;
        (
            remembered.agent_support_pending.clone(),
            remembered.prompt_history_pending.clone(),
        )
    };
    let (support, prompts) = pending(&driven);
    assert!(support.is_some(), "what reaches the agent was not read");
    assert_eq!(prompts, Some(PathBuf::from("/repo")));
    driven.attach.toggle_space_overview();
    driven.attach.toggle_space_overview();
    assert_eq!(pending(&driven), (support, prompts));

    let channels = &driven.attach.channels;
    let wait = std::time::Duration::from_secs(60);
    let answer = channels.prompts.receiver.recv_timeout(wait).unwrap();
    channels.prompts.sender.send(answer).unwrap();
    let answer = channels.support.receiver.recv_timeout(wait).unwrap();
    channels.support.sender.send(answer).unwrap();
    driven.pump();
    let remembered = &driven.attach.model.remembered;
    assert_eq!(remembered.prompt_history_pending, None);
    assert_eq!(remembered.agent_support_pending, None);
}

/// A prompt is read in the Agent section to be used again: enter on it
/// copies the whole prompt, however its lines were folded.
#[test]
fn enter_on_a_prompt_copies_it_whole() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-agent-copy"));
    let mut model = agent_session_in("/repo");
    let mut agent = drawer_over(&model);
    agent.support = Some(support_fixture());
    let root = agent.space_root.clone();
    let (space, behind) = {
        let space = model.session.as_ref().expect("session").selected_space();
        (space.id, space.selected_tab)
    };
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.agent = Some(agent);
    overview.answer = Some(Ok(project_space(
        Default::default(),
        Err("not read".to_owned()),
    )));
    let origin = uze_application::PromptOrigin {
        space_label: "uze".to_owned(),
        tab_id: 1,
        tab_label: "Agent".to_owned(),
        agent_binary: "claude".to_owned(),
        agent: None,
    };
    let long = "rebase onto main ".repeat(6);
    model.remembered.drawer_prompts = Some(crate::ui::orchestrator::PromptHistoryResolution {
        root,
        entries: vec![
            uze_application::PromptEntry::new(&origin, "fix the pipeline").unwrap(),
            uze_application::PromptEntry::new(&origin, &long).unwrap(),
        ],
    });
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let press = |driven: &mut Driven<'_>, code: crossterm::event::KeyCode| {
        driven.press_key(crossterm::event::KeyEvent::from(code));
        driven.frame();
    };
    press(&mut driven, crossterm::event::KeyCode::Enter);
    assert_eq!(
        driven.attach.model.clipboard.as_deref(),
        Some("fix the pipeline")
    );
    press(&mut driven, crossterm::event::KeyCode::Down);
    press(&mut driven, crossterm::event::KeyCode::Enter);
    assert_eq!(
        driven.attach.model.clipboard.as_deref(),
        Some(long.trim()),
        "enter did not copy the whole prompt"
    );
}

/// The overview stands in front of one tab and leaves the moment the
/// operator chooses another: a tab chord, an agent, or the server
/// moving the selection on. What it survives is a broadcast that
/// leaves the tab behind it in front.
#[test]
fn the_overview_leaves_for_whatever_agent_is_chosen_next() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-leaves"));
    let mut base = session("/repo");
    let space = base.selected_space().id;
    let shell = base.selected_tab().id;
    let agent_pane = base.add_tab(
        space,
        "agent 1".into(),
        None,
        80,
        24,
        PathBuf::from("/repo"),
    );
    base.update_pane_status(agent_pane, PathBuf::from("/repo"), "agent".into());
    let agent = base
        .selected_space()
        .tabs
        .iter()
        .find(|tab| tab.pane.id == agent_pane)
        .expect("the agent's tab")
        .id;
    base.select_tab(shell);
    let open_overview = |driven: &mut Driven<'_>| {
        driven.frame();
        let button = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == WorkspaceHit::OpenOverview).then_some(*rect))
            .expect("the space button is on the strip");
        driven.press(button.x + 1, button.y);
        let session = driven.attach.model.session.clone().expect("session");
        driven.attach.model.apply(
            ClientEvent::SessionUpdated { session },
            &identities_fixture(),
        );
        assert!(
            driven.attach.model.space_overview.is_some(),
            "the space button did not open the overview, or a broadcast put it away"
        );
    };

    // A tab chord chooses a tab of the strip, and the overview leaves.
    let mut driven = driven_again(&home, &base);
    open_overview(&mut driven);
    driven.press_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('1'),
        crossterm::event::KeyModifiers::ALT,
    ));
    assert!(
        driven.attach.model.space_overview.is_none(),
        "alt+1 was taken by the overview"
    );

    // The server moving the selection onto an agent.
    let mut driven = driven_again(&home, &base);
    open_overview(&mut driven);
    let mut moved = driven.attach.model.session.clone().expect("session");
    moved.select_tab(agent);
    driven.attach.model.apply(
        ClientEvent::SessionUpdated { session: moved },
        &identities_fixture(),
    );
    assert!(
        driven.attach.model.space_overview.is_none(),
        "the overview stayed over an agent it was not opened in front of"
    );

    // A section chord stays the overview's.
    let mut driven = driven_again(&home, &base);
    open_overview(&mut driven);
    driven.press_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('4'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .is_some_and(|overview| overview.section
                == crate::ui::orchestrator::space_overview::Section::Context),
        "4 did not show the Context section"
    );
}

/// A section longer than the pane scrolls: its groove is drawn, and
/// the wheel over the overview walks the list the way the arrows do,
/// carrying the selection past the first screenful.
#[test]
fn a_long_memory_section_scrolls_with_a_scrollbar_and_the_wheel() {
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-scroll"));
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let facts: Vec<Entry> = (0..40)
        .map(|index| Entry {
            name: format!("fact-{index:02}"),
            description: String::new(),
            kind: Kind::Feedback,
            scope: MemoryScope::Project,
            origin: "uze".to_owned(),
            source: None,
            path: PathBuf::from(format!("/facts/fact-{index:02}.md")),
            updated: None,
            writable: true,
            proposed: false,
        })
        .collect();
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
    overview.answer = Some(Ok(project_space(
        uze_application::SpaceMemory {
            project: facts,
            ..Default::default()
        },
        Err("not read here".to_owned()),
    )));

    let rows = frame_rows(&mut model);
    let thumb = theme::glyph(crate::ui::theme::Symbol::ScrollThumb);
    assert!(
        rows.iter().any(|row| row.contains(thumb.as_str())),
        "forty facts on a 24-row screen drew no scrollbar:\n{}",
        rows.join("\n")
    );
    assert!(
        !rows.iter().any(|row| row.contains("fact-39")),
        "the last fact is on screen before scrolling"
    );

    let mut driven = driven(model, &home);
    driven.frame();
    for _ in 0..39 {
        driven.mouse(40, 12, MouseEventKind::ScrollDown);
    }
    let rows = frame_rows(&mut driven.attach.model);
    assert!(
        rows.iter().any(|row| row.contains("fact-39")),
        "the wheel did not reach the last fact:\n{}",
        rows.join("\n")
    );
    // The wheel moved the view, not the selection: the keys find it
    // where they left it, and moving it brings the window back.
    let memory = crate::ui::orchestrator::space_overview::Section::Memory.index();
    let selected = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .expect("open")
            .rows[memory]
    };
    assert_eq!(selected(&driven), 0, "the wheel moved the selection");
    driven.frame();
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Down,
    ));
    driven.frame();
    assert_eq!(selected(&driven), 1);
    let rows = frame_rows(&mut driven.attach.model);
    assert!(
        rows.iter().any(|row| row.contains("fact-01")),
        "moving the selection did not bring it into view:\n{}",
        rows.join("\n")
    );
    // The pointer lights the row it is over and chooses nothing.
    driven.frame();
    driven.mouse(40, 14, MouseEventKind::Moved);
    assert!(
        matches!(
            driven.attach.model.hovered,
            Some(WorkspaceHit::OverviewRow(_))
        ),
        "the pointer over a fact is not on its row"
    );
    assert_eq!(selected(&driven), 1, "hovering moved the selection");
}

/// A project's overview with nothing to report but `memory` and the
/// `workspace:` declaration `workspace`.
fn project_space(
    memory: uze_application::SpaceMemory,
    workspace: std::result::Result<uze_application::DeclarationView, String>,
) -> uze_application::SpaceOverview {
    let report = uze_application::application::StatusReport {
        root: PathBuf::from("/repo"),
        instructions: uze_application::application::InstructionsFile {
            path: PathBuf::from("/repo/AGENTS.md"),
            exists: true,
            managed_regions: 0,
        },
        portability: uze_application::application::Portability::Portable,
        harnesses: Vec::new(),
        packages_installed: 0,
        packages_contributing_here: Vec::new(),
        project_lock: uze_application::application::ProjectLockStatus::Absent,
        drift: Default::default(),
        issues: Vec::new(),
        held_back: Vec::new(),
        memory: Default::default(),
    };
    uze_application::SpaceOverview::Project(Box::new(uze_application::ProjectSpace {
        status: uze_application::application::ProjectStatus {
            report,
            steps_not_spelled_here: Vec::new(),
            commands_awaiting_approval: None,
        },
        plugins: Default::default(),
        memory,
        workspace,
        preserved: Vec::new(),
    }))
}

/// The Workspace section is a form: a label per key and the control
/// its kind takes — a choice in a box that opens its options, a list
/// one entry per line — and a click on an option chooses it.
#[test]
fn the_workspace_form_draws_each_key_s_control_and_a_click_chooses() {
    use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-form"));
    let session = session(uze_testkit::temp::scratch(
        "orchestrator-overview-form-project",
    ));
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let declared = |key: DeclarationKey| match key {
        DeclarationKey::Worktree => Some(DeclaredValue::Text("always".to_owned())),
        DeclarationKey::Artifacts => Some(DeclaredValue::List(vec!["docs".to_owned()])),
        _ => None,
    };
    let view = DeclarationView {
        manifest: PathBuf::from("/repo/agents.yaml"),
        exists: true,
        entries: DeclarationKey::ALL
            .iter()
            .map(|key| DeclarationEntry {
                key: *key,
                declared: declared(*key),
                default: key.default_spelled(),
                description: key.description(),
            })
            .collect(),
        actions: Vec::new(),
        editable: Ok(()),
        problems: Vec::new(),
        choices: uze_application::DeclarationChoices {
            branches: vec!["main".to_owned(), "develop".to_owned()],
            ..Default::default()
        },
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));

    let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, 45));
    let screen = rows.join("\n");
    for expected in ["Worktree", "Target branch", "Artifacts", "docs", "always"] {
        assert!(
            screen.contains(expected),
            "{expected} is not on the form:\n{screen}"
        );
    }
    assert!(
        !screen.contains("workspace:"),
        "the form is drawn as YAML:\n{screen}"
    );

    let worktree = rows
        .iter()
        .find(|row| row.contains("Worktree"))
        .expect("worktree is on the form");
    let form = worktree.split('│').nth(1).unwrap_or_default();
    assert!(
        form.contains("always") && form.contains("manual"),
        "a pair shows both its options:\n{screen}"
    );

    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let hit = |driven: &Driven, wanted: &dyn Fn(&WorkspaceHit) -> bool| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| wanted(hit).then_some(*rect))
    };
    let manual = hit(&driven, &|hit| {
        matches!(hit, WorkspaceHit::OverviewChoice(0, 0))
    })
    .expect("worktree's first option is a target");
    driven.press(manual.x, manual.y);
    let overview = driven
        .attach
        .model
        .space_overview
        .as_ref()
        .expect("still open");
    assert_eq!(
        overview.edit.values.get(&DeclarationKey::Worktree),
        Some(&Some(DeclaredValue::Text("manual".to_owned()))),
        "the click did not choose manual"
    );

    let target = overview
        .workspace_rows()
        .iter()
        .position(|row| {
            *row == crate::ui::orchestrator::space_overview::WorkspaceRow::Key(
                DeclarationKey::Target,
            )
        })
        .expect("the target is a row");
    driven.frame();
    let target = hit(&driven, &|hit| *hit == WorkspaceHit::OverviewRow(target))
        .expect("the target's row is a target");
    driven.press(target.x, target.y);
    let overview = driven
        .attach
        .model
        .space_overview
        .as_ref()
        .expect("still open");
    assert_eq!(
        overview.picker_options(),
        vec!["main", "develop"],
        "the target does not offer the repository's branches"
    );
}

#[test]
fn the_workspace_form_edits_from_the_keyboard_and_a_click() {
    use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-form"));
    let session = session(uze_testkit::temp::scratch(
        "orchestrator-overview-form-project",
    ));
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let declared = |key: DeclarationKey| match key {
        DeclarationKey::Worktree => Some(DeclaredValue::Text("always".to_owned())),
        DeclarationKey::Artifacts => Some(DeclaredValue::List(vec!["docs".to_owned()])),
        _ => None,
    };
    let view = DeclarationView {
        manifest: PathBuf::from("/repo/agents.yaml"),
        exists: true,
        entries: DeclarationKey::ALL
            .iter()
            .map(|key| DeclarationEntry {
                key: *key,
                declared: declared(*key),
                default: key.default_spelled(),
                description: key.description(),
            })
            .collect(),
        actions: Vec::new(),
        editable: Ok(()),
        problems: Vec::new(),
        choices: Default::default(),
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));

    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let key = |driven: &mut Driven<'_>, code: crossterm::event::KeyCode| {
        driven.press_key(crossterm::event::KeyEvent::from(code));
        driven.frame();
    };
    // Both of a pair's options are on screen: enter takes the other one.
    key(&mut driven, crossterm::event::KeyCode::Enter);
    let values = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .expect("the overview is open")
            .edit
            .values
            .clone()
    };
    assert_eq!(
        values(&driven).get(&DeclarationKey::Worktree),
        Some(&Some(DeclaredValue::Text("manual".to_owned()))),
        "enter did not take the other option"
    );
    key(&mut driven, crossterm::event::KeyCode::Down);
    key(&mut driven, crossterm::event::KeyCode::Enter);
    key(&mut driven, crossterm::event::KeyCode::Char('4'));
    key(&mut driven, crossterm::event::KeyCode::Enter);
    assert_eq!(
        values(&driven).get(&DeclarationKey::Slots),
        Some(&Some(DeclaredValue::Number(4))),
        "a number typed into slots was not taken"
    );

    // A click on a value's box opens it for typing, as enter does.
    let target = driven
        .attach
        .model
        .hits
        .iter()
        .filter_map(|(rect, hit)| match hit {
            WorkspaceHit::OverviewRow(row) => Some((*rect, *row)),
            _ => None,
        })
        .find(|(_, row)| {
            driven
                .attach
                .model
                .space_overview
                .as_ref()
                .is_some_and(|overview| {
                    overview.workspace_rows().get(*row)
                        == Some(&crate::ui::orchestrator::space_overview::WorkspaceRow::Key(
                            DeclarationKey::Target,
                        ))
                })
        })
        .map(|(rect, _)| rect)
        .expect("target's row is on screen");
    driven.press(target.x + 20, target.y);
    assert!(
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .is_some_and(|overview| overview.edit.field.is_some()),
        "a click on target's box did not open it for typing"
    );
}

/// A `workspace:` section declaring nothing, read from `exists` or no
/// `agents.yaml` at all.
fn undeclared(exists: bool) -> uze_application::DeclarationView {
    use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView};
    DeclarationView {
        manifest: PathBuf::from("agents.yaml"),
        exists,
        entries: DeclarationKey::ALL
            .iter()
            .map(|key| DeclarationEntry {
                key: *key,
                declared: None,
                default: key.default_spelled(),
                description: key.description(),
            })
            .collect(),
        actions: Vec::new(),
        editable: Ok(()),
        problems: Vec::new(),
        choices: Default::default(),
    }
}

/// The Settings section of a real repository's overview, with no
/// `agents.yaml` yet, driven on a roomy terminal.
fn settings_over<'a>(home: &'a UzeHome, repository: &uze_testkit::git::Repository) -> Driven<'a> {
    let session = session(repository.root());
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.answer = Some(Ok(project_space(Default::default(), Ok(undeclared(false)))));
    let mut driven = driven(model, home).on_a_roomy_terminal();
    driven.frame();
    driven
}

/// Waits for the save out to answer, then lets the attach take it in as
/// its loop would, sending whatever was edited meanwhile.
fn await_save(driven: &mut Driven<'_>) {
    let answers = &driven.attach.channels.declaration_saves;
    let answer = answers
        .receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the save answered");
    answers.sender.send(answer).expect("the channel is open");
    driven.attach.absorb_space_overview_answers();
    driven.frame();
}

fn key(driven: &mut Driven<'_>, code: crossterm::event::KeyCode) {
    driven.press_key(crossterm::event::KeyEvent::from(code));
    driven.frame();
}

fn overview_of<'m>(
    driven: &'m Driven<'_>,
) -> &'m crate::ui::orchestrator::space_overview::SpaceOverviewView {
    driven
        .attach
        .model
        .space_overview
        .as_ref()
        .expect("the overview is open")
}

/// Choosing an option writes it: there is no Save to press, the edit
/// leaves the pending ones once the file holds it, the form keeps
/// showing it, and a toast says the file was created and what it says.
#[test]
fn an_edit_is_written_without_any_save() {
    use crossterm::event::KeyCode;
    use uze_application::{DeclarationKey, DeclaredValue};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-autosave"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-autosave");
    let mut driven = settings_over(&home, &repository);
    // Worktree opens on manual, its default; always is the next one.
    key(&mut driven, KeyCode::Enter);
    key(&mut driven, KeyCode::Down);
    key(&mut driven, KeyCode::Enter);
    assert!(
        overview_of(&driven).edit.saving.is_some(),
        "choosing an option sent no save"
    );
    await_save(&mut driven);

    let manifest = repository.root().join("agents.yaml");
    assert_eq!(
        std::fs::read_to_string(&manifest).expect("agents.yaml was created"),
        "workspace:\n  worktree: always\n"
    );
    let overview = overview_of(&driven);
    assert!(
        overview.edit.values.is_empty(),
        "a written edit is still pending"
    );
    assert_eq!(
        overview.value(DeclarationKey::Worktree),
        Some(&DeclaredValue::Text("always".to_owned())),
        "the form went back to the old value before the file was read again"
    );
    let toast = driven
        .attach
        .model
        .remembered
        .toasts
        .back()
        .expect("a toast says what was written");
    assert_eq!(toast.text, "agents.yaml created");
    assert_eq!(toast.detail, "worktree: always");
}

/// A value the workspace would refuse is never written: the edit stays
/// marked, its problem stands under its key, and discarding drops it.
#[test]
fn a_refused_edit_is_kept_with_its_problem_until_discarded() {
    use crossterm::event::KeyCode;
    use uze_application::DeclarationKey;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-refused"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-refused");
    let mut driven = settings_over(&home, &repository);
    key(&mut driven, KeyCode::Down);
    key(&mut driven, KeyCode::Enter);
    key(&mut driven, KeyCode::Char('x'));
    key(&mut driven, KeyCode::Enter);
    await_save(&mut driven);

    let manifest = repository.root().join("agents.yaml");
    assert!(!manifest.exists(), "a refused edit was written");
    let overview = overview_of(&driven);
    assert!(
        overview.edit.values.contains_key(&DeclarationKey::Slots),
        "the refused edit was dropped"
    );
    let reason = overview
        .problems()
        .iter()
        .find(|problem| problem.key.as_deref() == Some("slots"))
        .map(|problem| problem.reason.clone())
        .expect("the refusal is reported under slots");
    let screen = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
    let said: String = reason.chars().take(24).collect();
    assert!(
        screen.contains(&said),
        "the problem is not under its key:\n{screen}"
    );
    assert!(screen.contains("1 not saved"), "{screen}");

    key(&mut driven, KeyCode::Char('x'));
    let overview = overview_of(&driven);
    assert!(overview.edit.values.is_empty(), "discard kept the edit");
    assert!(overview.edit.saving.is_none(), "discarding sent a save");
    assert!(!manifest.exists());
}

/// One save is out at a time: what is edited meanwhile waits for it to
/// answer, and is then written on top of it.
#[test]
fn edits_made_while_a_save_is_out_are_written_after_it() {
    use crossterm::event::KeyCode;
    use uze_application::DeclarationKey;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-queued"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-queued");
    let mut driven = settings_over(&home, &repository);
    key(&mut driven, KeyCode::Enter);
    key(&mut driven, KeyCode::Down);
    key(&mut driven, KeyCode::Char('+'));
    let overview = overview_of(&driven);
    let out = overview
        .edit
        .saving
        .as_ref()
        .expect("the first save is out");
    assert!(
        out.values.contains_key(&DeclarationKey::Worktree)
            && !out.values.contains_key(&DeclarationKey::Slots),
        "the second edit went out beside the first"
    );
    assert!(overview.edit.values.contains_key(&DeclarationKey::Slots));

    await_save(&mut driven);
    let out = overview_of(&driven)
        .edit
        .saving
        .as_ref()
        .expect("the edit made meanwhile went out once the first answered");
    assert!(out.values.contains_key(&DeclarationKey::Slots));
    await_save(&mut driven);

    assert_eq!(
        std::fs::read_to_string(repository.root().join("agents.yaml")).unwrap(),
        "workspace:\n  worktree: always\n  slots: 1\n"
    );
    assert!(overview_of(&driven).edit.values.is_empty());
}

/// A number steps by its − and +, clicked or pressed: slots climbs
/// from no limit and falls back to it, idle days from the three the
/// workspace applies.
#[test]
fn a_number_steps_with_its_minus_and_plus() {
    use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-step"));
    let session = session(uze_testkit::temp::scratch(
        "orchestrator-overview-step-project",
    ));
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let view = DeclarationView {
        manifest: PathBuf::from("/repo/agents.yaml"),
        exists: true,
        entries: DeclarationKey::ALL
            .iter()
            .map(|key| DeclarationEntry {
                key: *key,
                declared: None,
                default: key.default_spelled(),
                description: key.description(),
            })
            .collect(),
        actions: Vec::new(),
        editable: Ok(()),
        problems: Vec::new(),
        choices: Default::default(),
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let step = |driven: &mut Driven<'_>, adds: bool| {
        let rect = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| match hit {
                WorkspaceHit::OverviewStep(row, up) if *up == adds => {
                    let overview = driven.attach.model.space_overview.as_ref()?;
                    (overview.workspace_rows().get(*row)
                        == Some(&crate::ui::orchestrator::space_overview::WorkspaceRow::Key(
                            DeclarationKey::Slots,
                        )))
                    .then_some(*rect)
                }
                _ => None,
            })
            .expect("slots draws its step");
        driven.press(rect.x + 1, rect.y);
        driven.frame();
    };
    let value = |driven: &Driven<'_>, key: DeclarationKey| {
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .expect("open")
            .edit
            .values
            .get(&key)
            .cloned()
    };
    step(&mut driven, true);
    step(&mut driven, true);
    assert_eq!(
        value(&driven, DeclarationKey::Slots),
        Some(Some(DeclaredValue::Number(2))),
        "two clicks on + from no limit"
    );
    step(&mut driven, false);
    step(&mut driven, false);
    assert_eq!(
        value(&driven, DeclarationKey::Slots),
        Some(None),
        "back below one is no limit, sent after the save of the first step"
    );

    // The key on idle days: down to it, and + from its default.
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Down,
    ));
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Char('+'),
    ));
    assert_eq!(
        value(&driven, DeclarationKey::IdleDays),
        Some(Some(DeclaredValue::Number(4))),
        "+ on idle days did not add one to its default"
    );
}

/// A link is chosen from what the repository ignores rather than
/// typed: the field offers those paths, typing narrows them, the
/// arrows walk them and enter takes the one the keys are on.
#[test]
fn a_link_is_picked_from_what_the_repository_ignores() {
    use uze_application::{DeclarationEntry, DeclarationKey, DeclarationView, DeclaredValue};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-pick"));
    let session = session(uze_testkit::temp::scratch(
        "orchestrator-overview-pick-project",
    ));
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let choices = uze_application::DeclarationChoices {
        links: vec![
            ".env".to_owned(),
            ".env.local".to_owned(),
            "node_modules".to_owned(),
        ],
        directories: Vec::new(),
        branches: Vec::new(),
    };
    let view = DeclarationView {
        manifest: PathBuf::from("/repo/agents.yaml"),
        exists: true,
        entries: DeclarationKey::ALL
            .iter()
            .map(|key| DeclarationEntry {
                key: *key,
                declared: None,
                default: key.default_spelled(),
                description: key.description(),
            })
            .collect(),
        actions: Vec::new(),
        editable: Ok(()),
        problems: Vec::new(),
        choices,
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let key = |driven: &mut Driven<'_>, code: crossterm::event::KeyCode| {
        driven.press_key(crossterm::event::KeyEvent::from(code));
        driven.frame();
    };
    let row_of = |rows: &[String], label: &str| {
        rows.iter()
            .position(|row| row.contains(label))
            .unwrap_or_else(|| panic!("{label} is not on the form"))
    };
    let closed = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40));
    // Down to Linked files, open it, narrow to `.env`, take the second.
    for _ in 0..3 {
        key(&mut driven, crossterm::event::KeyCode::Down);
    }
    key(&mut driven, crossterm::event::KeyCode::Enter);
    let open = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40));
    let screen = open.join("\n");
    assert!(
        screen.contains("node_modules"),
        "the ignored paths are not offered:\n{screen}"
    );
    // The list floats over the form: what is past it stays put.
    assert_eq!(
        row_of(&closed, "Target branch"),
        row_of(&open, "Target branch"),
        "opening the list moved the form:\n{screen}"
    );
    for typed in ['.', 'e'] {
        key(&mut driven, crossterm::event::KeyCode::Char(typed));
    }
    let screen = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(
        !screen.contains("node_modules"),
        "typing did not narrow:\n{screen}"
    );
    key(&mut driven, crossterm::event::KeyCode::Down);
    key(&mut driven, crossterm::event::KeyCode::Enter);
    let overview = driven.attach.model.space_overview.as_ref().expect("open");
    assert_eq!(
        overview.edit.values.get(&DeclarationKey::Link),
        Some(&Some(DeclaredValue::List(vec![".env.local".to_owned()]))),
        "enter did not take the offered path the keys were on"
    );
}

/// An action's instruction is prose: enter breaks the line rather
/// than finishing it, and the save chord takes it as written.
#[test]
fn an_action_is_written_over_several_lines() {
    use uze_application::{ActionState, DeclarationEntry, DeclarationKey, DeclarationView};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-action"));
    let session = session(uze_testkit::temp::scratch(
        "orchestrator-overview-action-project",
    ));
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let view = DeclarationView {
        manifest: PathBuf::from("/repo/agents.yaml"),
        exists: true,
        entries: DeclarationKey::ALL
            .iter()
            .map(|key| DeclarationEntry {
                key: *key,
                declared: None,
                default: key.default_spelled(),
                description: key.description(),
            })
            .collect(),
        actions: Vec::new(),
        editable: Ok(()),
        problems: Vec::new(),
        choices: Default::default(),
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    let first_action = overview
        .workspace_rows()
        .iter()
        .position(|row| {
            *row == crate::ui::orchestrator::space_overview::WorkspaceRow::Action(
                ActionState::OnChange,
            )
        })
        .expect("onChange is a row of the form");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.rows[1] = first_action;
    overview.answer = Some(Ok(project_space(Default::default(), Ok(view))));
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let press = |driven: &mut Driven<'_>, event: crossterm::event::KeyEvent| {
        driven.press_key(event);
        driven.frame();
    };
    let plain = crossterm::event::KeyEvent::from;
    press(&mut driven, plain(crossterm::event::KeyCode::Enter));
    for typed in "Commit".chars() {
        press(&mut driven, plain(crossterm::event::KeyCode::Char(typed)));
    }
    press(&mut driven, plain(crossterm::event::KeyCode::Enter));
    for typed in "then push".chars() {
        press(&mut driven, plain(crossterm::event::KeyCode::Char(typed)));
    }
    press(
        &mut driven,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('s'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    );
    let overview = driven.attach.model.space_overview.as_ref().expect("open");
    assert!(
        overview.edit.text.is_none(),
        "ctrl+s left the instruction open"
    );
    assert_eq!(
        overview.edit.actions.get(&ActionState::OnChange),
        Some(&Some("Commit\nthen push".to_owned())),
        "the instruction was not taken as written"
    );
}

/// The drawer is resized by dragging its rule, as every drawer is, and
/// a section longer than the pane scrolls with its handle on that rule
/// rather than in a groove of its own.
/// Put away and opened again, a space's overview returns to where it
/// was left — the section, the row it was on, and edits not yet saved
/// — and is read again rather than shown as it was.
#[test]
fn a_space_s_overview_is_found_as_it_was_left() {
    use uze_application::{DeclarationKey, DeclaredValue};
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Settings;
    overview.rows[1] = 3;
    overview.edit.values.insert(
        DeclarationKey::Delivery,
        Some(DeclaredValue::Text("merge".to_owned())),
    );
    overview.answer = Some(Err("read".to_owned()));

    model.put_away_space_overview();
    assert!(model.space_overview.is_none());
    model.open_space_overview(space, behind, true);

    let overview = model.space_overview.as_ref().expect("open again");
    assert_eq!(
        overview.section,
        crate::ui::orchestrator::space_overview::Section::Settings
    );
    assert_eq!(overview.rows[1], 3);
    assert_eq!(
        overview.edit.values.get(&DeclarationKey::Delivery),
        Some(&Some(DeclaredValue::Text("merge".to_owned()))),
        "an unsaved edit was lost"
    );
    assert!(overview.stale, "a kept overview is read again");
}

#[test]
fn the_overview_drawer_resizes_and_carries_the_scroll_handle() {
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-drawer"));
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let facts: Vec<Entry> = (0..60)
        .map(|index| Entry {
            name: format!("fact-{index:02}"),
            description: String::new(),
            kind: Kind::Feedback,
            scope: MemoryScope::Project,
            origin: "uze".to_owned(),
            source: None,
            path: PathBuf::from(format!("/facts/fact-{index:02}.md")),
            updated: None,
            writable: true,
            proposed: false,
        })
        .collect();
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
    overview.answer = Some(Ok(project_space(
        uze_application::SpaceMemory {
            project: facts,
            ..Default::default()
        },
        Err("not read here".to_owned()),
    )));
    let mut driven = driven(model, &home);
    driven.area = Rect::new(0, 0, 160, 40);
    driven.frame();
    let rule = driven.hit(|hit| *hit == WorkspaceHit::ResizeOverviewDrawer);

    let rows = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 160, 40));
    let thumb = theme::glyph(crate::ui::theme::Symbol::ScrollThumb);
    let columns_with_thumb: std::collections::BTreeSet<usize> = rows
        .iter()
        .flat_map(|row| {
            row.chars()
                .enumerate()
                .filter(|(_, glyph)| thumb.starts_with(*glyph))
                .map(|(column, _)| column)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        columns_with_thumb,
        std::collections::BTreeSet::from([usize::from(rule.x)]),
        "the scroll handle is not on the drawer's rule"
    );

    driven.press(rule.x, rule.y + 2);
    driven.mouse(
        rule.x - 12,
        rule.y + 2,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        rule.x - 12,
        rule.y + 2,
        MouseEventKind::Up(MouseButton::Left),
    );
    driven.frame();
    let moved = driven.hit(|hit| *hit == WorkspaceHit::ResizeOverviewDrawer);
    assert_eq!(
        moved.x,
        rule.x - 12,
        "dragging the rule did not widen the drawer"
    );
    assert!(!driven.attach.model.dragging_overview_drawer);
}

fn driven_again<'a>(home: &'a UzeHome, session: &uze_terminal::Session) -> Driven<'a> {
    driven(model_of(session.clone()), home)
}

/// The pointer lifts the − or + it is over, and a pair's option it is
/// over, without choosing either.
#[test]
fn a_stepper_s_button_and_a_pair_s_option_lift_under_the_pointer() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-lift"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-lift");
    let mut driven = settings_over(&home, &repository);
    driven.frame();
    let rect_of = |driven: &Driven<'_>, wanted: WorkspaceHit| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| (*hit == wanted).then_some(*rect))
            .unwrap_or_else(|| panic!("{wanted:?} is not drawn"))
    };
    let hover = theme::color(crate::ui::theme::Token::SurfaceHover);
    for wanted in [
        WorkspaceHit::OverviewStep(1, false),
        // `always`: `manual` is the default, so already chosen.
        WorkspaceHit::OverviewChoice(0, 1),
    ] {
        let rect = rect_of(&driven, wanted);
        driven.mouse(rect.x, rect.y, MouseEventKind::Moved);
        let area = driven.area;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(area.width, area.height))
                .unwrap();
        terminal
            .draw(|frame| {
                super::render::render(
                    frame,
                    &driven.attach.model,
                    &identities_fixture(),
                    &mut Vec::new(),
                    &mut super::render::FrameMetrics::default(),
                )
            })
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(rect.x, rect.y)].bg,
            hover,
            "{wanted:?} did not lift"
        );
    }
    assert!(
        overview_of(&driven).edit.values.is_empty(),
        "hovering chose something"
    );
}

/// A click on another row leaves what is being typed: an action's prose is
/// kept and saved, and the clicked row takes the keys.
#[test]
fn a_click_elsewhere_keeps_the_prose_and_moves_the_focus() {
    use crate::ui::orchestrator::space_overview::WorkspaceRow;
    use crossterm::event::KeyCode;
    use uze_application::ActionState;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-blur"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-blur");
    let mut driven = settings_over(&home, &repository);
    let rows = overview_of(&driven).workspace_rows();
    let action = rows
        .iter()
        .position(|row| *row == WorkspaceRow::Action(ActionState::ALL[0]))
        .expect("the first action is a row");
    driven
        .attach
        .model
        .space_overview
        .as_mut()
        .expect("open")
        .rows[1] = action;
    key(&mut driven, KeyCode::Enter);
    for typed in "Commit.".chars() {
        key(&mut driven, KeyCode::Char(typed));
    }
    assert!(
        overview_of(&driven).edit.text.is_some(),
        "the text area did not open"
    );

    driven.frame();
    let worktree = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::OverviewRow(0)).then_some(*rect))
        .expect("worktree's row is a target");
    driven.press(worktree.x, worktree.y);

    let overview = overview_of(&driven);
    assert!(overview.edit.text.is_none(), "the text area kept the keys");
    assert_eq!(
        overview.rows[1], 0,
        "the clicked row did not take the focus"
    );
    let sent = overview
        .edit
        .saving
        .as_ref()
        .map(|out| out.actions.get(&ActionState::ALL[0]).cloned());
    let held = overview.edit.actions.get(&ActionState::ALL[0]).cloned();
    assert!(
        sent == Some(Some(Some("Commit.".to_owned()))) || held == Some(Some("Commit.".to_owned())),
        "the prose was lost: sent {sent:?}, held {held:?}"
    );
}

/// A choice of up to three is drawn side by side, each option a target;
/// past three it goes behind a list.
#[test]
fn delivery_draws_its_three_options_side_by_side() {
    use crate::ui::orchestrator::space_overview::WorkspaceRow;
    use uze_application::{DeclarationKey, DeclaredValue};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-three"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-three");
    let mut driven = settings_over(&home, &repository);
    driven.frame();
    let row_of = |key| {
        overview_of(&driven)
            .workspace_rows()
            .iter()
            .position(|row| *row == WorkspaceRow::Key(key))
            .expect("the key is a row")
    };
    let delivery = row_of(DeclarationKey::Delivery);
    let branch = row_of(DeclarationKey::Branch);
    let targets = |row: usize| -> Vec<Rect> {
        driven
            .attach
            .model
            .hits
            .iter()
            .filter(|(_, hit)| matches!(hit, WorkspaceHit::OverviewChoice(at, _) if *at == row))
            .map(|(rect, _)| *rect)
            .collect()
    };
    assert_eq!(
        targets(delivery).len(),
        3,
        "delivery's options are not side by side"
    );
    assert!(
        targets(branch).is_empty(),
        "five options are drawn side by side"
    );

    let push = targets(delivery)[2];
    driven.press(push.x, push.y);
    let chosen = overview_of(&driven)
        .edit
        .saving
        .as_ref()
        .and_then(|out| out.values.get(&DeclarationKey::Delivery).cloned())
        .or_else(|| {
            overview_of(&driven)
                .edit
                .values
                .get(&DeclarationKey::Delivery)
                .cloned()
        });
    assert_eq!(chosen, Some(Some(DeclaredValue::Text("push".to_owned()))));
}
