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
