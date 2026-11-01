use std::fs;

use uze_core::{
    home::UzeHome,
    integration::{AttachmentReceipt, AttachmentState, BuildInputs, ManagedArtifact},
    store::{PackageId, StoredPackage},
};

use super::*;

const STORE: VendorStore = VendorStore {
    vendor: "test",
    dialect: 1,
    build_name: package_id_name,
    carried: &[],
};

struct Machine {
    _scratch: uze_testkit::temp::TempDir,
    home: UzeHome,
    package: StoredPackage,
}

/// A home holding one package's sources, `guides/plan.md` among them.
fn machine(label: &str) -> Machine {
    let scratch = uze_testkit::temp::TempDir::new(label);
    let home = UzeHome::at(scratch.path().join("uze"));
    let id = PackageId::from_marketplace_plugin("market", "flow", &home.plugins_dir()).unwrap();
    let root = home.plugin_dir(&id);
    fs::create_dir_all(root.join("guides")).unwrap();
    fs::write(root.join("plugin.json"), r#"{"name":"flow"}"#).unwrap();
    fs::write(root.join("guides/plan.md"), "plan").unwrap();
    let package = StoredPackage {
        active_name: "flow".to_owned(),
        manifest: root.join("plugin.json"),
        provenance: uze_core::acquisition::Provenance {
            requested: uze_core::acquisition::PackageSource::Local { path: root.clone() },
            resolved: uze_core::acquisition::ResolvedSource::Local { path: root.clone() },
        },
        root,
        id,
    };
    Machine {
        _scratch: scratch,
        home,
        package,
    }
}

/// Publishes the build as a delivery does, counting each render.
fn publish_with(
    machine: &Machine,
    store: &VendorStore,
    digest: &str,
    recorded: Option<&AttachmentReceipt>,
    renders: &mut usize,
) -> AttachmentReceipt {
    let inputs = BuildInputs {
        digest,
        linked: false,
    };
    publish(
        Publication::new(
            store,
            &machine.home,
            "test",
            &machine.package,
            &inputs,
            recorded,
        ),
        |dir, _root| {
            *renders += 1;
            mirror_package(&machine.package, dir, &[], &[])?;
            Ok(Some(Origin::Generated))
        },
    )
    .unwrap()
}

#[test]
fn a_build_is_rendered_into_the_vendor_store_and_inspected_by_content() {
    let machine = machine("build-render");
    let mut renders = 0;
    let receipt = publish_with(&machine, &STORE, "one", None, &mut renders);
    let build = machine.home.build_dir("test", "flow@market");
    assert_eq!(renders, 1);
    assert_eq!(
        fs::read_to_string(build.join("guides/plan.md")).unwrap(),
        "plan"
    );
    assert_eq!(receipt.resource_identity.as_deref(), Some(BUILD_IDENTITY));
    let recorded = RecordedBuild::of(&receipt).unwrap();
    assert_eq!(recorded.root, build);
    assert_eq!(recorded.origin, Some(Origin::Generated));
    assert_eq!(recorded.source, Some(BuildSource::Pinned));
    assert!(recorded.files.contains_key("guides/plan.md"));
    assert_eq!(inspect(&receipt).state, AttachmentState::Matched);
    assert_eq!(
        STORE.root_of(&machine.package.root).path(),
        build,
        "`${{PLUGIN_ROOT}}` names the build, never the sources"
    );
}

#[test]
fn only_a_changed_key_renders_again_and_a_file_a_hook_added_survives() {
    let machine = machine("build-key");
    let mut renders = 0;
    let first = publish_with(&machine, &STORE, "one", None, &mut renders);
    let build = machine.home.build_dir("test", "flow@market");
    fs::create_dir_all(build.join("node_modules")).unwrap();
    fs::write(build.join("node_modules/dep.js"), "built").unwrap();
    assert_eq!(
        inspect(&first).state,
        AttachmentState::Matched,
        "a file a hook added is not drift"
    );

    let again = publish_with(&machine, &STORE, "one", Some(&first), &mut renders);
    assert_eq!(renders, 1, "an unchanged key is not rendered again");
    assert_eq!(again, first);
    assert!(build.join("node_modules/dep.js").is_file());

    let changed = publish_with(&machine, &STORE, "two", Some(&first), &mut renders);
    assert_eq!(renders, 2, "a changed package is rendered again");
    assert_ne!(changed, first);
    assert!(
        !build.join("node_modules").exists(),
        "a changed package replaces its build whole"
    );
}

#[test]
fn an_upgrade_of_uze_or_of_the_rendering_renders_again() {
    let machine = machine("build-upgrade");
    let mut renders = 0;
    let mut first = publish_with(&machine, &STORE, "one", None, &mut renders);
    // As an earlier UZE recorded it.
    if let ManagedArtifact::IntegrationOwned { detail, .. } = &mut first.artifact {
        detail.get_mut("key").unwrap()["uze"] = serde_json::json!("0.0.1");
    }
    publish_with(&machine, &STORE, "one", Some(&first), &mut renders);
    assert_eq!(
        renders, 2,
        "a build an earlier UZE rendered is rendered again"
    );

    let current = publish_with(&machine, &STORE, "one", None, &mut renders);
    let rendering_changed = VendorStore {
        dialect: 2,
        ..STORE
    };
    publish_with(
        &machine,
        &rendering_changed,
        "one",
        Some(&current),
        &mut renders,
    );
    assert_eq!(
        renders, 4,
        "a harness's rendering that changed renders again"
    );
}

#[test]
fn an_edited_build_is_drift_and_is_kept_on_update() {
    let machine = machine("build-edited");
    let mut renders = 0;
    let first = publish_with(&machine, &STORE, "one", None, &mut renders);
    let build = machine.home.build_dir("test", "flow@market");
    fs::write(build.join("guides/plan.md"), "edited by hand").unwrap();
    let inspection = inspect(&first);
    assert_eq!(inspection.state, AttachmentState::Drifted);
    assert!(
        inspection.reason.contains("guides/plan.md"),
        "the edited file is named: {}",
        inspection.reason
    );

    let kept = publish_with(&machine, &STORE, "two", Some(&first), &mut renders);
    assert_eq!(renders, 1, "an edited build is never rendered over");
    assert_eq!(kept, first);
    assert_eq!(
        fs::read_to_string(build.join("guides/plan.md")).unwrap(),
        "edited by hand"
    );
    assert_eq!(
        detach(&first).unwrap().state,
        AttachmentState::Drifted,
        "nor removed"
    );
    assert!(build.is_dir());
}

#[test]
fn a_build_whose_bytes_are_missing_is_kept_and_not_rendered() {
    let machine = machine("build-bytes-missing");
    let mut renders = 0;
    let first = publish_with(&machine, &STORE, "one", None, &mut renders);
    fs::remove_dir_all(&machine.package.root).unwrap();
    let kept = publish_with(&machine, &STORE, "two", Some(&first), &mut renders);
    assert_eq!(renders, 1);
    assert_eq!(kept, first);
    assert_eq!(inspect(&kept).state, AttachmentState::Matched);
}

#[test]
fn a_matched_build_is_removed_and_a_missing_one_said() {
    let machine = machine("build-detach");
    let mut renders = 0;
    let receipt = publish_with(&machine, &STORE, "one", None, &mut renders);
    let build = machine.home.build_dir("test", "flow@market");
    assert_eq!(detach(&receipt).unwrap().state, AttachmentState::Missing);
    assert!(!build.exists());
    assert_eq!(inspect(&receipt).state, AttachmentState::Missing);
}

#[test]
fn a_write_into_the_build_never_reaches_the_sources() {
    let machine = machine("build-write");
    let mut renders = 0;
    publish_with(&machine, &STORE, "one", None, &mut renders);
    let build = machine.home.build_dir("test", "flow@market");
    fs::write(build.join("guides/plan.md"), "built over").unwrap();
    assert_eq!(
        fs::read_to_string(machine.package.root.join("guides/plan.md")).unwrap(),
        "plan",
        "a write into the build never reaches the sources"
    );
}
