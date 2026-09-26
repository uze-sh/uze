//! The work modal: its frame and sections, how the checkouts section
//! groups and what it says when it refuses, what the preserved section
//! lists, and what a change tells the operator.

use super::workspace_tests::{driven, key_event, preserved};
use super::*;
use crate::ui::widget::ToastKind;
use ratatui::{Terminal, backend::TestBackend};
use uze_application::{
    CheckoutOwner, CheckoutView, CheckoutsView, CleanUp, JoinedWork, KeptCheckout, RemovedCheckout,
};

const PROJECT: &str = "/work/project";

fn checkout(name: &str, owner: CheckoutOwner) -> CheckoutView {
    CheckoutView {
        path: Path::new(PROJECT).join(name),
        name: name.to_owned(),
        owner,
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

fn subagent(joinable: bool) -> CheckoutOwner {
    CheckoutOwner::Subagent {
        parent: "parser".to_owned(),
        parent_id: "p1".to_owned(),
        topic: Some("lexer".to_owned()),
        joinable,
    }
}

/// A model with the modal open on the checkouts section over
/// `checkouts`, as a read answered it.
fn showing(checkouts: Vec<CheckoutView>) -> WorkspaceModel {
    let mut model = WorkspaceModel {
        work: Some(WorkOverlay::open(
            WorkSection::Checkouts,
            Some(PathBuf::from(PROJECT)),
        )),
        ..WorkspaceModel::default()
    };
    model.remembered.checkouts_asked = 1;
    model.remembered.checkouts = Some(CheckoutsResolution {
        project: PathBuf::from(PROJECT),
        asked: 1,
        view: Some(CheckoutsView {
            primary: PathBuf::from(PROJECT),
            target: "main".to_owned(),
            total_bytes: checkouts.iter().map(|checkout| checkout.bytes).sum(),
            checkouts,
        }),
    });
    model
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

fn press(driven: &mut super::workspace_tests::Driven<'_>, scope: uze_keys::Scope, action: Action) {
    let chord = uze_keys::active()
        .chord_for(action, &[scope])
        .unwrap_or_else(|| panic!("{action} is bound in {scope:?}"));
    driven.press_key(key_event(chord));
}

fn on_checkouts(driven: &mut super::workspace_tests::Driven<'_>, action: Action) {
    press(driven, uze_keys::Scope::Checkouts, action);
}

fn asking(model: &WorkspaceModel) -> Option<CheckoutQuestion> {
    model.work.as_ref()?.checkouts.as_ref()?.asking
}

#[test]
fn the_modal_is_titled_work_and_lists_its_sections_with_their_counts() {
    let mut model = showing(vec![
        checkout(".worktrees/one", CheckoutOwner::Operator),
        checkout(".worktrees/two", CheckoutOwner::Operator),
    ]);
    model.remembered.preserved_work = vec![
        preserved("/repo", "t2", "yesterday", WorkStateView::Uncommitted),
        preserved("/other", "t3", "elsewhere", WorkStateView::Parked),
        preserved("/other", "t4", "older", WorkStateView::Parked),
    ];
    let lines = drawn(&model);

    let top = &lines[row_of(&lines, " work ")];
    assert!(top.contains('┌'), "the title sits on the frame: {top}");
    assert!(
        top.contains(&theme::glyph(Symbol::MarkClose)),
        "with the mark that closes it: {top}"
    );
    let preserved_row = row_of(&lines, "Preserved");
    assert!(
        lines[preserved_row].contains(&crate::ui::widget::text::small_digits(3)),
        "{}",
        lines[preserved_row]
    );
    row_of(&lines, "kept work");
    let checkouts_row = lines
        .iter()
        .position(|line| {
            let sidebar: String = line.chars().take(40).collect();
            sidebar.contains("Checkouts")
        })
        .expect("the section is in the sidebar");
    assert!(checkouts_row > preserved_row);
    assert!(
        lines[checkouts_row].contains(&crate::ui::widget::text::small_digits(2)),
        "{}",
        lines[checkouts_row]
    );
    row_of(&lines, "4.0 KB on disk");
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
fn a_section_is_moved_to_by_key_and_by_click_and_esc_closes() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-sections"));
    let mut model = showing(Vec::new());
    model.work.as_mut().unwrap().section = WorkSection::Preserved;
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    press(
        &mut driven,
        uze_keys::Scope::PreservedWork,
        Action::NextSection,
    );
    assert_eq!(
        driven.attach.model.work.as_ref().unwrap().section,
        WorkSection::Checkouts
    );
    on_checkouts(&mut driven, Action::PreviousSection);
    assert_eq!(
        driven.attach.model.work.as_ref().unwrap().section,
        WorkSection::Preserved
    );

    driven.frame();
    let entry = driven.hit(|hit| *hit == WorkspaceHit::WorkSection(WorkSection::Checkouts));
    driven.press(entry.x + 2, entry.y);
    assert_eq!(
        driven.attach.model.work.as_ref().unwrap().section,
        WorkSection::Checkouts,
        "a click on the sidebar moves too"
    );

    driven.frame();
    let close = driven.hit(|hit| *hit == WorkspaceHit::WorkClose);
    driven.press(close.x + 1, close.y);
    assert!(
        driven.attach.model.work.is_none(),
        "the close mark closes it"
    );

    driven.attach.model.work = Some(WorkOverlay::open(WorkSection::Checkouts, None));
    driven
        .attach
        .model
        .work
        .as_mut()
        .unwrap()
        .preserved
        .confirm_discard = true;
    on_checkouts(&mut driven, Action::Dismiss);
    assert!(
        driven.attach.model.work.is_some(),
        "esc withdraws a question first"
    );
    on_checkouts(&mut driven, Action::Dismiss);
    assert!(driven.attach.model.work.is_none(), "and then closes");
}

#[test]
fn the_space_menu_and_the_work_key_open_the_same_modal() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-work-open"));
    let model = super::workspace_tests::model_of(super::workspace_tests::session(PROJECT));
    let mut driven = driven(model, &home);

    press(&mut driven, uze_keys::Scope::Workspace, Action::ToggleWork);
    let work = driven.attach.model.work.as_ref().expect("the key opens it");
    assert_eq!(work.section, WorkSection::Preserved);
    assert_eq!(
        work.checkouts.as_ref().map(|checkouts| &checkouts.project),
        Some(&PathBuf::from(PROJECT)),
        "the checkouts of the space in front are read at once"
    );
    assert_eq!(
        driven.attach.model.remembered.checkouts_pending,
        Some(PathBuf::from(PROJECT))
    );
    press(
        &mut driven,
        uze_keys::Scope::PreservedWork,
        Action::ToggleWork,
    );
    assert!(
        driven.attach.model.work.is_none(),
        "and the same key closes it"
    );

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
        .perform_menu_action(MenuTarget::Space(space), Action::ShowCheckouts);
    assert_eq!(
        driven.attach.model.work.as_ref().map(|work| work.section),
        Some(WorkSection::Checkouts),
        "the space's menu opens it on its checkouts"
    );
}

#[test]
fn a_checkouts_read_that_is_still_out_is_said_rather_than_waited_for() {
    let model = WorkspaceModel {
        work: Some(WorkOverlay::open(
            WorkSection::Checkouts,
            Some(PathBuf::from(PROJECT)),
        )),
        ..WorkspaceModel::default()
    };
    let lines = drawn(&model);
    row_of(&lines, "reading every checkout…");
    row_of(&lines, "reading…");
}

#[test]
fn every_checkout_is_drawn_under_its_owner_by_its_branch() {
    let model = showing(vec![
        checkout("by-hand", CheckoutOwner::Operator),
        checkout(
            ".keeper/worktrees/own",
            CheckoutOwner::Harness {
                harness: "Keeper".to_owned(),
            },
        ),
        checkout(".worktrees/child", subagent(false)),
        checkout(
            ".worktrees/slot",
            CheckoutOwner::Agent {
                holder: Some("reviewer".to_owned()),
            },
        ),
        checkout(".worktrees/newer", CheckoutOwner::Unreadable),
    ]);
    let lines = drawn(&model);

    let order = [
        row_of(&lines, "AGENT SLOTS"),
        row_of(&lines, "branch-slot"),
        row_of(&lines, "SUBAGENTS"),
        row_of(&lines, "branch-child"),
        row_of(&lines, "HARNESS ISOLATION"),
        row_of(&lines, "branch-own"),
        row_of(&lines, "YOURS"),
        row_of(&lines, "branch-by-hand"),
        row_of(&lines, "UNREADABLE RECORD"),
        row_of(&lines, "branch-newer"),
    ];
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "each checkout under its owner, the owners in one order:\n{}",
        lines.join("\n")
    );
    assert!(lines[row_of(&lines, "branch-slot")].contains("reviewer"));
    assert!(lines[row_of(&lines, "branch-child")].contains("lexer · of parser"));
    assert!(lines[row_of(&lines, "branch-own")].contains("left to Keeper"));
    assert!(lines[row_of(&lines, "branch-slot")].contains("done · 2.0 KB"));
}

#[test]
fn only_the_selected_checkout_says_where_it_is() {
    let mut ahead = checkout(".worktrees/ahead", CheckoutOwner::Operator);
    ahead.in_target = false;
    ahead.ahead = 3;
    ahead.in_use = true;
    ahead.dirty = true;
    let model = showing(vec![
        checkout(".worktrees/a-first", CheckoutOwner::Operator),
        ahead,
    ]);
    let lines = drawn(&model);
    row_of(&lines, "/work/project/.worktrees/a-first");
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("/work/project/.worktrees/ahead")),
        "a path is the selection's detail, not every row's:\n{}",
        lines.join("\n")
    );
    assert!(
        lines[row_of(&lines, "branch-ahead")].contains("in use · uncommitted · 3 ahead"),
        "{}",
        lines[row_of(&lines, "branch-ahead")]
    );
}

#[test]
fn the_summary_cards_count_what_the_list_holds() {
    let mut in_use = checkout(
        ".worktrees/busy",
        CheckoutOwner::Agent {
            holder: Some("a".to_owned()),
        },
    );
    in_use.in_use = true;
    in_use.removal_refusal = Some("a process is working inside it".to_owned());
    let model = showing(vec![
        in_use,
        checkout(".worktrees/free", CheckoutOwner::Agent { holder: None }),
        checkout(".worktrees/mine", CheckoutOwner::Operator),
    ]);
    let lines = drawn(&model);
    let labels = row_of(&lines, "In use");
    for label in ["Free", "Can remove", "On disk"] {
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
        values.ends_with(&["1", "1", "2", "6.0", "KB"]),
        "in use 1, free 1, can remove 2, 6 KB: {values:?}"
    );
}

#[test]
fn a_removal_the_view_already_knows_is_refused_says_why_at_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-refused"));
    let mut dirty = checkout(".worktrees/draft", CheckoutOwner::Operator);
    dirty.dirty = true;
    dirty.removal_refusal = Some("it holds uncommitted work".to_owned());
    let model = showing(vec![dirty]);

    let lines = drawn(&model);
    row_of(&lines, "cannot remove: it holds uncommitted work");

    let mut driven = driven(model, &home);
    on_checkouts(&mut driven, Action::RemoveCheckout);

    let model = &driven.attach.model;
    let toast = model.remembered.toasts.back().expect("the refusal is said");
    assert_eq!(toast.kind, ToastKind::Failed);
    assert_eq!(toast.text, "not removed");
    assert_eq!(toast.detail, ".worktrees/draft: it holds uncommitted work");
    assert_eq!(asking(model), None, "nothing to ask");
    assert!(!model.remembered.checkout_change_pending);
}

#[test]
fn a_removal_is_asked_once_and_started_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-asked"));
    let model = showing(vec![checkout(".worktrees/done", CheckoutOwner::Operator)]);
    let mut driven = driven(model, &home);

    on_checkouts(&mut driven, Action::RemoveCheckout);
    assert_eq!(
        asking(&driven.attach.model),
        Some(CheckoutQuestion::Remove),
        "the key asks rather than removes"
    );
    let lines = drawn(&driven.attach.model);
    row_of(
        &lines,
        "remove branch-done? its branch is kept, and 2.0 KB is freed",
    );

    on_checkouts(&mut driven, Action::ConfirmCheckoutChange);
    on_checkouts(&mut driven, Action::RemoveCheckout);
    on_checkouts(&mut driven, Action::ConfirmCheckoutChange);

    assert!(driven.attach.model.remembered.checkout_change_pending);
    assert!(
        driven.attach.model.remembered.notice.is_some(),
        "and the operator is told something is running"
    );
}

#[test]
fn a_clean_up_asks_with_what_would_go_and_how_much() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-clean"));
    let mut big = checkout("../elsewhere", CheckoutOwner::Operator);
    big.bytes = 3 * 1024 * 1024 - 2048;
    let mut kept = checkout(".worktrees/working", CheckoutOwner::Operator);
    kept.dirty = true;
    let model = showing(vec![
        checkout(".worktrees/one", CheckoutOwner::Operator),
        big,
        kept,
        checkout(".worktrees/slot", CheckoutOwner::Agent { holder: None }),
    ]);
    let mut driven = driven(model, &home);

    on_checkouts(&mut driven, Action::CleanUpCheckouts);
    assert_eq!(
        asking(&driven.attach.model),
        Some(CheckoutQuestion::CleanUp)
    );
    let lines = drawn(&driven.attach.model).join("\n");
    assert!(
        lines.contains("clean up? removes 2 checkouts of yours that are clean, unused and in"),
        "{lines}"
    );
    assert!(lines.contains("main, freeing 3.0 MB"), "{lines}");

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-nothing"));
    let model = showing(vec![checkout(
        ".worktrees/slot",
        CheckoutOwner::Agent { holder: None },
    )]);
    let mut driven = super::workspace_tests::driven(model, &home);
    on_checkouts(&mut driven, Action::CleanUpCheckouts);
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
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-join"));
    let model = showing(vec![checkout(".worktrees/child", subagent(true))]);
    let lines = drawn(&model);
    assert!(lines[row_of(&lines, "branch-child")].contains("parked"));
    let mut driven = driven(model, &home);

    on_checkouts(&mut driven, Action::JoinCheckout);
    assert_eq!(asking(&driven.attach.model), Some(CheckoutQuestion::Join));
    let lines = drawn(&driven.attach.model);
    row_of(
        &lines,
        "join lexer into parser? its commits are replayed onto parser's branch",
    );
    on_checkouts(&mut driven, Action::ConfirmCheckoutChange);
    assert!(driven.attach.model.remembered.checkout_change_pending);

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-join-no"));
    let mut driven = super::workspace_tests::driven(
        showing(vec![checkout(".worktrees/child", subagent(false))]),
        &home,
    );
    on_checkouts(&mut driven, Action::JoinCheckout);
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
fn an_answer_to_an_earlier_read_is_dropped() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-stale"));
    let mut model = WorkspaceModel {
        work: Some(WorkOverlay::open(
            WorkSection::Checkouts,
            Some(PathBuf::from(PROJECT)),
        )),
        ..WorkspaceModel::default()
    };
    model.remembered.checkouts_asked = 2;
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
    assert!(driven.attach.model.remembered.checkouts.is_none());

    driven
        .attach
        .channels
        .checkouts
        .sender
        .send(answer(2, PROJECT))
        .unwrap();
    driven.pump();
    assert!(driven.attach.model.remembered.checkouts.is_some());
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

/// The list answers from the machine's records and subtracts the tabs
/// this client is in front of; a list that crosses projects names each
/// row's own, and the keys it names are the keymap's.
#[test]
fn the_preserved_section_lists_work_across_projects_with_the_keymaps_keys() {
    let mut model = WorkspaceModel {
        work: Some(WorkOverlay::open(WorkSection::Preserved, None)),
        ..WorkspaceModel::default()
    };
    model.remembered.preserved_work = vec![
        preserved("/repo", "t2", "yesterday", WorkStateView::Uncommitted),
        preserved("/other", "t3", "elsewhere", WorkStateView::Parked),
    ];
    let lines = drawn(&model);
    let yesterday = &lines[row_of(&lines, "yesterday")];
    assert!(
        yesterday.contains("repo · uncommitted changes"),
        "{yesterday}"
    );
    assert!(lines[row_of(&lines, "elsewhere")].contains("other · nobody is there"));
    row_of(&lines, "agent/t2 · /repo/.worktrees/t2");

    let text = lines.join("\n");
    let scopes = [uze_keys::Scope::Global, uze_keys::Scope::PreservedWork];
    for action in [
        Action::ResumeTask,
        Action::DeliverTask,
        Action::FinishTask,
        Action::DiscardTask,
        Action::NextSection,
        Action::Dismiss,
    ] {
        let chord = uze_keys::active()
            .chord_for(action, &scopes)
            .unwrap_or_else(|| panic!("{action} is bound here"));
        assert!(
            text.contains(&format!("{chord} {}", action.label().to_lowercase())),
            "{action} is named with the key the keymap binds:\n{text}"
        );
    }
}

#[test]
fn nothing_preserved_is_said_and_offers_nothing_to_act_on() {
    let model = WorkspaceModel {
        work: Some(WorkOverlay::open(WorkSection::Preserved, None)),
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
        screen.contains("nothing preserved — every task is either live or delivered"),
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
    let mut model = showing(vec![refused, checkout("by-hand", CheckoutOwner::Operator)]);
    model.remembered.preserved_work = vec![preserved(
        "/repo",
        "t2",
        "a label longer than the column",
        WorkStateView::Uncommitted,
    )];
    for section in WorkSection::ALL {
        model.work.as_mut().unwrap().section = section;
        for (width, height) in [(40, 12), (20, 6), (3, 3), (1, 1)] {
            drawn_at(&model, width, height);
        }
        let work = model.work.as_mut().unwrap();
        work.preserved.confirm_discard = true;
        work.checkouts.as_mut().unwrap().asking = Some(CheckoutQuestion::CleanUp);
        let lines = drawn_at(&model, 40, 12);
        row_of(&lines, " work ");
        model.work.as_mut().unwrap().withdraw();
    }
}
