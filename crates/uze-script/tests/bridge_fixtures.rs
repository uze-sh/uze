//! The agent bridge's reference scripts (`tests/_fixtures/canonical/bridge`)
//! answer what their extensions promise: a plugin check runs no script
//! against an event or a plan, so their answers are proved here.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use uze_contract::{Grant, Sound};
use uze_script::{CheckoutPath, Entry, EntryKind, Grants, Reach, ReachError, Script};

fn extension(id: &str, script: &str) -> Script {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/_fixtures/canonical/bridge/extensions")
        .join(id)
        .join(script);
    let script = uze_script::script_of(script, &std::fs::read(&path).unwrap()).unwrap();
    Script::compile(&script).unwrap()
}

/// A checkout holding a plan, or nothing at all.
struct Checkout {
    plan: Option<&'static str>,
}

impl Reach for Checkout {
    fn read_checkout(&self, path: &CheckoutPath) -> Result<String, ReachError> {
        match (path.as_str(), self.plan) {
            ("PLAN.md", Some(plan)) => Ok(plan.to_owned()),
            (other, _) => Err(ReachError(format!("`{other}` is not in the checkout"))),
        }
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(self
            .plan
            .map(|_| Entry {
                name: "PLAN.md".to_owned(),
                kind: EntryKind::File,
            })
            .into_iter()
            .collect())
    }
}

fn answer(script: &Script, input: Value, grants: Grants, plan: Option<&'static str>) -> Value {
    let envelope = script
        .answer(&input, &json!({}), &grants, Arc::new(Checkout { plan }))
        .outcome
        .unwrap();
    json!({
        "data": envelope.data,
        "notify": envelope.notify.map(|notify| json!([notify.title, notify.body])),
        "sound": envelope.sound.map(|sound| format!("{sound:?}")),
    })
}

fn event(name: &str) -> Value {
    json!({ "event": {
        "name": name, "agent": "a1", "harness": "claude",
        "checkout": "/work/uze", "at": 1_761_570_000_000_u64,
    } })
}

#[test]
fn chimes_sounds_a_turns_end_and_notifies_an_approval_asked() {
    let chime = extension("chimes", "chime.uze");
    let quiet = answer(&chime, json!({}), Grants::new(), None);
    assert_eq!(quiet, json!({ "data": {}, "notify": null, "sound": null }));

    let finished = answer(&chime, event("agent.turn.finished"), Grants::new(), None);
    assert_eq!(finished["data"]["said"], "turn finished");
    assert_eq!(finished["sound"], format!("{:?}", Sound::Done));
    assert!(finished["notify"].is_null());

    let asked = answer(&chime, event("agent.approval.asked"), Grants::new(), None);
    assert_eq!(asked["sound"], format!("{:?}", Sound::Attention));
    assert_eq!(
        asked["notify"],
        json!(["Approval", "An agent on claude asks for an approval"])
    );
}

#[test]
fn plan_review_reads_the_plans_title_and_open_steps() {
    let plan = extension("plan-review", "plan.uze");
    let grants = Grants::from([Grant::ReadCheckout]);
    let input = json!({ "file": "PLAN.md" });
    let read = answer(
        &plan,
        input.clone(),
        grants.clone(),
        Some("# Agent bridge\n\n- [x] Report\n- [ ] Write the fixtures\n- [ ] Prove the journey\n"),
    );
    assert_eq!(
        read["data"],
        json!({
            "file": "PLAN.md", "title": "Agent bridge", "left": 2,
            "open": [{ "text": "Write the fixtures" }, { "text": "Prove the journey" }],
        })
    );
    let none = answer(&plan, input, grants, None);
    assert_eq!(none["data"], json!({ "file": "PLAN.md" }));
}
