//! A script answers inside its bounds, reaching only what it was granted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uze_script::{
    CheckoutPath, Entry, EntryKind, Grant, Grants, Limit, Reach, ReachError, Script, ScriptError,
    TraceKind,
};

/// A checkout of two files, and a clock fixed at one second past the epoch.
struct FakeCheckout {
    read_delay: Duration,
}

impl Reach for FakeCheckout {
    fn read_checkout(&self, path: &CheckoutPath) -> Result<String, ReachError> {
        std::thread::sleep(self.read_delay);
        match path.as_str() {
            "plan.md" => Ok("---\nstatus: done\n---\n# Plan\n".to_owned()),
            other => Err(ReachError(format!("`{other}` is not in the checkout"))),
        }
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(vec![
            Entry {
                name: "plan.md".to_owned(),
                kind: EntryKind::File,
            },
            Entry {
                name: "phases".to_owned(),
                kind: EntryKind::Directory,
            },
        ])
    }

    fn clock(&self) -> Duration {
        Duration::from_secs(1)
    }
}

fn checkout() -> Arc<dyn Reach> {
    Arc::new(FakeCheckout {
        read_delay: Duration::ZERO,
    })
}

fn granted() -> Grants {
    Grants::from([Grant::ReadCheckout])
}

fn answer(source: &str, grants: &Grants) -> Result<uze_script::Envelope, ScriptError> {
    Script::compile(source.as_bytes())?
        .answer(&json!({}), &json!({}), grants, checkout())
        .outcome
}

fn limit_of(outcome: Result<uze_script::Envelope, ScriptError>) -> Limit {
    match outcome {
        Err(ScriptError::Limit { limit, .. }) => limit,
        other => panic!("expected a bound to stop the script, got {other:?}"),
    }
}

#[test]
fn an_envelope_carries_data_and_its_requests() {
    let envelope = answer(
        r#"fn answer(input, context) {
            #{ data: [1, "two", #{ three: 3.5 }], refresh: ["board"], toast: "saved", notice: () }
        }"#,
        &Grants::new(),
    )
    .unwrap();
    assert_eq!(envelope.data, json!([1, "two", {"three": 3.5}]));
    assert_eq!(envelope.refresh, ["board"]);
    assert_eq!(envelope.toast.as_deref(), Some("saved"));
    assert_eq!(envelope.notice, None);
}

#[test]
fn the_input_and_the_context_reach_the_script() {
    let script = Script::compile(
        br#"fn answer(input, context) { #{ data: `${input.phase} on ${context.checkout}` } }"#,
    )
    .unwrap();
    let answered = script.answer(
        &json!({"phase": "02"}),
        &json!({"checkout": "main"}),
        &Grants::new(),
        checkout(),
    );
    assert_eq!(answered.outcome.unwrap().data, json!("02 on main"));
}

#[test]
fn an_unknown_envelope_member_fails_the_answer() {
    let outcome = answer(
        "fn answer(input, context) { #{ data: 1, notify: \"hi\" } }",
        &Grants::new(),
    );
    assert!(
        matches!(&outcome, Err(ScriptError::Envelope(why)) if why.contains("`notify`")),
        "{outcome:?}"
    );
}

#[test]
fn an_answer_without_data_fails() {
    let outcome = answer(
        "fn answer(input, context) { #{ toast: \"x\" } }",
        &Grants::new(),
    );
    assert!(
        matches!(outcome, Err(ScriptError::Envelope(_))),
        "{outcome:?}"
    );
}

#[test]
fn a_loop_without_end_is_stopped_by_the_operation_bound() {
    let outcome = answer("fn answer(input, context) { loop {} }", &Grants::new());
    assert_eq!(limit_of(outcome), Limit::Operations);
}

#[test]
fn a_slow_host_is_stopped_by_the_deadline() {
    let script =
        Script::compile(br#"fn answer(input, context) { loop { read_checkout("plan.md"); } }"#)
            .unwrap();
    let slow: Arc<dyn Reach> = Arc::new(FakeCheckout {
        read_delay: Duration::from_millis(1),
    });
    let started = Instant::now();
    let outcome = script
        .answer(&json!({}), &json!({}), &granted(), slow)
        .outcome;
    assert_eq!(limit_of(outcome), Limit::Deadline);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn deep_recursion_is_stopped() {
    let outcome = answer(
        "fn down(n) { down(n + 1) } fn answer(input, context) { #{ data: down(0) } }",
        &Grants::new(),
    );
    assert_eq!(limit_of(outcome), Limit::CallDepth);
}

#[test]
fn a_giant_string_is_stopped() {
    let outcome = answer(
        r#"fn answer(input, context) { let s = "x"; loop { s += s; } }"#,
        &Grants::new(),
    );
    assert_eq!(limit_of(outcome), Limit::StringSize);
}

#[test]
fn a_giant_array_is_stopped() {
    let outcome = answer(
        "fn answer(input, context) { let a = []; a.pad(20000, 0); #{ data: a } }",
        &Grants::new(),
    );
    assert_eq!(limit_of(outcome), Limit::ArraySize);
}

#[test]
fn eval_is_refused() {
    let outcome = answer(
        r#"fn answer(input, context) { #{ data: eval("40 + 2") } }"#,
        &Grants::new(),
    );
    assert!(
        matches!(&outcome, Err(ScriptError::Compile { message, .. }) if message.contains("eval")),
        "{outcome:?}"
    );
}

#[test]
fn import_is_refused() {
    let outcome = answer(
        r#"import "secrets" as s; fn answer(input, context) { #{ data: 1 } }"#,
        &Grants::new(),
    );
    assert!(
        matches!(&outcome, Err(ScriptError::Compile { message, .. }) if message.contains("import")),
        "{outcome:?}"
    );
}

#[test]
fn an_undeclared_variable_does_not_compile() {
    let outcome = Script::compile(b"fn answer(input, context) { #{ data: nowhere } }");
    assert!(
        matches!(outcome, Err(ScriptError::Compile { .. })),
        "{outcome:?}"
    );
}

#[test]
fn a_compile_error_says_where() {
    let outcome = Script::compile(b"fn answer(input, context) {\n  let = 1;\n}");
    match outcome {
        Err(ScriptError::Compile { at: Some(at), .. }) => assert_eq!(at.line, 2),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_script_without_answer_does_not_compile() {
    assert!(Script::compile(b"fn other(a) { a }").is_err());
}

#[test]
fn read_checkout_without_its_grant_does_not_exist() {
    let outcome = answer(
        r#"fn answer(input, context) { #{ data: read_checkout("plan.md") } }"#,
        &Grants::new(),
    );
    assert!(
        matches!(&outcome, Err(ScriptError::MissingFunction { name, .. }) if name == "read_checkout"),
        "{outcome:?}"
    );
}

#[test]
fn with_its_grant_the_host_answers() {
    let envelope = answer(
        r#"fn answer(input, context) {
            let plan = front_matter(read_checkout("plan.md"));
            let names = list_checkout("").map(|entry| entry.name + ":" + entry.kind);
            #{ data: #{ status: plan.fields.status, entries: names, at: clock() } }
        }"#,
        &granted(),
    )
    .unwrap();
    assert_eq!(
        envelope.data,
        json!({"status": "done", "entries": ["plan.md:file", "phases:dir"], "at": 1000})
    );
}

#[test]
fn a_path_out_of_the_checkout_is_refused_before_the_host_is_asked() {
    for path in ["../secret", "/etc/passwd"] {
        let source =
            format!(r#"fn answer(input, context) {{ #{{ data: read_checkout("{path}") }} }}"#);
        let outcome = answer(&source, &granted());
        assert!(
            matches!(outcome, Err(ScriptError::Runtime { .. })),
            "{path}: {outcome:?}"
        );
    }
}

#[test]
fn a_thrown_error_reads_as_the_script_said_it() {
    let outcome = answer(
        r#"fn answer(input, context) { throw "no plan" }"#,
        &Grants::new(),
    );
    assert!(
        matches!(&outcome, Err(ScriptError::Runtime { message, .. }) if message == "no plan"),
        "{outcome:?}"
    );
}

#[test]
fn print_and_debug_reach_the_trace() {
    let script = Script::compile(
        b"fn answer(input, context) {\n  print(\"hello\");\n  debug(42);\n  #{ data: () }\n}",
    )
    .unwrap();
    let answered = script.answer(&json!({}), &json!({}), &Grants::new(), checkout());
    assert!(answered.outcome.is_ok());
    let lines: Vec<_> = answered
        .trace
        .lines
        .iter()
        .map(|line| (line.kind, line.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [(TraceKind::Print, "hello"), (TraceKind::Debug, "42")]
    );
}

#[test]
fn the_trace_survives_a_failed_answer() {
    let script =
        Script::compile(br#"fn answer(input, context) { print("before"); throw "after" }"#)
            .unwrap();
    let answered = script.answer(&json!({}), &json!({}), &Grants::new(), checkout());
    assert!(answered.outcome.is_err());
    assert_eq!(answered.trace.lines[0].text, "before");
}

#[test]
fn the_primitives_do_the_heavy_work() {
    let envelope = answer(
        r#"fn answer(input, context) {
            let text = "phase-01: auth\nphase-02: billing\nnotes";
            let named = lines(text).filter(|line| regex_match(line, "^phase-\\d+"));
            let pairs = regex(text, "phase-(\\d+): (\\w+)").map(|m| m[1] + "=" + m[2]);
            let kept = glob("**/*.md", ["a/plan.md", "b.txt", "c/d/e.md"]);
            let parsed = json_parse("{\"n\": [1, 2]}");
            #{ data: #{ named: named.len(), pairs: pairs, kept: kept,
                        n: parsed.n[1], back: json_stringify(#{ k: true }) } }
        }"#,
        &Grants::new(),
    )
    .unwrap();
    assert_eq!(
        envelope.data,
        json!({
            "named": 2,
            "pairs": ["01=auth", "02=billing"],
            "kept": ["a/plan.md", "c/d/e.md"],
            "n": 2,
            "back": "{\"k\":true}"
        })
    );
}

/// D28 measured 0.19 ms to build an engine and 0.45 ms for the gallery's
/// heaviest answer; the targets are an answer under 20 ms at p95. Bounds
/// here are loose enough for a debug build on a loaded machine, and tight
/// enough to catch an order of magnitude.
#[test]
fn an_answer_costs_what_d28_measured() {
    let source = br#"fn answer(input, context) {
        let rows = [];
        for i in 0..50 { rows.push(#{ id: i, name: `row ${i}` }); }
        #{ data: rows }
    }"#;
    let started = Instant::now();
    let script = Script::compile(source).unwrap();
    let compiled = started.elapsed();

    let mut slowest = Duration::ZERO;
    for _ in 0..20 {
        let started = Instant::now();
        let answered = script.answer(&json!({}), &Value::Null, &Grants::new(), checkout());
        slowest = slowest.max(started.elapsed());
        assert!(answered.outcome.is_ok());
    }
    assert!(
        compiled < Duration::from_millis(20),
        "compiled in {compiled:?}"
    );
    assert!(
        slowest < Duration::from_millis(20),
        "slowest answer {slowest:?}"
    );
}
