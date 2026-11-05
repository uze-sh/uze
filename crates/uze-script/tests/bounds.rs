//! What a script hands the engine is held to a size that is exact: as long
//! as the bound is taken, one byte more is refused.

use std::sync::Arc;

use serde_json::{Value, json};
use uze_script::{
    CheckoutPath, Entry, Grants, Limit, Reach, ReachError, ResidentScript, Script, ScriptError,
    host_of,
};

/// 256 KiB, the largest script the engine compiles.
const SCRIPT_SIZE: usize = 262_144;
/// 8 KiB, the longest URL `fetch` reads.
const URL_SIZE: usize = 8_192;
const GLOB_PATTERN: usize = 1_024;
const GLOB_PATH: usize = 4_096;

struct NoCheckout;

impl Reach for NoCheckout {
    fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError("no checkout".to_owned()))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }
}

/// `source` followed by spaces, `size` bytes in all.
fn padded(source: &str, size: usize) -> Vec<u8> {
    let mut bytes = source.as_bytes().to_vec();
    bytes.resize(size, b' ');
    bytes
}

fn too_large<T: std::fmt::Debug>(outcome: Result<T, ScriptError>) {
    match outcome {
        Err(ScriptError::Limit {
            limit: Limit::ScriptSize,
            at: None,
        }) => {}
        other => panic!("expected the script size bound, got {other:?}"),
    }
}

#[test]
fn a_script_of_exactly_the_bound_compiles_and_answers() {
    let source = padded("fn answer(input, context) { #{ data: 1 } }", SCRIPT_SIZE);
    let answered = Script::compile(&source).unwrap().answer(
        &json!({}),
        &json!({}),
        &Grants::new(),
        Arc::new(NoCheckout),
    );
    assert_eq!(answered.outcome.unwrap().data, json!(1));
}

#[test]
fn a_script_one_byte_past_the_bound_is_refused() {
    too_large(Script::compile(&padded(
        "fn answer(input, context) { #{ data: 1 } }",
        SCRIPT_SIZE + 1,
    )));
}

#[test]
fn a_resident_script_of_exactly_the_bound_compiles() {
    let source = padded("fn render(size) { () }", SCRIPT_SIZE);
    ResidentScript::compile(&source).unwrap();
}

#[test]
fn a_resident_script_one_byte_past_the_bound_is_refused() {
    too_large(ResidentScript::compile(&padded(
        "fn render(size) { () }",
        SCRIPT_SIZE + 1,
    )));
}

fn url_of(size: usize) -> String {
    let start = "https://api.github.com/";
    format!("{start}{}", "a".repeat(size - start.len()))
}

#[test]
fn a_url_of_exactly_the_bound_is_read() {
    assert_eq!(host_of(&url_of(URL_SIZE)).unwrap(), "api.github.com");
}

#[test]
fn a_url_one_byte_past_the_bound_is_refused() {
    let refused = host_of(&url_of(URL_SIZE + 1)).unwrap_err();
    assert!(refused.0.contains("longer than 8 KiB"), "{refused}");
}

fn glob_match(pattern: &str, path: &str) -> Result<Value, ScriptError> {
    Script::compile(
        b"fn answer(input, context) { #{ data: glob_match(input.pattern, input.path) } }",
    )?
    .answer(
        &json!({"pattern": pattern, "path": path}),
        &json!({}),
        &Grants::new(),
        Arc::new(NoCheckout),
    )
    .outcome
    .map(|envelope| envelope.data)
}

fn refused_by_glob(outcome: Result<Value, ScriptError>) {
    assert!(
        matches!(&outcome, Err(ScriptError::Runtime { message, .. }) if message.contains("glob takes patterns up to")),
        "{outcome:?}"
    );
}

#[test]
fn a_glob_pattern_of_exactly_the_bound_is_matched() {
    let pattern = "a".repeat(GLOB_PATTERN);
    assert_eq!(glob_match(&pattern, &pattern).unwrap(), json!(true));
}

#[test]
fn a_glob_pattern_one_byte_past_the_bound_is_refused_whatever_the_path() {
    refused_by_glob(glob_match(&"a".repeat(GLOB_PATTERN + 1), "a"));
}

#[test]
fn a_glob_path_of_exactly_the_bound_is_matched() {
    assert_eq!(
        glob_match("**", &"a".repeat(GLOB_PATH)).unwrap(),
        json!(true)
    );
}

#[test]
fn a_glob_path_one_byte_past_the_bound_is_refused_whatever_the_pattern() {
    refused_by_glob(glob_match("**", &"a".repeat(GLOB_PATH + 1)));
}
