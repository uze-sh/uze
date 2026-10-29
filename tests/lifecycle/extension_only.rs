//! A plugin that carries only workspace extensions is UZE's own: stored,
//! found by the workspace, and never delivered to a harness — on install,
//! update and remove — while one that also carries skills is delivered as
//! before. What an earlier build delivered of one is taken back off
//! through its receipts, and only through them.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use uze_application::{
    UzeApplication,
    application::{AddPluginReport, PackageKind, RemovePluginReport, UpdatePluginReport},
};
use uze_core::{
    PackageSource, Result, UzeHome,
    capability::Resource,
    exposure::{ExposureMechanism, ExposurePlan, ManagedArtifact, PackageExposurePlan},
    integration::{AttachmentReceipt, HarnessDetection, IntegrationPort},
    router::{CompatibilityRoute, HarnessCapabilities},
    state,
    store::StoredPackage,
    trust::AlwaysTrust,
};
use uze_testkit::temp::TempDir;

/// A harness that would take any package as a plugin it generates, as one
/// real harness does: a file per package, counted.
struct EveryPackageAPlugin {
    root: PathBuf,
    attached: Arc<AtomicUsize>,
}

impl EveryPackageAPlugin {
    fn plugin_file(&self, package: &StoredPackage) -> PathBuf {
        self.root
            .join(format!("{}.plugin", package.id.plugin_name()))
    }

    /// The receipt an earlier build recorded, with the plugin it wrote.
    fn delivered_by_an_earlier_build(&self, home: &UzeHome, package: &StoredPackage) {
        let receipt = self.receipt(package);
        receipt.artifact.attach_standard().unwrap();
        state::record_receipt(home, receipt).unwrap();
    }

    fn receipt(&self, package: &StoredPackage) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: package.id.as_str().to_owned(),
            resource_identity: None,
            integration: self.id().to_owned(),
            artifact: ManagedArtifact::GeneratedFile {
                path: self.plugin_file(package),
                content: format!("plugin {}", package.id.as_str()),
            },
        }
    }
}

impl IntegrationPort for EveryPackageAPlugin {
    fn id(&self) -> &'static str {
        "every-package"
    }
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }
    fn detect(&self) -> HarnessDetection {
        HarnessDetection {
            present: true,
            version: None,
        }
    }
    fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
        ExposurePlan {
            route: CompatibilityRoute::Unsupported,
            mechanism: ExposureMechanism::Unsupported {
                rationale: "delivered in the plugin".to_owned(),
            },
            evidence: "test".to_owned(),
        }
    }
    fn package_exposure_plan(
        &self,
        package: &StoredPackage,
        resources: &[&Resource],
    ) -> Option<PackageExposurePlan> {
        Some(PackageExposurePlan {
            package_id: package.id.clone(),
            route: CompatibilityRoute::Native,
            envelope: uze_core::exposure::PackageEnvelope::Generated,
            provided_resource_identities: resources.iter().map(|r| r.identity()).collect(),
            evidence: "a generated plugin".to_owned(),
        })
    }
    fn attach_package(
        &self,
        package: &StoredPackage,
        _plan: &PackageExposurePlan,
    ) -> Result<Option<AttachmentReceipt>> {
        self.attached.fetch_add(1, Ordering::SeqCst);
        let receipt = self.receipt(package);
        receipt.artifact.attach_standard()?;
        Ok(Some(receipt))
    }
}

struct World {
    _scratch: TempDir,
    home: UzeHome,
    harness_root: PathBuf,
    attached: Arc<AtomicUsize>,
    app: UzeApplication,
}

fn world(label: &str) -> World {
    let scratch = TempDir::new(label);
    let home = UzeHome::at(scratch.join("home"));
    let harness_root = scratch.join("harness");
    fs::create_dir_all(&harness_root).unwrap();
    let attached = Arc::new(AtomicUsize::new(0));
    let app = UzeApplication::new(
        home.clone(),
        vec![Box::new(EveryPackageAPlugin {
            root: harness_root.clone(),
            attached: attached.clone(),
        })],
    );
    World {
        _scratch: scratch,
        home,
        harness_root,
        attached,
        app,
    }
}

impl World {
    fn harness(&self) -> EveryPackageAPlugin {
        EveryPackageAPlugin {
            root: self.harness_root.clone(),
            attached: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn install(&self, source: &Path) -> AddPluginReport {
        self.app
            .plugins()
            .add(PackageSource::local(source), &AlwaysTrust)
            .expect("the plugin installs")
    }

    fn package(&self, name: &str) -> StoredPackage {
        self.stored(name).expect("it is installed")
    }

    fn stored(&self, name: &str) -> Option<StoredPackage> {
        uze_core::UzeStore::new(self.home.clone())
            .packages()
            .unwrap()
            .into_iter()
            .find(|package| package.id.plugin_name() == name)
    }

    fn receipts(&self) -> Vec<AttachmentReceipt> {
        state::receipts(&self.home, None).unwrap()
    }

    fn harness_files(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.harness_root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect()
    }
}

/// The `focus` example: one extension, nothing else.
fn extension_only() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/focus")
}

#[test]
fn a_plugin_of_extensions_only_is_stored_and_found_but_reaches_no_harness() {
    let world = world("extension-only-install");

    let report = world.install(&extension_only());

    assert_eq!(report.plugin.id, "focus@local");
    assert_eq!(report.plugin.kind, PackageKind::Extension);
    assert!(report.deliveries.is_empty(), "{:?}", report.deliveries);
    assert!(report.attachments.is_empty());
    assert_eq!(world.attached.load(Ordering::SeqCst), 0);
    assert!(world.receipts().is_empty(), "{:?}", world.receipts());
    assert!(world.harness_files().is_empty());
    assert!(
        world
            .app
            .extensions()
            .installed()
            .iter()
            .any(|extension| extension.identity.id == "focus"),
        "the workspace still finds its extension"
    );
    assert!(
        world
            .app
            .plugins()
            .inspect("focus")
            .unwrap()
            .deliveries
            .is_empty(),
        "no harness is said to receive it"
    );
}

#[test]
fn update_and_remove_reach_no_harness_either() {
    let world = world("extension-only-update-remove");
    world.install(&extension_only());

    let updated = world
        .app
        .plugins()
        .update("focus", &AlwaysTrust)
        .expect("it updates");
    assert!(
        matches!(&updated, UpdatePluginReport::Updated { attachments, .. } if attachments.is_empty()),
        "{updated:?}"
    );
    assert_eq!(world.attached.load(Ordering::SeqCst), 0);
    assert!(world.receipts().is_empty());

    let removed = world.app.plugins().remove("focus").expect("it is removed");
    assert!(
        matches!(
            removed,
            RemovePluginReport::Removed {
                kind: PackageKind::Extension,
                ..
            }
        ),
        "{removed:?}"
    );
    assert!(world.stored("focus").is_none());
}

/// A plugin of extensions an earlier build delivered as a harness plugin:
/// the next update takes that plugin off through its receipt, and does not
/// put it back.
#[test]
fn an_update_takes_off_what_an_earlier_build_delivered() {
    let world = world("extension-only-upgrade-update");
    world.install(&extension_only());
    let package = world.package("focus");
    world
        .harness()
        .delivered_by_an_earlier_build(&world.home, &package);
    assert_eq!(world.harness_files().len(), 1);

    let updated = world.app.plugins().update("focus", &AlwaysTrust).unwrap();

    assert!(
        matches!(updated, UpdatePluginReport::Updated { .. }),
        "{updated:?}"
    );
    assert!(world.receipts().is_empty(), "{:?}", world.receipts());
    assert!(world.harness_files().is_empty());
    assert_eq!(world.attached.load(Ordering::SeqCst), 0);
}

#[test]
fn a_removal_takes_off_what_an_earlier_build_delivered() {
    let world = world("extension-only-upgrade-remove");
    world.install(&extension_only());
    let package = world.package("focus");
    world
        .harness()
        .delivered_by_an_earlier_build(&world.home, &package);

    let removed = world.app.plugins().remove("focus").unwrap();

    assert!(
        matches!(removed, RemovePluginReport::Removed { .. }),
        "{removed:?}"
    );
    assert!(world.receipts().is_empty());
    assert!(world.harness_files().is_empty());
}

/// Installing again — or `uze setup` delivering every stored package —
/// takes it off too, never anything its receipt does not own: a plugin
/// someone edited since stays, with its receipt.
#[test]
fn delivering_again_takes_off_only_what_still_matches_its_receipt() {
    let world = world("extension-only-upgrade-reinstall");
    world.install(&extension_only());
    let package = world.package("focus");
    let harness = world.harness();
    harness.delivered_by_an_earlier_build(&world.home, &package);

    world.install(&extension_only());
    assert!(world.receipts().is_empty());
    assert!(world.harness_files().is_empty());

    harness.delivered_by_an_earlier_build(&world.home, &package);
    let edited = harness.plugin_file(&package);
    fs::write(&edited, "edited by someone").unwrap();
    world.install(&extension_only());
    assert_eq!(
        world.receipts().len(),
        1,
        "the edited plugin keeps its receipt"
    );
    assert_eq!(fs::read_to_string(&edited).unwrap(), "edited by someone");
}

/// A plugin that carries a skill beside its extension is delivered as any
/// other.
#[test]
fn a_plugin_with_skills_beside_its_extensions_is_still_delivered() {
    let world = world("extension-and-skills");
    let source = world._scratch.join("both");
    uze_testkit::fixtures::copy_tree(&uze_testkit::fixtures::canonical("skill-plugin"), &source);
    uze_testkit::fixtures::copy_tree(
        &extension_only().join("extensions"),
        &source.join("extensions"),
    );

    let report = world.install(&source);

    assert_eq!(report.plugin.kind, PackageKind::Plugin);
    assert_eq!(report.deliveries.len(), 1);
    assert_eq!(world.attached.load(Ordering::SeqCst), 1);
    assert_eq!(world.receipts().len(), 1);
}
