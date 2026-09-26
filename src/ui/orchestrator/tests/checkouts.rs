//! The checkouts view: how it groups, what it says when it refuses, and
//! what a clean-up tells the operator.

use super::workspace_tests::{driven, key_event};
use super::*;
use crate::ui::widget::ToastKind;
use ratatui::{Terminal, backend::TestBackend};
use uze_application::{
    CheckoutOwner, CheckoutView, CheckoutsView, CleanUp, KeptCheckout, RemovedCheckout,
};

const PROJECT: &str = "/work/project";

fn checkout(name: &str, owner: CheckoutOwner) -> CheckoutView {
    CheckoutView {
        path: Path::new(PROJECT).join(name),
        name: name.to_owned(),
        owner,
        branch: Some(format!("branch-{name}")),
        dirty: false,
        in_target: true,
        in_use: false,
        last_changed: None,
        bytes: 2048,
        adoptable: false,
        removal_refusal: None,
    }
}

/// A model with the view open over `checkouts`, as a read answered it.
fn showing(checkouts: Vec<CheckoutView>) -> WorkspaceModel {
    let mut model = WorkspaceModel {
        checkouts: Some(CheckoutsOverlay::over(PathBuf::from(PROJECT))),
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

fn drawn(model: &WorkspaceModel) -> Vec<String> {
    let overlay = model.checkouts.as_ref().expect("open");
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            render_checkouts(frame, frame.area(), model, overlay, &mut Vec::new());
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

fn row_of(lines: &[String], text: &str) -> usize {
    lines
        .iter()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("{text:?} is drawn:\n{}", lines.join("\n")))
}

fn press(driven: &mut super::workspace_tests::Driven<'_>, action: Action) {
    let chord = uze_keys::active()
        .chord_for(action, &[uze_keys::Scope::Checkouts])
        .unwrap_or_else(|| panic!("{action} is bound in the checkouts view"));
    driven.press_key(key_event(chord));
}

#[test]
fn every_checkout_is_drawn_under_its_owner_in_one_order() {
    let model = showing(vec![
        checkout("by-hand", CheckoutOwner::Operator),
        checkout(
            ".keeper/worktrees/own",
            CheckoutOwner::Harness {
                harness: "Keeper".to_owned(),
            },
        ),
        checkout(
            ".worktrees/child",
            CheckoutOwner::Subagent {
                parent: "parser".to_owned(),
            },
        ),
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
        row_of(&lines, ".worktrees/slot"),
        row_of(&lines, "SUBAGENTS"),
        row_of(&lines, ".worktrees/child"),
        row_of(&lines, "HARNESS ISOLATION"),
        row_of(&lines, ".keeper/worktrees/own"),
        row_of(&lines, "YOURS"),
        row_of(&lines, "by-hand"),
        row_of(&lines, "UNREADABLE RECORD"),
        row_of(&lines, ".worktrees/newer"),
    ];
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "each checkout under its owner, the owners in one order:\n{}",
        lines.join("\n")
    );
    assert!(lines[row_of(&lines, ".worktrees/slot")].contains("reviewer"));
    assert!(lines[row_of(&lines, ".worktrees/child")].contains("of parser"));
    assert!(lines[row_of(&lines, ".keeper/worktrees/own")].contains("left to Keeper"));
    row_of(&lines, "5 checkouts · 10.0 KB on disk");
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
    press(&mut driven, Action::RemoveCheckout);

    let model = &driven.attach.model;
    let toast = model.remembered.toasts.back().expect("the refusal is said");
    assert_eq!(toast.kind, ToastKind::Failed);
    assert_eq!(toast.text, "not removed");
    assert_eq!(toast.detail, ".worktrees/draft: it holds uncommitted work");
    assert_eq!(
        model.checkouts.as_ref().unwrap().asking,
        None,
        "nothing to ask"
    );
    assert!(!model.remembered.checkout_change_pending);
}

#[test]
fn a_removal_is_asked_once_and_started_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-asked"));
    let model = showing(vec![checkout(".worktrees/done", CheckoutOwner::Operator)]);
    let mut driven = driven(model, &home);

    press(&mut driven, Action::RemoveCheckout);
    assert_eq!(
        driven.attach.model.checkouts.as_ref().unwrap().asking,
        Some(CheckoutQuestion::Remove),
        "the key asks rather than removes"
    );
    let lines = drawn(&driven.attach.model);
    row_of(&lines, "remove .worktrees/done? its branch is kept");

    press(&mut driven, Action::ConfirmCheckoutChange);
    press(&mut driven, Action::RemoveCheckout);
    press(&mut driven, Action::ConfirmCheckoutChange);

    assert!(driven.attach.model.remembered.checkout_change_pending);
    assert!(
        driven.attach.model.remembered.notice.is_some(),
        "and the operator is told something is running"
    );
}

#[test]
fn an_answer_to_an_earlier_read_is_dropped() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkouts-stale"));
    let mut model = WorkspaceModel {
        checkouts: Some(CheckoutsOverlay::over(PathBuf::from(PROJECT))),
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
fn a_clean_up_says_what_it_removed_what_it_kept_and_why() {
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
