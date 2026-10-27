//! A resident script keeps what it holds between calls, inside the bounds
//! of each call, and asks for ticks no faster than the host's rate.

use std::sync::Arc;

use serde_json::{Value, json};
use uze_script::{
    CheckoutPath, Entry, Grant, Grants, Limit, Reach, ReachError, Resident, ResidentScript,
    ScriptError, TIMER_FLOOR,
};

struct NoCheckout;

impl Reach for NoCheckout {
    fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError("no checkout".to_owned()))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }
}

fn started(source: &str) -> (Resident, Result<uze_script::Envelope, ScriptError>) {
    let script = Arc::new(ResidentScript::compile(source.as_bytes()).unwrap());
    let grants = Grants::from([Grant::Resident]);
    let (resident, answered) = Resident::start(script, &grants, Arc::new(NoCheckout), &json!({}));
    (resident, answered.outcome)
}

fn data(outcome: Result<uze_script::Envelope, ScriptError>) -> Value {
    outcome.unwrap().data
}

const COUNTER: &str = r#"
    fn start(context) { this.count = 0; true }
    fn event(event) { this.count += 1; event.name == "timer" }
    fn input(input) { this.last = input.key; true }
    fn render(size) {
        #{ type: "text", args: [`${this.count} ${this.last ?? "-"} ${size.width}x${size.height}`] }
    }
"#;

#[test]
fn what_a_script_keeps_on_this_survives_between_calls() {
    let (resident, outcome) = started(COUNTER);
    assert_eq!(data(outcome), json!(true));
    assert_eq!(
        data(resident.event(&json!({"name": "timer"})).outcome),
        json!(true)
    );
    assert_eq!(
        data(resident.event(&json!({"name": "other"})).outcome),
        json!(false)
    );
    assert_eq!(
        data(resident.input(&json!({"key": "up", "mods": []})).outcome),
        json!(true)
    );
    assert_eq!(
        data(resident.render(40, 12).outcome),
        json!({"type": "text", "args": ["2 up 40x12"]})
    );
    assert!(resident.stop().outcome.is_ok());
    assert!(
        resident.render(40, 12).outcome.is_err(),
        "a stopped session answers nothing"
    );
}

#[test]
fn a_runaway_render_is_stopped_at_its_bound_and_the_session_with_it() {
    let (resident, _) = started("fn render(size) { loop {} }");
    match resident.render(10, 10).outcome {
        Err(ScriptError::Limit { limit, .. }) => {
            assert!(
                matches!(limit, Limit::Operations | Limit::Deadline),
                "{limit:?}"
            );
        }
        other => panic!("expected a bound, got {other:?}"),
    }
    assert!(matches!(
        resident.stopped(),
        Some(ScriptError::Limit { .. })
    ));
    assert!(
        matches!(
            resident.render(10, 10).outcome,
            Err(ScriptError::Limit { .. })
        ),
        "every later call says the bound it stopped at"
    );
}

#[test]
fn each_call_has_its_own_budget() {
    // About a sixth of the operation bound per call: a session counting
    // across calls would stop before the last.
    let (resident, _) = started(
        "fn event(event) { let i = 0; while i < 40000 { i += 1; } true }
         fn render(size) { #{ type: \"text\", args: [\"x\"] } }",
    );
    for _ in 0..12 {
        assert!(resident.event(&json!({"name": "timer"})).outcome.is_ok());
    }
}

#[test]
fn a_timer_asked_sooner_than_the_rate_is_held_to_it() {
    let (resident, outcome) = started(
        "fn start(context) { timer(1); true }
         fn event(event) { timer(250); false }
         fn render(size) { #{ type: \"text\", args: [\"x\"] } }",
    );
    let floor = u64::try_from(TIMER_FLOOR.as_millis()).unwrap();
    assert!(floor * 30 >= 1000, "no more than 30 ticks a second");
    assert_eq!(outcome.unwrap().timer, Some(floor));
    assert_eq!(
        resident
            .event(&json!({"name": "timer"}))
            .outcome
            .unwrap()
            .timer,
        Some(250)
    );
    assert_eq!(
        resident.render(1, 1).outcome.unwrap().timer,
        None,
        "asked again, or not at all"
    );
}

#[test]
fn an_answer_beside_the_redraw_carries_its_envelope() {
    let (resident, _) = started(
        "fn input(input) { #{ redraw: true, toast: \"saved\", store: #{ best: 3 } } }
         fn render(size) { #{ type: \"text\", args: [\"x\"] } }",
    );
    let envelope = resident.input(&json!({"key": "s"})).outcome.unwrap();
    assert_eq!(envelope.data, json!(true));
    assert_eq!(envelope.toast.as_deref(), Some("saved"));
    assert_eq!(envelope.store.unwrap().get("best"), Some(&json!(3)));
}

#[test]
fn a_resident_script_defines_render() {
    assert!(ResidentScript::compile(b"fn start(context) { true }").is_err());
    assert!(ResidentScript::compile(b"fn render(size) { () }").is_ok());
}

#[test]
fn timer_is_a_function_only_the_resident_grant_names() {
    let refused = uze_script::ungranted_calls("fn render(size) { timer(5); }", &Grants::new());
    assert_eq!(refused, [("timer".to_owned(), Grant::Resident)]);
}
