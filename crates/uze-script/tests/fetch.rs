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
