//! A value a script nests deep never takes the process down (the audit's
//! SCRIPT-1): what a resident script keeps on `this` is held to the depth
//! a value may cross, and what one call nests is compared, printed and
//! dropped on a stack sized for it, whichever thread asked.

use std::sync::Arc;

use serde_json::json;
use uze_script::{
    CheckoutPath, Entry, Grant, Grants, Limit, Reach, ReachError, Resident, ResidentScript, Script,
    ScriptError,
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

fn resident(source: &str) -> Resident {
    let script = Arc::new(ResidentScript::compile(source.as_bytes()).unwrap());
    let grants = Grants::from([Grant::Resident]);
    let (resident, started) = Resident::start(script, &grants, Arc::new(NoCheckout), &json!({}));
    started.outcome.unwrap();
    resident
}

/// The audit's shape: every tick nests `this.a` eight levels deeper,
/// alternating arrays and maps so neither size bound is reached first.
const NESTING: &str = r#"
    fn start(context) { this.a = []; this.d = 0; true }
    fn event(event) {
        let k = 0;
        while k < 8 {
            if this.d % 2 == 0 { this.a = [this.a]; } else { this.a = #{ x: this.a }; }
            this.d += 1;
            k += 1;
        }
        true
    }
    fn render(size) { #{ type: "text", args: [`${this.d}`] } }
"#;

#[test]
fn a_resident_nesting_this_across_calls_is_stopped_at_the_crossing_depth() {
    let resident = resident(NESTING);
    let stopped = (0..4_000)
        .find_map(|call| {
            resident
                .event(&json!({"name": "timer"}))
                .outcome
                .err()
                .map(|error| (call, error))
        })
        .expect("the session stops");
    let (call, error) = stopped;
    assert_eq!(
        error,
        ScriptError::Limit {
            limit: Limit::Nesting,
            at: None
        }
    );
    assert!(call <= 17, "stopped once past 128 levels, at call {call}");
    assert_eq!(resident.stopped(), Some(error));
    assert!(
        resident.render(10, 1).outcome.is_err(),
        "a stopped session answers nothing"
    );
}

/// The same nesting through closures: each one captures the last, so a
/// function pointer holds what an array or a map would.
const CLOSURES: &str = r#"
    fn start(context) { this.k = 0; this.d = 0; true }
    fn event(event) {
        let x = this.k;
        let k = 0;
        while k < 8 {
            let y = x;
            x = || y;
            k += 1;
        }
        this.k = x;
        this.d += 8;
        true
    }
    fn render(size) { #{ type: "text", args: [`${this.d}`] } }
"#;

#[test]
fn a_resident_nesting_closures_in_this_is_stopped_at_the_crossing_depth() {
    let resident = resident(CLOSURES);
    let (call, error) = (0..4_000)
        .find_map(|call| {
            resident
                .event(&json!({"name": "timer"}))
                .outcome
                .err()
                .map(|error| (call, error))
        })
        .expect("the session stops");
    assert_eq!(
        error,
        ScriptError::Limit {
            limit: Limit::Nesting,
            at: None
        }
    );
    assert!(call <= 17, "stopped once past 128 levels, at call {call}");
}

/// Nests `a` 400 levels within one call, then compares and prints it: each
/// recursion Rhai makes, some 12 KiB a level in a debug build, past a
/// default 2 MiB thread stack.
const ONE_CALL: &str = r#"
    let a = [];
    let d = 0;
    while d < 400 {
        if d % 2 == 0 { a = [a]; } else { a = #{ x: a }; }
        d += 1;
    }
    let same = a == a;
    let printed = `${a}`;
"#;

#[test]
fn a_deep_value_built_in_one_answer_is_compared_and_dropped_on_the_script_stack() {
    let source = format!(
        "fn answer(input, context) {{ {ONE_CALL} #{{ data: [same, printed.len() > 0] }} }}"
    );
    let outcome = Script::compile(source.as_bytes())
        .unwrap()
        .answer(&json!({}), &json!({}), &Grants::new(), Arc::new(NoCheckout))
        .outcome;
    match outcome {
        Ok(envelope) => assert_eq!(envelope.data, json!([true, true])),
        Err(ScriptError::Limit {
            limit: Limit::Deadline | Limit::Operations,
            ..
        }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_deep_value_built_in_one_resident_call_is_compared_on_the_session_stack() {
    let source = format!(
        "fn start(context) {{ true }}
        fn event(event) {{ {ONE_CALL} same }}
        fn render(size) {{ #{{ type: \"text\", args: [\"x\"] }} }}"
    );
    let resident = resident(&source);
    match resident.event(&json!({"name": "timer"})).outcome {
        Ok(envelope) => assert_eq!(envelope.data, json!(true)),
        Err(ScriptError::Limit {
            limit: Limit::Deadline | Limit::Operations,
            ..
        }) => {}
        other => panic!("{other:?}"),
    }
}
