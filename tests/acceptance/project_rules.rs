//! A project's rules, read through the real binary: the agent's offline
//! check and what `uze status` tells a person, both from one validator.

use std::path::{Path, PathBuf};

use uze_testkit::{git::isolated_git_in as git, temp::TestEnvironment};

use crate::util::uze_bin;

const UI_RULE: &str =
    "---\ntrigger: glob\nglobs: **/src/ui/**\n---\nUI-RULE-BODY: buttons use the design tokens.\n";
const TESTING_RULE: &str = "---\ntrigger: model_decision\ndescription: When writing or changing tests\n---\nTESTING-RULE-BODY\n";

/// A repository whose `.agents/rules/` holds `rules`.
fn checkout(env: &TestEnvironment, rules: &[(&str, &str)]) -> PathBuf {
    let root = env.project.clone();
    git(&root, &["init", "--quiet", "-b", "main"]);
    if !rules.is_empty() {
        std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
    }
    for (name, text) in rules {
        std::fs::write(root.join(".agents/rules").join(name), text).unwrap();
    }
    root
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Every file under `root` but `.git/`, with its bytes.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_rule_with_an_unsupported_pattern_fails_the_check() {
    let env = TestEnvironment::isolated();
    let root = checkout(
        &env,
        &[(
            "ui.md",
            "---\ntrigger: glob\nglobs: src/**/*.{ts,tsx}\n---\nbody\n",
        )],
    );

    let output = env.run(uze_bin(), &["agent", "rules", "check"]);
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    let text = said(&output);
    assert!(text.contains(".agents/rules/ui.md"), "{text}");
    assert!(text.contains("src/**/*.{ts,tsx}"), "{text}");

    let output = env.run(
        uze_bin(),
        &[
            "agent",
            "rules",
            "check",
            root.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["not_rules"][0]["path"], ".agents/rules/ui.md");
    assert!(
        report["not_rules"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("src/**/*.{ts,tsx}")
    );
}

#[test]
fn a_valid_set_passes_and_is_summarized() {
    let env = TestEnvironment::isolated();
    let root = checkout(
        &env,
        &[
            ("ui.md", UI_RULE),
            ("testing.md", TESTING_RULE),
            ("always.md", "---\ntrigger: always_on\n---\nALWAYS\n"),
        ],
    );
    std::fs::create_dir_all(root.join("src/ui")).unwrap();

    // Any path inside the checkout names it.
    let output = env.run(
        uze_bin(),
        &[
            "agent",
            "rules",
            "check",
            root.join("src/ui").to_str().unwrap(),
        ],
    );
    assert!(output.status.success(), "{}", said(&output));
    let text = stdout(&output);
    for expected in [
        ".agents/rules/ui.md",
        "glob",
        "src/ui/**",
        ".agents/rules/testing.md",
        "model_decision",
        ".agents/rules/always.md",
        "always_on",
    ] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
    assert!(
        !text.contains("RULE-BODY") && !text.contains("ALWAYS\n"),
        "the check reports frontmatter, never a body: {text}"
    );

    let output = env.run(uze_bin(), &["agent", "rules", "check", "--format", "json"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let ui = report["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|rule| rule["path"] == ".agents/rules/ui.md")
        .expect("every rule is listed");
    assert_eq!(ui["trigger"], "glob");
    assert_eq!(ui["patterns"], serde_json::json!(["**/src/ui/**"]));
}

#[test]
fn a_checkout_without_rules_is_not_an_error() {
    let env = TestEnvironment::isolated();
    let root = checkout(&env, &[]);
    let output = env.run(uze_bin(), &["agent", "rules", "check"]);
    assert!(output.status.success(), "{}", said(&output));
    assert!(stdout(&output).contains("no rules"), "{}", said(&output));
    assert!(
        !root.join(".agents").exists(),
        "checking creates no rules directory"
    );
}

#[test]
fn the_rules_check_is_hidden_from_a_person_s_help() {
    let env = TestEnvironment::isolated();
    let output = env.run(uze_bin(), &["--help"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(!text.contains("rules"), "{text}");
}

#[test]
fn status_names_each_file_that_is_not_a_rule_and_never_a_body() {
    let env = TestEnvironment::isolated();
    checkout(
        &env,
        &[
            ("ui.md", UI_RULE),
            ("testing.md", TESTING_RULE),
            ("notes.md", "# Notes\n\nNOTES-BODY\n"),
            ("broken.md", "---\ntrigger: glob\n---\nBROKEN-BODY\n"),
            (
                "wide.md",
                "---\ntrigger: glob\nglobs: src/**/*.{ts,tsx}\n---\nWIDE-BODY\n",
            ),
        ],
    );

    let output = env.run(uze_bin(), &["status"]);
    assert!(output.status.success(), "{}", said(&output));
    let text = stdout(&output);
    assert!(text.contains("2 rules"), "{text}");
    assert!(text.contains("1 glob"), "{text}");
    assert!(text.contains("1 model_decision"), "{text}");
    // A file without a trigger is reported, not guessed.
    assert!(text.contains(".agents/rules/notes.md"), "{text}");
    assert!(text.contains("trigger"), "{text}");
    // A glob rule without patterns is invalid.
    assert!(text.contains(".agents/rules/broken.md"), "{text}");
    assert!(text.contains("without `globs`"), "{text}");
    // An unsupported construct names the pattern.
    assert!(text.contains("src/**/*.{ts,tsx}"), "{text}");
    assert!(
        !text.contains("-BODY"),
        "status never reads out a rule: {text}"
    );

    let output = env.run(uze_bin(), &["status", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["rules"]["rules"].as_array().unwrap().len(), 2);
    assert_eq!(report["rules"]["not_rules"].as_array().unwrap().len(), 3);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("-BODY"));
}

/// The project keeps its rules in one place, and reading them writes
/// nothing into it: no second rules directory for any harness, and no file
/// at all.
#[test]
fn reading_rules_writes_nothing_into_the_project() {
    let env = TestEnvironment::isolated();
    let root = checkout(&env, &[("ui.md", UI_RULE), ("testing.md", TESTING_RULE)]);
    let before = tree(&root);

    for args in [
        &["agent", "rules", "check"][..],
        &["agent", "rules", "check", "--format", "json"],
        &["status"],
        &["status", "--format", "json"],
    ] {
        let output = env.run(uze_bin(), args);
        assert!(output.status.success(), "{args:?}: {}", said(&output));
    }

    assert_eq!(tree(&root), before, "the checkout is unchanged");
    for harness_dir in [".claude", ".codex", ".opencode", ".agent", ".cursor"] {
        assert!(
            !root.join(harness_dir).exists(),
            "{harness_dir} was created"
        );
    }
}

#[test]
fn a_folder_registered_in_the_manifest_is_a_rule_of_the_checkout() {
    let env = TestEnvironment::isolated();
    let root = checkout(&env, &[("testing.md", TESTING_RULE)]);
    std::fs::create_dir_all(root.join(".agents/rules/team")).unwrap();
    std::fs::write(root.join(".agents/rules/team/ui.md"), UI_RULE).unwrap();
    std::fs::write(
        root.join(".agents/rules.json"),
        r#"{"entries": [{"path": ".agents/rules/team"}]}"#,
    )
    .unwrap();

    let output = env.run(uze_bin(), &["status", "--format", "json"]);
    assert!(output.status.success(), "{}", said(&output));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let paths: Vec<&str> = report["rules"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        [".agents/rules/team/ui.md", ".agents/rules/testing.md"]
    );

    let output = env.run(uze_bin(), &["agent", "rules", "check"]);
    assert!(output.status.success(), "{}", said(&output));
    assert!(stdout(&output).contains(".agents/rules/team/ui.md"));
}

#[test]
fn a_manifest_that_does_not_parse_adds_nothing_and_status_says_so() {
    let env = TestEnvironment::isolated();
    let root = checkout(&env, &[("testing.md", TESTING_RULE)]);
    std::fs::create_dir_all(root.join(".agents/rules/team")).unwrap();
    std::fs::write(root.join(".agents/rules/team/ui.md"), UI_RULE).unwrap();
    std::fs::write(
        root.join(".agents/rules.json"),
        r#"{ "entries": [ { "path": ".agents/rules/team" } "#,
    )
    .unwrap();

    let output = env.run(uze_bin(), &["status"]);
    assert!(output.status.success(), "{}", said(&output));
    let text = stdout(&output);
    assert!(
        text.contains(".agents/rules.json: registers nothing"),
        "{text}"
    );
    assert!(
        text.contains("1 rule"),
        "the flat rule is still the rule: {text}"
    );

    let output = env.run(uze_bin(), &["status", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["rules"]["rules"].as_array().unwrap().len(), 1);
    assert!(report["rules"]["manifest"]["unreadable"].is_string());

    let output = env.run(uze_bin(), &["agent", "rules", "check"]);
    assert!(
        !output.status.success(),
        "the check fails: {}",
        said(&output)
    );
}
