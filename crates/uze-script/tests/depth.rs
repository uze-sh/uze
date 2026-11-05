//! A value a script nests deep never takes the process down (the audit's
//! SCRIPT-1): what a resident script keeps on `this` is held to the depth
//! a value may cross, and what one call nests is compared, printed and
//! dropped on a stack sized for it, whichever thread asked.

use std::sync::Arc;

use serde_json::{Value, json};
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

/// Rhai's recursive closure captures the variable that holds it: a cycle
/// two levels deep, which is not depth.
#[test]
fn a_recursive_closure_kept_on_this_is_not_judged_deep() {
    let resident = resident(
        r#"
        fn start(context) {
            let fact = 0;
            fact = |n| if n <= 1 { 1 } else { n * fact.call(n - 1) };
            this.f = fact;
            true
        }
        fn event(event) { type_of(this.f) == "Fn" }
        fn render(size) { #{ type: "text", args: ["x"] } }
    "#,
    );
    let envelope = resident
        .event(&json!({"name": "timer"}))
        .outcome
        .expect("a recursive closure is an ordinary value");
    assert_eq!(envelope.data, json!(true));
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

/// How deep a value may cross: 128 containers around what they hold.
const CROSSING_DEPTH: usize = 128;

/// One way of nesting: in JSON, and as the Rhai that wraps `v` once more.
struct Shape {
    wrap: fn(Value) -> Value,
    wraps: &'static str,
}

const SHAPES: [Shape; 2] = [
    Shape {
        wrap: |value| json!([value]),
        wraps: "[v]",
    },
    Shape {
        wrap: |value| json!({ "x": value }),
        wraps: "#{ x: v }",
    },
];

impl Shape {
    fn nested(&self, levels: usize) -> Value {
        (0..levels).fold(json!(0), |value, _| (self.wrap)(value))
    }

    /// Rhai that leaves `v` nested `levels` deep.
    fn building(&self, levels: usize) -> String {
        format!(
            "let v = 0; let n = 0; while n < {levels} {{ v = {wraps}; n += 1; }}",
            wraps = self.wraps
        )
    }
}

fn answered(source: &str, input: &Value) -> Result<Value, ScriptError> {
    Script::compile(source.as_bytes())?
        .answer(input, &json!({}), &Grants::new(), Arc::new(NoCheckout))
        .outcome
        .map(|envelope| envelope.data)
}

#[test]
fn a_value_as_deep_as_the_bound_crosses_in_and_out() {
    for shape in &SHAPES {
        let value = shape.nested(CROSSING_DEPTH);
        let echoed = answered("fn answer(input, context) { #{ data: input } }", &value);
        assert_eq!(echoed.unwrap(), value, "{}", shape.wraps);
    }
}

#[test]
fn a_value_one_level_past_the_bound_is_refused_on_its_way_in() {
    for shape in &SHAPES {
        let outcome = answered(
            "fn answer(input, context) { #{ data: 1 } }",
            &shape.nested(CROSSING_DEPTH + 1),
        );
        assert_eq!(
            outcome,
            Err(ScriptError::Question(
                "the input is nested deeper than 128 levels".to_owned()
            )),
            "{}",
            shape.wraps
        );
    }
}

#[test]
fn a_value_a_script_built_as_deep_as_the_bound_crosses_out() {
    for shape in &SHAPES {
        let build = shape.building(CROSSING_DEPTH);
        let source = format!("fn answer(input, context) {{ {build} #{{ data: v }} }}");
        assert_eq!(
            answered(&source, &json!({})).unwrap(),
            shape.nested(CROSSING_DEPTH),
            "{}",
            shape.wraps
        );
    }
}

#[test]
fn a_value_a_script_built_one_level_past_the_bound_is_refused_on_its_way_out() {
    for shape in &SHAPES {
        let build = shape.building(CROSSING_DEPTH + 1);
        let source = format!("fn answer(input, context) {{ {build} #{{ data: v }} }}");
        assert_eq!(
            answered(&source, &json!({})),
            Err(ScriptError::Envelope(
                "`data` is nested deeper than 128 levels".to_owned()
            )),
            "{}",
            shape.wraps
        );
    }
}

/// A resident that keeps `v` on `this` and draws what it kept.
fn keeping(shape: &Shape, levels: usize) -> Resident {
    let build = shape.building(levels);
    resident(&format!(
        "fn event(event) {{ {build} this.kept = v; true }}
        fn render(size) {{ this.kept }}"
    ))
}

/// The bound on `this` is the depth a value may cross, so nothing a session
/// could render is refused (`audit-bounded-extensions`, SCRIPT-1).
#[test]
fn this_holding_what_may_cross_stays_and_can_be_rendered() {
    for shape in &SHAPES {
        let resident = keeping(shape, CROSSING_DEPTH);
        let tick = resident.event(&json!({"name": "timer"})).outcome;
        assert!(tick.is_ok(), "{}: {tick:?}", shape.wraps);
        assert_eq!(resident.stopped(), None, "{}", shape.wraps);
        assert_eq!(
            resident.render(1, 1).outcome.unwrap().data,
            shape.nested(CROSSING_DEPTH),
            "{}",
            shape.wraps
        );
    }
}

#[test]
fn this_holding_one_level_more_than_may_cross_stops_the_session() {
    for shape in &SHAPES {
        let resident = keeping(shape, CROSSING_DEPTH + 1);
        let nesting = ScriptError::Limit {
            limit: Limit::Nesting,
            at: None,
        };
        let tick = resident.event(&json!({"name": "timer"})).outcome;
        assert_eq!(tick.unwrap_err(), nesting, "{}", shape.wraps);
        assert_eq!(resident.stopped(), Some(nesting), "{}", shape.wraps);
    }
}
