//! What the workflows that build and publish uze may not do.
//!
//! A release is a binary strangers install and the updater renames over
//! the one on their `PATH`, so the pipeline that makes it is part of the
//! product's supply chain. GitHub's own settings (allowed actions, required
//! SHA pinning, rulesets) live outside the repository and can drift; these
//! rules live in it and fail the build on the change that breaks them.
//!
//! Line scans rather than a YAML parser, as everywhere else in this suite:
//! the workflows are written in one indentation style, and a rule that
//! stopped matching would show up as its own `scanned` assertion failing.

use std::{fs, path::PathBuf};

fn workflows() -> Vec<(String, String)> {
    let root = uze_testkit::workspace_root();
    let mut files = Vec::new();
    for directory in [".github/workflows", ".github/actions/journeys"] {
        let Ok(entries) = fs::read_dir(root.join(directory)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path: PathBuf = entry.path();
            if path.extension().is_some_and(|extension| extension == "yml") {
                let name = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string()
                    .replace('\\', "/");
                files.push((name, fs::read_to_string(&path).unwrap()));
            }
        }
    }
    assert!(
        files.len() >= 6,
        "found only {} workflow files",
        files.len()
    );
    files
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A third-party action is code that runs with the job's token: a tag can
/// be moved to other code after review, a commit cannot.
#[test]
fn every_action_is_pinned_to_a_commit() {
    let mut scanned = 0;
    let mut floating = Vec::new();
    for (file, text) in workflows() {
        for (index, line) in text.lines().enumerate() {
            let Some(reference) = line.trim().trim_start_matches("- ").strip_prefix("uses:") else {
                continue;
            };
            let reference = reference.split('#').next().unwrap_or("").trim();
            scanned += 1;
            if reference.starts_with("./") {
                continue;
            }
            let pinned = reference.rsplit_once('@').is_some_and(|(_, commit)| {
                commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
            if !pinned {
                floating.push(format!("{file}:{}: {reference}", index + 1));
            }
        }
    }
    assert!(scanned > 30, "scanned only {scanned} `uses:` lines");
    assert!(
        floating.is_empty(),
        "pin each action to a full commit SHA, with its version in a comment:\n{}",
        floating.join("\n")
    );
}

/// A tool `taiki-e/install-action` installs without a version is whatever
/// was published last, run in a job holding a token.
#[test]
fn every_installed_tool_names_its_version() {
    let mut scanned = 0;
    let mut floating = Vec::new();
    for (file, text) in workflows() {
        for (index, line) in text.lines().enumerate() {
            let Some(tools) = line.trim().strip_prefix("tool:") else {
                continue;
            };
            scanned += 1;
            for tool in tools.split(',').map(str::trim) {
                if !tool.contains('@') {
                    floating.push(format!("{file}:{}: {tool}", index + 1));
                }
            }
        }
    }
    assert!(scanned >= 5, "scanned only {scanned} `tool:` lines");
    assert!(
        floating.is_empty(),
        "give each tool a version (`name@x.y.z`):\n{}",
        floating.join("\n")
    );
}

/// `${{ }}` inside a `run:` script is pasted into the shell before it
/// runs, so a value that contains shell is executed. Through `env:` it is
/// a variable, and stays one.
#[test]
fn no_expression_is_pasted_into_a_script() {
    let mut scripts = 0;
    let mut pasted = Vec::new();
    for (file, text) in workflows() {
        let mut script_indent: Option<usize> = None;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if let Some(level) = script_indent {
                if trimmed.is_empty() || indent(line) > level {
                    if line.contains("${{") {
                        pasted.push(format!("{file}:{}: {}", index + 1, line.trim()));
                    }
                    continue;
                }
                script_indent = None;
            }
            if let Some(command) = trimmed.trim_start_matches("- ").strip_prefix("run:") {
                scripts += 1;
                let command = command.trim();
                if command.contains("${{") {
                    pasted.push(format!("{file}:{}: {}", index + 1, line.trim()));
                }
                if command.starts_with('|') || command.starts_with('>') {
                    script_indent = Some(indent(line));
                }
            }
        }
    }
    assert!(scripts > 50, "scanned only {scripts} `run:` steps");
    assert!(
        pasted.is_empty(),
        "pass each value through the step's `env:` and read it as a variable:\n{}",
        pasted.join("\n")
    );
}

/// The jobs that push are the only ones whose checkout keeps the token in
/// `.git/config`, where every later step of the job can read it.
#[test]
fn only_a_job_that_pushes_keeps_its_credentials() {
    const PUSHING: [(&str, &str); 2] = [
        (".github/workflows/release.yml", "propose"),
        (".github/workflows/release.yml", "tag"),
    ];
    let mut checkouts = 0;
    let mut kept = Vec::new();
    for (file, text) in workflows() {
        let lines: Vec<&str> = text.lines().collect();
        let mut job = "";
        for (index, line) in lines.iter().enumerate() {
            if indent(line) == 2
                && let Some(name) = line.trim().strip_suffix(':')
            {
                job = name;
            }
            if !line.contains("uses: actions/checkout@") {
                continue;
            }
            checkouts += 1;
            let step = indent(line) + 2;
            let dropped = lines[index + 1..]
                .iter()
                .take_while(|next| next.trim().is_empty() || indent(next) >= step)
                .any(|next| next.trim() == "persist-credentials: false");
            let pushes = PUSHING.contains(&(file.as_str(), job));
            if !dropped && !pushes {
                kept.push(format!("{file}:{} (job `{job}`)", index + 1));
            }
        }
    }
    assert!(checkouts > 15, "scanned only {checkouts} checkouts");
    assert!(
        kept.is_empty(),
        "add `persist-credentials: false` to each checkout that does not push:\n{}",
        kept.join("\n")
    );
}

/// The release grants nothing by default, builds from source alone, holds
/// its signing key in a protected environment, and never replaces an asset
/// already published.
#[test]
fn the_release_workflow_holds_least_and_signs_what_it_publishes() {
    let release =
        fs::read_to_string(uze_testkit::workspace_root().join(".github/workflows/release.yml"))
            .unwrap();
    assert!(
        release.lines().any(|line| line == "permissions: {}"),
        "release.yml must grant nothing at the top level"
    );
    let package = release
        .split("\n  package:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n  publish:\n").next())
        .expect("a `package` job before `publish`");
    assert!(
        !package.contains("rust-cache"),
        "a shipped binary is built without a cache other runs wrote"
    );
    assert!(package.contains("contents: read"));
    let publish = release
        .split("\n  publish:\n")
        .nth(1)
        .expect("a `publish` job");
    assert!(publish.contains("environment: release"));
    assert!(publish.contains("secrets.UZE_RELEASE_SIGNING_KEY"));
    assert!(publish.contains("ssh-keygen -q -Y sign -n uze-release"));
    assert!(!release.contains("--clobber"));
    assert!(
        release.contains("app_id=${ACTIONS_APP_ID}")
            && release.contains("ACTIONS_APP_ID: \"15368\""),
        "only GitHub Actions' own Gate may release a commit"
    );
}

/// What GitHub enforces on `main` and on tags is versioned here; a
/// ruleset committed disabled is a ruleset nobody turns on.
#[test]
fn the_versioned_rulesets_are_meant_to_be_enforced() {
    let root = uze_testkit::workspace_root().join(".github/rulesets");
    let mut found = 0;
    for entry in fs::read_dir(&root).unwrap().filter_map(Result::ok) {
        let text = fs::read_to_string(entry.path()).unwrap();
        found += 1;
        assert!(
            text.contains("\"enforcement\": \"active\""),
            "{} is not active",
            entry.path().display()
        );
    }
    assert!(found >= 2);
    let main = fs::read_to_string(root.join("main.json")).unwrap();
    assert!(
        main.contains("\"integration_id\": 15368"),
        "the required Gate must come from GitHub Actions"
    );
}
