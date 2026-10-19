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

/// The Agent section holds the support answer read for its agent, listed
/// in the project's and the harness's groups: the
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
    let row_of = |text: &str| {
        rows.iter()
            .position(|row| row.contains(text))
            .unwrap_or_else(|| panic!("no {text}:\n{}", rows.join("\n")))
    };
    // In its two groups: what of the project reaches the agent, then what
    // its harness supports.
    assert!(row_of("project") < row_of("AGENTS.md"));
    assert!(row_of("AGENTS.md") < row_of("supports"));
    assert!(row_of("supports") < row_of("Skills"));
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
                == crate::ui::orchestrator::space_overview::Section::Memory),
        "4 did not show the Memory section"
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
            cites: Vec::new(),
            missing: Vec::new(),
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

/// `/` searches the Memory section: what is typed narrows the facts by
/// name, description, kind or origin, the count says how many of all it
/// let through, enter reads the one selected among them, and esc takes the
/// words away before it takes the overview away.
#[test]
fn the_memory_search_narrows_the_facts_and_reads_the_one_selected() {
    use crossterm::event::{KeyCode, KeyEvent};
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-memory-search",
    ));
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let fact = |name: &str, description: &str, origin: &str| Entry {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: Kind::Feedback,
        scope: MemoryScope::Project,
        origin: origin.to_owned(),
        source: None,
        path: PathBuf::from(format!("/facts/{name}.md")),
        updated: None,
        writable: true,
        proposed: false,
        cites: Vec::new(),
        missing: Vec::new(),
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
    overview.answer = Some(Ok(project_space(
        uze_application::SpaceMemory {
            project: vec![
                fact(
                    "reply-in-ptbr",
                    "answer in Brazilian Portuguese",
                    "claude-code",
                ),
                fact("wsl-kill", "kill a group by pid with libc", "uze"),
                fact("lab-image", "the Lab image matches the checkout", "codex"),
            ],
            ..Default::default()
        },
        Err("not read here".to_owned()),
    )));

    let mut driven = driven(model, &home);
    driven.frame();
    let key = |code| KeyEvent::from(code);
    driven.press_key(key(KeyCode::Char('/')));
    for character in "LIBC".chars() {
        driven.press_key(key(KeyCode::Char(character)));
    }
    let rows = frame_rows(&mut driven.attach.model);
    let screen = rows.join("\n");
    assert!(screen.contains("wsl-kill"), "the match is gone:\n{screen}");
    assert!(
        !screen.contains("reply-in-ptbr") && !screen.contains("lab-image"),
        "facts the words do not match are still listed:\n{screen}"
    );
    assert!(
        screen.contains("1 of 3 facts"),
        "the count does not say it narrowed:\n{screen}"
    );

    // Enter leaves the search on its words and reads the fact selected
    // among those it let through, not the first of all of them.
    driven.press_key(key(KeyCode::Enter));
    let overview = driven.attach.model.space_overview.as_ref().expect("open");
    assert!(!overview.filtering, "enter left the search taking text");
    assert_eq!(overview.memory_filter, "LIBC");
    assert_eq!(
        overview.fact.as_ref().map(|(name, _)| name.as_str()),
        Some("wsl-kill")
    );

    // Esc puts the fact away, then takes the words away, and only then
    // the overview.
    for _ in 0..3 {
        driven.press_key(key(KeyCode::Esc));
    }
    let overview = driven
        .attach
        .model
        .space_overview
        .as_ref()
        .expect("still open");
    assert!(overview.memory_filter.is_empty(), "esc kept the words");
    assert_eq!(overview.memory_facts().len(), 3);
}

/// A fact the machine keeps reaches this project's agents as much as the
/// project's own do, so it is listed beside them, saying which it is.
#[test]
fn machine_facts_are_listed_beside_the_projects() {
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let fact = |name: &str, scope| Entry {
        name: name.to_owned(),
        description: String::new(),
        kind: Kind::User,
        scope,
        origin: "uze".to_owned(),
        source: None,
        path: PathBuf::from(format!("/facts/{name}.md")),
        updated: None,
        writable: true,
        proposed: false,
        cites: Vec::new(),
        missing: Vec::new(),
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
    overview.answer = Some(Ok(project_space(
        uze_application::SpaceMemory {
            project: vec![fact("reply-in-ptbr", MemoryScope::Project)],
            machine: vec![fact("user-favorite-anime", MemoryScope::Machine)],
            ..Default::default()
        },
        Err("not read here".to_owned()),
    )));

    let screen = frame_rows_at(&mut model, Rect::new(0, 0, 200, 30)).join("\n");
    let anime = screen
        .lines()
        .find(|row| row.contains("user-favorite-anime"))
        .unwrap_or_else(|| panic!("the machine fact is not listed:\n{screen}"));
    assert!(
        anime.contains("machine"),
        "its scope is not said:\n{screen}"
    );
    assert!(
        screen.contains("2 facts"),
        "the count leaves it out:\n{screen}"
    );
}

/// The facts read newest first, whichever memory keeps them, and one with
/// no date goes last.
#[test]
fn memory_facts_are_listed_newest_first() {
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let fact = |name: &str, scope, updated: Option<&str>| Entry {
        name: name.to_owned(),
        description: String::new(),
        kind: Kind::Project,
        scope,
        origin: "uze".to_owned(),
        source: None,
        path: PathBuf::from(format!("/facts/{name}.md")),
        updated: updated.map(str::to_owned),
        writable: true,
        proposed: false,
        cites: Vec::new(),
        missing: Vec::new(),
    };
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.answer = Some(Ok(project_space(
        uze_application::SpaceMemory {
            project: vec![
                fact("undated", MemoryScope::Project, None),
                fact("oldest", MemoryScope::Project, Some("2026-09-01")),
                fact("newest", MemoryScope::Project, Some("2026-10-17")),
            ],
            machine: vec![fact("middle", MemoryScope::Machine, Some("2026-10-02"))],
            ..Default::default()
        },
        Err("not read here".to_owned()),
    )));

    let names: Vec<&str> = overview
        .memory_facts()
        .into_iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["newest", "middle", "oldest", "undated"]);
}

/// A fact's body is read as the Markdown it is written in: emphasis is
/// drawn, not spelled out in asterisks.
#[test]
fn a_fact_body_is_drawn_as_markdown() {
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = crate::ui::orchestrator::space_overview::Section::Memory;
    overview.answer = Some(Ok(project_space(
        uze_application::SpaceMemory {
            project: vec![Entry {
                name: "reply-in-ptbr".to_owned(),
                description: "reply in pt-BR".to_owned(),
                kind: Kind::Feedback,
                scope: MemoryScope::Project,
                origin: "claude-code".to_owned(),
                source: None,
                path: PathBuf::from("/facts/reply-in-ptbr.md"),
                updated: None,
                writable: false,
                proposed: false,
                cites: Vec::new(),
                missing: Vec::new(),
            }],
            ..Default::default()
        },
        Err("not read here".to_owned()),
    )));
    overview.drawer = true;
    overview.fact = Some((
        "reply-in-ptbr".to_owned(),
        Some(Ok("Reply in Portuguese.\n\n**Why:** they asked.".to_owned())),
    ));

    let screen = frame_rows_at(&mut model, Rect::new(0, 0, 80, 40)).join("\n");
    assert!(screen.contains("Why:"), "the body is not drawn:\n{screen}");
    assert!(
        !screen.contains("**Why"),
        "the emphasis is spelled out:\n{screen}"
    );
}

/// The overview open on its Memory section, its drawer standing over the
/// section, listing a fact per name.
fn memory_overview(names: &[&str]) -> WorkspaceModel {
    use uze_application::application::memory::{Entry, Kind, MemoryScope};
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let facts = names
        .iter()
        .map(|name| Entry {
            name: (*name).to_owned(),
            description: String::new(),
            kind: Kind::Feedback,
            scope: MemoryScope::Project,
            origin: "uze".to_owned(),
            source: None,
            path: PathBuf::from(format!("/facts/{name}.md")),
            updated: None,
            writable: true,
            proposed: false,
            cites: Vec::new(),
            missing: Vec::new(),
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
    overview.drawer = true;
    model
}

/// A body of `count` numbered lines, each its own paragraph.
fn long_body(count: usize) -> String {
    (0..count)
        .map(|line| format!("body-line-{line:02}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn drawer_rect(driven: &Driven<'_>) -> Rect {
    driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::OverviewDrawer)
        .map(|(rect, _)| *rect)
        .expect("the drawer's text is a target")
}

/// A fact's body longer than the drawer scrolls: its handle is drawn, the
/// wheel over the drawer reaches the last line, and the way back up costs
/// only what the text has, not the notches spent past its end.
#[test]
fn a_long_fact_body_scrolls_to_its_last_line_in_the_drawer() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-drawer"));
    let mut model = memory_overview(&["long-fact"]);
    model.space_overview.as_mut().expect("open").fact =
        Some(("long-fact".to_owned(), Some(Ok(long_body(30)))));

    let rows = frame_rows(&mut model);
    let thumb = theme::glyph(crate::ui::theme::Symbol::ScrollThumb);
    assert!(
        rows.iter().any(|row| row.contains(thumb.as_str())),
        "a body longer than the drawer drew no scrollbar:\n{}",
        rows.join("\n")
    );
    assert!(
        !rows.iter().any(|row| row.contains("body-line-29")),
        "the body's last line is on screen before scrolling"
    );

    let mut driven = driven(model, &home);
    driven.frame();
    let drawer = drawer_rect(&driven);
    for _ in 0..40 {
        driven.mouse(drawer.x + 1, drawer.y + 1, MouseEventKind::ScrollDown);
    }
    let rows = frame_rows(&mut driven.attach.model);
    assert!(
        rows.iter().any(|row| row.contains("body-line-29")),
        "the wheel did not reach the body's last line:\n{}",
        rows.join("\n")
    );
    let memory = crate::ui::orchestrator::space_overview::Section::Memory.index();
    let overview = driven.attach.model.space_overview.as_ref().expect("open");
    assert_eq!(overview.rows[memory], 0, "the wheel moved the selection");

    driven.frame();
    for _ in 0..6 {
        driven.press_key(crossterm::event::KeyEvent::from(
            crossterm::event::KeyCode::PageUp,
        ));
    }
    let rows = frame_rows(&mut driven.attach.model);
    assert!(
        rows.iter().any(|row| row.contains("memory · read-only")),
        "page up did not bring the drawer back to its top:\n{}",
        rows.join("\n")
    );
}

/// The drawer is read from its top on every row it is opened on: the
/// next row, the same row come back to, and another section.
#[test]
fn choosing_another_row_reads_the_drawer_from_its_top() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-drawer-reset",
    ));
    let mut model = memory_overview(&["first-fact", "second-fact"]);
    model.space_overview.as_mut().expect("open").fact =
        Some(("first-fact".to_owned(), Some(Ok(long_body(30)))));
    let mut driven = driven(model, &home);
    let key = |code| crossterm::event::KeyEvent::from(code);
    let scrolled = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .expect("open")
            .drawer_scroll()
    };

    driven.frame();
    driven.press_key(key(crossterm::event::KeyCode::PageDown));
    driven.frame();
    assert!(scrolled(&driven) > 0, "page down did not scroll the drawer");

    driven.press_key(key(crossterm::event::KeyCode::Down));
    driven.frame();
    assert_eq!(scrolled(&driven), 0, "the next row opened scrolled");
    driven.press_key(key(crossterm::event::KeyCode::Up));
    driven.frame();
    assert_eq!(scrolled(&driven), 0, "the row come back to opened scrolled");

    driven
        .attach
        .model
        .space_overview
        .as_mut()
        .expect("open")
        .fact = Some(("first-fact".to_owned(), Some(Ok(long_body(30)))));
    driven.frame();
    driven.press_key(key(crossterm::event::KeyCode::PageDown));
    driven.frame();
    assert!(scrolled(&driven) > 0, "page down did not scroll the drawer");
    driven.press_key(key(crossterm::event::KeyCode::Right));
    driven.press_key(key(crossterm::event::KeyCode::Left));
    driven.frame();
    assert_eq!(
        scrolled(&driven),
        0,
        "another section left the drawer scrolled"
    );
}

/// On a terminal too narrow for the drawer beside the section, the drawer
/// is opened by Enter, and opens at its top whatever the page keys did
/// while it was closed.
#[test]
fn a_drawer_opened_reads_from_its_top_after_page_keys_while_closed() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-drawer-closed",
    ));
    let mut model = memory_overview(&["long-fact"]);
    model.space_overview.as_mut().expect("open").drawer = false;
    let mut driven = driven(model, &home);
    let key = |code| crossterm::event::KeyEvent::from(code);
    driven.press_key(key(crossterm::event::KeyCode::PageDown));
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    let overview = driven.attach.model.space_overview.as_ref().expect("open");
    assert!(overview.drawer, "Enter opened the drawer");
    assert_eq!(overview.drawer_scroll(), 0, "the drawer opened scrolled");
}

/// A fact citing what the checkout lacks is marked in the Memory tab, and
/// its drawer says which paths; a fact citing nothing carries no mark.
#[test]
fn a_fact_citing_what_the_checkout_lacks_is_marked_in_the_tab_and_the_drawer() {
    let mut model = memory_overview(&["moved-file", "still-there"]);
    let overview = model.space_overview.as_mut().expect("open");
    if let Some(Ok(uze_application::SpaceOverview::Project(project))) = overview.answer.as_mut() {
        project.memory.project[0].missing = vec!["crates/a/gone.rs".to_owned()];
    }
    overview.drawer = false;
    let mark = theme::glyph(crate::ui::theme::Symbol::MarkAttention);
    let rows = frame_rows(&mut model);
    let screen = rows.join("\n");
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("{mark} moved-file"))),
        "the stale fact is not marked:\n{screen}"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains(&format!("{mark} still-there"))),
        "a fact citing nothing is marked:\n{screen}"
    );

    let overview = model.space_overview.as_mut().expect("open");
    overview.drawer = true;
    // The mark sits under the fact's details, past a drawer this short:
    // scrolled to its end, which the frame settles on its last screenful.
    overview.scroll_drawer(i16::MAX);
    let screen = frame_rows(&mut model).join("\n");
    assert!(
        screen.contains("cites what this checkout does not have")
            && screen.contains("crates/a/gone.rs"),
        "the drawer does not name the missing path:\n{screen}"
    );
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
        .find(|row| row.contains("Worktree") && !row.contains("Worktrees"))
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
    // Said beside the key, not in a toast.
    assert!(
        driven.attach.model.remembered.toasts.is_empty(),
        "a write raised a toast"
    );
    assert!(
        overview
            .edit
            .saved
            .values
            .contains_key(&DeclarationKey::Worktree)
    );
    let screen = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 160, 45)).join("\n");
    let worktree = screen
        .lines()
        .find(|line| line.contains("Worktree") && !line.contains("Worktrees"))
        .expect("worktree is on the form");
    assert!(
        worktree.contains("saved"),
        "the key is not marked saved:\n{screen}"
    );
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
    let screen = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 200, 40)).join("\n");
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
            cites: Vec::new(),
            missing: Vec::new(),
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
    let (other, rect) = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::OverviewRow(row) if *row != action => Some((*row, *rect)),
            _ => None,
        })
        .expect("another row is on screen");
    driven.press(rect.x, rect.y);

    let overview = overview_of(&driven);
    assert!(overview.edit.text.is_none(), "the text area kept the keys");
    assert_eq!(
        overview.rows[1], other,
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

/// A segmented choice changes only on a click on one of its options: the
/// rest of its line selects it and leaves the value as it was.
#[test]
fn a_click_beside_a_segmented_choice_selects_it_without_changing_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-label"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-label");
    let mut driven = settings_over(&home, &repository);
    driven
        .attach
        .model
        .space_overview
        .as_mut()
        .expect("open")
        .rows[1] = 1;
    driven.frame();
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::OverviewRow(0)).then_some(*rect))
        .expect("worktree's row is a target");
    driven.press(row.x + 2, row.y);
    let overview = overview_of(&driven);
    assert_eq!(overview.rows[1], 0, "the click did not select the row");
    assert!(
        overview.edit.values.is_empty() && overview.edit.saving.is_none(),
        "a click on the label changed the choice"
    );
}

/// A click on no target at all — here the group heading above — still
/// leaves an open list of options, closing it without choosing.
#[test]
fn a_click_on_nothing_closes_an_open_combobox() {
    use crate::ui::orchestrator::space_overview::WorkspaceRow;
    use crossterm::event::KeyCode;
    use uze_application::DeclarationKey;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-outside"));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-outside");
    let mut driven = settings_over(&home, &repository);
    let target = overview_of(&driven)
        .workspace_rows()
        .iter()
        .position(|row| *row == WorkspaceRow::Key(DeclarationKey::Target))
        .expect("the target is a row");
    driven
        .attach
        .model
        .space_overview
        .as_mut()
        .expect("open")
        .rows[1] = target;
    driven.frame();
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::OverviewRow(target)).then_some(*rect))
        .expect("the target's row is drawn");
    key(&mut driven, KeyCode::Enter);
    assert!(
        overview_of(&driven).edit.field.is_some(),
        "the combobox did not open"
    );

    driven.press(row.x + 2, row.y - 1);
    let overview = overview_of(&driven);
    assert!(overview.edit.field.is_none(), "the combobox stayed open");
    assert!(
        overview.edit.values.is_empty(),
        "closing it chose an option"
    );
}

/// The selected prompt slides through what its column cannot show, and
/// asks for the next frame to keep sliding; the others stay cut.
#[test]
fn the_selected_prompt_slides_and_the_others_hold_still() {
    let mut model = agent_session_in("/repo");
    let agent = drawer_over(&model);
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
    let long = format!("start {} end", "of a long prompt ".repeat(12));
    model.remembered.drawer_prompts = Some(crate::ui::orchestrator::PromptHistoryResolution {
        root,
        entries: vec![
            uze_application::PromptEntry::new(&origin, &long).unwrap(),
            uze_application::PromptEntry::new(&origin, &format!("other {long}")).unwrap(),
        ],
    });
    let frame_at = |model: &mut WorkspaceModel, tick: usize| {
        model.tick = tick;
        let area = Rect::new(0, 0, 140, 30);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(area.width, area.height))
                .unwrap();
        let mut metrics = super::render::FrameMetrics::default();
        terminal
            .draw(|frame| {
                super::render::render(
                    frame,
                    model,
                    &identities_fixture(),
                    &mut Vec::new(),
                    &mut metrics,
                )
            })
            .unwrap();
        let rows: Vec<String> = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol().to_owned())
                    .collect()
            })
            .collect();
        (rows, metrics.marquee)
    };
    // The other prompt's line, and the selected one just above it: the
    // columns that would name them give way on a narrow section.
    let lines = |rows: &[String]| {
        let other = rows
            .iter()
            .position(|row| row.contains("other"))
            .unwrap_or_else(|| panic!("the other prompt is not drawn:\n{}", rows.join("\n")));
        (rows[other - 1].clone(), rows[other].clone())
    };
    let (before, sliding) = frame_at(&mut model, 0);
    assert!(
        sliding,
        "a selected prompt that does not fit asked for no next frame"
    );
    let (after, _) = frame_at(&mut model, 200);
    let (selected_before, other_before) = lines(&before);
    let (selected_after, other_after) = lines(&after);
    assert_ne!(
        selected_before, selected_after,
        "the selected prompt did not slide"
    );
    assert_eq!(other_before, other_after, "a prompt not selected moved");
}

/// `count` worktrees UZE made for agents, each on its own branch and of
/// its own size, plus one a person made, which the section leaves out.
fn worktrees(count: usize) -> uze_application::CheckoutsView {
    use uze_application::{CheckoutOwner, CheckoutView};
    let mut checkouts: Vec<CheckoutView> = (0..count)
        .map(|index| CheckoutView {
            path: PathBuf::from(format!("/repo/.worktrees/slot-{index:02}")),
            owner: CheckoutOwner::Agent {
                holder: Some(format!("agent-{index:02}")),
                live: true,
            },
            task: Some(format!("task-{index:02}")),
            branch: Some(format!("feat/w-{index:02}")),
            dirty: false,
            ahead: index,
            last_changed: None,
            bytes: (index as u64 + 1) * 3 * 1024 * 1024,
        })
        .collect();
    checkouts.push(CheckoutView {
        path: PathBuf::from("/elsewhere/by-hand"),
        owner: CheckoutOwner::Operator,
        task: None,
        branch: Some("someone/by-hand".to_owned()),
        dirty: true,
        ahead: 0,
        last_changed: None,
        bytes: 1,
    });
    uze_application::CheckoutsView {
        primary: PathBuf::from("/repo"),
        checkouts,
    }
}

/// A model with the overview open on its Worktrees section, answered
/// with `view`.
fn worktrees_over(view: Option<uze_application::CheckoutsView>) -> WorkspaceModel {
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("the overview is open");
    overview.section = Section::Worktrees;
    overview.answer = Some(Ok(project_space(
        Default::default(),
        Err("not read here".to_owned()),
    )));
    overview.worktrees.answer = Some(view);
    model
}

use crate::ui::orchestrator::space_overview::{Section, WorktreesResolution};

fn worktree_selected(driven: &Driven<'_>) -> usize {
    driven
        .attach
        .model
        .space_overview
        .as_ref()
        .expect("open")
        .rows[Section::Worktrees.index()]
}

fn card_rects(driven: &Driven<'_>) -> Vec<(usize, Rect)> {
    driven
        .attach
        .model
        .hits
        .iter()
        .filter_map(|(rect, hit)| match hit {
            WorkspaceHit::OverviewRow(index) => Some((*index, *rect)),
            _ => None,
        })
        .collect()
}

/// A card per worktree UZE made, with its branch and its size, and the
/// cause line adding them up; a checkout a person made is not one.
#[test]
fn the_worktrees_section_draws_a_card_per_worktree() {
    let mut model = worktrees_over(Some(worktrees(3)));
    let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, 40));
    let screen = rows.join("\n");
    for (branch, size) in [
        ("feat/w-00", "3.0 MB"),
        ("feat/w-01", "6.0 MB"),
        ("feat/w-02", "9.0 MB"),
    ] {
        assert!(screen.contains(branch), "{branch} has no card:\n{screen}");
        assert!(
            screen.contains(size),
            "{branch}'s size is missing:\n{screen}"
        );
    }
    assert!(
        !screen.contains("someone/by-hand"),
        "a person's checkout is drawn as the space's:\n{screen}"
    );
    assert!(
        screen.contains("3 worktrees · 18.0 MB on disk"),
        "the cause line does not add them up:\n{screen}"
    );
    assert!(screen.contains("working"), "no card says where it stands");
}

/// The keys walk the cards one at a time, and a click selects one.
#[test]
fn the_keys_and_a_click_select_a_worktree() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-keys",
    ));
    let mut driven = driven(worktrees_over(Some(worktrees(4))), &home);
    driven.area = Rect::new(0, 0, 160, 40);
    driven.frame();
    let down = || crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Down);
    driven.press_key(down());
    driven.press_key(down());
    assert_eq!(worktree_selected(&driven), 2);
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Up,
    ));
    assert_eq!(worktree_selected(&driven), 1);
    driven.frame();
    let (_, rect) = card_rects(&driven)
        .into_iter()
        .find(|(index, _)| *index == 3)
        .expect("the fourth card is drawn");
    driven.press(rect.x + 2, rect.y + 2);
    assert_eq!(worktree_selected(&driven), 3, "the click did not select it");
}

/// The wheel scrolls the grid a row of cards at a time and leaves the
/// selection where it was; only whole cards are ever drawn.
#[test]
fn the_wheel_scrolls_the_cards_without_selecting() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-wheel",
    ));
    let mut driven = driven(worktrees_over(Some(worktrees(24))), &home);
    driven.area = Rect::new(0, 0, 120, 30);
    driven.frame();
    let first_drawn = |driven: &Driven<'_>| {
        card_rects(driven)
            .iter()
            .map(|(index, _)| *index)
            .min()
            .expect("cards are drawn")
    };
    assert_eq!(first_drawn(&driven), 0);
    let (_, over) = card_rects(&driven)[0];
    for _ in 0..3 {
        driven.mouse(over.x + 2, over.y + 2, MouseEventKind::ScrollDown);
    }
    driven.frame();
    assert!(
        first_drawn(&driven) > 0,
        "the wheel did not scroll the cards"
    );
    assert_eq!(
        worktree_selected(&driven),
        0,
        "the wheel moved the selection"
    );
    let pane = driven.area;
    for (index, rect) in card_rects(&driven) {
        assert_eq!(rect.height, 5, "card {index} is cut: {rect:?}");
        assert!(
            rect.y > 8 && rect.bottom() < pane.bottom(),
            "{index}: {rect:?}"
        );
    }
}

/// The pointer over a card lifts its border, short of the accent the
/// selected card wears, and selects nothing.
#[test]
fn a_worktree_card_lifts_under_the_pointer() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-lift",
    ));
    let mut driven = driven(worktrees_over(Some(worktrees(3))), &home);
    driven.area = Rect::new(0, 0, 160, 40);
    driven.frame();
    let rect_of = |driven: &Driven<'_>, wanted: usize| {
        card_rects(driven)
            .into_iter()
            .find(|(index, _)| *index == wanted)
            .expect("the card is drawn")
            .1
    };
    let border = |driven: &Driven<'_>, rect: Rect| {
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
        terminal.backend().buffer()[(rect.x, rect.y)].fg
    };
    let tone = |token| theme::color(token);
    use crate::ui::theme::Token;
    let resting = rect_of(&driven, 2);
    assert_eq!(border(&driven, resting), tone(Token::BorderFaint));
    assert_eq!(border(&driven, rect_of(&driven, 0)), tone(Token::Accent));
    driven.mouse(resting.x + 2, resting.y + 2, MouseEventKind::Moved);
    assert_eq!(
        border(&driven, resting),
        tone(Token::BorderDefault),
        "the card under the pointer did not lift"
    );
    assert_eq!(worktree_selected(&driven), 0, "hovering selected the card");
}

/// Bringing the section to front asks for the measure off the frame,
/// which says it is measuring meanwhile; an answer to an earlier read, or
/// to an overview put away, is dropped.
#[test]
fn the_worktrees_are_measured_off_the_frame_and_a_stale_answer_is_dropped() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-read",
    ));
    let mut model = worktrees_over(None);
    let overview = model.space_overview.as_mut().expect("open");
    overview.section = Section::Agent;
    overview.worktrees.answer = None;
    let space = overview.space;
    let mut driven = driven(model, &home);
    driven.frame();
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Char('5'),
    ));
    let read = |driven: &Driven<'_>| {
        let overview = driven.attach.model.space_overview.as_ref().expect("open");
        (
            overview.section,
            overview.worktrees.pending,
            overview.worktrees.asked,
        )
    };
    assert_eq!(
        read(&driven),
        (Section::Worktrees, true, 1),
        "no read was asked"
    );
    let rows = frame_rows(&mut driven.attach.model);
    assert!(
        rows.iter().any(|row| row.contains("Measuring worktrees")),
        "the section does not say it is measuring:\n{}",
        rows.join("\n")
    );

    let model = &mut driven.attach.model;
    let stale = WorktreesResolution {
        space,
        asked: 0,
        view: Some(worktrees(2)),
    };
    assert!(
        !model.absorb_space_worktrees(stale),
        "an earlier read was kept"
    );
    let elsewhere = WorktreesResolution {
        space: uze_terminal::SpaceId(space.0 + 1),
        asked: 1,
        view: Some(worktrees(2)),
    };
    assert!(
        !model.absorb_space_worktrees(elsewhere),
        "another space's read was kept"
    );
    let current = WorktreesResolution {
        space,
        asked: 1,
        view: Some(worktrees(2)),
    };
    assert!(model.absorb_space_worktrees(current));
    let overview = model.space_overview.as_ref().expect("open");
    assert!(!overview.worktrees.pending);
    assert_eq!(overview.worktrees().len(), 2);

    model.put_away_space_overview();
    let late = WorktreesResolution {
        space,
        asked: 1,
        view: Some(worktrees(5)),
    };
    assert!(
        !model.absorb_space_worktrees(late),
        "an overview put away took an answer"
    );
}

/// With no worktree, the section says every agent works in this checkout.
#[test]
fn no_worktrees_says_every_agent_works_here() {
    let mut model = worktrees_over(Some(uze_application::CheckoutsView {
        primary: PathBuf::from("/repo"),
        checkouts: Vec::new(),
    }));
    let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, 40));
    assert!(
        rows.iter().any(|row| row.contains("NO WORKTREES"))
            && rows
                .iter()
                .any(|row| row.contains("Every agent works in this checkout")),
        "{}",
        rows.join("\n")
    );
}

/// Uncommitted work with no agent there warns on the section's tab; a
/// worktree whose agent is working there does not.
#[test]
fn uncommitted_work_nobody_attends_warns() {
    use crate::ui::orchestrator::space_overview::Health;
    let mut view = worktrees(2);
    let mut model = worktrees_over(Some(view.clone()));
    let health = |model: &WorkspaceModel| {
        model
            .space_overview
            .as_ref()
            .expect("open")
            .health(Section::Worktrees)
    };
    assert_eq!(health(&model), Health::Fine);
    view.checkouts[0].dirty = true;
    view.checkouts[0].owner = uze_application::CheckoutOwner::Agent {
        holder: Some("gone".to_owned()),
        live: false,
    };
    model
        .space_overview
        .as_mut()
        .expect("open")
        .worktrees
        .answer = Some(Some(view));
    assert_eq!(health(&model), Health::Warning);
}

/// The cards stand by where each worktree stands — a free slot behind the
/// ones an agent holds — and the selection follows its worktree when a
/// new answer reorders them.
#[test]
fn worktrees_are_ordered_by_where_they_stand_and_the_selection_follows() {
    use uze_application::CheckoutOwner;
    let mut view = worktrees(3);
    // The first slot is free: it goes last.
    view.checkouts[0].owner = CheckoutOwner::Agent {
        holder: None,
        live: false,
    };
    view.checkouts[0].task = None;
    let mut model = worktrees_over(Some(view.clone()));
    let order = |model: &WorkspaceModel| -> Vec<PathBuf> {
        let overview = model.space_overview.as_ref().expect("open");
        let project = overview.project().expect("answered");
        model
            .worktree_cards(overview, project)
            .iter()
            .map(|card| card.checkout.path.clone())
            .collect()
    };
    let slot = |index: usize| PathBuf::from(format!("/repo/.worktrees/slot-{index:02}"));
    assert_eq!(order(&model), vec![slot(1), slot(2), slot(0)]);

    // On the free one, then it is taken: it moves up, the keys with it.
    let section = Section::Worktrees.index();
    model.space_overview.as_mut().expect("open").rows[section] = 2;
    view.checkouts[0].owner = CheckoutOwner::Agent {
        holder: Some("agent-00".to_owned()),
        live: true,
    };
    view.checkouts[0].task = Some("task-00".to_owned());
    let overview = model.space_overview.as_mut().expect("open");
    overview.worktrees.asked += 1;
    let asked = overview.worktrees.asked;
    let space = overview.space;
    model.absorb_space_worktrees(WorktreesResolution {
        space,
        asked,
        view: Some(view),
    });
    let at = model.space_overview.as_ref().expect("open").rows[section];
    assert_eq!(
        order(&model)[at],
        slot(0),
        "the selection stayed on its place"
    );
}

/// Two worktrees: the first an agent works in, the second a free slot no
/// task holds.
fn with_a_free_one() -> uze_application::CheckoutsView {
    let mut view = worktrees(2);
    view.checkouts[1].owner = uze_application::CheckoutOwner::Agent {
        holder: None,
        live: false,
    };
    view.checkouts[1].task = None;
    view
}

/// The overview on its Worktrees section, the free worktree selected.
fn on_the_free_worktree(home: &UzeHome) -> Driven<'_> {
    let mut driven = driven(worktrees_over(Some(with_a_free_one())), home);
    driven.area = Rect::new(0, 0, 160, 40);
    driven.frame();
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Down,
    ));
    driven.frame();
    driven
}

fn removal_asked(driven: &Driven<'_>) -> bool {
    driven.attach.model.asks_overview_question()
}

fn press_char(driven: &mut Driven<'_>, character: char) {
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Char(character),
    ));
}

/// Only a free worktree offers removal: its drawer's button and the key.
/// A worktree an agent works in offers neither.
#[test]
fn only_a_free_worktree_offers_removal() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-removal-offered",
    ));
    let mut driven = driven(worktrees_over(Some(with_a_free_one())), &home);
    driven.area = Rect::new(0, 0, 160, 40);
    driven.frame();
    let offered = |driven: &Driven<'_>| {
        let hits = &driven.attach.model.hits;
        (
            hits.iter()
                .any(|(_, hit)| *hit == WorkspaceHit::OverviewRemoveWorktree),
            hits.iter()
                .any(|(_, hit)| *hit == WorkspaceHit::HintAction(uze_keys::Action::RemoveItem)),
        )
    };
    assert_eq!(offered(&driven), (false, false), "a working one is offered");
    press_char(&mut driven, 'd');
    assert!(!removal_asked(&driven), "the key asked about a working one");

    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Down,
    ));
    driven.frame();
    assert_eq!(
        offered(&driven),
        (true, true),
        "the free one is not offered"
    );
    let (button, _) = *driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::OverviewRemoveWorktree)
        .expect("the drawer's button");
    driven.press(button.x, button.y);
    assert!(removal_asked(&driven), "the button did not ask");
}

/// The question names the worktree and its size, and esc withdraws it
/// with nothing sent.
#[test]
fn the_removal_question_names_the_worktree_and_esc_withdraws_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-removal-asked",
    ));
    let mut driven = on_the_free_worktree(&home);
    press_char(&mut driven, 'd');
    assert!(removal_asked(&driven));
    let screen = frame_rows_at(&mut driven.attach.model, driven.area).join("\n");
    assert!(
        screen.contains("Remove this worktree?") && screen.contains(".worktrees/slot-01 (6.0 MB)"),
        "the question does not name what it removes:\n{screen}"
    );

    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Esc,
    ));
    assert!(!removal_asked(&driven));
    let overview = driven.attach.model.space_overview.as_ref();
    assert!(
        overview.is_some_and(|overview| !overview.removing),
        "esc put the overview away or removed something"
    );
    assert!(
        driven
            .attach
            .channels
            .worktree_removals
            .receiver
            .try_recv()
            .is_err()
    );
}

/// Confirming sends one removal off the frame and says so in the header;
/// no second is asked while it is out. A refusal is toasted, the card
/// stays, and the section is read again.
#[test]
fn a_confirmed_removal_runs_off_the_frame_and_a_refusal_is_said() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-removal-sent",
    ));
    let mut driven = on_the_free_worktree(&home);
    press_char(&mut driven, 'd');
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Enter,
    ));
    assert!(!removal_asked(&driven), "enter on cancel did not withdraw");
    assert!(
        !driven
            .attach
            .model
            .space_overview
            .as_ref()
            .unwrap()
            .removing,
        "enter on cancel removed it"
    );

    press_char(&mut driven, 'd');
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Right,
    ));
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Enter,
    ));
    assert!(!removal_asked(&driven));
    assert!(
        driven
            .attach
            .model
            .space_overview
            .as_ref()
            .unwrap()
            .removing
    );
    let notice = driven
        .attach
        .model
        .remembered
        .notice
        .as_ref()
        .map(|notice| notice.text.clone());
    assert_eq!(
        notice.as_deref(),
        Some("removing worktree .worktrees/slot-01")
    );
    press_char(&mut driven, 'd');
    assert!(!removal_asked(&driven), "a second removal was asked");

    let channels = &driven.attach.channels;
    let answered = channels
        .worktree_removals
        .receiver
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the removal answers");
    assert_eq!(answered.name, ".worktrees/slot-01");
    channels
        .worktree_removals
        .sender
        .send(RemovalResolution {
            outcome: Err("a process is working inside it".to_owned()),
            ..answered
        })
        .unwrap();
    let asked_before = driven
        .attach
        .model
        .space_overview
        .as_ref()
        .unwrap()
        .worktrees
        .asked;
    driven.attach.absorb_space_overview_answers();

    let model = &driven.attach.model;
    assert!(
        model.remembered.notice.is_none(),
        "the header still says it"
    );
    let toast = model.remembered.toasts.back().expect("the outcome is said");
    assert_eq!(toast.text, "worktree kept");
    assert_eq!(
        toast.detail,
        ".worktrees/slot-01: a process is working inside it"
    );
    let overview = model.space_overview.as_ref().unwrap();
    assert!(!overview.removing);
    assert_eq!(overview.worktrees().len(), 2, "the card went");
    assert!(
        overview.worktrees.pending && overview.worktrees.asked > asked_before,
        "the section was not read again"
    );
}

/// A removal that went says what it freed, and the section is read again.
#[test]
fn a_removed_worktree_says_what_it_freed_and_is_read_again() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-worktree-removal-done",
    ));
    let mut driven = on_the_free_worktree(&home);
    let space = driven.attach.model.space_overview.as_ref().unwrap().space;
    driven
        .attach
        .model
        .space_overview
        .as_mut()
        .unwrap()
        .removing = true;
    driven
        .attach
        .channels
        .worktree_removals
        .sender
        .send(RemovalResolution {
            space,
            name: ".worktrees/slot-01".to_owned(),
            bytes: 6 * 1024 * 1024,
            outcome: Ok(()),
        })
        .unwrap();
    driven.attach.absorb_space_overview_answers();

    let model = &driven.attach.model;
    let toast = model.remembered.toasts.back().expect("the outcome is said");
    assert_eq!(toast.text, "worktree removed");
    assert_eq!(toast.detail, ".worktrees/slot-01 · 6.0 MB freed");
    let overview = model.space_overview.as_ref().unwrap();
    assert!(!overview.removing);
    assert!(overview.worktrees.pending, "the section was not read again");
}

use crate::ui::orchestrator::worktree_removal::RemovalResolution;

/// A section with nothing to list says so as every empty surface does:
/// its title centred where the list would stand, what to do beneath.
#[test]
fn an_empty_section_is_said_in_the_middle_of_its_room() {
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("open");
    overview.section = Section::Plugins;
    overview.answer = Some(Ok(project_space(
        Default::default(),
        Err("not read".to_owned()),
    )));
    let height = 40;
    let rows = frame_rows_at(&mut model, Rect::new(0, 0, 160, height));
    let title = rows
        .iter()
        .position(|row| row.contains("NO PLUGINS DECLARED"))
        .unwrap_or_else(|| panic!("the empty plugins say nothing:\n{}", rows.join("\n")));
    assert!(
        title > usize::from(height) / 3,
        "the title stands at the top rather than in the middle: row {title}"
    );
    assert!(
        rows[title + 1..]
            .iter()
            .any(|row| row.contains("<plugin>@<market>")),
        "no hint under the title"
    );
    // Said once: no cause line repeating it, and no drawer about nothing.
    let screen = rows.join("\n");
    assert!(
        !screen.contains("no plugins declared"),
        "said twice:\n{screen}"
    );
    // With no drawer, the title is centred on the whole pane, right of
    // the sidebar, rather than on the half a drawer would leave.
    let column = rows[title].chars().collect::<Vec<_>>();
    let start = rows[title].find("NO PLUGINS DECLARED").expect("the title");
    let start = rows[title][..start].chars().count();
    let middle = start + "NO PLUGINS DECLARED".len() / 2;
    let pane_middle = (column.len() + 31) / 2;
    assert!(
        middle.abs_diff(pane_middle) <= 4,
        "the title is centred on {middle}, not on the pane's {pane_middle}:\n{screen}"
    );

    // The same for a space with no worktrees.
    let overview = model.space_overview.as_mut().expect("open");
    overview.section = Section::Worktrees;
    overview.worktrees.answer = Some(Some(uze_application::CheckoutsView {
        primary: PathBuf::from("/repo"),
        checkouts: Vec::new(),
    }));
    let screen = frame_rows_at(&mut model, Rect::new(0, 0, 160, height)).join("\n");
    assert!(screen.contains("NO WORKTREES"), "{screen}");
    assert!(!screen.contains("0 worktrees"), "said twice:\n{screen}");
}

/// The plugins are a compact table, the marketplace and the state among
/// its columns; a lock in sync is said by the cause line,
/// not by a line of its own. The drawer folds what it says as it narrows
/// rather than cutting it.
#[test]
fn the_plugins_are_a_compact_table_and_the_drawer_folds_as_it_narrows() {
    use uze_application::application::{Freshness, FreshnessState};
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let overview = model.space_overview.as_mut().expect("open");
    overview.section = Section::Plugins;
    let mut answer = project_space(Default::default(), Err("not read".to_owned()));
    if let uze_application::SpaceOverview::Project(project) = &mut answer {
        let plugin = |name: &str, state: FreshnessState| uze_application::SpacePlugin {
            plugin: name.to_owned(),
            package: format!("{name}@ai"),
            locked: true,
            installed: true,
            differs_from_lock: false,
            contributing: true,
            commit: Some("3605f817abcdef0123".to_owned()),
            freshness: Some(Freshness {
                state,
                established_at_unix: None,
            }),
        };
        project.plugins.plugins = vec![
            plugin("code", FreshnessState::UpToDate),
            plugin("git", FreshnessState::Behind { commits: Some(3) }),
        ];
    }
    overview.answer = Some(Ok(answer));

    let screen = frame_rows_at(&mut model, Rect::new(0, 0, 160, 24)).join("\n");
    assert!(
        screen.contains("2 plugins · agents.lock in sync"),
        "{screen}"
    );
    let row = |name: &str| {
        screen
            .lines()
            .find(|row| row.contains(&format!(" {name} ")) && row.contains(" ai "))
            .unwrap_or_else(|| panic!("no row for {name}:\n{screen}"))
            .to_owned()
    };
    assert!(row("code").contains("installed"), "{screen}");
    assert!(row("git").contains("3 behind"), "{screen}");
    assert!(
        !screen.lines().any(|row| row.contains("agents.lock ")
            && row.contains("in sync")
            && !row.contains("plugins")),
        "the lock in sync has a line of its own:\n{screen}"
    );

    // Narrow, the commit is folded in the drawer rather than cut short.
    let screen = frame_rows_at(&mut model, Rect::new(0, 0, 100, 24)).join("\n");
    assert!(screen.contains("3605f817abcdef0123"), "{screen}");
}

/// The foot counts what there is to check, and a click on the count lists
/// it over the client, harnesses that will not read the context included
/// now that no section says it; a click elsewhere or esc puts it away.
#[test]
fn the_count_in_the_foot_lists_the_space_s_issues() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-overview-issues"));
    let session = session("/repo");
    let space = session.selected_space().id;
    let behind = session.selected_tab().id;
    let mut model = model_of(session);
    model.open_space_overview(space, behind, true);
    let mut answer = project_space(Default::default(), Err("not read".to_owned()));
    if let uze_application::SpaceOverview::Project(project) = &mut answer {
        project
            .status
            .report
            .harnesses
            .push(uze_application::application::StatusHarness {
                integration: "codex".to_owned(),
                display_name: "Codex".to_owned(),
                context: None,
                context_unread: Some("the project is not trusted".to_owned()),
                agent_context: None,
                outdated: None,
            });
    }
    model.space_overview.as_mut().expect("open").answer = Some(Ok(answer));
    let mut driven = driven(model, &home);
    driven.area = Rect::new(0, 0, 160, 40);
    driven.frame();

    let count = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| *hit == WorkspaceHit::OverviewIssues)
            .map(|(rect, _)| *rect)
            .expect("the count is a button")
    };
    let screen = frame_rows_at(&mut driven.attach.model, driven.area).join("\n");
    // The harness, and agents.yaml unread.
    assert!(screen.contains("2 to check"), "{screen}");

    let button = count(&driven);
    // Under the pointer the words take the hue and keep their weight.
    driven.attach.model.hovered = Some(WorkspaceHit::OverviewIssues);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(
        driven.area.width,
        driven.area.height,
    ))
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
    let words = &terminal.backend().buffer()[(button.right() - 1, button.y)];
    assert!(
        !words.modifier.contains(ratatui::style::Modifier::BOLD),
        "the hover made the count bold"
    );
    assert_ne!(
        words.fg,
        theme::color(uze_theme::Token::TextMuted),
        "the hover lit nothing"
    );
    driven.attach.model.hovered = None;
    driven.frame();
    driven.press(button.x, button.y);
    assert!(driven.attach.model.shows_space_issues());
    let screen = frame_rows_at(&mut driven.attach.model, driven.area).join("\n");
    for said in [
        "To check",
        "Codex",
        "the project is not trusted",
        "agents.yaml",
    ] {
        assert!(screen.contains(said), "{said} is not listed:\n{screen}");
    }
    driven.frame();
    driven.press(0, 0);
    assert!(
        !driven.attach.model.shows_space_issues(),
        "a click outside kept it"
    );

    driven.frame();
    let button = count(&driven);
    driven.press(button.x, button.y);
    assert!(driven.attach.model.shows_space_issues());
    driven.press_key(crossterm::event::KeyEvent::from(
        crossterm::event::KeyCode::Esc,
    ));
    assert!(!driven.attach.model.shows_space_issues(), "esc kept it");
    assert!(
        driven.attach.model.space_overview.is_some(),
        "esc put the overview away with the list"
    );
}

/// Every key the overview's foot names is pressed by clicking it, in each
/// section a memory fixture draws, with its drawer open or not and while a
/// search is being typed: the click lands where the keystroke does, in the
/// scope that is live. The Agent section's own question is
/// `the_clear_prompts_question_answers_a_click`.
#[test]
fn clicking_an_overview_foot_hint_is_pressing_its_key() {
    use crate::ui::orchestrator::space_overview::Section;
    use uze_keys::Scope;
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-foot-hint-click",
    ));
    // Everything a foot key of these sections can move: a projection that
    // left one out would let a click that did nothing pass as the key.
    let state = |driven: &Driven<'_>| {
        format!(
            "{:?}",
            driven.attach.model.space_overview.as_ref().map(|overview| (
                overview.section,
                overview.filtering,
                overview.memory_filter.as_str(),
                overview.fact.as_ref().map(|(name, _)| name.as_str()),
                overview.drawer,
                overview.rows,
                overview.asked,
                overview.issues_open,
                overview
                    .edit
                    .field
                    .as_ref()
                    .map(|field| (field.key, field.item)),
                overview.edit.text.is_some(),
                overview.edit.values.len(),
                overview.question.is_some(),
            ))
        )
    };
    let browsing = [
        Section::Memory,
        Section::Plugins,
        Section::Worktrees,
        Section::Settings,
    ]
    .into_iter()
    .flat_map(|section| [(section, false, false), (section, true, false)]);
    let searching = [(Section::Memory, false, true)];
    for (section, drawer, filtering) in browsing.chain(searching) {
        let open = || {
            let mut model = memory_overview(&["a", "b", "c"]);
            let overview = model.space_overview.as_mut().expect("the overview is open");
            overview.section = section;
            overview.drawer = drawer;
            overview.filtering = filtering;
            let mut driven = driven(model, &home).on_a_roomy_terminal();
            driven.frame();
            driven
        };
        let live = if filtering {
            Scope::SpaceOverviewFilter
        } else {
            Scope::SpaceOverview
        };
        let scopes = [Scope::Global, Scope::Workspace, live];
        let hints: Vec<_> = open()
            .attach
            .model
            .hits
            .iter()
            .filter_map(|(rect, hit)| match hit {
                WorkspaceHit::HintAction(action) => Some((*rect, *action)),
                _ => None,
            })
            .collect();
        assert!(!hints.is_empty(), "{section:?} drew no hint to click");
        for (rect, action) in hints {
            let mut clicked = open();
            clicked.press(rect.x + rect.width / 2, rect.y);
            let chord = uze_keys::active()
                .chord_for(action, &scopes)
                .expect("a hint names a bound key");
            let mut pressed = open();
            pressed.press_key(key_event(chord));
            assert_eq!(
                state(&clicked),
                state(&pressed),
                "clicking {action:?} in {section:?} (drawer {drawer}, searching {filtering}) \
                 is not pressing it"
            );
        }
    }
}

/// Clearing a space's prompts is asked first, in a dialog over the
/// client: Cancel leaves them, Clear sends them away, each by a click.
#[test]
fn clearing_the_prompts_is_asked_in_a_dialog() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-clear-click",
    ));
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
    model.remembered.drawer_prompts = Some(crate::ui::orchestrator::PromptHistoryResolution {
        root,
        entries: vec![uze_application::PromptEntry::new(&origin, "fix the pipeline").unwrap()],
    });
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let clear = *driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::HintAction(uze_keys::Action::ClearPromptHistory))
        .map(|(rect, _)| rect)
        .expect("the foot offers clearing");
    let answer = |driven: &Driven<'_>, confirm: bool| {
        *driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| *hit == WorkspaceHit::OverviewAnswer(confirm))
            .map(|(rect, _)| rect)
            .expect("the dialog offers both answers")
    };

    driven.press(clear.x, clear.y);
    driven.frame();
    assert!(
        driven.attach.model.asks_overview_question(),
        "nothing was asked"
    );
    let screen = frame_rows_at(&mut driven.attach.model, driven.area).join("\n");
    assert!(
        screen.contains("Clear this space's prompts?"),
        "the dialog does not say what it clears:\n{screen}"
    );
    let cancel = answer(&driven, false);
    driven.press(cancel.x, cancel.y);
    assert!(!driven.attach.model.asks_overview_question());
    assert!(
        driven.attach.channels.prompts.receiver.try_recv().is_err(),
        "cancel cleared the prompts"
    );

    driven.frame();
    driven.press(clear.x, clear.y);
    driven.frame();
    let confirm = answer(&driven, true);
    driven.press(confirm.x, confirm.y);
    assert!(!driven.attach.model.asks_overview_question());
    let cleared = driven
        .attach
        .channels
        .prompts
        .receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("clear sent nothing off the frame");
    assert!(cleared.entries.is_empty(), "the prompts are still there");
}

/// A hint clicked while a field or an instruction is being typed is the key
/// the typing scope offers, not a click outside the form: "cancel" discards
/// what was typed instead of saving it first, and "apply" takes it.
#[test]
fn a_hint_clicked_while_typing_acts_in_the_typing_scope() {
    use crate::ui::orchestrator::space_overview::WorkspaceRow;
    use crossterm::event::KeyCode;
    use uze_application::{ActionState, DeclarationKey};
    use uze_keys::Action;
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-overview-typing-hint",
    ));
    let repository = uze_testkit::git::Repository::new("orchestrator-overview-typing-hint");
    fn typing_into_slots<'a>(
        home: &'a UzeHome,
        repository: &uze_testkit::git::Repository,
    ) -> Driven<'a> {
        let mut driven = settings_over(home, repository);
        key(&mut driven, KeyCode::Down);
        key(&mut driven, KeyCode::Enter);
        key(&mut driven, KeyCode::Char('x'));
        assert!(
            overview_of(&driven).edit.field.is_some(),
            "the slots field did not open"
        );
        driven
    }
    // The foot's hint for the key: the lowest one drawn, where the foot is.
    let foot_hint = |driven: &Driven<'_>, action: Action| {
        driven
            .attach
            .model
            .hits
            .iter()
            .filter(|(_, hit)| *hit == WorkspaceHit::HintAction(action))
            .map(|(rect, _)| *rect)
            .max_by_key(|rect| rect.y)
            .unwrap_or_else(|| panic!("the foot offers {action:?} while typing"))
    };

    let mut driven = typing_into_slots(&home, &repository);
    let cancel = foot_hint(&driven, Action::Dismiss);
    driven.press(cancel.x, cancel.y);
    let overview = overview_of(&driven);
    assert!(overview.edit.field.is_none(), "cancel left the field open");
    assert!(
        overview.edit.values.is_empty() && overview.edit.saving.is_none(),
        "cancel saved what was typed"
    );

    let mut driven = typing_into_slots(&home, &repository);
    let apply = foot_hint(&driven, Action::Activate);
    driven.press(apply.x, apply.y);
    let overview = overview_of(&driven);
    assert!(overview.edit.field.is_none(), "apply left the field open");
    assert!(
        overview.edit.values.contains_key(&DeclarationKey::Slots),
        "apply did not take what was typed"
    );

    // An instruction is the one typing state a click outside the form
    // saves outright, so it is the one "cancel" must be seen to discard.
    let mut driven = settings_over(&home, &repository);
    let action = overview_of(&driven)
        .workspace_rows()
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
        "the instruction did not open"
    );
    let cancel = foot_hint(&driven, Action::Dismiss);
    driven.press(cancel.x, cancel.y);
    let overview = overview_of(&driven);
    assert!(
        overview.edit.text.is_none(),
        "cancel left the instruction open"
    );
    assert!(
        overview.edit.actions.is_empty() && overview.edit.saving.is_none(),
        "cancel saved the instruction"
    );
}
