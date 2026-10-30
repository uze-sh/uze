//! The official `contributions` extension (`plugins/workspace`) counts a
//! person once, however each machine they committed from spelled their
//! name: a plugin check runs its views against fixtures, never its script
//! against a log, so the grouping is proved here.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use uze_script::{CheckoutPath, Entry, Grant, Grants, ProgramReply, Reach, ReachError, Script};

fn heatmap() -> Script {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/workspace/extensions/contributions/heatmap.uze");
    let script = uze_script::script_of("heatmap.uze", &std::fs::read(&path).unwrap()).unwrap();
    Script::compile(&script).unwrap()
}

/// A repository whose `git log` answers `log`, on 2026-10-30.
struct Repository {
    log: &'static str,
}

impl Reach for Repository {
    fn read_checkout(&self, path: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError(format!("`{}` is not read", path.as_str())))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }

    fn clock(&self) -> Duration {
        Duration::from_secs(1_793_361_600)
    }

    fn store(&self) -> Result<Value, ReachError> {
        Ok(json!({}))
    }

    fn run_program(&self, _: &str, _: &[String]) -> Result<ProgramReply, ReachError> {
        Ok(ProgramReply {
            code: Some(0),
            stdout: self.log.to_owned(),
            stderr: String::new(),
        })
    }
}

fn answer(log: &'static str) -> Value {
    let grants = Grants::from([Grant::Exec("git".into()), Grant::Store]);
    heatmap()
        .answer(
            &json!({}),
            &json!({}),
            &grants,
            Arc::new(Repository { log }),
        )
        .outcome
        .unwrap()
        .data
}

#[test]
fn one_email_is_one_person_under_the_name_they_sign_with_most() {
    let data = answer(concat!(
        "2026-10-30\tR o m u l l o\tdev@example.com\ta1\tfix: one\n",
        "2026-10-30\tRomullo\tDev@Example.com \ta2\tfix: two\n",
        "2026-10-29\tRomullo\tdev@example.com\ta3\tfix: three\n",
        "2026-10-29\tdependabot[bot]\tbot@example.com\ta4\tbuild: bump\n",
    ));
    assert_eq!(
        data["authors"],
        json!([
            { "name": "Romullo", "count": 3, "ratio": 1.0 },
            { "name": "dependabot[bot]", "count": 1, "ratio": 1.0 / 3.0 },
        ])
    );
    let authors: Vec<&Value> = data["chosen"]["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|commit| &commit["author"])
        .collect();
    assert_eq!(authors, ["Romullo", "Romullo"], "the day's commits too");
}

#[test]
fn a_commit_with_no_email_is_its_name() {
    let data = answer(concat!(
        "2026-10-30\tAna\t\ta1\tfix: one\n",
        "2026-10-30\tana \t\ta2\tfix: two\n",
        "2026-10-30\tBob\t\ta3\tfix: three\n",
    ));
    let counts: Vec<&Value> = data["authors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|author| &author["count"])
        .collect();
    assert_eq!(counts, [2, 1], "Ana once, apart from Bob");
}
