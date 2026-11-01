//! Acceptance: a harness reads only its own vendor store.
//!
//! Everything a harness is pointed at — a marketplace source, a package root
//! named by `${PLUGIN_ROOT}`, a hook wrapper, the rules engine — lives in
//! `$UZE_HOME/store/<vendor>/`, so deleting any other part of the home
//! leaves every harness working. Proven here by walking what the harnesses
//! hold after a real install, not by asking UZE what it wrote.

use std::path::{Path, PathBuf};

use uze_testkit::fixtures;
use uze_testkit::process::native;
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
pub(crate) fn is_on_demand_scratch(path: &str, uze_home: &str) -> bool {
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
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ')' | ']' | '`' | '…')
                })
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

/// The bundled `uze` plugin is an ordinary delivery: setup attaches it to
/// every harness with receipts (its build carries the rules engine), and
/// `remove` refuses it, so only an uninstall that purges takes it off.
#[test]
fn the_bundled_plugin_is_delivered_with_receipts_and_remove_refuses_it() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    env.run_ok(uze_bin(), &["setup"]);
    let ledger = || -> Vec<serde_json::Value> {
        let text = std::fs::read(env.uze_home.join("state/attachments.json")).unwrap();
        let ledger: serde_json::Value = serde_json::from_slice(&text).unwrap();
        ledger["receipts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|receipt| receipt["package_id"] == "uze@uze")
            .cloned()
            .collect()
    };
    let before = ledger();
    for integration in ["claude-code", "codex", "opencode", "antigravity"] {
        assert!(
            before
                .iter()
                .any(|receipt| receipt["integration"] == integration
                    && receipt["resource_identity"] == "build"),
            "{integration} holds the bundled plugin's build: {before:?}"
        );
    }
    assert!(
        env.uze_home
            .join("store/claude/uze@uze/engine/rules")
            .is_file(),
        "the rules engine rides the bundled plugin's build"
    );

    let refused = env.run(uze_bin(), &["remove", "-m", "uze"]);
    assert!(
        !refused.status.success(),
        "remove refuses the bundled plugin"
    );
    assert_eq!(ledger(), before, "nothing it delivered was taken off");
}

/// OpenCode's `uze.ts` is delivered by setup on no package's account, and
/// owned by a receipt like everything else UZE delivers.
#[test]
fn opencodes_runtime_plugin_is_owned_by_a_receipt() {
    let env = TestEnvironment::isolated();
    let _harnesses = install_fake_harnesses(&env);
    env.run_ok(uze_bin(), &["setup"]);
    let text = std::fs::read_to_string(env.uze_home.join("state/attachments.json")).unwrap();
    let ledger: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(
        ledger["receipts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|receipt| {
                receipt["integration"] == "opencode"
                    && receipt["package_id"] == "setup"
                    && receipt["artifact"]["INTEGRATION_OWNED"]["kind"] == "runtime-plugin"
            }),
        "{text}"
    );
}

/// What a build names inside itself — Claude's generated manifest naming
/// `${PLUGIN_ROOT}/bin/server` above all, a path Claude would put on its
/// `PATH` were it at the plugin root — is a file the build holds.
#[test]
fn every_path_a_build_names_is_a_file_it_holds() {
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

    let claude_store = env.uze_home.join("store/claude");
    let build = std::fs::read_dir(&claude_store)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("store-tie@"))
        })
        .expect("store-tie has a Claude build");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(build.join(".claude-plugin/plugin.json")).unwrap())
            .unwrap();
    let command = manifest["mcpServers"]["store-tie"]["command"]
        .as_str()
        .unwrap();
    assert!(
        Path::new(command).is_file(),
        "Claude's MCP server names {command}, which the build does not hold"
    );
    assert!(
        !build.join("bin").exists(),
        "a `bin/` at the plugin root would land on Claude's PATH"
    );

    let uze_home = env.uze_home.to_string_lossy().into_owned();
    let mut files = Vec::new();
    files_under(&env.uze_home.join("store"), &[], &mut files);
    let dangling: Vec<_> = files
        .iter()
        .filter(|file| {
            !file
                .components()
                .any(|component| component.as_os_str() == ".uze-carried")
        })
        .flat_map(|file| {
            let text = std::fs::read_to_string(file).unwrap_or_default();
            text.match_indices(&uze_home)
                .map(|(at, _)| {
                    let tail = &text[at..];
                    let end = tail
                        .find(|c: char| {
                            c.is_whitespace()
                                || matches!(c, '"' | '\'' | ',' | ')' | ']' | '`' | '…')
                        })
                        .unwrap_or(tail.len());
                    (file.clone(), tail[..end].to_owned())
                })
                .collect::<Vec<_>>()
        })
        .filter(|(_, path)| !is_on_demand_scratch(path, &uze_home))
        .filter(|(_, path)| !Path::new(path).exists())
        .collect();
    assert!(
        dangling.is_empty(),
        "a build names what it does not hold: {dangling:?}"
    );
}

/// Every file under `root`, relative to it, with its bytes.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    files_under(root, &[], &mut files);
    let mut tree: Vec<_> = files
        .into_iter()
        .map(|file| {
            let bytes = std::fs::read(&file).unwrap_or_default();
            (file.strip_prefix(root).unwrap().to_path_buf(), bytes)
        })
        .collect();
    tree.sort();
    tree
}

/// The `delivered` cell `uze doctor` gives one harness, e.g. `✓ 9/9`.
fn delivered_cell(doctor: &str, harness: &str) -> String {
    doctor
        .lines()
        .find(|line| line.trim_start().starts_with(harness))
        .and_then(|line| {
            line.split_whitespace()
                .rev()
                .nth(1)
                .zip(line.split_whitespace().last())
        })
        .map(|(mark, count)| format!("{mark} {count}"))
        .unwrap_or_default()
}

/// The vendor store is a tier of its own: deleting one harness's stops
/// that harness's plugins — its marketplace now names a directory that is
/// gone — and leaves every other harness's alone, until the next delivery
/// renders it again, byte for byte.
#[test]
fn deleting_one_vendor_store_stops_that_harness_until_the_next_delivery() {
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
    let claude_store = env.uze_home.join(native("store/claude"));
    let codex_store = env.uze_home.join(native("store/codex"));
    let claude_before = tree(&claude_store);
    let codex_before = tree(&codex_store);
    assert!(
        claude_before
            .iter()
            .any(|(path, _)| path.starts_with("store-tie@test")),
        "the package has a Claude build to lose: {:?}",
        claude_before.iter().map(|(p, _)| p).collect::<Vec<_>>()
    );
    let doctor = |env: &TestEnvironment| {
        String::from_utf8_lossy(&env.run(uze_bin(), &["doctor"]).stdout).into_owned()
    };
    let whole = doctor(&env);

    std::fs::remove_dir_all(&claude_store).unwrap();

    let without = doctor(&env);
    let registered = std::fs::read_to_string(env.root().join("fake-state/claude/marketplaces"))
        .unwrap_or_default();
    assert!(
        registered.contains(&format!("uze-store\t{}", claude_store.display())),
        "Claude's marketplace still names the store that is gone: {registered}"
    );
    assert!(
        delivered_cell(&without, "Claude Code").starts_with('×'),
        "Claude's deliveries are gone: {without}"
    );
    for harness in ["Codex", "OpenCode", "Antigravity"] {
        assert_eq!(
            delivered_cell(&without, harness),
            delivered_cell(&whole, harness),
            "{harness} is untouched: {without}"
        );
    }
    assert_eq!(
        tree(&codex_store),
        codex_before,
        "Codex's store is untouched"
    );

    env.run_ok(uze_bin(), &["setup"]);

    assert_eq!(
        tree(&claude_store),
        claude_before,
        "the next delivery renders Claude's store again"
    );
    let again = doctor(&env);
    assert_eq!(
        delivered_cell(&again, "Claude Code"),
        delivered_cell(&whole, "Claude Code"),
        "{again}"
    );
}

/// OpenCode's hook document names the root its handlers run in, and runs
/// them there with `PLUGIN_ROOT` set to it: that root is the package's
/// build, so a hook keeps running with the sources gone.
#[test]
fn opencodes_hook_handlers_run_in_the_build() {
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

    let store = env.uze_home.join("store/opencode");
    let mut documents = Vec::new();
    files_under(&store, &[], &mut documents);
    let document = documents
        .into_iter()
        .find(|path| path.ends_with("hooks/uze-groups.json"))
        .expect("store-tie's hooks reach OpenCode as a document in its build");
    let parsed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&document).unwrap()).unwrap();
    let root = parsed["root"].as_str().unwrap();
    assert!(
        Path::new(root).starts_with(&store),
        "OpenCode runs the handlers in {root}, outside its vendor store"
    );
}

/// Codex runs no hook it has not reviewed, so the install names each UZE
/// hook it holds back and the review to do, and `doctor` says it again.
#[test]
fn the_install_and_doctor_name_each_hook_codex_holds_back() {
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

    let installed = env.run_ok(
        uze_bin(),
        &add.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let doctor = env.run(uze_bin(), &["doctor"]);

    for output in [&installed, &doctor] {
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            said.contains("guard of store-tie@test held back"),
            "the held-back hook is named: {said}"
        );
        assert!(said.contains("hook review"), "the review to do: {said}");
    }
}
