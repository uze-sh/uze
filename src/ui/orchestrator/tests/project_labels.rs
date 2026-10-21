//! The operator's own name for a project: where every surface reads it,
//! and the menu and the double click that set it.

use super::support::*;
use super::*;
use crossterm::event::{MouseButton, MouseEventKind};
use std::time::Duration;

/// An agent working in a project directory of its own, named `uze`, with
/// the root the client resolved for it.
fn agent_in_a_project(label: &str) -> (UzeHome, WorkspaceModel, TabId, PathBuf) {
    let home = UzeHome::at(uze_testkit::temp::scratch(&format!("{label}-home")));
    let project = uze_testkit::temp::scratch(label).join("uze");
    std::fs::create_dir_all(&project).unwrap();
    let mut model = agent_session_in(project.to_str().unwrap());
    stamp_first_tab(&mut model, "a1");
    model.note_space_projects(&identities_fixture());
    let tab = first_tab(&model).id;
    let root = model
        .project_of_tab(tab)
        .expect("the agent's project is known");
    assert_eq!(
        root,
        project.canonicalize().unwrap(),
        "a directory outside Git is its own project"
    );
    (home, model, tab, root)
}

fn application(home: &UzeHome) -> uze_application::UzeApplication {
    uze_application::UzeApplication::new(home.clone(), Vec::new())
}

/// Labels `root` on this machine and hands the client what it would read.
fn labelled(home: &UzeHome, model: &mut WorkspaceModel, root: &Path, label: &str) {
    let application = application(home);
    let labels = application.project_labels();
    labels.set(root, label).unwrap();
    model.remembered.project_labels = labels.all().unwrap();
}

fn screen(model: &mut WorkspaceModel) -> String {
    frame_rows(model).join("\n")
}

/// The rect the last frame drew the project of `tab`'s row in.
fn project_on_row(driven: &Driven<'_>, tab: TabId) -> Rect {
    driven.hit(|hit| *hit == WorkspaceHit::ProjectName(tab))
}

/// The rect of the line `tab`'s row names its agent on: the first of the
/// row's two.
fn name_of_row(driven: &Driven<'_>, tab: TabId) -> Rect {
    driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::SelectTab(tab))
        .map(|(rect, _)| *rect)
        .expect("the agent's row is a target")
}

fn double_click(driven: &mut Driven<'_>, at: Rect) {
    driven.press(at.x, at.y);
    driven.press(at.x, at.y);
}

fn typed(driven: &mut Driven<'_>, text: &str) {
    for character in text.chars() {
        driven.press_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(character),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
}

/// One key of the open menu, by the chord the active keymap gives it.
fn in_the_menu(driven: &mut Driven<'_>, action: Action) {
    let chord = uze_keys::active()
        .chord_for(
            action,
            &[
                uze_keys::Scope::Global,
                uze_keys::Scope::Workspace,
                uze_keys::Scope::ContextMenu,
            ],
        )
        .expect("the action has a key");
    driven.press_key(key_event(chord));
}

fn chord(driven: &mut Driven<'_>, spelled: &str) {
    driven.press_key(key_event(uze_keys::Chord::parse(spelled).unwrap()));
}

/// Waits for the thread writing a name to answer, then for the read that
/// answer asks for, and absorbs each as the attach loop does.
fn written_and_read_back(driven: &mut Driven<'_>) {
    let channels = driven.attach.channels;
    let written = channels
        .project_label_writes
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the write answered");
    channels.project_label_writes.sender.send(written).unwrap();
    driven.pump();
    let latest = driven.attach.model.remembered.project_labels_asked;
    let read = std::iter::from_fn(|| {
        channels
            .project_labels
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .ok()
    })
    .find(|read| read.asked == latest)
    .expect("the read after the write answered");
    channels.project_labels.sender.send(read).unwrap();
    driven.pump();
}

#[test]
fn a_row_names_its_project_by_its_label() {
    let (home, mut model, _, root) = agent_in_a_project("labels-row");
    assert_eq!(model.project_named(&root), "uze");

    labelled(&home, &mut model, &root, "uze core");

    assert!(
        screen(&mut model).contains("uze core"),
        "{}",
        screen(&mut model)
    );
}

#[test]
fn a_double_click_on_the_project_renames_the_project() {
    let (home, model, tab, _) = agent_in_a_project("labels-double-click");
    let mut driven = driven(model, &home);
    driven.frame();

    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);

    let (target, buffer) = driven.attach.model.renaming.clone().expect("an edit");
    assert!(target.is_project_of(tab), "{target:?}");
    assert_eq!(buffer.text(), "uze", "seeded with the name the row shows");
}

#[test]
fn a_double_click_on_the_name_still_renames_the_agent() {
    let (home, model, tab, _) = agent_in_a_project("labels-double-click-name");
    let mut driven = driven(model, &home);
    driven.frame();
    let name = name_of_row(&driven, tab);

    double_click(&mut driven, name);

    assert_eq!(
        driven
            .attach
            .model
            .renaming
            .as_ref()
            .map(|(target, _)| target.clone()),
        Some(RenameTarget::Tab(tab))
    );
}

#[test]
fn a_right_click_on_the_project_opens_the_agents_menu() {
    let (home, model, tab, _) = agent_in_a_project("labels-right-click");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);

    driven.mouse(
        project.x,
        project.y,
        MouseEventKind::Down(MouseButton::Right),
    );

    let menu = driven.attach.model.context_menu.as_ref().expect("a menu");
    assert_eq!(menu.target, MenuTarget::Tab(tab));
}

#[test]
fn the_agents_rename_opens_agent_and_project() {
    let (home, model, tab, root) = agent_in_a_project("labels-menu");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    driven.mouse(
        project.x,
        project.y,
        MouseEventKind::Down(MouseButton::Right),
    );
    let menu = driven.attach.model.context_menu.as_mut().unwrap();
    let rename = menu
        .items
        .iter()
        .position(|action| *action == Action::RenameSelection)
        .unwrap();
    assert_eq!(
        menu.labels[rename], "Rename",
        "no ellipsis: the arrow says it opens"
    );
    assert_eq!(menu.group(rename), Some(&MenuGroup::Rename));

    menu.selected = rename;
    in_the_menu(&mut driven, Action::FocusNext);
    in_the_menu(&mut driven, Action::SelectNext);
    in_the_menu(&mut driven, Action::Activate);

    assert_eq!(
        driven
            .attach
            .model
            .renaming
            .as_ref()
            .map(|(target, _)| target.clone()),
        Some(RenameTarget::Project { tab, root })
    );
}

#[test]
fn an_entry_that_opens_a_submenu_ends_in_an_arrow() {
    let (home, model, tab, _) = agent_in_a_project("labels-arrow");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    driven.mouse(
        project.x,
        project.y,
        MouseEventKind::Down(MouseButton::Right),
    );

    let rows = frame_rows(&mut driven.attach.model);
    let arrow = theme::glyph(Symbol::Submenu);
    let rename = rows
        .iter()
        .find(|row| row.contains(" Rename "))
        .expect("the menu is drawn");
    let entry = rename
        .split('│')
        .find(|cell| cell.contains("Rename"))
        .unwrap();
    assert!(entry.trim_end().ends_with(&arrow), "{rename:?}");
    let close = rows.iter().find(|row| row.contains("Close tab"));
    assert!(close.is_none_or(|row| !row.contains(&arrow)), "{close:?}");
}

#[test]
fn a_shells_rename_opens_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("labels-shell"));
    let mut model = model_of(session("/repo"));
    let shell = first_tab(&model).id;
    let menu = {
        let mut driven = driven(std::mem::take(&mut model), &home);
        driven.frame();
        let row = driven.hit(|hit| *hit == WorkspaceHit::SelectTab(shell));
        driven.mouse(row.x, row.y, MouseEventKind::Down(MouseButton::Right));
        driven.attach.model.context_menu.take().expect("a menu")
    };
    let rename = menu
        .items
        .iter()
        .position(|action| *action == Action::RenameSelection)
        .expect("a shell can be renamed");
    assert_eq!(menu.group(rename), None);
}

#[test]
fn confirming_a_name_keeps_it_and_every_row_reads_it() {
    let (home, model, tab, root) = agent_in_a_project("labels-commit");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);
    typed(&mut driven, " core");
    chord(&mut driven, "enter");

    written_and_read_back(&mut driven);

    assert_eq!(
        application(&home).project_labels().all().unwrap().of(&root),
        Some("uze core")
    );
    assert!(screen(&mut driven.attach.model).contains("uze core"));
    assert_eq!(
        first_tab(&driven.attach.model).label,
        "Agent",
        "the agent's own name is unchanged"
    );
}

#[test]
fn escape_leaves_the_label_as_it_was() {
    let (home, model, tab, root) = agent_in_a_project("labels-escape");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);
    typed(&mut driven, " core");

    chord(&mut driven, "esc");

    assert!(driven.attach.model.renaming.is_none());
    assert!(!home.config_path().exists());
    assert_eq!(driven.attach.model.project_named(&root), "uze");
}

#[test]
fn a_name_that_cannot_be_kept_is_said_and_the_row_keeps_its_name() {
    let (home, model, tab, root) = agent_in_a_project("labels-unwritable");
    std::fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
    std::fs::write(home.config_path(), "[projects\n").unwrap();
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);
    typed(&mut driven, " core");
    chord(&mut driven, "enter");

    let channels = driven.attach.channels;
    let written = channels
        .project_label_writes
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the write answered");
    assert!(written.outcome.is_err());
    channels.project_label_writes.sender.send(written).unwrap();
    driven.pump();

    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(said.contains("project not renamed"), "{said}");
    assert_eq!(
        std::fs::read_to_string(home.config_path()).unwrap(),
        "[projects\n"
    );
    assert_eq!(driven.attach.model.project_named(&root), "uze");
}

#[test]
fn approval_names_the_project_by_its_label() {
    let (home, mut model, _, root) = agent_in_a_project("labels-approval");
    std::fs::write(
        root.join("agents.yaml"),
        "workspace:\n  gate:\n    posix: make test\n    windows: make test\n",
    )
    .unwrap();
    labelled(&home, &mut model, &root, "uze core");
    let awaiting = application(&home)
        .workspace()
        .commands_awaiting_approval(&root)
        .expect("an unapproved project waits");

    model.commands_await(awaiting, false);

    let said = format!("{:?}", model.toast_stack());
    assert!(said.contains("uze core: gate commands"), "{said}");
    model.review_commands();
    assert!(screen(&mut model).contains("uze core"));
}

#[test]
fn an_agent_whose_checkout_was_removed_still_offers_its_project() {
    let home = UzeHome::at(uze_testkit::temp::scratch("labels-lost"));
    let primary = PathBuf::from("/work/uze");
    let checkout = primary.join(".worktrees").join("t1");
    let model = agent_over_a_lost_checkout(
        &checkout,
        &primary,
        task_in(
            checkout.to_str().unwrap(),
            "Agent",
            WorkStateView::Shelved,
            0,
        ),
    );
    let tab = first_tab(&model).id;
    let mut driven = driven(model, &home);
    driven.frame();
    let row = name_of_row(&driven, tab);
    driven.mouse(row.x, row.y, MouseEventKind::Down(MouseButton::Right));

    let menu = driven.attach.model.context_menu.as_ref().expect("a menu");
    let rename = menu
        .items
        .iter()
        .position(|action| *action == Action::RenameSelection)
        .unwrap();
    assert_eq!(menu.group(rename), Some(&MenuGroup::Rename));
}

#[test]
fn names_that_cannot_be_read_are_said_once_and_rows_keep_their_directories() {
    let (home, model, _, root) = agent_in_a_project("labels-unreadable");
    std::fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
    std::fs::write(home.config_path(), "[projects\n").unwrap();
    let mut driven = driven(model, &home);

    driven.pump();
    let channels = driven.attach.channels;
    let read = channels
        .project_labels
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the read answered");
    assert!(read.labels.is_err());
    let again = ProjectLabelsRead {
        asked: read.asked,
        labels: Err("still unreadable".to_owned()),
    };
    channels.project_labels.sender.send(read).unwrap();
    driven.pump();
    channels.project_labels.sender.send(again).unwrap();
    driven.pump();

    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert_eq!(said.matches("project names unread").count(), 1, "{said}");
    assert_eq!(driven.attach.model.project_named(&root), "uze");
    assert_eq!(
        std::fs::read_to_string(home.config_path()).unwrap(),
        "[projects\n"
    );
}

#[test]
fn the_name_lands_on_the_project_the_edit_began_on() {
    let (home, model, tab, root) = agent_in_a_project("labels-root-at-begin");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);
    typed(&mut driven, " core");
    driven.attach.model.projects.clear();

    chord(&mut driven, "enter");
    written_and_read_back(&mut driven);

    assert_eq!(
        application(&home).project_labels().all().unwrap().of(&root),
        Some("uze core")
    );
}

#[test]
fn a_second_name_waits_for_the_first_and_is_the_one_kept() {
    let (home, model, tab, root) = agent_in_a_project("labels-in-order");
    let mut driven = driven(model, &home);
    driven.frame();
    for name in [" first", " second"] {
        let project = project_on_row(&driven, tab);
        double_click(&mut driven, project);
        typed(&mut driven, name);
        chord(&mut driven, "enter");
        driven.frame();
    }

    let channels = driven.attach.channels;
    let first = channels
        .project_label_writes
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the first write answered");
    assert_eq!(
        application(&home).project_labels().all().unwrap().of(&root),
        Some("uze first"),
        "the second is not written while the first is out"
    );
    channels.project_label_writes.sender.send(first).unwrap();
    driven.pump();
    written_and_read_back(&mut driven);

    assert_eq!(driven.attach.model.project_named(&root), "uze second");
}

#[test]
fn the_caret_stays_on_screen_over_a_long_name() {
    let (home, mut model, tab, root) = agent_in_a_project("labels-long");
    labelled(
        &home,
        &mut model,
        &root,
        "a project name far longer than any sidebar is wide",
    );
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);

    let screen = screen(&mut driven.attach.model);
    let caret = theme::glyph(Symbol::CursorText);
    let row = screen
        .lines()
        .find(|row| row.contains("is wide"))
        .expect("the edit is drawn, cut at its head");

    let (_, edited) = row.split_once("is wide").unwrap();
    assert!(edited.contains(&caret), "{row:?}");
}

#[test]
fn agents_in_checkouts_of_one_project_share_its_label() {
    let home = UzeHome::at(uze_testkit::temp::scratch("labels-checkouts-home"));
    let primary = uze_testkit::temp::scratch("labels-checkouts").join("uze");
    let checkout = primary.join(".worktrees").join("a2");
    std::fs::create_dir_all(&checkout).unwrap();
    let mut model = two_agent_session(primary.to_str().unwrap(), checkout.to_str().unwrap());
    model.note_space_projects(&identities_fixture());
    let root = model.project_of_tab(TabId(1)).unwrap();
    assert_eq!(model.project_of_tab(TabId(2)), Some(root.clone()));

    labelled(&home, &mut model, &root, "uze core");

    let rows = screen(&mut model);
    assert_eq!(rows.matches("uze core").count(), 2, "{rows}");
}

#[test]
fn renaming_the_agent_from_the_menu_edits_the_agents_name() {
    let (home, model, tab, _) = agent_in_a_project("labels-menu-agent");
    let mut driven = driven(model, &home);
    driven.frame();
    let row = name_of_row(&driven, tab);
    driven.mouse(row.x, row.y, MouseEventKind::Down(MouseButton::Right));
    let menu = driven.attach.model.context_menu.as_mut().unwrap();
    menu.selected = menu
        .items
        .iter()
        .position(|action| *action == Action::RenameSelection)
        .unwrap();

    in_the_menu(&mut driven, Action::FocusNext);
    in_the_menu(&mut driven, Action::Activate);

    assert_eq!(
        driven.attach.model.renaming,
        Some((
            RenameTarget::Tab(tab),
            RenameBuffer::new("Agent".to_owned())
        ))
    );
}

#[test]
fn every_gesture_but_a_double_click_finds_the_row_on_the_project() {
    let (home, model, tab, _) = agent_in_a_project("labels-shadowed");
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);

    assert_eq!(
        driven.attach.model.hit_at(project.x, project.y),
        Some(WorkspaceHit::SelectTab(tab)),
        "a click, a right click and a drag all look the hit up here"
    );
}

#[test]
fn an_approval_read_names_the_project_by_the_label_of_its_root() {
    let (home, model, _, root) = agent_in_a_project("labels-approval-root");
    std::fs::write(
        root.join("agents.yaml"),
        "workspace:\n  gate:\n    posix: make test\n    windows: make test\n",
    )
    .unwrap();
    let mut awaiting = application(&home)
        .workspace()
        .commands_awaiting_approval(&root)
        .expect("an unapproved project waits");
    awaiting.project = root.join("spelled-below");
    let mut driven = driven(model, &home);
    labelled(&home, &mut driven.attach.model, &root, "uze core");

    driven
        .attach
        .channels
        .commands_awaiting
        .sender
        .send(AwaitingRead {
            awaiting,
            root: root.clone(),
        })
        .unwrap();
    driven.pump();

    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(!said.contains("spelled-below"), "named by its root: {said}");
    assert!(said.contains("uze core: gate commands"), "{said}");
}

#[test]
fn a_long_name_cut_on_both_sides_of_the_caret_marks_both_cuts() {
    let (home, mut model, tab, root) = agent_in_a_project("labels-long-middle");
    labelled(
        &home,
        &mut model,
        &root,
        "a project name far longer than any sidebar is wide",
    );
    let mut driven = driven(model, &home);
    driven.frame();
    let project = project_on_row(&driven, tab);
    double_click(&mut driven, project);
    for _ in 0.."wide".len() {
        chord(&mut driven, "left");
    }

    let screen = screen(&mut driven.attach.model);
    let caret = theme::glyph(Symbol::CursorText);
    let ellipsis = theme::glyph(Symbol::Ellipsis);
    let row = screen
        .lines()
        .find(|row| row.contains("is "))
        .expect("the edit is drawn");
    let (_, after_caret) = row.rsplit_once(&caret).expect("the caret is drawn");

    assert!(row.contains(&ellipsis), "{row:?}");
    assert!(
        after_caret
            .trim_end()
            .trim_end_matches('│')
            .trim_end()
            .ends_with(&ellipsis),
        "the text after the caret says it goes on: {row:?}"
    );
}
