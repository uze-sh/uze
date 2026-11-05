//! `fetch` exists only under a `net:` grant, asks the host only for a host
//! it names, and hands the script the reply, never the secret.

use std::sync::{Arc, Mutex};

use serde_json::json;
use uze_script::{
    CheckoutPath, Entry, FetchReply, FetchRequest, Grant, Grants, Reach, ReachError, Script,
};

/// A host that answers every request with a recorded run, and remembers
/// what it was asked.
#[derive(Default)]
struct Forge {
    asked: Mutex<Vec<FetchRequest>>,
}

impl Reach for Forge {
    fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError("no checkout".to_owned()))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }

    fn fetch(&self, request: &FetchRequest) -> Result<FetchReply, ReachError> {
        self.asked.lock().unwrap().push(request.clone());
        Ok(FetchReply {
            status: 200,
            body: r#"{"workflow_runs":[{"name":"CI","conclusion":"success"}]}"#.to_owned(),
        })
    }
}

fn github() -> Grants {
    Grants::from([Grant::Net("api.github.com".into())])
}

fn run(source: &str, grants: &Grants) -> (Result<uze_script::Envelope, String>, Arc<Forge>) {
    let forge = Arc::new(Forge::default());
    let outcome = Script::compile(source.as_bytes())
        .unwrap()
        .answer(&json!({}), &json!({}), grants, forge.clone())
        .outcome
        .map_err(|error| error.to_string());
    (outcome, forge)
}

#[test]
fn a_granted_host_is_asked_and_its_json_read() {
    let (envelope, forge) = run(
        r#"fn answer(input, context) {
            let reply = fetch("https://api.github.com/repos/o/r/actions/runs",
                #{ secret: "token", scheme: "Bearer", headers: #{ Accept: "application/json" } });
            #{ data: #{ status: reply.status, first: reply.json.workflow_runs[0].conclusion } }
        }"#,
        &github(),
    );
    assert_eq!(
        envelope.unwrap().data,
        json!({ "status": 200, "first": "success" })
    );
    let asked = forge.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].host, "api.github.com");
    assert_eq!(asked[0].secret.as_ref().unwrap().preference, "token");
}

#[test]
fn another_host_is_refused_before_anything_is_asked() {
    let (envelope, forge) = run(
        r#"fn answer(input, context) { fetch("https://example.com/") }"#,
        &github(),
    );
    let refused = envelope.unwrap_err();
    assert!(
        refused.contains("`example.com` is a host this extension may not reach"),
        "{refused}"
    );
    assert!(forge.asked.lock().unwrap().is_empty());
}

#[test]
fn without_a_net_grant_there_is_no_fetch() {
    let source = r#"fn answer(input, context) { fetch("https://api.github.com/") }"#;
    let (envelope, forge) = run(source, &Grants::new());
    assert!(envelope.unwrap_err().contains("fetch"));
    assert!(forge.asked.lock().unwrap().is_empty());
    assert_eq!(
        uze_script::ungranted_calls(source, &Grants::new()),
        [("fetch".to_owned(), uze_script::ANY_HOST)]
    );
    assert!(uze_script::ungranted_calls(source, &github()).is_empty());
}

#[test]
fn an_answer_asks_the_host_a_bounded_number_of_times() {
    let (envelope, forge) = run(
        r#"fn answer(input, context) {
            for n in 0..40 { fetch("https://api.github.com/"); }
            #{ data: () }
        }"#,
        &github(),
    );
    let refused = envelope.unwrap_err();
    assert!(refused.contains("16 calls"), "{refused}");
    assert_eq!(forge.asked.lock().unwrap().len(), 16);
}

/// What `fetch` makes of `scheme`, written into the script as Rhai text.
fn with_scheme(scheme: &str) -> (Result<uze_script::Envelope, String>, Arc<Forge>) {
    run(
        &format!(
            r#"fn answer(input, context) {{
                fetch("https://api.github.com/", #{{ secret: "token", scheme: "{scheme}" }});
                #{{ data: () }}
            }}"#
        ),
        &github(),
    )
}

#[test]
fn a_scheme_that_is_not_one_word_is_refused_before_anything_is_asked() {
    for scheme in ["", "Bearer token", r"Bearer\r\nHost: example.com"] {
        let (envelope, forge) = with_scheme(scheme);
        let refused = envelope.unwrap_err();
        assert!(refused.contains("is not a scheme"), "{scheme}: {refused}");
        assert!(forge.asked.lock().unwrap().is_empty(), "{scheme}");
    }
}

#[test]
fn a_scheme_of_letters_digits_and_hyphens_is_placed_as_asked() {
    let (envelope, forge) = with_scheme("Be-ar3r");
    envelope.unwrap();
    let asked = forge.asked.lock().unwrap();
    assert_eq!(
        asked[0].secret.as_ref().unwrap().scheme.as_deref(),
        Some("Be-ar3r")
    );
}

/// What `fetch` handed the host, for `options` written as Rhai text.
fn asked_with(options: &str) -> FetchRequest {
    let (envelope, forge) = run(
        &format!(
            r#"fn answer(input, context) {{
                fetch("https://api.github.com/", {options});
                #{{ data: () }}
            }}"#
        ),
        &github(),
    );
    envelope.unwrap();
    forge.asked.lock().unwrap().remove(0)
}

/// The name is what the host writes as the request's method, so it is the
/// one HTTP reads.
#[test]
fn the_method_sent_is_the_one_asked() {
    for (options, name) in [
        ("#{}", "GET"),
        (r#"#{ method: "GET" }"#, "GET"),
        (r#"#{ method: "get" }"#, "GET"),
        (r#"#{ method: "POST" }"#, "POST"),
        (r#"#{ method: "post" }"#, "POST"),
    ] {
        assert_eq!(asked_with(options).method.name(), name, "{options}");
    }
}

#[test]
fn a_text_body_is_sent_as_given() {
    let asked = asked_with(r#"#{ method: "POST", body: "not \"json\": {" }"#);
    assert_eq!(asked.body.as_deref(), Some(r#"not "json": {"#));
}
