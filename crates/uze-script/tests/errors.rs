//! What a script that did not answer says about why reads on the surface
//! and in the inspector: the bound by name, the place in its text when
//! there is one, and nothing in its place when there is none.

use std::sync::Arc;

use serde_json::json;
use uze_script::{
    CheckoutPath, Entry, Grants, Limit, Location, Reach, ReachError, Script, ScriptError, script_of,
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

#[test]
fn a_limit_error_names_its_bound() {
    for (limit, says) in [
        (Limit::Operations, "operations"),
        (Limit::Deadline, "deadline"),
        (Limit::CallDepth, "call depth"),
        (Limit::ExpressionDepth, "expression depth"),
        (Limit::StringSize, "string size"),
        (Limit::ArraySize, "array size"),
        (Limit::MapSize, "map size"),
        (Limit::Nesting, "nesting depth"),
        (Limit::ScriptSize, "script size"),
    ] {
        assert_eq!(
            ScriptError::Limit { limit, at: None }.to_string(),
            format!("the script crossed its {says} bound")
        );
    }
}

#[test]
fn an_error_with_a_location_says_its_line_and_column() {
    let at = Some(Location { line: 3, column: 7 });
    assert_eq!(
        ScriptError::Limit {
            limit: Limit::StringSize,
            at,
        }
        .to_string(),
        "the script crossed its string size bound at 3:7"
    );
    assert_eq!(
        ScriptError::Runtime {
            message: "no plan".to_owned(),
            at,
        }
        .to_string(),
        "the script failed at 3:7: no plan"
    );
}

#[test]
fn an_error_without_a_location_says_none() {
    assert_eq!(
        ScriptError::Runtime {
            message: "no plan".to_owned(),
            at: None,
        }
        .to_string(),
        "the script failed: no plan"
    );
    assert_eq!(
        ScriptError::MissingFunction {
            name: "store".to_owned(),
            at: None,
        }
        .to_string(),
        "the script calls `store`, which this engine does not have"
    );
}

#[test]
fn a_script_that_does_not_compile_says_where_it_stopped() {
    let refused = Script::compile(b"fn answer(input, context) {\n  let = 1;\n}").unwrap_err();
    let ScriptError::Compile { at: Some(at), .. } = &refused else {
        panic!("{refused:?}");
    };
    assert!(
        refused
            .to_string()
            .contains(&format!(" at 2:{}:", at.column)),
        "{refused}"
    );
}

#[test]
fn an_answer_nested_too_deep_says_why() {
    let source = "fn answer(input, context) {
        let v = 0;
        let n = 0;
        while n < 129 { v = [v]; n += 1; }
        #{ data: v }
    }";
    let refused = Script::compile(source.as_bytes())
        .unwrap()
        .answer(&json!({}), &json!({}), &Grants::new(), Arc::new(NoCheckout))
        .outcome
        .unwrap_err()
        .to_string();
    assert_eq!(
        refused,
        "the answer is not an envelope: `data` is nested deeper than 128 levels"
    );
}

#[test]
fn a_second_script_block_is_reported_at_its_own_line_and_column() {
    let component = "<view>\nText \"a\"\n</view>\n\n<script>\n</script>\n\n<script>\n</script>\n";
    match script_of("board.uze", component.as_bytes()) {
        Err(ScriptError::Compile { message, at }) => {
            assert_eq!(message, "a second `<script>` block");
            assert_eq!(at, Some(Location { line: 8, column: 1 }));
        }
        other => panic!("{other:?}"),
    }
}
