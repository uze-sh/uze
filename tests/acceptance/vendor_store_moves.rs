//! Acceptance: an install an earlier release made moves to the vendor
//! store without losing a plugin.
//!
//! An earlier release registered Claude's and Codex's `uze-store` under the
//! runtime tree, and installed packages shipping their own envelope through
//! a second marketplace, `uze-local`. The next delivery moves the one and
//! retires the other, against the fake vendor CLIs, which behave as each
//! vendor was measured to (`experiments/{claude,codex}/source-tie.py`).

use std::path::{Path, PathBuf};

use uze_testkit::fixtures;
use uze_testkit::temp::TestEnvironment;

use crate::util::{install_fake_harnesses, uze_bin};

/// A machine with `store-tie` installed for every harness.
fn installed() -> (TestEnvironment, Vec<uze_testkit::fake_harness::FakeHarness>) {
    let env = TestEnvironment::isolated();
    let harnesses = install_fake_harnesses(&env);
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
    (env, harnesses)
}

fn fake_state(env: &TestEnvironment, vendor: &str) -> PathBuf {
    env.root().join("fake-state").join(vendor)
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The marketplaces a fake vendor CLI has registered, as `(name, root)`.
fn registered(env: &TestEnvironment, vendor: &str) -> Vec<(String, String)> {
    lines(&fake_state(env, vendor).join("marketplaces"))
        .into_iter()
        .filter_map(|line| {
            let (name, root) = line.split_once('\t')?;
            Some((name.to_owned(), root.to_owned()))
        })
        .collect()
}

fn set_registered(env: &TestEnvironment, vendor: &str, marketplaces: &[(String, String)]) {
    let text: String = marketplaces
        .iter()
        .map(|(name, root)| format!("{name}\t{root}\n"))
        .collect();
    std::fs::write(fake_state(env, vendor).join("marketplaces"), text).unwrap();
}

fn ledger(env: &TestEnvironment) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(env.uze_home.join("state/attachments.json")).unwrap())
        .unwrap()
}

/// Rewrites the receipt ledger as an earlier release left it: `change`
/// sees every receipt, and one it answers `false` for is dropped.
fn rewrite_ledger(env: &TestEnvironment, change: impl Fn(&mut serde_json::Value) -> bool) {
    let mut ledger = ledger(env);
    let receipts = ledger["receipts"].as_array_mut().unwrap();
    receipts.retain_mut(|receipt| change(receipt));
    std::fs::write(
        env.uze_home.join("state/attachments.json"),
        serde_json::to_vec_pretty(&ledger).unwrap(),
    )
    .unwrap();
}

/// An earlier release had no builds: dropping their receipts makes the next
/// delivery render every build, as it does on the first run after upgrade.
fn forget_builds(receipt: &mut serde_json::Value) -> bool {
    receipt["resource_identity"] != "build"
}

fn invocations(env: &TestEnvironment) -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(env.fake_bin.join(".invocations")) {
        for entry in entries.flatten() {
            found.extend(lines(&entry.path()));
        }
    }
    found
}

/// Codex refuses a same-name `add` from another root, so the move is a
/// `remove` then an `add`: the plugin is never uninstalled, and stays
/// installed and enabled through it.
#[test]
fn a_codex_store_registered_under_the_runtime_tree_is_repointed_not_reinstalled() {
    let (env, _harnesses) = installed();
    let store = env.uze_home.join("store/codex");
    let earlier = env.uze_home.join("runtime/attachments/codex/generated");
    std::fs::create_dir_all(&earlier).unwrap();
    set_registered(
        &env,
        "codex",
        &[("uze-store".to_owned(), earlier.display().to_string())],
    );
    rewrite_ledger(&env, forget_builds);

    env.run_ok(uze_bin(), &["setup"]);

    assert_eq!(
        registered(&env, "codex"),
        [("uze-store".to_owned(), store.display().to_string())]
    );
    let installed = lines(&fake_state(&env, "codex").join("installed"));
    assert!(
        installed.contains(&"store-tie@uze-store".to_owned()),
        "{installed:?}"
    );
    assert!(
        !invocations(&env)
            .iter()
            .any(|line| line.contains("plugin remove store-tie")),
        "the plugin was repointed, never uninstalled: {:?}",
        invocations(&env)
    );
}

/// Claude takes a same-name `add` from another root as the new root.
#[test]
fn a_claude_store_registered_under_the_runtime_tree_is_repointed_by_a_same_name_add() {
    let (env, _harnesses) = installed();
    let store = env.uze_home.join("store/claude");
    let earlier = env.uze_home.join("runtime/attachments/claude/generated");
    std::fs::create_dir_all(&earlier).unwrap();
    set_registered(
        &env,
        "claude",
        &[("uze-store".to_owned(), earlier.display().to_string())],
    );
    rewrite_ledger(&env, forget_builds);

    env.run_ok(uze_bin(), &["setup"]);

    assert_eq!(
        registered(&env, "claude"),
        [("uze-store".to_owned(), store.display().to_string())]
    );
    assert!(
        !invocations(&env)
            .iter()
            .any(|line| line.contains("plugin marketplace remove")),
        "Claude needs no remove to move a marketplace"
    );
}

/// A move whose `add` Codex refuses registers the earlier root again, so
/// the plugins it served keep working, and says so.
#[test]
fn a_codex_repoint_whose_add_fails_registers_the_earlier_root_again() {
    let (env, _harnesses) = installed();
    let store = env.uze_home.join("store/codex");
    let earlier = env.uze_home.join("runtime/attachments/codex/generated");
    std::fs::create_dir_all(&earlier).unwrap();
    set_registered(
        &env,
        "codex",
        &[("uze-store".to_owned(), earlier.display().to_string())],
    );
    rewrite_ledger(&env, forget_builds);
    std::fs::write(
        fake_state(&env, "codex").join("refuse-add"),
        format!("{}\n", store.display()),
    )
    .unwrap();

    let output = env.run(uze_bin(), &["setup"]);

    assert_eq!(
        registered(&env, "codex"),
        [("uze-store".to_owned(), earlier.display().to_string())],
        "the earlier root is registered again"
    );
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(said.contains("registered at"), "{said}");
}

/// The marketplace earlier releases installed author envelopes through,
/// as they left it: the plugin receipt names it, the vendor holds the
/// plugin under it, and `uze-local` is registered beside `uze-store`.
fn as_installed_through_uze_local(
    env: &TestEnvironment,
    vendor: &str,
    integration: &str,
) -> PathBuf {
    let local = env
        .uze_home
        .join("runtime/attachments")
        .join(vendor)
        .join("explicit");
    let catalogue = if vendor == "codex" {
        local.join(".agents/plugins/marketplace.json")
    } else {
        local.join(".claude-plugin/marketplace.json")
    };
    std::fs::create_dir_all(catalogue.parent().unwrap()).unwrap();
    std::fs::write(
        &catalogue,
        r#"{"name":"uze-local","plugins":[{"name":"store-tie","source":{"source":"local","path":"./store-tie"}}]}"#,
    )
    .unwrap();
    let mut marketplaces = registered(env, vendor);
    marketplaces.push(("uze-local".to_owned(), local.display().to_string()));
    set_registered(env, vendor, &marketplaces);
    let installed = fake_state(env, vendor).join("installed");
    let held: String = lines(&installed)
        .into_iter()
        .map(|selector| {
            if selector == "store-tie@uze-store" {
                "store-tie@uze-local\n".to_owned()
            } else {
                format!("{selector}\n")
            }
        })
        .collect();
    std::fs::write(&installed, held).unwrap();
    let mirror = local.join("store-tie");
    let local_text = local.display().to_string();
    let mirror_text = mirror.display().to_string();
    let integration = integration.to_owned();
    rewrite_ledger(env, move |receipt| {
        if receipt["integration"] == integration.as_str()
            && receipt["resource_identity"].is_null()
            && receipt["package_id"] == "store-tie@test"
        {
            let owned = &mut receipt["artifact"]["INTEGRATION_OWNED"];
            owned["selector"] = serde_json::json!("store-tie@uze-local");
            owned["marketplace_root"] = serde_json::json!(local_text);
            owned["package_root"] = serde_json::json!(mirror_text);
        }
        true
    });
    local
}

/// Each plugin an earlier release installed through `uze-local` is
/// retired and installed again through `uze-store`, and `uze-local` goes
/// once no receipt names it. Enablement is not carried.
#[test]
fn a_plugin_installed_through_uze_local_moves_to_uze_store_and_uze_local_goes() {
    let (env, _harnesses) = installed();
    as_installed_through_uze_local(&env, "codex", "codex");

    env.run_ok(uze_bin(), &["setup"]);

    let installed = lines(&fake_state(&env, "codex").join("installed"));
    assert!(
        installed.contains(&"store-tie@uze-store".to_owned()),
        "{installed:?}"
    );
    assert!(
        !installed.contains(&"store-tie@uze-local".to_owned()),
        "{installed:?}"
    );
    assert!(
        !registered(&env, "codex")
            .iter()
            .any(|(name, _)| name == "uze-local"),
        "uze-local is unregistered once no receipt names it"
    );
    let ledger = ledger(&env).to_string();
    assert!(!ledger.contains("@uze-local"), "{ledger}");
    assert!(
        !env.uze_home
            .join("runtime/attachments/codex/explicit")
            .exists(),
        "the root uze-local was registered at goes once nothing names it"
    );
}

/// A Codex plugin the operator disabled under `uze-local` inspects as
/// drifted: it is left as it is and said, never installed a second time.
#[test]
fn a_disabled_codex_plugin_under_uze_local_is_left_and_reported() {
    let (env, _harnesses) = installed();
    as_installed_through_uze_local(&env, "codex", "codex");
    std::fs::write(
        fake_state(&env, "codex").join("disabled"),
        "store-tie@uze-local\n",
    )
    .unwrap();

    let output = env.run(uze_bin(), &["setup"]);

    let installed = lines(&fake_state(&env, "codex").join("installed"));
    assert!(
        installed.contains(&"store-tie@uze-local".to_owned()),
        "{installed:?}"
    );
    assert!(
        !installed.contains(&"store-tie@uze-store".to_owned()),
        "never installed beside itself: {installed:?}"
    );
    assert!(output.status.success());
    // Said where a delivery that changed since it was made is said.
    let doctor = env.run(uze_bin(), &["doctor"]);
    let said = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        said.contains("store-tie") && said.to_lowercase().contains("drift"),
        "{said}"
    );
}
