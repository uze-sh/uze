//! Starting agents and spaces: the agent picker, the new-space prompt and
//! the directories it offers.

use super::WorkspaceHit;
use super::support::*;
use super::{
    AgentView, RootPicker, WorkStateView, WorkspaceModel,
    render::{self, compute_layout},
};
use crate::ui::theme::{self, Token};
use crossterm::event::MouseEventKind;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::Path;
use uze_core::UzeHome;
use uze_terminal::ClientRequest;

/// The picker offers both kinds until the root's profile answers, and
/// The prompt asks one question — where — and any directory can
/// answer it. It used to ask for a kind as well, before the agent
/// existed and before anyone knew what it was for.
#[test]
fn the_picker_asks_only_where() {
    let plain = uze_testkit::temp::TempDir::new("picker-one-question");
    std::fs::create_dir_all(plain.join("notes")).unwrap();
    let mut model = agent_session_in("/repo");
    model.root_picker = Some(RootPicker::opened_in(
        &plain.path().display().to_string(),
        None,
    ));

    let Sidebar { rows, .. } = sidebar(&model, &identities_fixture());
    assert!(
        !rows.iter().any(|row| row.contains("worktree")),
        "no kind to choose: {rows:#?}"
    );
    assert_eq!(
        model.root_picker.as_ref().map(RootPicker::match_count),
        Some(1),
        "and a plain directory is offered like any other: {rows:#?}"
    );
}

/// A session whose one space is rooted at `root` — a real directory,
/// so the picker and the client compare the same canonical path.
fn session_rooted_at(root: &Path) -> WorkspaceModel {
    model_of(session(root))
}

/// Creating a space is reachable from the keyboard, and the chord opens
/// exactly what the pointer's `new` opens: the picker, listing where
/// the selected space's neighbours are and standing on the space.
#[test]
fn the_new_space_chord_opens_the_picker_the_pointer_opens() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-new-space-chord"));
    let mut driven = driven(agent_session_in("/repo"), &home);
    let chord = uze_keys::active()
        .chord_for(uze_keys::Action::NewSpace, &[uze_keys::Scope::Workspace])
        .expect("space creation is reachable from the keyboard");

    driven.press_key(key_event(chord));

    let picker = driven
        .attach
        .model
        .root_picker
        .as_ref()
        .expect("the picker opened");
    // Where a project beside this one would be. Whether the space's
    // own root is then marked is a question about real directories,
    // and `root_picker`'s own tests answer it over a temp tree — this
    // session's `/repo` is a name, not a directory.
    assert_eq!(
        picker.base(),
        Path::new("/"),
        "listing where a project beside this one would be"
    );
    assert!(
        picker.input().is_empty(),
        "with nothing typed for the operator"
    );
}

/// AltGr types into the picker: a Windows console reports it as
/// Ctrl+Alt, and `/` is AltGr+Q on ABNT2, so a path typed there lost
/// every separator.
#[test]
fn altgr_types_its_character_into_the_picker() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-altgr"));
    let mut driven = driven(agent_session_in("/repo"), &home);
    let chord = uze_keys::active()
        .chord_for(uze_keys::Action::NewSpace, &[uze_keys::Scope::Workspace])
        .expect("space creation is reachable from the keyboard");
    driven.press_key(key_event(chord));

    let alt_graph = KeyModifiers::CONTROL | KeyModifiers::ALT;
    driven.press_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    driven.press_key(KeyEvent::new(KeyCode::Char('/'), alt_graph));
    driven.press_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    let picker = driven
        .attach
        .model
        .root_picker
        .as_ref()
        .expect("the picker is still open");
    assert_eq!(picker.input(), "g/e");
}

/// The key opens the same menu the space header's button does, in the
/// same place — under the button, not wherever a keyboard gesture lands.
#[test]
fn the_new_agent_key_opens_the_picker_under_its_button() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-by-key"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
    driven.frame();
    let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);
    let chord = uze_keys::active()
        .chord_for(
            uze_keys::Action::NewAgent,
            &[uze_keys::Scope::Global, uze_keys::Scope::Workspace],
        )
        .expect("the picker is reachable from the keyboard");

    driven.press_key(key_event(chord));

    let picker = driven
        .attach
        .model
        .agent_picker
        .as_ref()
        .expect("the key opened the picker");
    assert_eq!(picker.anchor, sparkle, "anchored under the ✦ button");
}

/// Resuming from recent asks the same question as a new agent, and it
/// hangs off the row it was asked from — not off the frame's corner,
/// which is where a list with no click behind it once put it.
#[test]
fn resuming_from_recent_opens_the_picker_under_its_row() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-by-resume"));
    set_up_every_harness(&home);
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    // The harness it ran with is no longer set up, so there is a
    // question to ask.
    let mut kept = preserved("/repo", "t9", "kept", WorkStateView::Shelved);
    kept.harness = "a-retired-harness".to_owned();
    model.remembered.preserved_work = vec![kept];
    model.recent.open = true;
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::RecentRow(0)).then_some(*rect))
        .expect("the row is drawn");
    let resume = uze_keys::active()
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::Recent])
        .expect("enter is bound in recent");

    driven.press_key(key_event(resume));

    let picker = driven
        .attach
        .model
        .agent_picker
        .as_ref()
        .expect("the resume asked which harness");
    assert_eq!(picker.anchor, row, "anchored under the row it came from");
    assert!(
        !driven.attach.model.recent.open,
        "and recent made way for the tree"
    );
}

/// An agent is resumed with the harness it ran with, without being
/// asked: its conversation is that harness's, and the picker would
/// only offer the chance to start it somewhere it has none.
#[test]
fn resuming_from_recent_launches_the_harness_it_ran_with() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-resume-same-harness",
    ));
    set_up_every_harness(&home);
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    model.remembered.preserved_work =
        vec![preserved("/repo", "t9", "kept", WorkStateView::Shelved)];
    model.recent.open = true;
    let mut driven = driven(model, &home);

    let resume = uze_keys::active()
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::Recent])
        .expect("enter is bound in recent");
    driven.press_key(key_event(resume));

    assert!(
        driven.attach.model.agent_picker.is_none(),
        "nothing is asked"
    );
    assert!(
        driven.attach.model.placement_pending,
        "and the agent is being placed"
    );
}

/// A machine where no harness was set up has nothing to launch: the
/// picker lists none of the harnesses it merely knows, and its one row
/// takes the operator to Integrations, where one is set up.
#[test]
fn with_no_harness_set_up_the_agent_picker_leads_to_integrations() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-picker-unconfigured",
    ));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
    driven.frame();
    let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);
    driven.press(sparkle.x, sparkle.y);
    let picker = driven
        .attach
        .model
        .agent_picker
        .as_ref()
        .expect("the picker opened");
    assert!(
        picker.options.is_empty(),
        "no harness is offered before one is set up"
    );

    driven.frame();
    let set_up = driven.hit(|hit| *hit == WorkspaceHit::SetUpAgent);
    driven.press(set_up.x, set_up.y);
    assert!(
        driven.attach.model.agent_picker.is_none(),
        "the picker closed"
    );
    assert_eq!(manage_route(&driven), crate::ui::model::Route::Harnesses);
}

/// With one harness set up the picker would be a single row to
/// confirm, so asking for an agent starts it: the placement is asked
/// for at once and no picker opens.
#[test]
fn with_one_harness_set_up_a_new_agent_starts_without_a_picker() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-single"));
    let only = crate::ui::orchestrator::agent_identities(&home)
        .into_iter()
        .next()
        .expect("a harness is registered");
    set_up_harness(&home, &only);
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
    driven.frame();
    let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);

    driven.press(sparkle.x, sparkle.y);

    assert!(
        driven.attach.model.agent_picker.is_none(),
        "no picker for a choice of one"
    );
    assert!(
        driven.attach.model.placement_pending,
        "the agent was started"
    );
}

/// The control placing an agent wears the hue of the agent receiving
/// keystrokes, held back until the pointer asks for it.
#[test]
fn the_new_agent_control_wears_the_current_agents_hue_on_hover() {
    let hue_of_new = |model: &mut WorkspaceModel| {
        let area = Rect::new(0, 0, 80, 24);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render::render(
                    frame,
                    model,
                    &identities_fixture(),
                    &mut hits,
                    &mut render::FrameMetrics::default(),
                )
            })
            .unwrap();
        model.hits = hits;
        let buffer = terminal.backend().buffer().clone();
        let (y, row) = buffer_rows(&buffer)
            .into_iter()
            .enumerate()
            .find(|(_, row)| row.contains(" new"))
            .expect("the selected space carries the control");
        let column = row[..row.find(" new").unwrap()].chars().count() as u16 + 1;
        (column, y as u16, buffer[(column, y as u16)].fg)
    };
    let mut model = agent_with_task(WorkStateView::Ready, 1);
    let (column, row, resting) = hue_of_new(&mut model);
    assert_eq!(
        resting,
        theme::color(Token::StateWarningMuted),
        "held back at rest"
    );
    assert_eq!(
        model.hit_at(column, row),
        Some(WorkspaceHit::NewAgentMenu),
        "the pointer over the word is over the control, not the row it sits on"
    );

    model.hovered = Some(WorkspaceHit::NewAgentMenu);
    assert_eq!(
        hue_of_new(&mut model).2,
        theme::color(Token::StateWarning),
        "the pointer restores the hue"
    );
}

/// The `new` prompt is a chooser, not a text field: the sidebar
/// itself lists the directories the typed segment still matches, and
/// clicking one is the same choice Enter makes.
#[test]
fn the_new_space_prompt_lists_the_directories_it_matches() {
    let root = uze_testkit::temp::TempDir::new("sidebar-root-picker");
    for directory in ["engine", "extensions", "docs"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    let mut model = agent_session_in("/repo");
    model.root_picker = Some(RootPicker::opened_in(
        &root.path().display().to_string(),
        None,
    ));

    let Sidebar { rows, hits, .. } = sidebar(&model, &identities_fixture());
    assert!(
        rows.iter().any(|row| row.contains("engine"))
            && rows.iter().any(|row| row.contains("docs")),
        "the listing is on screen: {rows:?}"
    );
    assert!(
        hits.iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::PickSpaceRoot(_))),
        "every offered directory is clickable"
    );

    if let Some(picker) = model.root_picker.as_mut() {
        for character in "ex".chars() {
            picker.typed(character);
        }
    }
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        rows.iter().any(|row| row.contains("extensions")),
        "what matches stays: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("docs")),
        "what stopped matching is gone: {rows:?}"
    );
}

/// The picker is a small table: what is being typed and the
/// directories it matches, all in one column.
/// A name the query is the head of says so in the query's own hue; one
/// matched further in is left alone.
#[test]
fn the_picker_lines_its_rows_up_in_columns() {
    let root = uze_testkit::temp::TempDir::new("sidebar-picker-grid");
    for directory in ["craude", "cortex", "scribble"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    let mut model = agent_session_in("/repo");
    let mut picker = RootPicker::opened_in(&root.path().display().to_string(), None);
    for character in "cr".chars() {
        picker.typed(character);
    }
    model.root_picker = Some(picker);

    let Sidebar { rows, buffer, .. } = sidebar(&model, &identities_fixture());
    let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
    let prompt = rows
        .iter()
        .position(|row| row.contains(&cursor))
        .expect("the prompt is drawn");
    let column = |row: &str, text: &str| row[..row.find(text).unwrap()].chars().count();
    // What is typed and the directories it matches, all in one column.
    let values = column(&rows[prompt], "cr");
    for offset in 1..=2 {
        let row = &rows[prompt + offset];
        let name = row.trim_start();
        assert_eq!(
            column(row, name.split(' ').next().unwrap()),
            values,
            "every row answers in one column: {rows:?}"
        );
    }

    // "craude" is what "cr" is the head of; "scribble" matched further in.
    let head = |row: usize, at: u16| buffer[(values as u16 + at, row as u16)].fg;
    assert_eq!(head(prompt + 1, 0), theme::color(Token::Accent), "{rows:?}");
    assert_eq!(
        head(prompt + 2, 0),
        theme::color(Token::TextInactive),
        "{rows:?}"
    );
}

/// A listing taller than the column is reached with the wheel too: it
/// walks the directories the way the arrow keys do, and the window
/// follows the one selected.
#[test]
fn the_wheel_walks_the_directories_the_picker_offers() {
    let home = UzeHome::at(uze_testkit::temp::scratch("picker-wheel"));
    let root = uze_testkit::temp::TempDir::new("sidebar-picker-wheel");
    for index in 0..40 {
        std::fs::create_dir_all(root.join(format!("directory-{index:02}"))).unwrap();
    }
    let mut model = agent_session_in("/repo");
    model.root_picker = Some(RootPicker::opened_in(
        &root.path().display().to_string(),
        None,
    ));
    let mut driven = driven(model, &home);
    driven.frame();
    let last_row = |driven: &Driven<'_>| {
        let rows = sidebar(&driven.attach.model, &identities_fixture()).rows;
        rows.iter()
            .rev()
            .find(|row| row.contains("directory-"))
            .cloned()
            .expect("the listing is drawn")
    };
    let before = last_row(&driven);

    for _ in 0..25 {
        driven.mouse(1, 6, MouseEventKind::ScrollDown);
    }

    let picker = driven
        .attach
        .model
        .root_picker
        .as_ref()
        .expect("still open");
    assert_eq!(
        picker.selection(),
        Some(24),
        "the first turn takes the row in front, and the rest walk it"
    );
    assert_ne!(last_row(&driven), before, "and the window follows it");

    for _ in 0..40 {
        driven.mouse(1, 6, MouseEventKind::ScrollUp);
    }
    let picker = driven
        .attach
        .model
        .root_picker
        .as_ref()
        .expect("still open");
    assert_eq!(picker.selection(), Some(0), "and stops at the top");
}

/// The picker asks the one question a space has — where — standing
/// where the spaces it would join stand, with the directories directly
/// under it and no blank row wedged between them.
#[test]
fn the_prompt_stands_where_the_spaces_it_would_join_stand() {
    let root = uze_testkit::temp::TempDir::new("sidebar-root-place");
    for directory in ["engine", "extensions"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    let mut model = agent_session_in("/repo");
    let rows = sidebar(&model, &identities_fixture()).rows;
    let spaces_row = rows
        .iter()
        .position(|row| row.contains("repo"))
        .expect("the first space's header is drawn");

    model.root_picker = Some(RootPicker::opened_in(
        &root.path().display().to_string(),
        None,
    ));
    let Sidebar { rows, buffer, .. } = sidebar(&model, &identities_fixture());
    let prompt = rows
        .iter()
        .position(|row| row.contains(&theme::glyph(crate::ui::theme::Symbol::CursorText)))
        .expect("the prompt is drawn");

    assert_eq!(
        prompt, spaces_row,
        "the prompt stands where the spaces do: {rows:?}"
    );
    assert!(
        rows[prompt + 1].contains("engine"),
        "and the listing starts under it: {rows:?}"
    );
    // The panel on the darker of the two surfaces, and the row the
    // keyboard answers to — what is typed — lifted onto the lighter one.
    let surface = |row: usize| buffer[(2, row as u16)].bg;
    assert_eq!(
        surface(prompt),
        theme::color(Token::SurfaceRaised),
        "what is typed is lifted: {rows:?}"
    );
    // Nothing is chosen in the listing yet, so no row of it is lifted.
    for row in [prompt + 1, prompt + 2] {
        assert_eq!(
            surface(row),
            theme::color(Token::SurfaceRaisedSubtle),
            "the listing is the panel: {rows:?}"
        );
    }
}

/// The prompt opens with nothing in it: the directory it is rooted at
/// is the row's own context, not text waiting to be deleted.
#[test]
fn the_prompt_opens_empty_over_the_directory_it_is_rooted_at() {
    let mut model = agent_session_in("/repo");
    model.root_picker = Some(RootPicker::opened_in("~", None));

    let rows = sidebar(&model, &identities_fixture()).rows;
    let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
    let prompt = rows
        .iter()
        .find(|row| row.contains(&cursor))
        .expect("the prompt row is drawn");
    // The rail leading the row, stripped exactly once: it is the same
    // glyph the caret is drawn with (both are `bar.thin`, told apart
    // by hue), so trimming every leading one would take the caret
    // this asserts on with it.
    let rail = theme::glyph(crate::ui::theme::Symbol::BarThin);
    let typed = prompt
        .trim_start()
        .strip_prefix(rail.as_str())
        .unwrap_or(prompt)
        .trim_start();
    assert!(
        typed.starts_with(&cursor),
        "nothing is typed for the operator: {prompt}"
    );
    assert!(
        prompt.contains('~'),
        "and the row says where it is: {prompt}"
    );
}

/// Two trees in one column — directories and spaces — would leave no
/// telling which one is being chosen from, so the picker takes the
/// column while it is open and gives it straight back when it closes.
#[test]
fn the_open_prompt_has_the_sidebar_to_itself() {
    let root = uze_testkit::temp::TempDir::new("sidebar-root-alone");
    std::fs::create_dir_all(root.join("engine")).unwrap();
    let mut model = agent_session_in("/repo");

    model.root_picker = Some(RootPicker::opened_in(
        &root.path().display().to_string(),
        None,
    ));
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        rows.iter().any(|row| row.contains("engine")),
        "the directories are what is on offer: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("Agent")),
        "the agents step aside: {rows:?}"
    );

    model.root_picker = None;
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        rows.iter().any(|row| row.contains("Agent")),
        "and come back when it closes: {rows:?}"
    );
}

/// A root several levels deep is longer than the sidebar is wide, so
/// its head gives way — and the moment something is typed the line is
/// only that. The root stood pinned to the right of the line all the
/// way through, saying where the prompt was in a second place; the
/// two could disagree, and the one that went stale was the pinned
/// half.
#[test]
fn a_long_root_gives_way_and_then_gives_the_line_over_to_what_is_typed() {
    let root = uze_testkit::temp::TempDir::new("sidebar-root-elide");
    std::fs::create_dir_all(root.join("a-very-long-directory-name/inner")).unwrap();
    let mut model = agent_session_in("/repo");
    let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
    let prompt_row = |model: &WorkspaceModel| {
        sidebar(model, &identities_fixture())
            .rows
            .into_iter()
            .find(|row| row.contains(&cursor))
            .expect("the prompt row is drawn")
    };

    let mut picker = RootPicker::opened_in(
        &root
            .join("a-very-long-directory-name")
            .display()
            .to_string(),
        None,
    );
    model.root_picker = Some(picker);
    let prompt = prompt_row(&model);
    assert!(
        prompt.contains('\u{2026}'),
        "where the typing starts from, head first to give way: {prompt}"
    );

    picker = model.root_picker.take().expect("the prompt is open");
    for character in "inn".chars() {
        picker.typed(character);
    }
    model.root_picker = Some(picker);
    let prompt = prompt_row(&model);
    assert!(prompt.contains("inn\u{258f}"), "{prompt}");
    assert!(
        !prompt.contains('\u{2026}'),
        "and the root is not repeated beside what was typed: {prompt}"
    );
}

/// A task with no checkout left, waiting to be put back in one.
fn unfinished_task(id: &str, branch: &str) -> AgentView {
    let mut task = task_in("/repo/.worktrees/ai", id, WorkStateView::Shelved, 1);
    task.id = id.to_owned();
    task.branch = branch.to_owned();
    task.checkout = None;
    task
}

/// The picker opens over the tree it was asked from, so its rows sit
/// on top of sidebar rows drawn — and pushed — before them. The click
/// search the tree itself uses takes the first rect a point lands in,
/// which is what puts a mark's own hit ahead of its row; an overlay
/// has to take the last one instead, or the half of every option row
/// standing over the tree belongs to the row underneath it.
#[test]
fn a_picker_row_over_the_tree_answers_its_own_click() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-overlap"));
    set_up_every_harness(&home);
    let model = agent_over_a_lost_checkout(
        Path::new("/repo/.worktrees/ai"),
        Path::new("/repo"),
        unfinished_task("t1", "agent/t1"),
    );
    let mut driven = driven(model, &home);

    driven.frame();
    let resume = driven.hit(|hit| matches!(hit, WorkspaceHit::ResumeLostCheckout(_)));
    driven.press(resume.x, resume.y);
    assert!(
        driven.attach.model.agent_picker.is_some(),
        "the resume opens the picker"
    );

    driven.frame();
    let option = driven.hit(|hit| matches!(hit, WorkspaceHit::PickAgent(0)));
    let tree_ends = compute_layout(Rect::new(0, 0, 80, 24), None).pane.x;
    assert!(
        option.x < tree_ends,
        "the row this is about starts over the tree: {option:?}"
    );
    driven.press(option.x, option.y);
    assert!(
        driven.attach.model.placement_pending,
        "the harness under the pointer answered, not the row beneath it"
    );
}

/// A directory another space already holds is opened all the same:
/// one repository is routinely worth two spaces (one per branch, one
/// per thing being tried), and the prompt is an explicit request for
/// one — not a lookup of what is already open.
#[test]
fn a_directory_a_space_already_holds_is_opened_again() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-again"));
    let root = uze_testkit::temp::TempDir::new("orchestrator-space-root");
    std::fs::create_dir_all(root.join("inner")).unwrap();
    let mut model = session_rooted_at(root.path());
    // The directory being listed is what an untouched prompt lands on
    // (see `RootPicker`) — here, the space's own root.
    model.root_picker = Some(RootPicker::opened_in(
        &root.path().display().to_string(),
        None,
    ));
    let mut driven = driven(model, &home);

    driven.frame();
    let enter = uze_keys::active()
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::RootPicker])
        .expect("the prompt is answered from the keyboard");
    driven.press_key(key_event(enter));

    let sent = driven.sent();
    assert!(
        sent.iter().any(|request| matches!(
            request,
            ClientRequest::CreateSpace { seat, .. } if seat.root == root.path()
        )),
        "the pick is asked for, not looked up: {sent:?}"
    );
}

#[test]
fn a_directory_no_space_holds_is_opened_as_one() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-space-new"));
    let root = uze_testkit::temp::TempDir::new("orchestrator-space-new-root");
    std::fs::create_dir_all(root.join("inner")).unwrap();
    let mut model = session_rooted_at(root.path());
    model.root_picker = Some(RootPicker::opened_in(
        &root.path().display().to_string(),
        None,
    ));
    let mut driven = driven(model, &home);

    driven.frame();
    let row = driven.hit(|hit| matches!(hit, WorkspaceHit::PickSpaceRoot(_)));
    driven.press(row.x, row.y);

    let sent = driven.sent();
    assert!(
        sent.iter().any(|request| matches!(
            request,
            ClientRequest::CreateSpace { seat, .. } if seat.root == root.join("inner")
        )),
        "{sent:?}"
    );
}
