//! Starting agents: the new agent's directory and harness steps, the
//! popup both are drawn in, and the space a new agent opens in.

use super::WorkspaceHit;
use super::support::*;
use super::{
    AgentView, DirectoryStep, PlacementResolution, RootPicker, WorkStateView, WorkspaceModel,
    render::{self, compute_layout},
};
use crate::ui::theme::{self, Token};
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::Path;
use uze_core::UzeHome;
use uze_terminal::{ClientEvent, ClientRequest};

/// The directory step open over `directory`, for the space in front,
/// anchored where a `new` would be.
fn directory_step_over(model: &mut WorkspaceModel, directory: &Path) {
    let picker = RootPicker::opened_in(&directory.display().to_string(), None);
    let rows = render::new_agent_popup_rows(picker.match_count());
    let space = model.session.as_ref().unwrap().selected_space().id;
    model.directory_step = Some(DirectoryStep {
        picker,
        space,
        anchor: Rect::new(20, 1, 3, 1),
        rows,
    });
}

/// The whole frame, as text and as cells.
fn drawn(model: &mut WorkspaceModel) -> (Vec<String>, Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
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
    (buffer_rows(&buffer), buffer)
}

/// The line typed into: the row under the one naming the steps.
fn prompt_row_of(rows: &[String]) -> String {
    let steps = rows
        .iter()
        .position(|row| row.contains("directory › harness"))
        .expect("the popup is drawn");
    rows[steps + 1].clone()
}

/// Enter on the directory step, as the keyboard sends it.
fn confirm_the_directory(driven: &mut Driven<'_>) {
    let enter = uze_keys::active()
        .chord_for(uze_keys::Action::Activate, &[uze_keys::Scope::RootPicker])
        .expect("the directory step is answered from the keyboard");
    driven.press_key(key_event(enter));
}

/// The directory step opens where the space is working: the project of
/// the agent in front; with the space's own shell in front, the project of
/// the agent started last, however its tabs were dragged; with no agent,
/// nowhere in particular.
#[test]
fn the_directory_step_offers_where_the_space_is_working() {
    let mut session = session("/home/someone");
    let space = session.workspace.selected_space;
    let shell = session.selected_tab().id;
    assert!(
        model_of(session.clone()).directory_to_offer().is_none(),
        "a space with nothing but its shell offers nowhere in particular"
    );
    let mut agent_in = |cwd: &str| {
        let pane = session.add_tab(space, "agent".into(), None, 80, 24, cwd.into());
        session.update_pane_status(pane, cwd.into(), "agent".into());
        session.selected_tab().id
    };
    let front = agent_in("/front");
    let landing = agent_in("/landing");
    session.reorder_tab(landing, Some(front));

    session.select_tab(front);
    assert_eq!(
        model_of(session.clone()).directory_to_offer(),
        Some(Path::new("/front").to_path_buf()),
        "the agent in front's project"
    );
    session.select_tab(shell);
    assert_eq!(
        model_of(session).directory_to_offer(),
        Some(Path::new("/landing").to_path_buf()),
        "the project of the agent started last, though it was dragged first"
    );
}

/// The step asks one question — where — and any directory answers it.
#[test]
fn the_directory_step_offers_any_directory() {
    let plain = uze_testkit::temp::TempDir::new("picker-one-question");
    std::fs::create_dir_all(plain.join("notes")).unwrap();
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, plain.path());

    let (rows, _) = drawn(&mut model);
    assert!(rows.iter().any(|row| row.contains("notes")), "{rows:#?}");
    assert_eq!(
        model
            .directory_step
            .as_ref()
            .map(|step| step.picker.match_count()),
        Some(1),
        "a plain directory is offered like any other: {rows:#?}"
    );
}

/// A space is created by its name alone: the chord opens one, its shell
/// at home, and puts it up for its name as soon as the session shows it.
#[test]
fn the_new_space_chord_opens_a_space_and_asks_its_name() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-new-space-chord"));
    let mut driven = driven(agent_session_in("/repo"), &home);
    let chord = uze_keys::active()
        .chord_for(uze_keys::Action::NewSpace, &[uze_keys::Scope::Workspace])
        .expect("space creation is reachable from the keyboard");

    driven.press_key(key_event(chord));

    let sent = driven.sent();
    assert!(
        sent.iter().any(|request| matches!(
            request,
            ClientRequest::CreateSpace { label: None, cwd, .. }
                if Some(cwd.as_path()) == uze_platform::home::user_home().as_deref()
        )),
        "a space is asked for, its shell at home: {sent:?}"
    );

    let mut session = driven.attach.model.session.clone().unwrap();
    let in_front = session.workspace.selected_space;
    // Another client opened a space first: it is not this one's to name.
    let elsewhere = session
        .create_space(None, "/elsewhere".into(), 80, 24)
        .space;
    session.workspace.selected_space = in_front;
    driven.attach.model.apply(
        ClientEvent::SessionUpdated {
            session: session.clone(),
        },
        &identities_fixture(),
    );
    assert!(
        driven.attach.model.renaming.is_none(),
        "a space another client opened, {elsewhere:?}, was put up for its name"
    );
    let created = session
        .create_space(None, "/home/someone".into(), 80, 24)
        .space;
    driven.attach.model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    assert!(
        matches!(
            &driven.attach.model.renaming,
            Some((super::RenameTarget::Space(space), buffer)) if *space == created && buffer.text().is_empty()
        ),
        "the new space is put up for its name, nothing typed for the operator"
    );
}

/// AltGr types into the directory step: a Windows console reports it as
/// Ctrl+Alt, and `/` is AltGr+Q on ABNT2, so a path typed there lost
/// every separator.
#[test]
fn altgr_types_its_character_into_the_directory_step() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-altgr"));
    let mut driven = driven(agent_session_in("/repo"), &home);
    let chord = uze_keys::active()
        .chord_for(
            uze_keys::Action::NewAgent,
            &[uze_keys::Scope::Global, uze_keys::Scope::Workspace],
        )
        .expect("a new agent is reachable from the keyboard");
    driven.press_key(key_event(chord));

    let alt_graph = KeyModifiers::CONTROL | KeyModifiers::ALT;
    driven.press_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    driven.press_key(KeyEvent::new(KeyCode::Char('/'), alt_graph));
    driven.press_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    let step = driven
        .attach
        .model
        .directory_step
        .as_ref()
        .expect("the step is still open");
    assert_eq!(step.picker.input(), "g/e");
}

/// The key opens the same step the space header's button does, in the
/// same place — beside the button — listing where the agent in front's
/// project sits, so another agent there is one Enter away.
#[test]
fn the_new_agent_key_opens_the_directory_step_beside_its_button() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-picker-by-key"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home).on_a_roomy_terminal();
    driven.frame();
    let sparkle = driven.hit(|hit| *hit == WorkspaceHit::NewAgentMenu);
    let chord = uze_keys::active()
        .chord_for(
            uze_keys::Action::NewAgent,
            &[uze_keys::Scope::Global, uze_keys::Scope::Workspace],
        )
        .expect("the step is reachable from the keyboard");

    driven.press_key(key_event(chord));

    let step = driven
        .attach
        .model
        .directory_step
        .as_ref()
        .expect("the key opened the directory step");
    assert_eq!(step.anchor, sparkle, "anchored beside the `new` button");
    assert_eq!(
        step.picker.base(),
        Path::new("/"),
        "listing where the agent's project, `/repo`, sits"
    );
    assert!(step.picker.input().is_empty(), "with nothing typed");
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
    confirm_the_directory(&mut driven);
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

/// With one harness set up the harness step would be a single row to
/// confirm, so choosing the directory starts the agent: the placement is
/// asked for at once and no harness is asked for.
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
    confirm_the_directory(&mut driven);

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

/// The step lists the directories the typed segment still matches, and
/// clicking one is the same choice Enter makes.
#[test]
fn the_directory_step_lists_the_directories_it_matches() {
    let root = uze_testkit::temp::TempDir::new("popup-directory-step");
    for directory in ["engine", "extensions", "docs"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());

    let (rows, _) = drawn(&mut model);
    assert!(
        rows.iter().any(|row| row.contains("engine"))
            && rows.iter().any(|row| row.contains("docs")),
        "the listing is on screen: {rows:?}"
    );
    assert!(
        model
            .hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::PickDirectory(_))),
        "every offered directory is clickable"
    );

    if let Some(step) = model.directory_step.as_mut() {
        for character in "ex".chars() {
            step.picker.typed(character);
        }
    }
    let (rows, _) = drawn(&mut model);
    assert!(
        rows.iter().any(|row| row.contains("extensions")),
        "what matches stays: {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("docs")),
        "what stopped matching is gone: {rows:?}"
    );
}

/// What is being typed and the directories it matches stand in one
/// column, and a name the query is the head of says so in the accent;
/// one matched further in is left alone.
#[test]
fn the_directory_step_lines_its_rows_up_in_one_column() {
    let root = uze_testkit::temp::TempDir::new("popup-picker-grid");
    for directory in ["craude", "cortex", "scribble"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());
    if let Some(step) = model.directory_step.as_mut() {
        for character in "cr".chars() {
            step.picker.typed(character);
        }
    }

    let (rows, buffer) = drawn(&mut model);
    let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
    let prompt = rows
        .iter()
        .position(|row| row.contains(&format!("cr{cursor}")))
        .expect("the prompt is drawn");
    let column = |row: &str, text: &str| row[..row.find(text).unwrap()].chars().count();
    let values = column(&rows[prompt], "cr");
    assert_eq!(column(&rows[prompt + 1], "craude"), values, "{rows:?}");
    assert_eq!(column(&rows[prompt + 2], "scribble"), values, "{rows:?}");
    // "craude", which "cr" is the head of, is the one Enter takes, so it
    // wears the harness step's bar; "scribble" matched further in.
    let cell = |row: usize| &buffer[(values as u16, row as u16)];
    assert_eq!(cell(prompt + 1).bg, theme::color(Token::Accent), "{rows:?}");
    assert_eq!(
        cell(prompt + 2).fg,
        theme::color(Token::TextInactive),
        "{rows:?}"
    );
}

/// A listing taller than the popup is reached with the wheel too: it
/// walks the directories the way the arrow keys do, and the window
/// follows the one selected.
#[test]
fn the_wheel_walks_the_directories_the_step_offers() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-wheel"));
    let root = uze_testkit::temp::TempDir::new("popup-picker-wheel");
    for index in 0..40 {
        std::fs::create_dir_all(root.join(format!("directory-{index:02}"))).unwrap();
    }
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());
    let mut driven = driven(model, &home);
    driven.frame();
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| matches!(hit, WorkspaceHit::PickDirectory(_)).then_some(*rect))
        .expect("the listing is drawn");

    for _ in 0..25 {
        driven.frame();
        driven.mouse(row.x, row.y, MouseEventKind::ScrollDown);
    }
    let selection = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .directory_step
            .as_ref()
            .expect("still open")
            .picker
            .selection()
    };
    assert_eq!(
        selection(&driven),
        Some(24),
        "the first turn takes the row in front, and the rest walk it"
    );

    for _ in 0..40 {
        driven.frame();
        driven.mouse(row.x, row.y, MouseEventKind::ScrollUp);
    }
    assert_eq!(selection(&driven), Some(0), "and stops at the top");
}

/// Both steps float beside the `new` they were asked from, over the
/// pane, at one place and one size, and the sidebar under them does not
/// move: a popup, never rows pushed into the column.
#[test]
fn both_steps_float_beside_new_at_one_place_and_size() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-one-place"));
    set_up_every_harness(&home);
    let root = uze_testkit::temp::TempDir::new("popup-one-place-root");
    for directory in ["engine", "extensions"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    let mut model = agent_session_in("/repo");
    let before = sidebar(&model, &identities_fixture()).rows;
    directory_step_over(&mut model, root.path());
    assert_eq!(
        sidebar(&model, &identities_fixture()).rows,
        before,
        "nothing in the sidebar moves"
    );
    let mut driven = driven(model, &home);
    driven.frame();
    let card = |driven: &Driven<'_>, wanted: fn(&WorkspaceHit) -> bool| {
        let rows: Vec<Rect> = driven
            .attach
            .model
            .hits
            .iter()
            .filter(|(_, hit)| wanted(hit))
            .map(|(rect, _)| *rect)
            .collect();
        (rows[0].x, rows[0].y)
    };
    let directories = card(&driven, |hit| matches!(hit, WorkspaceHit::PickDirectory(_)));

    confirm_the_directory(&mut driven);
    driven.frame();
    let harnesses = card(&driven, |hit| matches!(hit, WorkspaceHit::PickAgent(_)));
    assert_eq!(
        harnesses.0, directories.0,
        "the harnesses stand where the directories stood"
    );
    let picker = driven.attach.model.agent_picker.as_ref().expect("asked");
    assert_eq!(
        picker.rows,
        Some(render::new_agent_popup_rows(2)),
        "and keep the popup's size"
    );
}

/// The step opens with nothing typed: the directory it lists is the
/// row's own context, not text waiting to be deleted.
#[test]
fn the_directory_step_opens_empty_over_the_directory_it_lists() {
    let mut model = agent_session_in("/repo");
    let home = uze_platform::home::user_home().expect("a home directory");
    directory_step_over(&mut model, &home);

    let (rows, _) = drawn(&mut model);
    let prompt = prompt_row_of(&rows);
    assert!(prompt.contains('~'), "the row says where it is: {prompt}");
}

/// A directory several levels deep is longer than the popup is wide, so
/// its head gives way — and the moment something is typed the line is
/// only that.
#[test]
fn a_long_directory_gives_way_to_what_is_typed() {
    let root = uze_testkit::temp::TempDir::new("popup-root-elide");
    let deep = root.join("a-very-long-directory-name-that-will-not-fit-the-popup");
    std::fs::create_dir_all(deep.join("inner")).unwrap();
    let mut model = agent_session_in("/repo");
    let cursor = theme::glyph(crate::ui::theme::Symbol::CursorText);
    directory_step_over(&mut model, &deep);
    let prompt_row = |model: &mut WorkspaceModel| prompt_row_of(&drawn(model).0);

    let prompt = prompt_row(&mut model);
    assert!(
        prompt.contains('\u{2026}'),
        "head first to give way: {prompt}"
    );

    if let Some(step) = model.directory_step.as_mut() {
        for character in "inn".chars() {
            step.picker.typed(character);
        }
    }
    let prompt = prompt_row(&mut model);
    assert!(prompt.contains(&format!("inn{cursor}")), "{prompt}");
    assert!(
        !prompt.contains('\u{2026}'),
        "nothing repeated beside it: {prompt}"
    );
}

/// A click on a directory is Enter on it: the popup moves to the
/// harnesses, for that directory.
#[test]
fn clicking_a_directory_is_choosing_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-click-directory"));
    let root = uze_testkit::temp::TempDir::new("popup-click-directory-root");
    std::fs::create_dir_all(root.join("engine")).unwrap();
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());
    let mut driven = driven(model, &home);
    driven.frame();
    let row = driven.hit(|hit| matches!(hit, WorkspaceHit::PickDirectory(0)));

    driven.press(row.x, row.y);

    assert!(
        driven.attach.model.directory_step.is_none(),
        "the step answered"
    );
    let picker = driven
        .attach
        .model
        .agent_picker
        .as_ref()
        .expect("the harnesses are asked for");
    assert!(
        matches!(
            &picker.launch,
            super::AgentLaunch::New(place) if place.directory == root.join("engine")
        ),
        "for the directory clicked"
    );
}

/// A click on the line being typed into, or anywhere in the popup that is
/// not a row, is inside it: the popup stays open, as typed.
#[test]
fn a_click_on_the_line_being_typed_keeps_the_popup_open() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-click-input"));
    let root = uze_testkit::temp::TempDir::new("popup-click-input-root");
    std::fs::create_dir_all(root.join("engine")).unwrap();
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());
    let mut driven = driven(model, &home);
    driven.frame();
    let first_row = driven.hit(|hit| matches!(hit, WorkspaceHit::PickDirectory(0)));

    // The line typed into stands right above the first directory.
    driven.press(first_row.x + 2, first_row.y - 1);

    assert!(
        driven.attach.model.directory_step.is_some(),
        "a click on the input closed the popup"
    );
}

/// The popup names its two steps and lights the one it is on: the
/// directory first, then the harness, with the directory chosen standing
/// in for its word once it is.
#[test]
fn the_popup_names_its_steps_and_lights_the_one_it_is_on() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-steps"));
    set_up_every_harness(&home);
    let root = uze_testkit::temp::TempDir::new("popup-steps-root");
    std::fs::create_dir_all(root.join("engine")).unwrap();
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());
    let (rows, _) = drawn(&mut model);
    assert!(
        rows.iter().any(|row| row.contains("directory › harness")),
        "the steps are named: {rows:#?}"
    );

    let mut driven = driven(model, &home);
    driven.frame();
    let row = driven.hit(|hit| matches!(hit, WorkspaceHit::PickDirectory(0)));
    driven.press(row.x, row.y);
    let (rows, _) = drawn(&mut driven.attach.model);
    assert!(
        rows.iter().any(|row| row.contains("engine › harness")),
        "the directory chosen names the first step: {rows:#?}"
    );
}

/// The harness step goes back to the directory step, on the directory
/// chosen: by a click on that directory, or by `Backspace`.
#[test]
fn the_harness_step_goes_back_to_the_directory_chosen() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-back"));
    set_up_every_harness(&home);
    let root = uze_testkit::temp::TempDir::new("popup-back-root");
    std::fs::create_dir_all(root.join("engine")).unwrap();
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, root.path());
    let mut driven = driven(model, &home);
    let to_harnesses = |driven: &mut Driven<'_>| {
        driven.frame();
        let row = driven.hit(|hit| matches!(hit, WorkspaceHit::PickDirectory(0)));
        driven.press(row.x, row.y);
        assert!(
            driven.attach.model.agent_picker.is_some(),
            "on the harnesses"
        );
        driven.frame();
    };
    let back_on_engine = |driven: &Driven<'_>| {
        let step = driven
            .attach
            .model
            .directory_step
            .as_ref()
            .expect("back on the directories");
        assert!(driven.attach.model.agent_picker.is_none());
        assert_eq!(
            step.picker.chosen().as_deref(),
            Some(root.join("engine").as_path()),
            "on the directory chosen"
        );
    };

    to_harnesses(&mut driven);
    let back = driven.hit(|hit| *hit == WorkspaceHit::NewAgentBack);
    driven.press(back.x, back.y);
    back_on_engine(&driven);

    to_harnesses(&mut driven);
    driven.press_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Backspace,
        crossterm::event::KeyModifiers::NONE,
    ));
    back_on_engine(&driven);
}

/// Esc closes the popup, whichever step it is on, and nothing is placed.
#[test]
fn esc_closes_the_new_agent_popup_and_places_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("popup-esc"));
    let mut model = agent_session_in("/repo");
    directory_step_over(&mut model, Path::new("/"));
    let mut driven = driven(model, &home);
    let esc = uze_keys::active()
        .chord_for(uze_keys::Action::Dismiss, &[uze_keys::Scope::RootPicker])
        .expect("the step is dismissed from the keyboard");

    driven.press_key(key_event(esc));

    assert!(
        driven.attach.model.directory_step.is_none(),
        "the popup closed"
    );
    assert!(driven.attach.model.agent_picker.is_none());
    assert!(
        !driven.attach.model.placement_pending,
        "and nothing is placed"
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

/// A new agent opens in the space its `new` was pressed in, whatever
/// project it runs in and whichever space is in front by the time its
/// checkout is ready.
#[test]
fn a_new_agent_opens_in_the_space_it_was_asked_from() {
    let repository = uze_testkit::git::Repository::new("orchestrator-new-in-origin");
    let root = repository.root().to_path_buf();
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-new-in-origin-home",
    ));
    let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
    let placement = app
        .workspace()
        .place_new_agent(&root, None, "claude-code", &[])
        .expect("the agent is placed");
    let model = three_spaces();
    let origin = model.session.as_ref().unwrap().workspace.spaces[0].id;
    let mut driven = driven(model, &home);

    driven.placements_answered(PlacementResolution {
        label: "agent 2".to_owned(),
        command: vec!["claude".to_owned()],
        placement: Ok(placement),
        replacing: None,
        space: Some(origin),
    });

    let sent = driven.sent();
    assert!(
        sent.iter().any(|request| matches!(
            request,
            ClientRequest::CreateTab { space, .. } if *space == Some(origin)
        )),
        "the tab opens in the space it was asked from, not the one in front: {sent:?}"
    );
    assert!(
        !sent
            .iter()
            .any(|request| matches!(request, ClientRequest::CreateSpace { .. })),
        "and no space is opened for its project: {sent:?}"
    );
}
