//! `mcp` exists only under an `mcp:` grant, calls only the tool a grant
//! names, and hands the script the tool's result as data
//! (`extension-mcp` D1, D6).

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use uze_script::{CheckoutPath, Entry, Grant, Grants, Reach, ReachError, Script};

/// A host whose `rtc` server answers `rtc_timesheet` with a month and
/// fails every other tool, remembering what it was asked.
#[derive(Default)]
struct Tracker {
    asked: Mutex<Vec<(String, String, Value)>>,
}

impl Reach for Tracker {
    fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError("no checkout".to_owned()))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }

    fn mcp(&self, server: &str, tool: &str, arguments: &Value) -> Result<Value, ReachError> {
        self.asked
            .lock()
            .unwrap()
            .push((server.to_owned(), tool.to_owned(), arguments.clone()));
        match tool {
            "rtc_timesheet" => Ok(json!({ "month": arguments["month"], "days": [{ "hours": 8 }] })),
            _ => Err(ReachError("RTC said 401".to_owned())),
        }
    }
}

fn granted(tools: &[&str]) -> Grants {
    tools
        .iter()
        .map(|tool| Grant::Mcp {
            server: "rtc".into(),
            tool: (*tool).into(),
        })
        .collect()
}

fn run(source: &str, grants: &Grants) -> (Result<uze_script::Envelope, String>, Arc<Tracker>) {
    let tracker = Arc::new(Tracker::default());
    let outcome = Script::compile(source.as_bytes())
        .unwrap()
        .answer(&json!({}), &json!({}), grants, tracker.clone())
        .outcome
        .map_err(|error| error.to_string());
    (outcome, tracker)
}

#[test]
fn a_granted_tool_is_called_with_its_arguments_and_answers_data() {
    let (envelope, tracker) = run(
        r#"fn answer(input, context) {
            let month = mcp("rtc", "rtc_timesheet", #{ month: "2026-10" });
            #{ data: #{ month: month.month, hours: month.days[0].hours } }
        }"#,
        &granted(&["rtc_timesheet"]),
    );
    assert_eq!(
        envelope.unwrap().data,
        json!({ "month": "2026-10", "hours": 8 })
    );
    assert_eq!(
        *tracker.asked.lock().unwrap(),
        [(
            "rtc".to_owned(),
            "rtc_timesheet".to_owned(),
            json!({ "month": "2026-10" })
        )]
    );
}

#[test]
fn a_tool_no_grant_names_is_refused_before_the_server_is_asked() {
    let (envelope, tracker) = run(
        r#"fn answer(input, context) { mcp("rtc", "rtc_create", #{ title: "x" }) }"#,
        &granted(&["rtc_timesheet"]),
    );
    let refused = envelope.unwrap_err();
    assert!(refused.contains("`mcp:rtc/rtc_create`"), "{refused}");
    assert!(tracker.asked.lock().unwrap().is_empty());
}

#[test]
fn without_a_grant_there_is_no_mcp_and_a_failure_can_be_caught() {
    let (envelope, _) = run(
        r#"fn answer(input, context) { mcp("rtc", "rtc_timesheet") }"#,
        &Grants::new(),
    );
    assert!(envelope.unwrap_err().contains("mcp"));

    let (envelope, _) = run(
        r#"fn answer(input, context) {
            let said = "";
            try { mcp("rtc", "rtc_query"); } catch (error) { said = `${error}`; }
            #{ data: #{ said: said } }
        }"#,
        &granted(&["rtc_query"]),
    );
    let said = envelope.unwrap().data["said"].as_str().unwrap().to_owned();
    assert!(said.contains("RTC said 401"), "{said}");
}
