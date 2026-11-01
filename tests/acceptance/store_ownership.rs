//! Acceptance: a harness reads only its own vendor store.
//!
//! Everything a harness is pointed at — a marketplace source, a package root
//! named by `${PLUGIN_ROOT}`, a hook wrapper, the rules engine — lives in
//! `$UZE_HOME/store/<vendor>/`, so deleting any other part of the home
//! leaves every harness working. Proven here by walking what the harnesses
//! hold after a real install, not by asking UZE what it wrote.

use std::path::{Path, PathBuf};

use uze_testkit::fixtures;
use uze_testkit::temp::TestEnvironment;

use crate::util::{install_fake_harnesses, uze_bin};

/// The vendor whose territory `relative` lies in, read from the path the
/// way a person would: by the harness directory it sits under.
fn owning_vendor(relative: &Path) -> Option<&'static str> {
    let text = relative.to_string_lossy();
    [
        ("claude", "claude"),
        ("codex", "codex"),
        (".agents", "codex"),
        ("opencode", "opencode"),
        ("gemini", "antigravity"),
        ("antigravity", "antigravity"),
    ]
    .into_iter()
    .find(|(marker, _)| text.contains(marker))
    .map(|(_, vendor)| vendor)
}

/// Scratch directories a reader recreates on demand: the one declared
/// exception to "a harness reads only its store".
fn is_on_demand_scratch(path: &str, uze_home: &str) -> bool {
    ["runtime/sessions", "runtime/rules/sessions"]
        .iter()
        .any(|scratch| path.starts_with(&format!("{uze_home}/{scratch}")))
}

fn files_under(root: &Path, skip: &[PathBuf], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if skip.iter().any(|skipped| path.starts_with(skipped)) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            files_under(&path, skip, out);
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

/// Every `$UZE_HOME` path a harness file names, with the file that names it.
pub(crate) fn uze_home_references(env: &TestEnvironment) -> Vec<(PathBuf, String)> {
    let uze_home = env.uze_home.to_string_lossy().into_owned();
    let skip = [
        env.uze_home.clone(),
        env.project.clone(),
        // The fake CLIs' own invocation log: a transcript, not configuration.
        env.fake_bin.join(".invocations"),
    ];
    let mut files = Vec::new();
    files_under(env.root(), &skip, &mut files);
    let mut found = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let mut rest = text.as_str();
        while let Some(at) = rest.find(&uze_home) {
            let tail = &rest[at..];
            let end = tail
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ')' | ']'))
                .unwrap_or(tail.len());
            found.push((file.clone(), tail[..end].to_owned()));
            rest = &tail[end..];
        }
    }
    found
}

#[test]
fn no_harness_artifact_names_outside_its_store() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    env.run(uze_bin(), &["setup"]);
    let (market, add) = uze_testkit::marketplace::marketplace_add_args(
        env.root(),
        &fixtures::canonical("store-tie"),
    );
    env.run_ok(
        uze_bin(),
        &market.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    env.run_ok(
        uze_bin(),
        &add.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let uze_home = env.uze_home.to_string_lossy().into_owned();
    let references = uze_home_references(&env);
    assert!(
        !references.is_empty(),
        "the install pointed no harness at the UZE home; the walk proves nothing"
    );
    let strays: Vec<_> = references
        .into_iter()
        .filter(|(file, path)| {
            if is_on_demand_scratch(path, &uze_home) {
                return false;
            }
            let relative = file.strip_prefix(env.root()).unwrap_or(file);
            // The store itself is the marketplace a harness registers, so
            // its root counts as inside it as much as a build does.
            match owning_vendor(relative) {
                Some(vendor) => {
                    let store = format!("{uze_home}/store/{vendor}");
                    path != &store && !path.starts_with(&format!("{store}/"))
                }
                None => true,
            }
        })
        .collect();
    assert!(
        strays.is_empty(),
        "harness files that point outside their own vendor store:\n{}",
        strays
            .iter()
            .map(|(file, path)| format!("  {} -> {path}", file.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
