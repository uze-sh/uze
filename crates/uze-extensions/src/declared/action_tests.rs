//! Declared actions from fixture manifests and canned answers: what a
//! gesture runs, what it asks first, and what an answer's envelope does.

use std::path::PathBuf;

use serde_json::{Value, json};
use uze_contract::{ActionDecl, Does, Envelope, NodeName, SourceDecl, SourceKind};

use super::source_tests::{Answering, answered, checkout};
use super::tests::{SECTION, SOURCE, SPACE, VIEW, act, compiled_view, extension, first, n, tree};
use super::*;
use crate::view::NodeKind;

fn action(id: &str, does: Does) -> ActionDecl {
    ActionDecl {
        id: id.to_owned(),
        label: format!("{}{}", id[..1].to_uppercase(), &id[1..]),
        on: VIEW.to_owned(),
        key: None,
        confirm: None,
        does,
        with: Vec::new(),
        then: None,
    }
}

/// A board of rows, each with a button running `action` on its row, the
/// extension declaring `actions` and a second source, `log`.
fn board(actions: Vec<ActionDecl>) -> DeclaredExtension {
    let view = compiled_view(
        vec![
            n(NodeName::Stack)
                .each("rows")
                .child(n(NodeName::Text).arg("{name}"))
                .child(
                    n(NodeName::Button)
                        .arg("Go")
                        .literal("action", json!("go"))
                        .prop("item", "{.}"),
                )
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.actions = actions;
    compiled.manifest.sources.push(SourceDecl::new(
        "log",
        SourceKind::Script("log.uze".to_owned()),
    ));
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.open(VIEW, &checkout("/work/a"));
    let rows = json!({ "rows": [
        { "name": "auth", "path": "src/auth.rs", "log": "one\ntwo" },
        { "name": "../up", "path": "../secrets", "log": "x" },
    ] });
    ask_all(&mut engine, answered(rows));
    engine
}

/// Presses the `row`th button of the view.
fn press(engine: &mut DeclaredExtension, row: usize) -> Outcome {
    let root = tree(engine);
    let buttons: Vec<usize> = super::tests::nodes(&root)
        .into_iter()
        .filter(|node| matches!(node.kind, NodeKind::Button { .. }))
        .map(|node| node.id)
        .collect();
    engine.hit(VIEW, act(buttons[row], NodeAct::Press), SPACE)
}

/// Runs what the engine asks now and hands each answer back, answering
/// with `answer`; the outcomes, in order.
fn ask_all(engine: &mut DeclaredExtension, answer: Answer) -> (Vec<Question>, Vec<Outcome>) {
    let host = Answering::with(answer);
    let outcomes = engine
        .jobs()
        .into_iter()
        .map(|job| {
            let answer = (job.run)(&host);
            engine.absorb(answer)
        })
        .collect();
    (host.asked.into_inner(), outcomes)
}

#[test]
fn a_refresh_action_asks_its_sources_again_past_the_remembered_answer() {
    let mut engine = board(vec![action("go", Does::Refresh(vec![SOURCE.to_owned()]))]);
    assert!(engine.jobs().is_empty(), "asked once");

    assert_eq!(press(&mut engine, 0), Outcome::Stay);
    let (asked, _) = ask_all(&mut engine, answered(json!({ "rows": [] })));
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].declaration, SOURCE);
    assert_eq!(asked[0].asking, Asking::Refresh);
    assert!(engine.jobs().is_empty(), "refreshed once");
    assert!(engine.recorded()[0].declared);
}

#[test]
fn an_answer_refreshes_another_source_and_never_itself_or_back() {
    let mut engine = board(Vec::new());
    engine.open(VIEW, &checkout("/work/a"));
    let envelope = Envelope {
        data: json!({ "rows": [] }),
        refresh: vec![SOURCE.to_owned(), "log".to_owned()],
        ..Envelope::default()
    };
    let answer = Answer::Answered {
        envelope,
        trace: Vec::new(),
    };
    ask_all(&mut engine, answer.clone());
    assert!(
        engine.jobs().is_empty(),
        "an answer does not ask itself again, and `log` is shown nowhere"
    );
    // Once `log` is shown, it is asked past the remembered answer; what it
    // answers asks nothing again, so the two cannot keep asking each other.
    engine.compiled.manifest.surfaces[1].source = "log".to_owned();
    engine.focus(Some(&checkout("/work/a")));
    let (asked, _) = ask_all(&mut engine, answer);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].declaration, "log");
    assert_eq!(asked[0].asking, Asking::Refresh);
    assert!(engine.jobs().is_empty(), "{:?}", engine.asked);
}

#[test]
fn a_confirmed_action_asks_with_its_arguments_attributed_and_runs_only_once_accepted() {
    let mut go = action(
        "go",
        Does::Call {
            function: "archive".to_owned(),
            file: Some("archive.uze".to_owned()),
        },
    );
    go.label = "Archive".to_owned();
    go.confirm = Some("Archive {item.name}?".to_owned());
    let mut engine = board(vec![go]);

    assert_eq!(press(&mut engine, 0), Outcome::Stay);
    let asked = engine
        .view(VIEW, SPACE)
        .expect("a view")
        .confirm
        .expect("a question");
    assert_eq!(asked.title, "Archive auth?");
    assert_eq!(asked.subject, "gsd-band · gsd@luizfbalves");
    assert!(
        asked.body.starts_with("Runs archive in archive.uze on "),
        "{asked:?}"
    );
    assert!(asked.body.contains("\"auth\""), "{asked:?}");
    assert_eq!(asked.confirm, "Archive");
    assert!(!asked.on_confirm, "the keyboard starts on the way out");
    assert!(engine.queued.is_empty(), "nothing runs until accepted");
    assert_eq!(engine.press(VIEW, "go"), Outcome::Stay, "nor while asking");

    assert_eq!(
        engine.command(VIEW, Command::Close, SPACE),
        Outcome::Stay,
        "esc answers no, and the view stays"
    );
    assert!(engine.asking(VIEW).is_none());
    assert!(engine.jobs().is_empty());

    press(&mut engine, 0);
    engine.command(VIEW, Command::FocusNext, SPACE);
    assert!(engine.asking(VIEW).expect("asking").on_confirm);
    let outcome = engine.command(VIEW, Command::Activate, SPACE);
    assert!(matches!(&outcome, Outcome::Notice(span) if span.text == "Archive…"));
    let (asked, _) = ask_all(&mut engine, answered(Value::Null));
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].asking, Asking::Action);
    assert_eq!(asked[0].declaration, "go");
    assert_eq!(asked[0].checkout, Some(PathBuf::from("/work/a")));
    assert_eq!(asked[0].input["item"]["name"], "auth");

    press(&mut engine, 0);
    assert_eq!(
        engine.hit(VIEW, ViewHit::Answer(false), SPACE),
        Outcome::Stay
    );
    assert!(engine.jobs().is_empty(), "a click on cancel runs nothing");
}

#[test]
fn a_confirmed_action_pressed_in_a_section_asks_in_its_view() {
    let mut go = action("go", Does::Refresh(vec![SOURCE.to_owned()]));
    go.confirm = Some("Again?".to_owned());
    let mut engine = board(vec![go]);
    engine.close(VIEW);
    let outcome = engine.act_on(SECTION, "go", Value::Null);
    assert_eq!(outcome, Outcome::Show(VIEW.to_owned()));
    assert_eq!(
        engine.asking(VIEW).expect("asked in the view").title,
        "Again?"
    );
    assert!(engine.asking(SECTION).is_none());
}

#[test]
fn an_action_that_fails_or_may_not_run_says_why_in_a_toast() {
    let mut engine = board(vec![action(
        "go",
        Does::Call {
            function: "go".to_owned(),
            file: Some("go.uze".to_owned()),
        },
    )]);
    press(&mut engine, 0);
    let (_, outcomes) = ask_all(
        &mut engine,
        Answer::Failed {
            reason: "go.uze: the script crossed its deadline bound".to_owned(),
            trace: Vec::new(),
        },
    );
    assert_eq!(
        outcomes,
        [Outcome::Failed {
            title: "Go failed".to_owned(),
            detail: "go.uze: the script crossed its deadline bound".to_owned(),
        }]
    );
    press(&mut engine, 0);
    let (_, outcomes) = ask_all(
        &mut engine,
        Answer::withheld("needs enabling in this project"),
    );
    assert_eq!(
        outcomes,
        [Outcome::Failed {
            title: "Go did not run".to_owned(),
            detail: "needs enabling in this project".to_owned(),
        }]
    );
}

#[test]
fn an_actions_envelope_toasts_says_its_notice_and_refreshes() {
    let mut engine = board(vec![action(
        "go",
        Does::Call {
            function: "go".to_owned(),
            file: Some("go.uze".to_owned()),
        },
    )]);
    press(&mut engine, 0);
    let envelope = Envelope {
        data: Value::Null,
        refresh: vec![SOURCE.to_owned()],
        toast: Some("Archived \u{1b}[31mauth".to_owned()),
        notice: Some("1 archived".to_owned()),
        store: None,
        notify: None,
        sound: None,
        timer: None,
    };
    let (_, outcomes) = ask_all(
        &mut engine,
        Answer::Answered {
            envelope,
            trace: Vec::new(),
        },
    );
    assert_eq!(outcomes, [Outcome::Toast("Archived auth".to_owned())]);
    let view = engine.view(VIEW, SPACE).expect("a view");
    assert_eq!(view.notice.expect("said").text, "1 archived");
    let (asked, _) = ask_all(&mut engine, answered(json!({ "rows": [] })));
    assert_eq!(asked[0].asking, Asking::Refresh, "{asked:?}");
}

#[test]
fn a_short_copy_is_copied_and_a_long_one_asks_first() {
    let mut engine = board(vec![action("go", Does::Clipboard("{path}".to_owned()))]);
    assert_eq!(
        press(&mut engine, 0),
        Outcome::Copy("src/auth.rs".to_owned())
    );

    let mut engine = board(vec![action("go", Does::Clipboard("{log}".to_owned()))]);
    assert_eq!(press(&mut engine, 0), Outcome::Stay);
    let asked = engine.asking(VIEW).expect("asks first").clone();
    assert_eq!(asked.title, "Copy 2 lines?");
    assert_eq!(asked.body, "one\ntwo");
    assert_eq!(
        engine.hit(VIEW, ViewHit::Answer(true), SPACE),
        Outcome::Copy("one\ntwo".to_owned())
    );

    let long = "x".repeat(COPY_UNASKED_MAX + 1);
    let mut engine = board(vec![action("go", Does::Clipboard(long.clone()))]);
    press(&mut engine, 0);
    assert_eq!(
        engine.asking(VIEW).expect("asks first").title,
        format!("Copy {} bytes?", long.len())
    );
}

#[test]
fn a_path_is_opened_only_inside_the_checkout() {
    let mut engine = board(vec![action("go", Does::OpenPath("{path}".to_owned()))]);
    assert_eq!(
        press(&mut engine, 0),
        Outcome::OpenPath {
            project: PathBuf::from("/work/a"),
            target: PathBuf::from("/work/a/src/auth.rs"),
        }
    );
    assert_eq!(
        press(&mut engine, 1),
        Outcome::Failed {
            title: "Go did not run".to_owned(),
            detail: "`../secrets` is not a path inside the checkout".to_owned(),
        }
    );
}

#[test]
fn an_open_action_shows_its_own_surface() {
    let mut engine = board(vec![action("go", Does::Open(VIEW.to_owned()))]);
    assert_eq!(press(&mut engine, 0), Outcome::Show(VIEW.to_owned()));
}

#[test]
fn a_key_runs_the_action_on_the_selected_row() {
    let view = compiled_view(
        vec![
            n(NodeName::List)
                .literal("id", json!("rows"))
                .each("rows")
                .child(n(NodeName::Text).arg("{name}"))
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.actions = vec![action("go", Does::Clipboard("{item.name}".to_owned()))];
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.set_data(
        SOURCE,
        Ok(json!({ "rows": [{ "name": "auth" }, { "name": "billing" }] })),
    );
    engine.open(VIEW, &checkout("/work/a"));
    let list = first(&tree(&engine), |kind| matches!(kind, NodeKind::List { .. }));
    engine.hit(VIEW, act(list.id, NodeAct::Row(1)), SPACE);
    assert_eq!(
        engine.press(VIEW, "go"),
        Outcome::Copy("billing".to_owned())
    );
}

/// A list of two rows, nothing selected, offering `actions` on the view.
fn list_without_a_selection(actions: Vec<ActionDecl>) -> DeclaredExtension {
    let view = compiled_view(
        vec![
            n(NodeName::List)
                .literal("id", json!("rows"))
                .each("rows")
                .child(n(NodeName::Text).arg("{name}"))
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.actions = actions;
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.set_data(
        SOURCE,
        Ok(json!({ "rows": [{ "name": "auth", "path": "src/auth.rs" }] })),
    );
    engine.open(VIEW, &checkout("/work/a"));
    tree(&engine);
    engine
}

#[test]
fn a_key_on_no_row_runs_nothing_that_reads_the_row() {
    let select_first = Outcome::Failed {
        title: "Go did not run".to_owned(),
        detail: "select a row first".to_owned(),
    };
    for does in [
        Does::Clipboard("{item.name}".to_owned()),
        Does::Clipboard("{name}".to_owned()),
        Does::OpenPath("{item.path}".to_owned()),
    ] {
        let mut engine = list_without_a_selection(vec![action("go", does.clone())]);
        assert_eq!(engine.press(VIEW, "go"), select_first, "{does:?}");
    }

    let mut archive = action(
        "go",
        Does::Call {
            function: "go".to_owned(),
            file: Some("go.uze".to_owned()),
        },
    );
    archive.confirm = Some("Archive {item.name}?".to_owned());
    let mut engine = list_without_a_selection(vec![archive]);
    assert_eq!(engine.press(VIEW, "go"), select_first);
    assert!(engine.asking(VIEW).is_none(), "nothing is asked");
    let (asked, _) = ask_all(&mut engine, answered(json!({ "rows": [] })));
    assert!(
        asked
            .iter()
            .all(|question| question.asking != Asking::Action),
        "nothing runs: {asked:?}"
    );

    let mut engine = list_without_a_selection(vec![action(
        "go",
        Does::Call {
            function: "go".to_owned(),
            file: Some("go.uze".to_owned()),
        },
    )]);
    assert!(
        matches!(engine.press(VIEW, "go"), Outcome::Notice(_)),
        "a script that names no row runs on none"
    );
}

#[test]
fn a_copy_of_nothing_never_reaches_the_clipboard() {
    let mut engine = list_without_a_selection(vec![action(
        "go",
        Does::Clipboard("{ui.missing}".to_owned()),
    )]);
    assert_eq!(
        engine.press(VIEW, "go"),
        Outcome::Failed {
            title: "Go did not run".to_owned(),
            detail: "there is nothing to copy".to_owned(),
        }
    );
}

#[test]
fn a_sources_input_resolves_its_references_and_a_change_asks_again() {
    let view = compiled_view(
        vec![
            n(NodeName::Tabs)
                .literal("id", json!("tab"))
                .child(n(NodeName::Tab).arg("Open").literal("value", json!("open")))
                .child(n(NodeName::Tab).arg("Done").literal("value", json!("done")))
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.sources[0].input = [
        ("tab".to_owned(), "{ui.tab}".to_owned()),
        ("where".to_owned(), "in {context.checkout}".to_owned()),
    ]
    .into_iter()
    .collect();
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.open(VIEW, &checkout("/work/a"));
    let (asked, _) = ask_all(&mut engine, answered(json!({})));
    assert_eq!(
        asked[0].input,
        json!({ "tab": "open", "where": "in /work/a" })
    );

    let tabs = first(&tree(&engine), |kind| matches!(kind, NodeKind::Tabs { .. }));
    engine.hit(VIEW, act(tabs.id, NodeAct::Tab(1)), SPACE);
    let (asked, _) = ask_all(&mut engine, answered(json!({})));
    assert_eq!(asked.len(), 1, "a new input is a new question");
    assert_eq!(asked[0].input["tab"], "done");
}

#[test]
fn an_action_on_rows_answered_for_the_previous_checkout_is_refused() {
    let view = compiled_view(
        vec![
            n(NodeName::List)
                .literal("id", json!("rows"))
                .each("rows")
                .child(n(NodeName::Text).arg("{name}"))
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    let mut go = action("go", Does::Clipboard("{item.name}".to_owned()));
    go.on = SECTION.to_owned();
    compiled.manifest.actions = vec![go];
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.focus(Some(&checkout("/work/a")));
    ask_all(
        &mut engine,
        answered(json!({ "rows": [{ "name": "auth" }] })),
    );
    let section = engine.section(SECTION, false).expect("a section");
    let list = first(&section.tree.expect("a tree"), |kind| {
        matches!(kind, NodeKind::List { .. })
    });
    engine.hit(SECTION, act(list.id, NodeAct::Row(0)), SPACE);
    assert_eq!(
        engine.press(SECTION, "go"),
        Outcome::Copy("auth".to_owned())
    );

    engine.focus(Some(&checkout("/work/b")));
    assert_eq!(
        engine.press(SECTION, "go"),
        Outcome::Failed {
            title: "Go did not run".to_owned(),
            detail: "the rows are still the previous checkout's".to_owned(),
        },
        "the row drawn is /work/a's, the action would run on /work/b"
    );
    ask_all(
        &mut engine,
        answered(json!({ "rows": [{ "name": "billing" }] })),
    );
    engine.section(SECTION, false);
    assert_eq!(
        engine.press(SECTION, "go"),
        Outcome::Copy("billing".to_owned())
    );
}

#[test]
fn the_actions_recorded_are_the_newest_few() {
    let mut engine = board(vec![action("go", Does::Refresh(vec![SOURCE.to_owned()]))]);
    for _ in 0..RECORDED_MAX + 10 {
        press(&mut engine, 0);
    }
    press(&mut engine, 1);
    let recorded = engine.recorded();
    assert_eq!(recorded.len(), RECORDED_MAX);
    assert_eq!(
        recorded.last().map(|last| &last.item["name"]),
        Some(&json!("../up"))
    );
}
