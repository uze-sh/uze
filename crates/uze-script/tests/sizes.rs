//! A package function whose result can be many times what it was handed
//! is refused before it allocates (the audit's SCRIPT-2): each shadow
//! counts the result first and answers the string, array or map bound,
//! and still behaves as Rhai's own for everything inside the bounds.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uze_script::{CheckoutPath, Entry, Grants, Limit, Reach, ReachError, Script, ScriptError};

struct NoCheckout;

impl Reach for NoCheckout {
    fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError("no checkout".to_owned()))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }
}

/// `body` after `s` is 512 KiB of `a` and `big` is 8 KiB of `b`, with the
/// script's own `twice` beside `answer`.
fn answer(body: &str) -> Result<Value, ScriptError> {
    let source = format!(
        r#"fn twice() {{ this * 2 }}
        fn answer(input, context) {{
            let s = "aaaaaaaaaaaaaaaa";
            while s.len < 512 * 1024 {{ s += s; }}
            let big = "bbbbbbbbbbbbbbbb";
            while big.len < 8 * 1024 {{ big += big; }}
            {body}
        }}"#
    );
    Script::compile(source.as_bytes())?
        .answer(&json!({}), &json!({}), &Grants::new(), Arc::new(NoCheckout))
        .outcome
        .map(|envelope| envelope.data)
}

/// The bound `body` crossed, and that it crossed it at once rather than
/// after building what it asked for.
fn refused_at_once(body: &str) -> Limit {
    let started = Instant::now();
    let outcome = answer(body);
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(1),
        "refused before allocating, not after: {took:?}"
    );
    match outcome {
        Err(ScriptError::Limit { limit, .. }) => limit,
        other => panic!("{other:?}"),
    }
}

#[test]
fn replace_refuses_a_result_past_the_string_bound_before_building_it() {
    // The audit's shape: 4 GiB asked for, the process aborted.
    assert_eq!(
        refused_at_once(r#"s.replace("a", big); #{ data: s.len }"#),
        Limit::StringSize
    );
}

#[test]
fn replace_of_a_character_refuses_the_same_way() {
    assert_eq!(
        refused_at_once(r#"s.replace('a', big); #{ data: s.len }"#),
        Limit::StringSize
    );
}

#[test]
fn map_refuses_results_past_the_string_bound_as_it_collects_them() {
    // Ten thousand fresh 512 KiB strings: 5 GiB held before Rhai's own
    // check saw the array.
    let body = r#"let a = []; a.pad(10000, 0); let r = a.map(|x, i| s + i); #{ data: r.len }"#;
    assert_eq!(refused_at_once(body), Limit::StringSize);
}

#[test]
fn zip_refuses_results_past_the_string_bound_as_it_collects_them() {
    let body =
        r#"let a = []; a.pad(10000, 0); let r = a.zip(a, |x, y, i| s + i); #{ data: r.len }"#;
    assert_eq!(refused_at_once(body), Limit::StringSize);
}

#[test]
fn map_counts_nested_elements_against_the_array_bound() {
    let body = r#"let a = []; a.pad(200, 0); let r = a.map(|x| { let row = []; row.pad(100, x); row }); #{ data: r.len }"#;
    assert_eq!(refused_at_once(body), Limit::ArraySize);
}

#[test]
fn the_shadows_answer_as_rhai_does_inside_the_bounds() {
    let data = answer(
        r#"
        let text = "a-b-c";
        text.replace("-", "+");
        let chars = "a-b";
        chars.replace('-', "::");
        let empty = "ab";
        empty.replace("", ".");
        let a = [1, 2, 3];
        #{ data: [
            text, chars, empty,
            a.map(|x| x * 10),
            a.map(|x, i| x + i),
            a.map(Fn("twice")),
            a.zip([10, 20], |x, y| x + y),
            a.zip([10, 20, 30], |x, y, i| x + y + i),
        ] }
        "#,
    )
    .unwrap();
    assert_eq!(
        data,
        json!([
            "a+b+c",
            "a::b",
            ".a.b.",
            [10, 20, 30],
            [1, 3, 5],
            [2, 4, 6],
            [11, 22],
            [11, 23, 35],
        ])
    );
}

/// The engine's bounds: 1 MiB of string, 10 000 elements, 10 000 entries.
const STRING_BOUND: usize = 1 << 20;
const ARRAY_BOUND: usize = 10_000;
const MAP_BOUND: usize = 10_000;

/// `body` as an answer, with `big` read from the input.
fn ask(body: &str, big: Value) -> uze_script::Answered {
    let source = format!("fn answer(input, context) {{ let big = input.big; {body} }}");
    Script::compile(source.as_bytes()).unwrap().answer(
        &json!({ "big": big }),
        &json!({}),
        &Grants::new(),
        Arc::new(NoCheckout),
    )
}

fn data_of(body: &str, big: Value) -> Value {
    ask(body, big).outcome.unwrap().data
}

fn text_of(size: usize, ending: &str) -> Value {
    json!(format!("{}{ending}", "a".repeat(size - ending.len())))
}

fn elements(count: usize) -> Value {
    json!(vec![0; count])
}

fn entries(count: usize) -> Value {
    Value::Object((0..count).map(|key| (key.to_string(), json!(0))).collect())
}

#[test]
fn replace_shrinking_a_string_of_the_bound_answers() {
    let body = r#"big.replace("a", ""); #{ data: big.len }"#;
    assert_eq!(data_of(body, text_of(STRING_BOUND, "")), json!(0));
}

#[test]
fn replace_with_no_hit_leaves_the_text_alone() {
    let body = r#"big.replace("z", "zz"); #{ data: big }"#;
    assert_eq!(data_of(body, json!("abc")), json!("abc"));
}

#[test]
fn replace_building_exactly_the_bound_answers() {
    let body = r#"big.replace("b", "cc"); #{ data: [big.len, big.ends_with("acc")] }"#;
    assert_eq!(
        data_of(body, text_of(STRING_BOUND - 1, "b")),
        json!([STRING_BOUND, true])
    );
}

#[test]
fn replace_building_one_byte_past_the_bound_is_refused() {
    for body in [
        r#"big.replace("b", "cc"); #{ data: big.len }"#,
        r#"big.replace('b', "cc"); #{ data: big.len }"#,
    ] {
        match ask(body, text_of(STRING_BOUND, "b")).outcome {
            Err(ScriptError::Limit { limit, .. }) => assert_eq!(limit, Limit::StringSize),
            other => panic!("{body}: {other:?}"),
        }
    }
}

/// `map` and `zip` each returning `big` from one callback: an array of one
/// element, and whatever `big` holds.
const ONE_RESULT: [&str; 2] = [
    "let r = [0].map(|x| big); #{ data: r.len }",
    "let r = [0].zip([0], |x, y| big); #{ data: r.len }",
];

#[test]
fn results_of_exactly_each_bound_answer() {
    for body in ONE_RESULT {
        assert_eq!(data_of(body, text_of(STRING_BOUND, "")), json!(1), "{body}");
        assert_eq!(data_of(body, elements(ARRAY_BOUND - 1)), json!(1), "{body}");
        assert_eq!(data_of(body, entries(MAP_BOUND)), json!(1), "{body}");
    }
}

#[test]
fn a_callback_is_not_called_again_once_a_bound_is_passed() {
    let bodies = [
        r#"let r = [0, 1, 2, 3].map(|x| { print("called"); big }); #{ data: r.len }"#,
        r#"let r = [0, 1, 2, 3].zip([0, 1, 2, 3], |x, y| { print("called"); big }); #{ data: r.len }"#,
    ];
    for body in bodies {
        for (big, bound) in [
            (text_of(STRING_BOUND / 2 + 1, ""), Limit::StringSize),
            (elements(ARRAY_BOUND / 2 + 1), Limit::ArraySize),
            (entries(MAP_BOUND / 2 + 1), Limit::MapSize),
        ] {
            let answered = ask(body, big);
            match answered.outcome {
                Err(ScriptError::Limit { limit, .. }) => assert_eq!(limit, bound, "{body}"),
                other => panic!("{body}: {other:?}"),
            }
            assert_eq!(answered.trace.lines.len(), 2, "{bound:?}: {body}");
            assert_eq!(answered.trace.dropped, 0, "{bound:?}: {body}");
        }
    }
}
