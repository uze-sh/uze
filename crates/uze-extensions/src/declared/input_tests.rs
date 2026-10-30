//! What a view takes from the operator: a select stepped, a field typed
//! into and filtering a list, a form sent to an action with the values its
//! `with` reads, and a two-step action whose answer fills a prompt.

use serde_json::{Value, json};
use uze_contract::{ActionDecl, Does, NodeName, Program, PromptText, Then};

use super::source_tests::{Answering, answered, checkout};
use super::tests::{SPACE, VIEW, act, compiled_view, extension, first, n, nodes, texts, tree};
use super::*;
use crate::contract::{AgentAsk, AgentRequest};
use crate::view::{self, NodeKind};

/// A view with a select, a field filtering a list, and a form sending a
/// comment and a tone to `comment`, its actions `actions`.
fn inputs(actions: Vec<ActionDecl>) -> DeclaredExtension {
    let view = compiled_view(
        vec![
            n(NodeName::Select)
                .arg("Sort")
                .literal("id", json!("sort"))
                .child(
                    n(NodeName::Choice)
                        .arg("Newest")
                        .literal("value", json!("new")),
                )
                .child(
                    n(NodeName::Choice)
                        .arg("Oldest")
                        .literal("value", json!("old")),
                )
                .item(),
            n(NodeName::Text)
                .arg("oldest first")
                .show("sort", "old")
                .item(),
            n(NodeName::Field)
                .arg("Filter")
                .literal("id", json!("query"))
                .prop("placeholder", "name")
                .item(),
            n(NodeName::List)
                .each("rows")
                .literal("filter", json!("ui.query"))
                .child(n(NodeName::Text).arg("{name}"))
                .item(),
            n(NodeName::Form)
                .literal("action", json!("comment"))
                .prop("submit", "Comment")
                .child(
                    n(NodeName::Field)
                        .arg("Comment")
                        .literal("id", json!("comment")),
                )
                .child(
                    n(NodeName::Select)
                        .literal("id", json!("tone"))
                        .child(n(NodeName::Choice).arg("kind"))
                        .child(n(NodeName::Choice).arg("blunt")),
                )
                .item(),
        ],
        Vec::new(),
        Vec::new(),
    );
    let mut compiled = extension(view);
    compiled.manifest.actions = actions;
    let mut engine = DeclaredExtension::new(compiled, "en");
    engine.open(VIEW, &checkout("/work/a"));
    let rows = json!({ "rows": [
        { "name": "auth", "path": "src/auth.rs" },
        { "name": "billing", "path": "src/billing.rs" },
    ] });
    let host = Answering::with(answered(rows));
    for job in engine.jobs() {
        let answer = (job.run)(&host);
        engine.absorb(answer);
    }
    engine
}

fn script(id: &str, with: &[&str]) -> ActionDecl {
    let mut action = ActionDecl::new(
        id,
        "Comment",
        VIEW,
        Does::Call {
            function: "comment".into(),
            file: Some("comment.uze".into()),
        },
    );
    action.with = with.iter().map(|read| (*read).to_owned()).collect();
    action
}

fn node_of(engine: &DeclaredExtension, kind: fn(&NodeKind) -> bool, nth: usize) -> view::Node {
    nodes(&tree(engine))
        .into_iter()
        .filter(|node| kind(&node.kind))
        .nth(nth)
        .cloned()
        .expect("the tree holds that node")
}

fn is_select(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Select { .. })
}

fn is_field(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Field { .. })
}

fn is_form(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Form { .. })
}

fn type_text(engine: &mut DeclaredExtension, text: &str) {
    for character in text.chars() {
        engine.command(VIEW, Command::Type(character), SPACE);
    }
}

/// What the engine asks the host now, each answered with `answer`; the
/// questions and what each answer came to.
fn run_jobs(engine: &mut DeclaredExtension, answer: Answer) -> (Vec<Question>, Vec<Outcome>) {
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
fn a_select_starts_on_its_first_choice_and_steps_by_click_or_key() {
    let mut engine = inputs(Vec::new());
    let select = node_of(&engine, is_select, 0);
    assert_eq!(
        select.kind,
        NodeKind::Select {
            label: "Sort".into(),
            options: vec!["Newest".into(), "Oldest".into()],
            chosen: 0,
            focused: false,
        }
    );
    assert!(!texts(&tree(&engine)).contains(&"oldest first".to_owned()));

    // A click on `›` steps on, and focuses it.
    engine.hit(VIEW, act(select.id, NodeAct::Step { forward: true }), SPACE);
    assert_eq!(engine.ui(VIEW).unwrap()["sort"], "old");
    assert!(texts(&tree(&engine)).contains(&"oldest first".to_owned()));
    assert!(matches!(
        node_of(&engine, is_select, 0).kind,
        NodeKind::Select {
            focused: true,
            chosen: 1,
            ..
        }
    ));

    // Enter on it steps on as the click does, around to the first.
    engine.command(VIEW, Command::Activate, SPACE);
    assert_eq!(engine.ui(VIEW).unwrap()["sort"], "new");
    // `‹` steps back, around to the last.
    engine.hit(
        VIEW,
        act(select.id, NodeAct::Step { forward: false }),
        SPACE,
    );
    assert_eq!(engine.ui(VIEW).unwrap()["sort"], "old");
}

/// A press on one of a select's options chooses it outright, and focuses
/// the select; an index it does not have chooses nothing.
#[test]
fn a_select_option_is_chosen_outright() {
    let mut engine = inputs(Vec::new());
    let select = node_of(&engine, is_select, 0);
    engine.hit(VIEW, act(select.id, NodeAct::Choose(1)), SPACE);
    assert_eq!(engine.ui(VIEW).unwrap()["sort"], "old");
    assert!(matches!(
        node_of(&engine, is_select, 0).kind,
        NodeKind::Select {
            focused: true,
            chosen: 1,
            ..
        }
    ));
    engine.hit(VIEW, act(select.id, NodeAct::Choose(9)), SPACE);
    assert_eq!(engine.ui(VIEW).unwrap()["sort"], "old");
}

#[test]
fn focus_moves_through_the_inputs_in_the_order_drawn() {
    let mut engine = inputs(Vec::new());
    let focused = |engine: &DeclaredExtension| {
        nodes(&tree(engine))
            .into_iter()
            .position(|node| {
                matches!(
                    node.kind,
                    NodeKind::Select { focused: true, .. }
                        | NodeKind::Field { focused: true, .. }
                        | NodeKind::Form { focused: true, .. }
                )
            })
            .map(|at| nodes(&tree(engine))[at].kind.clone())
    };
    assert_eq!(focused(&engine), None);
    engine.command(VIEW, Command::FocusNext, SPACE);
    assert!(matches!(focused(&engine), Some(NodeKind::Select { label, .. }) if label == "Sort"));
    engine.command(VIEW, Command::FocusNext, SPACE);
    assert!(matches!(focused(&engine), Some(NodeKind::Field { label, .. }) if label == "Filter"));
    // Enter on a field takes it to type into.
    engine.command(VIEW, Command::Activate, SPACE);
    assert!(engine.typing(VIEW));
}

#[test]
fn a_field_takes_text_while_typed_into_and_filters_its_list() {
    let mut engine = inputs(Vec::new());
    assert!(!engine.typing(VIEW));
    let field = node_of(&engine, is_field, 0);
    engine.hit(VIEW, act(field.id, NodeAct::Edit), SPACE);
    assert!(engine.typing(VIEW), "a press on a field types into it");

    type_text(&mut engine, "BIL");
    assert_eq!(
        node_of(&engine, is_field, 0).kind,
        NodeKind::Field {
            label: "Filter".into(),
            value: "BIL".into(),
            placeholder: "name".into(),
            focused: true,
            editing: true,
        }
    );
    let shown = texts(&tree(&engine));
    assert!(shown.contains(&"billing".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"auth".to_owned()), "{shown:?}");

    engine.command(VIEW, Command::EraseBack, SPACE);
    engine.command(VIEW, Command::EraseBack, SPACE);
    engine.command(VIEW, Command::EraseBack, SPACE);
    assert!(texts(&tree(&engine)).contains(&"auth".to_owned()));

    // Esc stops typing and leaves the view open; the next Esc closes it.
    assert_eq!(engine.command(VIEW, Command::Close, SPACE), Outcome::Stay);
    assert!(!engine.typing(VIEW));
    assert_eq!(engine.command(VIEW, Command::Close, SPACE), Outcome::Close);
}

#[test]
fn what_is_typed_is_one_line_bounded_and_without_control_characters() {
    let mut engine = inputs(Vec::new());
    let field = node_of(&engine, is_field, 0);
    engine.hit(VIEW, act(field.id, NodeAct::Edit), SPACE);
    engine.paste(VIEW, "au\u{1b}[31mth\nsecond line", SPACE);
    assert_eq!(engine.ui(VIEW).unwrap()["query"], "au[31mth");
    engine.paste(VIEW, &"x".repeat(FIELD_MAX * 2), SPACE);
    assert_eq!(engine.ui(VIEW).unwrap()["query"].chars().count(), FIELD_MAX);
}

#[test]
fn a_press_elsewhere_ends_the_typing() {
    let mut engine = inputs(Vec::new());
    let field = node_of(&engine, is_field, 0);
    engine.hit(VIEW, act(field.id, NodeAct::Edit), SPACE);
    let select = node_of(&engine, is_select, 0);
    engine.hit(VIEW, act(select.id, NodeAct::Step { forward: true }), SPACE);
    assert!(!engine.typing(VIEW));
}

#[test]
fn a_form_sends_the_fields_its_action_reads_cleaned() {
    let mut engine = inputs(vec![script("comment", &["form.*", "ui.sort"])]);
    let comment = node_of(&engine, is_field, 1);
    engine.hit(VIEW, act(comment.id, NodeAct::Edit), SPACE);
    type_text(&mut engine, "looks good");
    // Enter in a form's field sends the form.
    let sent = engine.command(VIEW, Command::Newline, SPACE);
    assert!(matches!(sent, Outcome::Notice(_)), "{sent:?}");
    assert!(!engine.typing(VIEW));
    let (asked, _) = run_jobs(&mut engine, answered(Value::Null));
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].asking, Asking::Action);
    assert_eq!(
        asked[0].input,
        json!({
            "item": null,
            "form": { "comment": "looks good", "tone": "kind" },
            "ui": { "sort": "new" },
        })
    );
}

#[test]
fn a_form_sends_only_what_its_action_names() {
    let mut engine = inputs(vec![script("comment", &["form.tone"])]);
    let form = node_of(&engine, is_form, 0);
    engine.hit(VIEW, act(form.id, NodeAct::Press), SPACE);
    let (asked, _) = run_jobs(&mut engine, answered(Value::Null));
    assert_eq!(
        asked[0].input,
        json!({ "item": null, "form": { "tone": "kind" } })
    );

    // An action that reads none of the form is not run, and says why.
    let mut engine = inputs(vec![script("comment", &[])]);
    let form = node_of(&engine, is_form, 0);
    let said = engine.hit(VIEW, act(form.id, NodeAct::Press), SPACE);
    assert!(
        matches!(&said, Outcome::Notice(span) if span.text.contains("reads none of the form")),
        "{said:?}"
    );
    assert!(run_jobs(&mut engine, answered(Value::Null)).0.is_empty());
}

#[test]
fn a_form_is_drawn_with_its_button() {
    let engine = inputs(Vec::new());
    let form = first(&tree(&engine), is_form);
    assert_eq!(
        form.kind,
        NodeKind::Form {
            submit: "comment".into(),
            focused: false,
        }
    );
    assert_eq!(form.children.len(), 2);
}

/// A `run` whose answer fills a prompt for the agent in front.
fn two_step(prompt: &str) -> DeclaredExtension {
    let mut action = ActionDecl::new(
        "review",
        "Review next",
        VIEW,
        Does::Run(Program::new(vec!["./bin/next.sh".into()])),
    );
    action.then = Some(Then {
        prompt: PromptText::Text(prompt.into()),
        target: uze_contract::AgentTarget::Own,
    });
    inputs(vec![action])
}

#[test]
fn a_two_step_action_fills_its_prompt_from_the_answer_on_the_gesture() {
    let mut engine = two_step("Review {answer.file}");
    assert!(engine.unattended.is_empty());
    let ran = engine.press(VIEW, "review");
    assert!(matches!(ran, Outcome::Notice(_)), "{ran:?}");
    let answer = answered(json!({ "file": "src/auth.rs\u{1b}[2J" }));
    let (asked, outcomes) = run_jobs(&mut engine, answer);
    assert_eq!(asked[0].declaration, "review");
    assert_eq!(
        outcomes,
        vec![Outcome::Agent(Box::new(AgentRequest {
            what: AgentAsk::Prompt {
                text: "Review src/auth.rs".into()
            },
            agent: None,
            action: "review".into(),
            label: "Review next".into(),
            unattended: false,
        }))]
    );
    assert!(
        engine.requests().is_empty(),
        "nothing goes out on its own: the prompt stands on the gesture"
    );
}

#[test]
fn an_answer_that_leaves_the_prompt_empty_sends_nothing() {
    let mut engine = two_step("{answer.file}");
    engine.press(VIEW, "review");
    let (_, outcomes) = run_jobs(&mut engine, answered(json!({})));
    assert!(
        matches!(&outcomes[0], Outcome::Failed { detail, .. } if detail.contains("nothing to say")),
        "{outcomes:?}"
    );
}

#[test]
fn a_failed_first_step_sends_no_prompt() {
    let mut engine = two_step("Review {answer.file}");
    engine.press(VIEW, "review");
    let failed = Answer::Failed {
        reason: "exit 1".into(),
        trace: Vec::new(),
    };
    let (_, outcomes) = run_jobs(&mut engine, failed);
    assert!(
        matches!(outcomes[0], Outcome::Failed { .. }),
        "{outcomes:?}"
    );
}
