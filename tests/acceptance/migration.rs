//! Acceptance: a machine an earlier release left with its package bytes
//! under `store/` moves them to `sources/` on the next mutation, fenced
//! against that earlier release (`specs/persisted-state`, "The package
//! bytes move to their own directory, fenced against older builds").
//!
//! The earlier layout is made from a real install, taken back to how the
//! release before `sources/` left it: the bytes under `store/plugins`, a
//! package registry with no shape and no digests, a receipt ledger in the
//! shape before the fence, and no build receipts. How each vendor's
//! marketplace is moved off the runtime tree is `vendor_store_moves`'s
//! subject; this suite is about the bytes and the records.

use std::path::{Path, PathBuf};

use uze_testkit::temp::TestEnvironment;

use crate::util::uze_bin;
use crate::vendor_store_moves::{
    fake_state, forget_builds, installed, invocations, ledger, lines, registered, rewrite_ledger,
    set_registered,
};

/// The shape of each fenced document the release before `sources/` wrote,
/// and so the newest it can read.
const EARLIER_REGISTRY_SHAPE: u64 = 1;
const EARLIER_LEDGER_SHAPE: u64 = 3;

fn registry_path(env: &TestEnvironment) -> PathBuf {
    env.uze_home.join("state/packages.json")
}

fn registry(env: &TestEnvironment) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(registry_path(env)).unwrap()).unwrap()
}

fn old_bytes(env: &TestEnvironment) -> PathBuf {
    env.uze_home.join("store/plugins")
}

fn new_bytes(env: &TestEnvironment) -> PathBuf {
    env.uze_home.join("sources/plugins")
}

/// Takes an installed machine back to the layout before `sources/`: every
/// marketplace's bytes in `store/plugins`, except those named in `moved`,
/// which an interrupted move already carried.
fn as_the_earlier_release_left_it(env: &TestEnvironment, moved: &[&str]) {
    std::fs::create_dir_all(old_bytes(env)).unwrap();
    for entry in std::fs::read_dir(new_bytes(env)).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if moved.contains(&name.as_str()) {
            continue;
        }
        std::fs::rename(entry.path(), old_bytes(env).join(&name)).unwrap();
    }
    let mut registry = registry(env);
    registry.as_object_mut().unwrap().remove("schema_version");
    for entry in registry["packages"].as_object_mut().unwrap().values_mut() {
        entry.as_object_mut().unwrap().remove("digest");
    }
    std::fs::write(
        registry_path(env),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    rewrite_ledger(env, forget_builds);
    let mut ledger = ledger(env);
    ledger["schema_version"] = serde_json::json!(EARLIER_LEDGER_SHAPE);
    std::fs::write(
        env.uze_home.join("state/attachments.json"),
        serde_json::to_vec_pretty(&ledger).unwrap(),
    )
    .unwrap();
}

/// Every file under `root`, relative to it, with its bytes.
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                files.push((relative, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    files.sort();
    files
}

/// The paths whose bytes differ between two trees, or that only one holds.
fn changed(before: &[(PathBuf, Vec<u8>)], after: &[(PathBuf, Vec<u8>)]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = before
        .iter()
        .filter(|entry| !after.contains(entry))
        .chain(after.iter().filter(|entry| !before.contains(entry)))
        .map(|(path, _)| path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn directory_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Every package the registry holds has a digest recorded.
fn assert_every_package_has_a_digest(env: &TestEnvironment) {
    let registry = registry(env);
    let packages = registry["packages"].as_object().unwrap();
    assert!(!packages.is_empty(), "{registry}");
    for (id, entry) in packages {
        assert!(
            entry["digest"].is_string(),
            "{id} has no digest: {registry}"
        );
    }
}

/// A file no acquisition would write, so finding it in `sources/` says the
/// bytes there are the ones that were moved, not fetched again.
const MOVED_NOT_FETCHED: &str = "moved-not-fetched";

fn mark_old_bytes(env: &TestEnvironment, marketplace: &str) -> PathBuf {
    let marker = old_bytes(env).join(marketplace).join(MOVED_NOT_FETCHED);
    std::fs::write(&marker, "the earlier release's bytes").unwrap();
    marker
}

#[test]
fn an_upgrade_moves_the_bytes_to_sources_unchanged_and_records_every_digest() {
    let (env, _harnesses) = installed();
    as_the_earlier_release_left_it(&env, &[]);
    mark_old_bytes(&env, "test");
    let before = tree(&old_bytes(&env));
    // What is not moved cannot be fetched instead.
    std::fs::remove_dir_all(env.root().join("market")).unwrap();

    env.run_ok(uze_bin(), &["setup"]);

    assert_eq!(
        tree(&new_bytes(&env)),
        before,
        "the bytes are in sources/, exactly as they were"
    );
    let vendor_stores = ["antigravity", "claude", "codex", "opencode"];
    assert!(
        directory_names(&env.uze_home.join("store"))
            .iter()
            .all(|name| vendor_stores.contains(&name.as_str())),
        "store/ holds only vendor stores: {:?}",
        directory_names(&env.uze_home.join("store"))
    );
    assert_every_package_has_a_digest(&env);
}

#[test]
fn an_interrupted_move_is_finished_by_the_next_run_and_nothing_is_acquired_again() {
    let (env, _harnesses) = installed();
    // The bundled plugin's marketplace had moved when the move stopped;
    // the one the operator added had not.
    let marketplaces = directory_names(&new_bytes(&env));
    assert!(
        marketplaces.contains(&"uze".to_owned()) && marketplaces.contains(&"test".to_owned()),
        "two marketplaces to split the move between: {marketplaces:?}"
    );
    as_the_earlier_release_left_it(&env, &["uze"]);
    // The fence goes first, so an interrupted move has already written it.
    let mut fenced = registry(&env);
    fenced["schema_version"] = serde_json::json!(2);
    std::fs::write(
        registry_path(&env),
        serde_json::to_vec_pretty(&fenced).unwrap(),
    )
    .unwrap();
    mark_old_bytes(&env, "test");
    let moved_already = tree(&new_bytes(&env).join("uze"));
    let not_yet = tree(&old_bytes(&env).join("test"));
    // The marketplace the bytes came from is gone: what is not moved
    // cannot be fetched instead.
    std::fs::remove_dir_all(env.root().join("market")).unwrap();

    env.run_ok(uze_bin(), &["setup"]);

    assert_eq!(tree(&new_bytes(&env).join("uze")), moved_already);
    assert_eq!(tree(&new_bytes(&env).join("test")), not_yet);
    assert!(!old_bytes(&env).exists(), "nothing is left to move");
    assert_every_package_has_a_digest(&env);
}

/// An earlier release reads a document in a shape newer than its own the
/// way this build does: it refuses it, by name, and changes nothing. That
/// rule is proven here on this build against a document one shape ahead
/// of it, since the earlier release is not in this suite; what the move
/// adds is that both documents are written one shape past the earlier
/// release's, so the same rule fences it.
#[test]
fn a_build_older_than_the_documents_refuses_them_by_name_and_loses_nothing() {
    let (env, _harnesses) = installed();
    as_the_earlier_release_left_it(&env, &[]);
    env.run_ok(uze_bin(), &["setup"]);

    let registry_shape = registry(&env)["schema_version"].as_u64().unwrap_or(1);
    let ledger_shape = ledger(&env)["schema_version"].as_u64().unwrap_or(1);
    assert!(
        registry_shape > EARLIER_REGISTRY_SHAPE && ledger_shape > EARLIER_LEDGER_SHAPE,
        "both documents are past what the earlier release reads: \
         packages {registry_shape}, attachments {ledger_shape}"
    );

    for (document, file, shape) in [
        ("packages", "state/packages.json", registry_shape),
        ("attachments", "state/attachments.json", ledger_shape),
    ] {
        let path = env.uze_home.join(file);
        let written = std::fs::read(&path).unwrap();
        let mut ahead: serde_json::Value = serde_json::from_slice(&written).unwrap();
        ahead["schema_version"] = serde_json::json!(shape + 1);
        let ahead = serde_json::to_vec_pretty(&ahead).unwrap();
        std::fs::write(&path, &ahead).unwrap();
        // What an older build's own layout step leaves before it refuses.
        std::fs::create_dir_all(old_bytes(&env)).unwrap();
        let sources = tree(&env.uze_home.join("sources"));
        let records = tree(&env.uze_home.join("state"));
        let stores = tree(&env.uze_home.join("store"));

        let refused = env.run(uze_bin(), &["setup"]);

        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&refused.stdout),
            String::from_utf8_lossy(&refused.stderr)
        );
        assert!(!refused.status.success(), "{document}: {said}");
        assert!(said.contains(file), "the refusal names {file}: {said}");
        assert_eq!(std::fs::read(&path).unwrap(), ahead, "{document} untouched");
        for (tier, before) in [
            ("sources", &sources),
            ("state", &records),
            ("store", &stores),
        ] {
            // The lock records who last held it, which is not a record.
            let changed: Vec<_> = changed(before, &tree(&env.uze_home.join(tier)))
                .into_iter()
                .filter(|path| path != Path::new("mutation.lock"))
                .collect();
            assert!(
                changed.is_empty(),
                "{document}: {tier} changed: {changed:?}"
            );
        }

        std::fs::write(&path, &written).unwrap();
    }

    env.run_ok(uze_bin(), &["setup"]);
    assert!(
        !old_bytes(&env).exists(),
        "the empty directory an older build left is removed"
    );
    assert_every_package_has_a_digest(&env);
}

/// The whole earlier machine at once: bytes under `store/`, Claude's and
/// Codex's `uze-store` registered under the runtime tree, no builds. After
/// the upgrade every harness still holds every plugin, enabled, and none
/// was uninstalled on the way.
#[test]
fn an_upgraded_install_keeps_every_plugin_installed_and_enabled() {
    let (env, _harnesses) = installed();
    let held_before: Vec<(&str, Vec<String>)> = ["claude", "codex"]
        .into_iter()
        .map(|vendor| (vendor, lines(&fake_state(&env, vendor).join("installed"))))
        .collect();
    for (vendor, held) in &held_before {
        assert!(
            held.contains(&"store-tie@uze-store".to_owned()),
            "{vendor}: {held:?}"
        );
    }
    as_the_earlier_release_left_it(&env, &[]);
    for vendor in ["claude", "codex"] {
        let earlier = env
            .uze_home
            .join("runtime/attachments")
            .join(vendor)
            .join("generated");
        std::fs::create_dir_all(&earlier).unwrap();
        set_registered(
            &env,
            vendor,
            &[("uze-store".to_owned(), earlier.display().to_string())],
        );
    }

    env.run_ok(uze_bin(), &["setup"]);

    for (vendor, held) in &held_before {
        let now = lines(&fake_state(&env, vendor).join("installed"));
        for selector in held {
            assert!(now.contains(selector), "{vendor} lost {selector}: {now:?}");
        }
        let disabled = lines(&fake_state(&env, vendor).join("disabled"));
        assert!(disabled.is_empty(), "{vendor} disabled {disabled:?}");
        assert_eq!(
            registered(&env, vendor),
            [(
                "uze-store".to_owned(),
                env.uze_home
                    .join("store")
                    .join(vendor)
                    .display()
                    .to_string()
            )]
        );
    }
    let invoked = invocations(&env);
    assert!(
        !invoked
            .iter()
            .any(|line| line.contains("plugin uninstall") || line.contains("plugin remove")),
        "no plugin was uninstalled on the way: {invoked:?}"
    );
    for vendor in ["claude", "codex", "antigravity", "opencode"] {
        let builds = directory_names(&env.uze_home.join("store").join(vendor));
        assert!(
            builds.iter().any(|build| build.contains("store-tie")),
            "{vendor} has the package's build: {builds:?}"
        );
    }
}
