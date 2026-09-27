//! The work modal: its sidebar of projects, the one list of the project in
//! front and how it groups, what each row offers and what a change asks
//! and tells the operator.

use super::workspace_tests::{driven, key_event, model_of, preserved, session};
use super::*;
use crate::ui::widget::ToastKind;
use ratatui::{Terminal, backend::TestBackend};
use uze_application::{
    CheckoutOwner, CheckoutView, CheckoutsView, CleanUp, JoinedWork, KeptCheckout, PreservedWork,
    RemovedCheckout,
};

const PROJECT: &str = "/work/project";
const OTHER: &str = "/work/other";

fn checkout(name: &str, owner: CheckoutOwner) -> CheckoutView {
    CheckoutView {
        path: Path::new(PROJECT).join(name),
        name: name.to_owned(),
        owner,
        task: None,
        branch: Some(format!(
            "branch-{}",
            name.rsplit('/').next().unwrap_or(name)
        )),
        dirty: false,
        in_target: true,
        ahead: 0,
        in_use: false,
        last_changed: None,
        bytes: 2048,
        adoptable: false,
        removal_refusal: None,
    }
}

fn slot(holder: Option<&str>, live: bool) -> CheckoutOwner {
    CheckoutOwner::Agent {
        holder: holder.map(str::to_owned),
        live,
    }
}

fn subagent(joinable: bool) -> CheckoutOwner {
    CheckoutOwner::Subagent {
        parent: "parser".to_owned(),
        parent_id: "p1".to_owned(),
        topic: Some("lexer".to_owned()),
        joinable,
    }
}

fn answered(checkouts: Vec<CheckoutView>) -> ProjectRead {
    ProjectRead {
        asked: 1,
        pending: false,
        answer: Some(Some(CheckoutsView {
            primary: PathBuf::from(PROJECT),
            target: "main".to_owned(),
            total_bytes: checkouts.iter().map(|checkout| checkout.bytes).sum(),
            checkouts,
        })),
    }
}

/// A model with a space open on the project and the modal open on it,
/// its checkouts read as `checkouts` and `kept` the machine's kept work.
fn showing_with(checkouts: Vec<CheckoutView>, kept: Vec<PreservedWork>) -> WorkspaceModel {
    let mut model = model_of(session(PROJECT));
    model.remembered.preserved_work = kept;
    model.remembered.checkouts_asked = 1;
    let mut overlay = WorkOverlay::open(Some(PathBuf::from(PROJECT)));
    overlay
        .reads
        .insert(PathBuf::from(PROJECT), answered(checkouts));
    model.work = Some(overlay);
    model
}

fn showing(checkouts: Vec<CheckoutView>) -> WorkspaceModel {
    showing_with(checkouts, Vec::new())
}

fn drawn_at(model: &WorkspaceModel, width: u16, height: u16) -> Vec<String> {
    let overlay = model.work.as_ref().expect("open");
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            render_work(frame, frame.area(), model, overlay, &mut Vec::new());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect()
}

fn drawn(model: &WorkspaceModel) -> Vec<String> {
    drawn_at(model, 140, 40)
}

fn row_of(lines: &[String], text: &str) -> usize {
    lines
        .iter()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("{text:?} is drawn:\n{}", lines.join("\n")))
}

/// The sidebar's columns of a drawn line.
fn sidebar(line: &str) -> String {
    line.chars().take(33).collect()
}

fn press(driven: &mut super::workspace_tests::Driven<'_>, action: Action) {
    let chord = uze_keys::active()
        .chord_for(action, &[uze_keys::Scope::Work])
        .unwrap_or_else(|| panic!("{action} is bound in the work modal"));
    driven.press_key(key_event(chord));
}

fn asking(model: &WorkspaceModel) -> Option<WorkQuestion> {
    model.work.as_ref()?.asking
}

fn front_key(model: &WorkspaceModel) -> PathBuf {
    let work = model.work.as_ref().expect("open");
    front(model, work).expect("a project in front").1.key
}

/// The line the buttons are drawn on: the last one naming Clean up, which
/// every row offers.
fn buttons(lines: &[String]) -> String {
    lines
        .iter()
        .rev()
        .find(|line| line.contains("Clean up"))
        .cloned()
        .unwrap_or_else(|| panic!("the buttons are drawn:\n{}", lines.join("\n")))
}

#[test]
fn the_sidebar_lists_every_project_with_what_needs_you_and_its_size() {
    let mut matched = checkout(".worktrees/t1", slot(Some("t1"), false));
    matched.dirty = true;
    let model = showing_with(
        vec![matched, checkout("by-hand", CheckoutOwner::Operator)],
        vec![
            preserved(PROJECT, "t1", "yesterday", WorkStateView::Uncommitted),
            preserved(OTHER, "t3", "elsewhere", WorkStateView::Parked),
            preserved(OTHER, "t4", "older", WorkStateView::Parked),
        ],
    );
    let lines = drawn(&model);

    let top = &lines[row_of(&lines, " work ")];
    assert!(top.contains('┌'), "the title sits on the frame: {top}");
    assert!(
        top.contains(&theme::glyph(Symbol::MarkClose)),
        "with the mark that closes it: {top}"
    );
    let project = lines
        .iter()
        .position(|line| sidebar(line).contains("project"))
        .expect("the space's project is in the sidebar");
    let other = lines
        .iter()
        .position(|line| sidebar(line).contains("other"))
        .expect("a project with only kept work is too");
    assert!(project < other, "the space's project first");
    assert!(
        sidebar(&lines[project]).contains(&crate::ui::widget::text::small_digits(1)),
        "one of its rows needs you: {}",
        lines[project]
    );
    assert!(
        sidebar(&lines[other]).contains(&crate::ui::widget::text::small_digits(2)),
        "{}",
        lines[other]
    );
    assert!(
        sidebar(&lines[project + 1]).contains("4.0 KB on disk"),
        "its size once read: {}",
        lines[project + 1]
    );
    assert!(
        !sidebar(&lines[other + 1]).contains(char::is_alphanumeric)
            && !sidebar(&lines[other + 1]).contains(&theme::glyph(Symbol::Ellipsis)),
        "and nothing for one never read: {:?}",
        lines[other + 1]
    );
    let header = row_of(&lines, "/work/project");
    assert!(
        lines[header - 1].contains("project"),
        "the header names the project over its root"
    );
}

#[test]
fn the_modal_keeps_a_reading_width_on_a_wide_terminal() {
    let model = showing(Vec::new());
    let lines = drawn_at(&model, 240, 40);
    let top = &lines[row_of(&lines, " work ")];
    let left = top.chars().position(|glyph| glyph == '┌').unwrap();
    let right = top.chars().position(|glyph| glyph == '┐').unwrap();
    assert!(
        right - left < 121,
        "held to a reading width, not the terminal's: {}",
        right - left
    );
}

#[test]
fn projects_are_moved_between_by_key_and_by_click_and_each_is_read_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-projects"));
    let model = showing_with(
        Vec::new(),
        vec![preserved(OTHER, "t3", "elsewhere", WorkStateView::Parked)],
    );
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    press(&mut driven, Action::NextProject);
    assert_eq!(front_key(&driven.attach.model), PathBuf::from(OTHER));
    let work = driven.attach.model.work.as_ref().unwrap();
    let read = work
        .reads
        .get(Path::new(OTHER))
        .expect("read when first in front");
    assert!(read.pending);
    assert_eq!(read.asked, 2);
    let lines = drawn(&driven.attach.model);
    row_of(&lines, "reading…");
    row_of(&lines, "agent/t3");

    press(&mut driven, Action::PreviousProject);
    assert_eq!(front_key(&driven.attach.model), PathBuf::from(PROJECT));
    press(&mut driven, Action::PreviousProject);
    assert_eq!(
        front_key(&driven.attach.model),
        PathBuf::from(OTHER),
        "it wraps"
    );
    assert_eq!(
        driven.attach.model.remembered.checkouts_asked, 2,
        "a project already read is not read again"
    );

    driven.frame();
    let entry = driven.hit(|hit| *hit == WorkspaceHit::WorkProject(0));
    driven.press(entry.x + 2, entry.y);
    assert_eq!(
        front_key(&driven.attach.model),
        PathBuf::from(PROJECT),
        "a click on the sidebar moves too"
    );
}

#[test]
fn an_answer_to_an_earlier_read_or_another_project_is_dropped() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-stale"));
    let mut model = model_of(session(PROJECT));
    let mut overlay = WorkOverlay::open(Some(PathBuf::from(PROJECT)));
    overlay.reads.insert(
        PathBuf::from(PROJECT),
        ProjectRead {
            asked: 2,
            pending: true,
            answer: None,
        },
    );
    model.work = Some(overlay);
    let mut driven = driven(model, &home);
    let answer = |asked, project: &str| CheckoutsResolution {
        project: PathBuf::from(project),
        asked,
        view: Some(CheckoutsView::default()),
    };

    let sender = &driven.attach.channels.checkouts.sender;
    sender.send(answer(1, PROJECT)).unwrap();
    sender.send(answer(2, "/somewhere/else")).unwrap();
    driven.pump();
    let read = |driven: &super::workspace_tests::Driven<'_>| {
        driven.attach.model.work.as_ref().unwrap().reads[Path::new(PROJECT)]
            .answer
            .is_some()
    };
    assert!(!read(&driven));
    assert!(
        !driven
            .attach
            .model
            .work
            .as_ref()
            .unwrap()
            .reads
            .contains_key(Path::new("/somewhere/else")),
        "an answer nobody asked for is not kept"
    );

    driven
        .attach
        .channels
        .checkouts
        .sender
        .send(answer(2, PROJECT))
        .unwrap();
    driven.pump();
    assert!(read(&driven));
}

#[test]
fn every_row_is_listed_under_what_it_asks_of_you() {
    let mut in_use = checkout("by-hand-busy", CheckoutOwner::Operator);
    in_use.in_use = true;
    let mut holding = checkout(".worktrees/held", slot(None, false));
    holding.in_target = false;
    holding.ahead = 2;
    let mut theirs = checkout(".worktrees/t1", slot(Some("t1"), false));
    theirs.task = Some("t1".to_owned());
    let mut kept = preserved(PROJECT, "t1", "first", WorkStateView::Parked);
    kept.branch = "branch-t1".to_owned();
    let mut gone = preserved(PROJECT, "t9", "lost checkout", WorkStateView::Parked);
    gone.checkout = None;
    let model = showing_with(
        vec![
            checkout("by-hand", CheckoutOwner::Operator),
            checkout(
                ".keeper/worktrees/own",
                CheckoutOwner::Harness {
                    harness: "Keeper".to_owned(),
                },
            ),
            checkout(".worktrees/child", subagent(true)),
            checkout(".worktrees/free", slot(None, false)),
            checkout(".worktrees/live", slot(Some("reviewer"), true)),
            in_use,
            holding,
            theirs,
        ],
        vec![kept, gone],
    );
    let lines = drawn(&model);

    let order = [
        row_of(&lines, "NEEDS YOU"),
        row_of(&lines, "IN PROGRESS"),
        row_of(&lines, "READY FOR THE NEXT AGENT"),
        row_of(&lines, "OTHERS"),
    ];
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "the groups in one order:\n{}",
        lines.join("\n")
    );
    let under = |branch: &str| {
        let at = row_of(&lines, branch);
        order.iter().rposition(|heading| *heading < at).unwrap()
    };
    for (branch, group) in [
        ("branch-t1", 0),
        ("agent/t9", 0),
        ("branch-child", 0),
        ("branch-held", 0),
        ("branch-live", 1),
        ("branch-by-hand-busy", 1),
        ("branch-free", 2),
        ("branch-by-hand ", 3),
        ("branch-own", 3),
    ] {
        assert_eq!(under(branch), group, "{branch}:\n{}", lines.join("\n"));
    }
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("branch-t1"))
            .count(),
        1,
        "a task and the checkout it left are one row"
    );
    let gone = &lines[row_of(&lines, "agent/t9")];
    assert!(
        gone.contains("lost checkout · branch only"),
        "a task whose checkout is gone says so: {gone}"
    );
    assert!(lines[row_of(&lines, "branch-child")].contains("lexer · of parser"));
    assert!(lines[row_of(&lines, "branch-own")].contains("left to Keeper"));
    assert!(lines[row_of(&lines, "branch-live")].contains("reviewer"));
    let held = &lines[row_of(&lines, "branch-held")];
    assert!(
        held.contains("2 ahead") && held.contains("2.0 KB"),
        "{held}"
    );
}

#[test]
fn the_cards_count_what_the_rows_show_and_wear_their_marks() {
    let mut busy = checkout(".worktrees/busy", slot(Some("a"), true));
    busy.in_use = true;
    let lines = drawn(&showing_with(
        vec![
            busy,
            checkout(".worktrees/free", slot(None, false)),
            checkout(".worktrees/mine", CheckoutOwner::Operator),
        ],
        vec![preserved(PROJECT, "t2", "kept", WorkStateView::Parked)],
    ));
    let labels = row_of(&lines, "Needs you");
    for label in ["In progress", "Free", "Can remove", "On disk"] {
        assert!(lines[labels].contains(label), "{}", lines[labels]);
    }
    let values: Vec<&str> = lines[labels + 1]
        .split_whitespace()
        .filter(|word| {
            word.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '.')
        })
        .collect();
    assert!(
        values.ends_with(&["1", "1", "1", "1", "6.0", "KB"]),
        "needs you 1, in progress 1, free 1, can remove 1, 6 KB: {values:?}"
    );

    let cards = &lines[labels];
    for (card, branch) in [
        ("Needs you", "agent/t2"),
        ("In progress", "branch-busy"),
        ("Free", "branch-free"),
        ("Can remove", "branch-mine"),
    ] {
        let at = cards.find(card).unwrap();
        let mark = cards[..at].trim_end().chars().last().unwrap();
        assert!(
            lines[row_of(&lines, branch)].contains(mark),
            "{branch} wears {card}'s mark {mark:?}"
        );
    }
}

#[test]
fn each_row_offers_only_what_applies_to_it() {
    let mut adoptable = checkout(".worktrees/by-hand", CheckoutOwner::Operator);
    adoptable.adoptable = true;
    let mut model = showing_with(
        vec![
            adoptable,
            checkout(".worktrees/child", subagent(true)),
            checkout(".worktrees/t1", slot(Some("t1"), false)),
        ],
        vec![preserved(PROJECT, "t1", "kept", WorkStateView::Parked)],
    );
    let selected_on = |model: &mut WorkspaceModel, branch: &str| {
        let work = model.work.as_ref().unwrap();
        let (_, project) = front(model, work).unwrap();
        let index = rows_of(model, work, &project)
            .iter()
            .position(|row| row.title() == branch)
            .unwrap();
        model.work.as_mut().unwrap().selected = index;
        buttons(&drawn(model))
    };

    let task = selected_on(&mut model, "agent/t1");
    for offered in ["Resume", "Deliver", "Mark done", "Open space", "Discard"] {
        assert!(task.contains(offered), "a task offers {offered}: {task}");
    }
    for absent in ["Adopt", "Join", "Remove"] {
        assert!(!task.contains(absent), "a task offers no {absent}: {task}");
    }

    let mine = selected_on(&mut model, "branch-by-hand");
    for offered in ["Open space", "Adopt", "Remove"] {
        assert!(mine.contains(offered), "{offered}: {mine}");
    }
    for absent in ["Resume", "Deliver", "Mark done", "Discard", "Join"] {
        assert!(!mine.contains(absent), "no {absent}: {mine}");
    }

    let child = selected_on(&mut model, "branch-child");
    assert!(child.contains("Join"), "{child}");
    let lines = drawn(&model);
    let foot = lines
        .iter()
        .rev()
        .find(|line| line.contains("esc"))
        .unwrap();
    assert!(
        foot.contains("d remove") && foot.contains("j join"),
        "the foot names the key under what it does here: {foot}"
    );
}

#[test]
fn the_same_key_discards_a_task_and_removes_a_checkout_and_says_which() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-take-away"));
    let model = showing_with(
        vec![
            checkout(".worktrees/done", CheckoutOwner::Operator),
            checkout(".worktrees/t1", slot(Some("t1"), false)),
        ],
        vec![preserved(PROJECT, "t1", "kept", WorkStateView::Parked)],
    );
    let mut driven = driven(model, &home);

    press(&mut driven, Action::DiscardTask);
    assert_eq!(asking(&driven.attach.model), Some(WorkQuestion::Discard));
    let lines = drawn(&driven.attach.model).join("\n");
    assert!(
        lines.contains("discard agent/t1? its checkout and its branch are deleted, and the"),
        "{lines}"
    );
    press(&mut driven, Action::Dismiss);
    assert_eq!(asking(&driven.attach.model), None, "esc withdraws it");
    assert!(
        driven.attach.model.work.is_some(),
        "and leaves the modal open"
    );

    press(&mut driven, Action::SelectNext);
    press(&mut driven, Action::DiscardTask);
    assert_eq!(asking(&driven.attach.model), Some(WorkQuestion::Remove));
    let lines = drawn(&driven.attach.model);
    row_of(
        &lines,
        "remove branch-done? its branch is kept, and 2.0 KB is freed",
    );
    press(&mut driven, Action::ConfirmDiscard);
    press(&mut driven, Action::DiscardTask);
    press(&mut driven, Action::ConfirmDiscard);
    assert!(
        driven.attach.model.remembered.checkout_change_pending,
        "confirmed once, started once"
    );
    assert!(driven.attach.model.remembered.notice.is_some());
}

#[test]
fn a_task_whose_checkout_is_gone_is_discarded_as_a_branch() {
    let mut gone = preserved(PROJECT, "t9", "lost", WorkStateView::Parked);
    gone.checkout = None;
    let mut model = showing_with(Vec::new(), vec![gone]);
    model.work.as_mut().unwrap().asking = Some(WorkQuestion::Discard);
    let lines = drawn(&model).join("\n");
    assert!(
        lines.contains("discard agent/t9? its branch is deleted, and the work on it is lost"),
        "{lines}"
    );
    model.work.as_mut().unwrap().asking = None;
    let lines = drawn(&model);
    row_of(&lines, "no checkout — the branch keeps the work");
    assert!(!buttons(&lines).contains("Open space"));
}

#[test]
fn a_removal_the_view_already_knows_is_refused_says_why_at_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-refused"));
    let mut dirty = checkout(".worktrees/draft", CheckoutOwner::Operator);
    dirty.dirty = true;
    dirty.removal_refusal = Some("it holds uncommitted work".to_owned());
    let model = showing(vec![dirty]);

    let lines = drawn(&model);
    row_of(&lines, "kept: it holds uncommitted work");

    let mut driven = driven(model, &home);
    press(&mut driven, Action::DiscardTask);

    let model = &driven.attach.model;
    let toast = model.remembered.toasts.back().expect("the refusal is said");
    assert_eq!(toast.kind, ToastKind::Failed);
    assert_eq!(toast.text, "not removed");
    assert_eq!(toast.detail, "branch-draft: it holds uncommitted work");
    assert_eq!(asking(model), None, "nothing to ask");
    assert!(!model.remembered.checkout_change_pending);
}

/// UZE recycles its own slots, so a refusal to remove one is the same
/// sentence under every row, and is not said.
#[test]
fn an_agents_slot_does_not_say_why_it_cannot_be_removed() {
    let mut held = checkout(".worktrees/slot", slot(None, true));
    held.removal_refusal = Some("an agent holds it".to_owned());
    let lines = drawn(&showing(vec![held]));
    assert!(
        !lines.iter().any(|line| line.contains("an agent holds it")),
        "{}",
        lines.join("\n")
    );
}

#[test]
fn a_clean_up_asks_with_what_would_go_and_how_much() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-clean"));
    let mut big = checkout("../elsewhere", CheckoutOwner::Operator);
    big.bytes = 3 * 1024 * 1024 - 2048;
    let mut kept = checkout(".worktrees/working", CheckoutOwner::Operator);
    kept.dirty = true;
    let model = showing(vec![
        checkout(".worktrees/one", CheckoutOwner::Operator),
        big,
        kept,
        checkout(".worktrees/slot", slot(None, false)),
    ]);
    let mut driven = driven(model, &home);

    press(&mut driven, Action::CleanUpCheckouts);
    assert_eq!(asking(&driven.attach.model), Some(WorkQuestion::CleanUp));
    let lines = drawn(&driven.attach.model).join("\n");
    assert!(
        lines.contains("clean up? removes 2 checkouts of yours that are clean, unused and in"),
        "{lines}"
    );
    assert!(lines.contains("main, freeing 3.0 MB"), "{lines}");

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-nothing"));
    let model = showing(vec![checkout(".worktrees/slot", slot(None, false))]);
    let mut driven = super::workspace_tests::driven(model, &home);
    press(&mut driven, Action::CleanUpCheckouts);
    assert_eq!(
        asking(&driven.attach.model),
        None,
        "nothing to remove is said, not asked"
    );
    let toast = driven.attach.model.remembered.toasts.back().unwrap();
    assert_eq!(toast.text, "nothing to clean up");
}

#[test]
fn a_parked_agents_subagent_is_joined_on_asking_and_a_running_ones_is_refused() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-join"));
    let model = showing(vec![checkout(".worktrees/child", subagent(true))]);
    let lines = drawn(&model);
    assert!(lines[row_of(&lines, "branch-child")].contains("parked"));
    let mut driven = driven(model, &home);

    press(&mut driven, Action::JoinCheckout);
    assert_eq!(asking(&driven.attach.model), Some(WorkQuestion::Join));
    let lines = drawn(&driven.attach.model);
    row_of(
        &lines,
        "join lexer into parser? its commits are replayed onto parser's branch",
    );
    press(&mut driven, Action::ConfirmDiscard);
    assert!(driven.attach.model.remembered.checkout_change_pending);

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-join-no"));
    let mut driven = super::workspace_tests::driven(
        showing(vec![checkout(".worktrees/child", subagent(false))]),
        &home,
    );
    press(&mut driven, Action::JoinCheckout);
    assert_eq!(asking(&driven.attach.model), None);
    let toast = driven.attach.model.remembered.toasts.back().unwrap();
    assert_eq!(toast.text, "not joined");
    assert!(
        toast.detail.contains("parser is still running"),
        "{}",
        toast.detail
    );
}

#[test]
fn the_work_key_and_the_space_menu_open_on_the_spaces_project() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-open"));
    let mut model = model_of(session(PROJECT));
    model.remembered.preserved_work =
        vec![preserved(OTHER, "t3", "elsewhere", WorkStateView::Parked)];
    let mut driven = driven(model, &home);

    let chord = uze_keys::active()
        .chord_for(Action::ToggleWork, &[uze_keys::Scope::Workspace])
        .unwrap();
    driven.press_key(key_event(chord));
    assert_eq!(
        front_key(&driven.attach.model),
        PathBuf::from(PROJECT),
        "the space in front, even with work kept elsewhere"
    );
    assert!(
        driven.attach.model.work.as_ref().unwrap().reads[Path::new(PROJECT)].pending,
        "and its checkouts are read at once"
    );
    press(&mut driven, Action::ToggleWork);
    assert!(driven.attach.model.work.is_none(), "the same key closes it");

    let space = driven
        .attach
        .model
        .session
        .as_ref()
        .unwrap()
        .workspace
        .selected_space;
    driven
        .attach
        .perform_menu_action(MenuTarget::Space(space), Action::ShowSpaceWork);
    assert_eq!(front_key(&driven.attach.model), PathBuf::from(PROJECT));
}

#[test]
fn with_no_space_it_opens_on_the_first_project_that_needs_you() {
    let mut model = WorkspaceModel::default();
    model.remembered.preserved_work = vec![
        preserved(PROJECT, "t1", "delivering", WorkStateView::Integrating),
        preserved(OTHER, "t3", "elsewhere", WorkStateView::Parked),
    ];
    model.work = Some(WorkOverlay::open(None));
    assert_eq!(front_key(&model), PathBuf::from(OTHER));
}

#[test]
fn esc_withdraws_a_question_first_and_then_closes() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-esc"));
    let mut model = showing(vec![checkout(".worktrees/done", CheckoutOwner::Operator)]);
    model.work.as_mut().unwrap().asking = Some(WorkQuestion::Remove);
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    press(&mut driven, Action::Dismiss);
    assert!(
        driven.attach.model.work.is_some(),
        "the question goes first"
    );
    assert_eq!(asking(&driven.attach.model), None);
    press(&mut driven, Action::Dismiss);
    assert!(driven.attach.model.work.is_none(), "and then the modal");

    driven.attach.model.work = Some(WorkOverlay::open(Some(PathBuf::from(PROJECT))));
    driven.frame();
    let close = driven.hit(|hit| *hit == WorkspaceHit::WorkClose);
    driven.press(close.x + 1, close.y);
    assert!(
        driven.attach.model.work.is_none(),
        "the close mark closes it"
    );
}

#[test]
fn what_a_project_holds_is_said_when_it_is_nothing_or_not_yet_known() {
    let lines = drawn(&showing(Vec::new()));
    row_of(&lines, "nothing here besides the project's own checkout");

    let mut model = showing(Vec::new());
    model.work.as_mut().unwrap().reads.insert(
        PathBuf::from(PROJECT),
        ProjectRead {
            asked: 2,
            pending: true,
            answer: None,
        },
    );
    let lines = drawn(&model);
    row_of(&lines, "reading…");
    let project = lines
        .iter()
        .position(|line| sidebar(line).contains("project"))
        .unwrap();
    assert!(
        sidebar(&lines[project + 1]).contains(&theme::glyph(Symbol::Ellipsis)),
        "{}",
        lines[project + 1]
    );
}

/// Facts line up in columns, so the same fact sits in the same place on
/// every row, a task's included.
#[test]
fn every_rows_facts_line_up_in_columns() {
    let mut busy = checkout(".worktrees/busy", CheckoutOwner::Operator);
    busy.in_use = true;
    busy.in_target = false;
    busy.ahead = 12;
    let mut theirs = checkout(".worktrees/t1", slot(Some("t1"), false));
    theirs.task = Some("t1".to_owned());
    let model = showing_with(
        vec![
            busy,
            checkout(".worktrees/quiet", CheckoutOwner::Operator),
            theirs,
        ],
        vec![preserved(PROJECT, "t1", "kept", WorkStateView::GateFailed)],
    );
    let lines = drawn(&model);
    let size_column = |branch: &str| {
        let line = &lines[row_of(&lines, branch)];
        let byte = line.find("KB").expect("a size is drawn");
        line[..byte].chars().count()
    };
    assert_eq!(size_column("branch-busy"), size_column("branch-quiet"));
    assert_eq!(size_column("branch-busy"), size_column("agent/t1"));
    assert!(lines[row_of(&lines, "agent/t1")].contains("checks failed"));
}

/// An agent labelled from its branch is not named twice on its row.
#[test]
fn a_label_the_branch_already_says_is_not_repeated() {
    let named = checkout(
        ".worktrees/space-control",
        slot(Some("space control"), false),
    );
    let lines = drawn(&showing(vec![named]));
    let row = &lines[row_of(&lines, "branch-space-control")];
    assert_eq!(row.matches("control").count(), 1, "{row}");
}

#[test]
fn a_change_says_what_it_came_to() {
    let clean_up = CleanUp {
        removed: vec![
            RemovedCheckout {
                name: ".worktrees/one".to_owned(),
                bytes: 3 * 1024 * 1024,
            },
            RemovedCheckout {
                name: "../elsewhere".to_owned(),
                bytes: 1024 * 1024,
            },
        ],
        kept: vec![KeptCheckout {
            name: ".worktrees/working".to_owned(),
            reason: "it holds uncommitted work".to_owned(),
        }],
        left_to_harness: vec![".keeper/worktrees/own".to_owned()],
    };
    let (kind, title, detail) = describe_change(&CheckoutOutcome::CleanedUp(clean_up));
    assert_eq!(kind, ToastKind::Done);
    assert_eq!(title, "cleaned up");
    assert_eq!(
        detail,
        "removed 2 · freed 4.0 MB · kept .worktrees/working: it holds uncommitted work · \
         1 left to their harness"
    );

    let (kind, title, detail) = describe_change(&CheckoutOutcome::CleanedUp(CleanUp::default()));
    assert_eq!(kind, ToastKind::Told);
    assert_eq!(title, "nothing to clean up");
    assert_eq!(
        detail,
        "no checkout of yours is clean, unused and in the target"
    );

    let (kind, title, detail) = describe_change(&CheckoutOutcome::Joined {
        topic: "lexer".to_owned(),
        parent: "parser".to_owned(),
        answer: Ok(JoinedWork::Joined { commits: 2 }),
    });
    assert_eq!(
        (kind, title.as_str(), detail.as_str()),
        (ToastKind::Done, "joined", "lexer into parser · 2 commits")
    );
    let (kind, title, _) = describe_change(&CheckoutOutcome::Joined {
        topic: "lexer".to_owned(),
        parent: "parser".to_owned(),
        answer: Ok(JoinedWork::Conflicted {
            checkout: PathBuf::from("/work/project/.worktrees/child"),
            paths: vec![PathBuf::from("src/lib.rs")],
        }),
    });
    assert_eq!((kind, title.as_str()), (ToastKind::Warned, "join paused"));

    let mut model = WorkspaceModel::default();
    let (kind, title, detail) = describe_change(&CheckoutOutcome::Removed {
        name: ".worktrees/done".to_owned(),
        answer: Err("a process is working inside it".to_owned()),
    });
    model.raise_toast(kind, title, detail, None);
    let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
    terminal
        .draw(|frame| {
            render::render(frame, &model, &[], &mut Vec::new(), &mut Default::default());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let screen: String = (0..buffer.area.height)
        .flat_map(|row| (0..buffer.area.width).map(move |column| (column, row)))
        .map(|position| buffer[position].symbol().to_owned())
        .collect();
    assert!(screen.contains("not removed"), "{screen}");
    assert!(
        screen.contains("a process is working inside it"),
        "{screen}"
    );
}

#[test]
fn with_nothing_anywhere_it_says_so_and_offers_nothing_to_act_on() {
    let model = WorkspaceModel {
        work: Some(WorkOverlay::open(None)),
        ..WorkspaceModel::default()
    };
    let mut hits = Vec::new();
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal
        .draw(|frame| {
            render_work(
                frame,
                frame.area(),
                &model,
                model.work.as_ref().unwrap(),
                &mut hits,
            )
        })
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol().to_owned())
        .collect();
    assert!(
        screen.contains("nothing kept, and no space open"),
        "{screen}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::WorkAction(_))),
        "a disabled button answers no click: {hits:?}"
    );
}

#[test]
fn the_modal_draws_on_a_small_terminal_without_panicking() {
    let mut refused = checkout(".worktrees/a-long-name-for-a-checkout", subagent(true));
    refused.removal_refusal = Some("a process is working inside it".to_owned());
    let mut model = showing_with(
        vec![refused, checkout("by-hand", CheckoutOwner::Operator)],
        vec![preserved(
            OTHER,
            "t2",
            "a label longer than the column",
            WorkStateView::Uncommitted,
        )],
    );
    for question in [
        None,
        Some(WorkQuestion::CleanUp),
        Some(WorkQuestion::Discard),
    ] {
        model.work.as_mut().unwrap().asking = question;
        for (width, height) in [(40, 12), (20, 6), (3, 3), (1, 1)] {
            drawn_at(&model, width, height);
        }
        let lines = drawn_at(&model, 40, 12);
        row_of(&lines, " work ");
    }
}
