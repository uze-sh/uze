//! Delivery to one harness against a scripted integration: what is
//! attached, what an earlier build left is retired, what is held back,
//! and which name each capability is given.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use uze_core::{
    capability::Capability,
    exposure::{ExposurePlan, PackageEnvelope, PackageExposurePlan},
    integration::{AttachmentInspection, BuildInputs, HarnessDetection, PublicationStatus},
    router::{CompatibilityRoute, HarnessCapabilities},
};

use super::*;
use crate::application::lifecycle::effective::{PlannedDelivery, delivery_route_of};

/// What a scripted harness answers, and what it was asked, in order.
#[derive(Default)]
struct Script {
    /// What inspecting the receipt at a location answers: matched unless
    /// named here.
    states: BTreeMap<PathBuf, AttachmentState>,
    /// Locations whose detach the harness answers as still there.
    holds_on_to: Vec<PathBuf>,
    /// The artifact it plans for every resource; none is unsupported.
    plan: Option<ManagedArtifact>,
    route: Option<CompatibilityRoute>,
    /// The names it offers, first choice first; empty is the default.
    candidates: Vec<String>,
    /// Declines every capability it plans: attaches nothing for it.
    declines: bool,
    /// Refuses every capability's name as held by something else.
    name_held: bool,
    package_plan: Option<PackageExposurePlan>,
    /// Selectors of package receipts from a route this build no longer
    /// takes.
    retired_routes: Vec<String>,
    /// The builds it renders, one per call.
    builds: Vec<AttachmentReceipt>,
    linked: Vec<bool>,
    republish_fails: bool,
    unpublished: bool,
    asked: Vec<String>,
}

struct Scripted {
    id: &'static str,
    script: Arc<Mutex<Script>>,
}

impl Scripted {
    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.script.lock().unwrap()
    }
}

impl IntegrationPort for Scripted {
    fn id(&self) -> &'static str {
        self.id
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
        let script = self.script();
        match &script.plan {
            Some(artifact) => ExposurePlan {
                route: script.route.unwrap_or(CompatibilityRoute::Native),
                mechanism: ExposureMechanism::Managed(artifact.clone()),
                evidence: "scripted".to_owned(),
            },
            None => ExposurePlan {
                route: CompatibilityRoute::Unsupported,
                mechanism: ExposureMechanism::Unsupported {
                    rationale: "scripted".to_owned(),
                },
                evidence: "scripted".to_owned(),
            },
        }
    }

    fn exposure_name_candidates(&self, resource: &Resource) -> Vec<String> {
        let candidates = self.script().candidates.clone();
        if candidates.is_empty() {
            uze_core::integration::default_exposure_name_candidates(resource)
        } else {
            candidates
        }
    }

    fn attach_receipt(&self, resource: &Resource) -> Result<Option<AttachmentReceipt>> {
        let script = self.script();
        if script.name_held {
            return Err(UzeError::ManagedEntryConflict(PathBuf::from("held")));
        }
        let Some(artifact) = script.plan.clone().filter(|_| !script.declines) else {
            return Ok(None);
        };
        Ok(Some(AttachmentReceipt {
            package_id: resource.package_id.as_str().to_owned(),
            resource_identity: Some(resource.identity()),
            integration: self.id.to_owned(),
            artifact,
        }))
    }

    fn package_exposure_plan(
        &self,
        _package: &StoredPackage,
        _resources: &[&Resource],
    ) -> Option<PackageExposurePlan> {
        self.script().package_plan.clone()
    }

    fn package_receipt_serves(&self, receipt: &AttachmentReceipt) -> bool {
        match &receipt.artifact {
            ManagedArtifact::IntegrationOwned { selector, .. } => {
                !self.script().retired_routes.contains(selector)
            }
            _ => true,
        }
    }

    fn attach_package(
        &self,
        package: &StoredPackage,
        _plan: &PackageExposurePlan,
    ) -> Result<Option<AttachmentReceipt>> {
        self.script().asked.push("attach package".to_owned());
        Ok(Some(package_receipt(self.id, package, "current")))
    }

    fn publish_build(
        &self,
        _package: &StoredPackage,
        inputs: &BuildInputs<'_>,
        _recorded: Option<&AttachmentReceipt>,
    ) -> Result<Option<AttachmentReceipt>> {
        let mut script = self.script();
        script.linked.push(inputs.linked);
        if script.builds.is_empty() {
            return Ok(None);
        }
        Ok(Some(script.builds.remove(0)))
    }

    fn inspect_receipt(&self, receipt: &AttachmentReceipt) -> AttachmentInspection {
        let state = self
            .script()
            .states
            .get(&receipt.artifact.location())
            .copied()
            .unwrap_or(AttachmentState::Matched);
        AttachmentInspection {
            state,
            reason: format!("scripted as {state:?}"),
        }
    }

    fn detach_receipt(&self, receipt: &AttachmentReceipt) -> Result<AttachmentInspection> {
        let location = receipt.artifact.location();
        let mut script = self.script();
        script.asked.push(format!("detach {}", location.display()));
        if script.holds_on_to.contains(&location) {
            return Ok(AttachmentInspection {
                state: AttachmentState::Drifted,
                reason: "edited since".to_owned(),
            });
        }
        script.states.insert(location, AttachmentState::Missing);
        Ok(AttachmentInspection {
            state: AttachmentState::Missing,
            reason: String::new(),
        })
    }

    fn republish_packages(&self, _packages: &[StoredPackage]) -> Result<()> {
        let mut script = self.script();
        script.asked.push("republish".to_owned());
        if script.republish_fails {
            return Err(UzeError::ExposureUnavailable("view refused".to_owned()));
        }
        Ok(())
    }

    fn publication(&self, _packages: &[StoredPackage]) -> PublicationStatus {
        if self.script().unpublished {
            PublicationStatus::Unpublished("stale".to_owned())
        } else {
            PublicationStatus::Published
        }
    }
}

fn package_receipt(
    integration: &str,
    package: &StoredPackage,
    selector: &str,
) -> AttachmentReceipt {
    AttachmentReceipt {
        package_id: package.id.as_str().to_owned(),
        resource_identity: None,
        integration: integration.to_owned(),
        artifact: ManagedArtifact::IntegrationOwned {
            kind: "plugin".to_owned(),
            selector: selector.to_owned(),
            detail: BTreeMap::new(),
        },
    }
}

/// A scratch UZE home holding the skill fixture, and the harnesses it
/// knows, each scripted.
struct Machine {
    root: PathBuf,
    home: UzeHome,
    scripts: Vec<Arc<Mutex<Script>>>,
    app: UzeApplication,
    package: StoredPackage,
}

impl Machine {
    fn new(label: &str, harnesses: &[&'static str]) -> Self {
        let root = uze_testkit::temp::scratch(label);
        let home = UzeHome::at(root.join("uze"));
        let installer = UzeApplication::new(home.clone(), Vec::new());
        installer
            .plugins()
            .add(
                PackageSource::local(crate::application::tests::fixture()),
                &trust::AlwaysTrust,
            )
            .unwrap();
        let package = installer
            .store
            .packages()
            .unwrap()
            .into_iter()
            .find(|package| package.id.plugin_name() == "uze-agent-skill-conformance")
            .unwrap();
        let scripts: Vec<Arc<Mutex<Script>>> = harnesses
            .iter()
            .map(|_| Arc::new(Mutex::new(Script::default())))
            .collect();
        let integrations: Vec<Box<dyn IntegrationPort>> = harnesses
            .iter()
            .zip(&scripts)
            .map(|(&id, script)| {
                Box::new(Scripted {
                    id,
                    script: script.clone(),
                }) as Box<dyn IntegrationPort>
            })
            .collect();
        let app = UzeApplication::new(home.clone(), integrations);
        Self {
            root,
            home,
            scripts,
            app,
            package,
        }
    }

    fn script(&self, at: usize) -> std::sync::MutexGuard<'_, Script> {
        self.scripts[at].lock().unwrap()
    }

    fn harness(&self, at: usize) -> &dyn IntegrationPort {
        self.app.integrations[at].as_ref()
    }

    fn asked(&self, at: usize) -> Vec<String> {
        self.script(at).asked.clone()
    }

    fn resource(&self) -> Resource {
        uze_core::engine::package_resources(&self.package)
            .unwrap()
            .remove(0)
    }

    fn server(&self, name: &str) -> Resource {
        Resource::from_package_named(
            self.package.id.clone(),
            self.package.root.clone(),
            Capability {
                kind: CapabilityKind::Mcp,
                path: self.package.root.join("mcp.json"),
                payload: Vec::new(),
            },
            name.to_owned(),
        )
    }

    fn deliver(&self, receipt: AttachmentReceipt) -> AttachmentReceipt {
        state::record_receipt(&self.home, receipt.clone()).unwrap();
        receipt
    }

    fn receipts(&self) -> Vec<AttachmentReceipt> {
        state::receipts(&self.home, None).unwrap()
    }

    fn deliver_package(&self, at: usize) -> PackageDelivery {
        let resources = uze_core::engine::package_resources(&self.package).unwrap();
        let resources: Vec<&Resource> = resources.iter().collect();
        self.app
            .deliver_package_to(
                &self.package,
                &resources,
                self.harness(at),
                NativeDelivery::Allowed,
            )
            .unwrap()
    }

    fn plan_package(&self, at: usize) -> PlannedDelivery {
        let resources = uze_core::engine::package_resources(&self.package).unwrap();
        let resources: Vec<&Resource> = resources.iter().collect();
        self.app
            .plan_delivery_to(&self.package, &resources, self.harness(at))
    }

    /// A package plan that provides the fixture's one capability.
    fn provides_the_capability(&self, at: usize) {
        let identity = self.resource().identity();
        self.script(at).package_plan = Some(PackageExposurePlan {
            package_id: self.package.id.clone(),
            route: CompatibilityRoute::Native,
            envelope: PackageEnvelope::Generated,
            provided_resource_identities: [identity].into_iter().collect(),
            evidence: "scripted".to_owned(),
        });
    }

    fn skills(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// A receipt on `integration` for the fixture's capability.
    fn capability_receipt(
        &self,
        integration: &str,
        artifact: ManagedArtifact,
    ) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: self.package.id.as_str().to_owned(),
            resource_identity: Some(self.resource().identity()),
            integration: integration.to_owned(),
            artifact,
        }
    }

    fn build(&self, integration: &str, digest: &str) -> AttachmentReceipt {
        AttachmentReceipt {
            package_id: self.package.id.as_str().to_owned(),
            resource_identity: Some(uze_core::integration::BUILD_IDENTITY.to_owned()),
            integration: integration.to_owned(),
            artifact: ManagedArtifact::GeneratedTree {
                path: self.root.join("builds").join(integration),
                digest: digest.to_owned(),
            },
        }
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn linked(path: PathBuf) -> ManagedArtifact {
    ManagedArtifact::SymlinkReference {
        target: path.with_extension("target"),
        path,
    }
}

fn tree(path: PathBuf) -> ManagedArtifact {
    ManagedArtifact::GeneratedTree {
        path,
        digest: String::new(),
    }
}

fn entry(name: &str, command: &str) -> ManagedArtifact {
    ManagedArtifact::VendorConfigEntry {
        entry_name: name.to_owned(),
        transport: "stdio".to_owned(),
        command: PathBuf::from(command),
        args: Vec::new(),
        cwd: None,
        environment: Vec::new(),
        enabled: None,
    }
}

fn detach(artifact: &ManagedArtifact) -> String {
    format!("detach {}", artifact.location().display())
}

#[test]
fn a_matched_receipt_is_detached_and_forgotten() {
    let machine = Machine::new("attach-retire-matched", &["alpha", "beta"]);
    let receipt =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    let elsewhere =
        machine.deliver(machine.capability_receipt("beta", linked(machine.root.join("other"))));

    let gone = machine
        .app
        .retire_receipt(machine.harness(0), &receipt)
        .unwrap();

    assert!(gone);
    assert_eq!(machine.asked(0), vec![detach(&receipt.artifact)]);
    assert_eq!(machine.receipts(), vec![elsewhere]);
}

#[test]
fn a_missing_receipt_is_only_forgotten() {
    let machine = Machine::new("attach-retire-missing", &["alpha"]);
    let receipt =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    machine
        .script(0)
        .states
        .insert(receipt.artifact.location(), AttachmentState::Missing);

    let gone = machine
        .app
        .retire_receipt(machine.harness(0), &receipt)
        .unwrap();

    assert!(gone);
    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
    assert!(machine.receipts().is_empty());
}

#[test]
fn a_changed_receipt_is_left_where_it_is() {
    let machine = Machine::new("attach-retire-changed", &["alpha"]);
    let receipt =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    machine
        .script(0)
        .states
        .insert(receipt.artifact.location(), AttachmentState::Drifted);

    let gone = machine
        .app
        .retire_receipt(machine.harness(0), &receipt)
        .unwrap();

    assert!(!gone);
    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
    assert_eq!(machine.receipts(), vec![receipt]);
}

#[test]
fn a_receipt_whose_detach_leaves_it_there_is_kept() {
    let machine = Machine::new("attach-retire-held", &["alpha"]);
    let receipt =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    machine
        .script(0)
        .holds_on_to
        .push(receipt.artifact.location());

    let gone = machine
        .app
        .retire_receipt(machine.harness(0), &receipt)
        .unwrap();

    assert!(!gone);
    assert_eq!(machine.receipts(), vec![receipt]);
}

#[test]
fn an_entry_another_harness_also_holds_loses_only_this_receipt() {
    let machine = Machine::new("attach-retire-shared", &["alpha", "beta"]);
    let shared = linked(machine.skills().join("one"));
    let receipt = machine.deliver(machine.capability_receipt("alpha", shared.clone()));
    let other = machine.deliver(machine.capability_receipt("beta", shared));

    let gone = machine
        .app
        .retire_receipt(machine.harness(0), &receipt)
        .unwrap();

    assert!(gone);
    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
    assert_eq!(machine.receipts(), vec![other]);
}

#[test]
fn the_receipts_a_package_plan_covers_are_this_harness_capabilities_it_provides() {
    let machine = Machine::new("attach-covered", &["alpha", "beta"]);
    machine.provides_the_capability(0);
    let covered =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    machine.deliver(machine.capability_receipt("beta", linked(machine.skills().join("one"))));
    machine.deliver(package_receipt("alpha", &machine.package, "current"));
    let mut uncovered = machine.capability_receipt("alpha", linked(machine.skills().join("two")));
    uncovered.resource_identity = Some("package:other".to_owned());
    machine.deliver(uncovered);
    let plan = machine.script(0).package_plan.clone().unwrap();

    let receipts = machine
        .app
        .covered_receipts(&machine.package, machine.harness(0), &plan)
        .unwrap();

    assert_eq!(receipts, vec![covered]);
}

#[test]
fn a_package_that_no_longer_reaches_a_harness_is_taken_off_only_there() {
    let machine = Machine::new("attach-retire-package", &["alpha", "beta"]);
    let build = machine.deliver(machine.build("alpha", "one"));
    let capability =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    let other =
        machine.deliver(machine.capability_receipt("beta", linked(machine.root.join("beta"))));

    machine
        .app
        .retire_package_receipts(&machine.package, machine.harness(0))
        .unwrap();

    assert_eq!(
        machine.asked(0),
        vec![detach(&capability.artifact), detach(&build.artifact)]
    );
    assert_eq!(machine.receipts(), vec![other]);
}

#[test]
fn a_build_rendered_again_says_so_and_one_unchanged_does_not() {
    let machine = Machine::new("attach-publish-build", &["alpha", "beta"]);
    machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("one"))));
    machine.deliver(machine.build("beta", "one"));
    let first = machine.build("alpha", "one");
    machine.script(0).builds = vec![first.clone(), first.clone(), machine.build("alpha", "two")];

    let rendered = machine
        .app
        .publish_build(&machine.package, machine.harness(0))
        .unwrap();
    let unchanged = machine
        .app
        .publish_build(&machine.package, machine.harness(0))
        .unwrap();
    let rendered_again = machine
        .app
        .publish_build(&machine.package, machine.harness(0))
        .unwrap();
    let none = machine
        .app
        .publish_build(&machine.package, machine.harness(0))
        .unwrap();

    assert!(rendered && rendered_again);
    assert!(!unchanged && !none);
    assert!(machine.receipts().contains(&machine.build("alpha", "two")));
    // Installed from a path, not through a linked marketplace.
    assert!(machine.script(0).linked.iter().all(|linked| !linked));
}

#[test]
fn every_view_is_rebuilt_and_one_that_fails_is_said() {
    let machine = Machine::new("attach-republish", &["alpha", "beta"]);
    machine.script(1).republish_fails = true;

    let outcomes = machine.app.republish_all();

    let errors: Vec<(&str, bool)> = outcomes
        .iter()
        .map(|outcome| (outcome.integration.as_str(), outcome.error.is_some()))
        .collect();
    assert_eq!(errors, [("alpha", false), ("beta", true)]);
}

#[test]
fn rebuilding_with_no_report_still_rebuilds_every_view() {
    let machine = Machine::new("attach-republish-reporting", &["alpha", "beta"]);
    machine.script(1).republish_fails = true;

    machine.app.republish_all_reporting();

    assert_eq!(machine.asked(0), vec!["republish".to_owned()]);
    assert_eq!(machine.asked(1), vec!["republish".to_owned()]);
}

#[test]
fn only_a_view_that_no_longer_matches_is_rebuilt() {
    let machine = Machine::new("attach-republish-unpublished", &["alpha", "beta"]);
    machine.script(1).unpublished = true;

    let outcomes = machine
        .app
        .republish_unpublished(std::slice::from_ref(&machine.package));

    let rebuilt: Vec<&str> = outcomes.iter().map(|o| o.integration.as_str()).collect();
    assert_eq!(rebuilt, ["beta"]);
    assert!(machine.asked(0).is_empty());
}

#[test]
fn a_server_is_given_a_name_like_a_skill() {
    let machine = Machine::new("attach-name-server", &["alpha"]);
    machine.script(0).candidates = vec!["review-server".to_owned()];

    let resolved = machine
        .app
        .resolve_exposure_name(&machine.server("review"), machine.harness(0))
        .unwrap();

    assert_eq!(
        resolved.resolved_exposure_name.as_deref(),
        Some("review-server")
    );
}

#[test]
fn the_name_this_harness_already_gave_a_capability_is_kept() {
    let machine = Machine::new("attach-name-existing", &["alpha", "beta"]);
    machine.script(0).candidates = vec!["fresh".to_owned()];
    machine.deliver(machine.capability_receipt("beta", linked(machine.skills().join("beta-name"))));
    let mut unrelated =
        machine.capability_receipt("alpha", linked(machine.skills().join("unrelated")));
    unrelated.resource_identity = Some("package:other".to_owned());
    unrelated.package_id = "other@uze".to_owned();
    machine.deliver(unrelated);
    machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("given"))));

    let resolved = machine
        .app
        .resolve_exposure_name(&machine.resource(), machine.harness(0))
        .unwrap();

    assert_eq!(resolved.resolved_exposure_name.as_deref(), Some("given"));
}

/// A receipt of another capability, on `integration`, holding `artifact`.
fn claim(integration: &str, artifact: ManagedArtifact) -> AttachmentReceipt {
    AttachmentReceipt {
        package_id: "other@uze".to_owned(),
        resource_identity: Some("package:other@uze:skills/other/SKILL.md".to_owned()),
        integration: integration.to_owned(),
        artifact,
    }
}

fn names(machine: &Machine, first: &str, second: &str) {
    let mut script = machine.script(0);
    script.candidates = vec![first.to_owned(), second.to_owned()];
    script.plan = Some(linked(machine.skills().join(first)));
}

#[test]
fn a_name_held_in_another_place_does_not_contend() {
    let machine = Machine::new("attach-name-elsewhere", &["alpha"]);
    names(&machine, "review", "review-2");
    machine.deliver(claim(
        "alpha",
        ManagedArtifact::GeneratedFile {
            path: machine.root.join("agents").join("review.md"),
            content: String::new(),
        },
    ));

    let resolved = machine
        .app
        .resolve_exposure_name(&machine.resource(), machine.harness(0))
        .unwrap();

    assert_eq!(resolved.resolved_exposure_name.as_deref(), Some("review"));
}

#[test]
fn a_name_another_harness_holds_does_not_contend() {
    let machine = Machine::new("attach-name-other-harness", &["alpha"]);
    names(&machine, "review", "review-2");
    machine.deliver(claim("beta", linked(machine.skills().join("review"))));

    let resolved = machine
        .app
        .resolve_exposure_name(&machine.resource(), machine.harness(0))
        .unwrap();

    assert_eq!(resolved.resolved_exposure_name.as_deref(), Some("review"));
}

#[test]
fn a_name_this_harness_holds_for_another_capability_gives_the_next() {
    let machine = Machine::new("attach-name-next", &["alpha"]);
    names(&machine, "review", "review-2");
    machine.deliver(claim("alpha", linked(machine.skills().join("review"))));

    let resolved = machine
        .app
        .resolve_exposure_name(&machine.resource(), machine.harness(0))
        .unwrap();

    assert_eq!(resolved.resolved_exposure_name.as_deref(), Some("review-2"));
}

#[test]
fn every_name_held_for_another_capability_is_a_projection_conflict() {
    let machine = Machine::new("attach-name-conflict", &["alpha"]);
    names(&machine, "review", "review-2");
    let planned_target = machine.skills().join("review.target");
    machine.deliver(claim("alpha", linked(machine.skills().join("review"))));
    let holder = machine.deliver(AttachmentReceipt {
        package_id: "third@uze".to_owned(),
        resource_identity: Some("package:third@uze:skills/third/SKILL.md".to_owned()),
        ..claim("alpha", linked(machine.skills().join("review-2")))
    });

    let refused = machine
        .app
        .resolve_exposure_name(&machine.resource(), machine.harness(0));

    let Err(UzeError::ProjectionConflict(details)) = refused else {
        panic!("not a projection conflict: {refused:?}");
    };
    assert_eq!(details.requested_target, planned_target);
    assert_eq!(
        details.existing_target,
        machine.skills().join("review-2.target")
    );
    assert_eq!(details.existing, holder.resource_identity.unwrap());
}

#[test]
fn the_target_a_conflict_names_is_a_links_target_or_the_entry_itself() {
    let link = claim_of(linked(PathBuf::from("skills").join("review")));
    let file = claim_of(ManagedArtifact::GeneratedFile {
        path: PathBuf::from("agents").join("review.md"),
        content: String::new(),
    });

    assert_eq!(
        artifact_owned_target(&link),
        PathBuf::from("skills").join("review.target")
    );
    assert_eq!(
        artifact_owned_target(&file),
        PathBuf::from("agents").join("review.md")
    );
}

fn claim_of(artifact: ManagedArtifact) -> AttachmentReceipt {
    AttachmentReceipt {
        package_id: "other@uze".to_owned(),
        resource_identity: None,
        integration: "alpha".to_owned(),
        artifact,
    }
}

#[test]
fn only_entries_in_one_place_compete_for_a_name() {
    let skills = PathBuf::from("skills");
    let agents = PathBuf::from("agents");
    let file = |dir: &Path| ManagedArtifact::GeneratedFile {
        path: dir.join("review.md"),
        content: String::new(),
    };
    let hook_file = |dir: &Path| ManagedArtifact::ManagedHookFile {
        path: dir.join("hook.js"),
    };
    let hook = |config: &str| ManagedArtifact::HookConfigEntry {
        config_file: PathBuf::from(config),
        entry_name: "uze-review".to_owned(),
        event: uze_core::capability::hook::HookEvent::Stop,
        expected: String::new(),
        wrapper: PathBuf::from("wrapper"),
    };

    assert!(same_name_space(
        &linked(skills.join("a")),
        &tree(skills.join("b"))
    ));
    assert!(!same_name_space(
        &linked(skills.join("a")),
        &tree(agents.join("b"))
    ));
    assert!(same_name_space(&file(&agents), &file(&agents)));
    assert!(!same_name_space(&file(&agents), &file(&skills)));
    assert!(same_name_space(&hook_file(&agents), &hook_file(&agents)));
    assert!(same_name_space(&entry("a", "x"), &entry("b", "y")));
    assert!(same_name_space(&hook("hooks.json"), &hook("hooks.json")));
    assert!(!same_name_space(&hook("hooks.json"), &hook("other.json")));
    assert!(!same_name_space(
        &linked(skills.join("a")),
        &entry("a", "x")
    ));
}

#[test]
fn a_config_entry_saying_something_else_under_its_name_is_restated() {
    assert!(!config_entry_restated(
        &entry("server", "old"),
        &entry("server", "old")
    ));
    assert!(config_entry_restated(
        &entry("server", "old"),
        &entry("server", "new")
    ));
    // Which name it holds is the rename rule's question.
    assert!(!config_entry_restated(
        &entry("server", "old"),
        &entry("renamed", "old")
    ));
    assert!(!config_entry_restated(
        &linked(PathBuf::from("a")),
        &linked(PathBuf::from("b"))
    ));
}

#[test]
fn only_a_refusal_about_one_name_is_reported_rather_than_failing() {
    assert!(refuses_one_name(&UzeError::ManagedEntryDrift(
        PathBuf::from("a")
    )));
    assert!(refuses_one_name(&UzeError::ManagedEntryConflict(
        PathBuf::from("a")
    )));
    assert!(!refuses_one_name(&UzeError::ExposureUnavailable(
        "disk".to_owned()
    )));
}

#[test]
fn a_capability_short_of_native_is_a_shortfall_and_a_native_one_is_not() {
    let machine = Machine::new("attach-shortfall", &[]);
    let resource = machine.resource();

    assert!(
        CapabilityShortfall::of(&resource, (CompatibilityRoute::Native, String::new())).is_none()
    );
    let shortfall = CapabilityShortfall::of(
        &resource,
        (CompatibilityRoute::Adaptable, "lost".to_owned()),
    )
    .unwrap();
    assert_eq!(shortfall.route, CompatibilityRoute::Adaptable);
    assert_eq!(shortfall.capability, "uze-e2e");
}

#[test]
fn a_package_of_extensions_alone_reaches_no_harness() {
    let machine = Machine::new("attach-extensions-only", &["alpha"]);
    let only_extensions = machine.root.join("extensions-only");
    fs::create_dir_all(only_extensions.join("extensions").join("board")).unwrap();
    fs::write(
        only_extensions
            .join("extensions")
            .join("board")
            .join("extension.kdl"),
        "",
    )
    .unwrap();
    let extensions = StoredPackage {
        root: only_extensions,
        ..machine.package.clone()
    };
    let empty = StoredPackage {
        root: machine.root.join("empty"),
        ..machine.package.clone()
    };
    let resource = machine.resource();

    assert!(!reaches_a_harness(&extensions, &[]));
    assert!(reaches_a_harness(&extensions, &[&resource]));
    assert!(reaches_a_harness(&empty, &[]));
    assert!(!delivers_to(&extensions, &[], machine.harness(0)));
    machine.provides_the_capability(0);
    assert!(!delivers_to(&extensions, &[], machine.harness(0)));
    machine.script(0).package_plan.as_mut().unwrap().envelope = PackageEnvelope::Own;
    assert!(delivers_to(&extensions, &[], machine.harness(0)));
}

#[test]
fn a_renamed_capability_is_taken_off_under_its_old_name() {
    let machine = Machine::new("attach-superseded-renamed", &["alpha"]);
    names(&machine, "review", "review-2");
    let old =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("old"))));

    let held = machine
        .app
        .retire_superseded_receipts(&machine.resource(), machine.harness(0))
        .unwrap();

    assert!(held.is_none());
    assert_eq!(machine.asked(0), vec![detach(&old.artifact)]);
}

#[test]
fn a_capability_delivered_as_planned_is_left_alone() {
    let machine = Machine::new("attach-superseded-current", &["alpha"]);
    names(&machine, "review", "review-2");
    machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("review"))));

    machine
        .app
        .retire_superseded_receipts(&machine.resource(), machine.harness(0))
        .unwrap();

    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
}

#[test]
fn a_capability_planned_as_another_kind_of_artifact_is_taken_off() {
    let machine = Machine::new("attach-superseded-reshaped", &["alpha"]);
    names(&machine, "review", "review-2");
    machine.script(0).plan = Some(tree(machine.skills().join("review")));
    let old = machine
        .deliver(machine.capability_receipt("alpha", linked(machine.skills().join("review"))));

    machine
        .app
        .retire_superseded_receipts(&machine.resource(), machine.harness(0))
        .unwrap();

    assert_eq!(machine.asked(0), vec![detach(&old.artifact)]);
}

#[test]
fn a_server_whose_entry_says_something_else_is_taken_off() {
    let machine = Machine::new("attach-superseded-restated", &["alpha"]);
    let server = machine.server("review");
    {
        let mut script = machine.script(0);
        script.candidates = vec!["review".to_owned()];
        script.plan = Some(entry("review", "new"));
    }
    let old = machine.deliver(AttachmentReceipt {
        resource_identity: Some(server.identity()),
        ..machine.capability_receipt("alpha", entry("review", "old"))
    });

    machine
        .app
        .retire_superseded_receipts(&server, machine.harness(0))
        .unwrap();

    assert_eq!(machine.asked(0), vec![detach(&old.artifact)]);
}

#[test]
fn a_superseded_delivery_someone_changed_holds_the_capability_back() {
    let machine = Machine::new("attach-superseded-changed", &["alpha"]);
    names(&machine, "review", "review-2");
    let old =
        machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("old"))));
    machine
        .script(0)
        .states
        .insert(old.artifact.location(), AttachmentState::Drifted);

    let held = machine
        .app
        .retire_superseded_receipts(&machine.resource(), machine.harness(0))
        .unwrap();

    assert!(held.is_some());
    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
    assert_eq!(machine.receipts(), vec![old]);
}

#[test]
fn a_co_holder_whose_own_plan_moved_on_lets_the_entry_go() {
    let machine = Machine::new("attach-co-holder-moved", &["alpha", "beta"]);
    names(&machine, "review", "review-2");
    let new_shape = tree(machine.skills().join("review"));
    machine.script(0).plan = Some(new_shape.clone());
    machine.script(1).plan = Some(new_shape);
    let shared = linked(machine.skills().join("review"));
    let old = machine.deliver(machine.capability_receipt("alpha", shared.clone()));
    machine.deliver(machine.capability_receipt("beta", shared));
    let unrelated = machine.deliver(claim("beta", linked(machine.root.join("unrelated"))));

    machine
        .app
        .retire_superseded_receipts(&machine.resource(), machine.harness(0))
        .unwrap();

    assert_eq!(machine.asked(0), vec![detach(&old.artifact)]);
    assert_eq!(machine.receipts(), vec![unrelated]);
}

#[test]
fn a_co_holder_still_planning_the_entry_keeps_it() {
    let machine = Machine::new("attach-co-holder-kept", &["alpha", "beta"]);
    names(&machine, "review", "review-2");
    machine.script(0).plan = Some(tree(machine.skills().join("review")));
    let shared = linked(machine.skills().join("review"));
    machine.script(1).plan = Some(shared.clone());
    machine.deliver(machine.capability_receipt("alpha", shared.clone()));
    let kept = machine.deliver(machine.capability_receipt("beta", shared));

    machine
        .app
        .retire_superseded_receipts(&machine.resource(), machine.harness(0))
        .unwrap();

    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
    assert_eq!(machine.receipts(), vec![kept]);
}

#[test]
fn a_package_another_harness_holds_is_still_installed_here() {
    let machine = Machine::new("attach-package-other-harness", &["alpha"]);
    machine.provides_the_capability(0);
    machine.deliver(package_receipt("beta", &machine.package, "current"));

    machine.deliver_package(0);

    assert_eq!(machine.asked(0), vec!["attach package".to_owned()]);
}

#[test]
fn a_package_still_installed_as_built_is_not_installed_again() {
    let machine = Machine::new("attach-package-idempotent", &["alpha"]);
    machine.provides_the_capability(0);

    machine.deliver_package(0);
    let again = machine.deliver_package(0);

    assert_eq!(machine.asked(0), vec!["attach package".to_owned()]);
    assert_eq!(again.attachments.len(), 1);
}

#[test]
fn a_package_whose_build_was_rendered_again_is_installed_again() {
    let machine = Machine::new("attach-package-rebuilt", &["alpha"]);
    machine.provides_the_capability(0);
    machine.script(0).builds = vec![machine.build("alpha", "one"), machine.build("alpha", "two")];

    machine.deliver_package(0);
    machine.deliver_package(0);

    assert_eq!(
        machine.asked(0),
        vec!["attach package".to_owned(), "attach package".to_owned()]
    );
}

#[test]
fn a_route_the_package_no_longer_takes_comes_off_before_the_current_one_goes_on() {
    let machine = Machine::new("attach-package-old-route", &["alpha"]);
    machine.provides_the_capability(0);
    machine
        .script(0)
        .retired_routes
        .push("old-route".to_owned());
    let old = machine.deliver(package_receipt("alpha", &machine.package, "old-route"));

    let delivery = machine.deliver_package(0);

    assert_eq!(
        machine.asked(0),
        vec![detach(&old.artifact), "attach package".to_owned()]
    );
    assert!(!delivery.plan_set_aside);
}

#[test]
fn a_route_that_cannot_come_off_sets_the_package_aside() {
    let machine = Machine::new("attach-package-old-route-held", &["alpha"]);
    machine.provides_the_capability(0);
    machine
        .script(0)
        .retired_routes
        .push("old-route".to_owned());
    let old = machine.deliver(package_receipt("alpha", &machine.package, "old-route"));
    machine
        .script(0)
        .states
        .insert(old.artifact.location(), AttachmentState::Drifted);

    let delivery = machine.deliver_package(0);

    assert!(delivery.plan_set_aside);
    assert_eq!(delivery.blocked.len(), 1);
    assert!(machine.asked(0).is_empty(), "{:?}", machine.asked(0));
}

#[test]
fn a_capability_the_harness_declines_keeps_the_route_it_planned() {
    let machine = Machine::new("attach-declined", &["alpha"]);
    {
        let mut script = machine.script(0);
        script.plan = Some(linked(machine.skills().join("review")));
        script.route = Some(CompatibilityRoute::Adaptable);
        script.declines = true;
    }

    let delivery = machine.deliver_package(0);

    let routes: Vec<CompatibilityRoute> = delivery.shortfalls.iter().map(|s| s.route).collect();
    assert_eq!(routes, [CompatibilityRoute::Adaptable]);
    assert!(delivery.attachments.is_empty());
}

#[test]
fn a_capability_whose_name_is_held_is_reported_and_the_delivery_goes_on() {
    let machine = Machine::new("attach-name-held", &["alpha"]);
    {
        let mut script = machine.script(0);
        script.plan = Some(linked(machine.skills().join("review")));
        script.name_held = true;
    }

    let delivery = machine.deliver_package(0);

    assert_eq!(delivery.blocked.len(), 1);
    assert_eq!(
        delivery.blocked[0].capability,
        machine.resource().identity()
    );
}

#[test]
fn a_plan_set_aside_is_delivered_capability_by_capability() {
    let plan = PackageExposurePlan {
        package_id: uze_core::store::PackageId::from_marketplace_plugin(
            "uze",
            "demo",
            Path::new("plugin.json"),
        )
        .unwrap(),
        route: CompatibilityRoute::Native,
        envelope: PackageEnvelope::Generated,
        provided_resource_identities: Default::default(),
        evidence: String::new(),
    };

    assert!(matches!(
        delivery_route_of(Some(&plan), true),
        DeliveryRoute::CapabilityByCapability { .. }
    ));
    assert!(matches!(
        delivery_route_of(Some(&plan), false),
        DeliveryRoute::Package { .. }
    ));
}

fn capability(identity: &str, route: CompatibilityRoute, blocked: bool) -> CapabilityDelivery {
    CapabilityDelivery {
        identity: identity.to_owned(),
        kind: CapabilityKind::AgentSkill,
        provided_by_package: false,
        plan: None,
        exposed_name: None,
        route,
        location: None,
        evidence: String::new(),
        blocked: blocked.then(|| "held".to_owned()),
    }
}

#[test]
fn a_harness_is_expected_to_hold_only_what_has_a_route_and_nothing_blocks() {
    let planned = PlannedDelivery {
        package_plan: None,
        route: delivery_route_of(None, false),
        capabilities: vec![
            capability("unsupported", CompatibilityRoute::Unsupported, false),
            capability("blocked", CompatibilityRoute::Native, true),
            capability("delivered", CompatibilityRoute::Adaptable, false),
        ],
        shortfalls: Vec::new(),
    };

    let expected: Vec<&str> = planned.expected().map(|c| c.identity.as_str()).collect();

    assert_eq!(expected, ["delivered"]);
}

#[test]
fn the_plan_sets_the_package_aside_while_a_covered_capability_changed() {
    let machine = Machine::new("effective-set-aside", &["alpha"]);
    machine.provides_the_capability(0);
    let covered = machine
        .deliver(machine.capability_receipt("alpha", linked(machine.skills().join("review"))));
    machine
        .script(0)
        .states
        .insert(covered.artifact.location(), AttachmentState::Drifted);

    let planned = machine.plan_package(0);

    assert!(matches!(
        planned.route,
        DeliveryRoute::CapabilityByCapability { .. }
    ));
    assert!(!planned.capabilities[0].provided_by_package);
}

#[test]
fn the_plan_hands_a_capability_still_matching_to_the_package() {
    let machine = Machine::new("effective-package", &["alpha"]);
    machine.provides_the_capability(0);
    machine.deliver(machine.capability_receipt("alpha", linked(machine.skills().join("review"))));

    let planned = machine.plan_package(0);

    assert!(matches!(planned.route, DeliveryRoute::Package { .. }));
    assert!(planned.capabilities[0].provided_by_package);
}

#[test]
fn a_package_already_installed_stays_the_route_whatever_its_old_capabilities_say() {
    let machine = Machine::new("effective-installed", &["alpha"]);
    machine.provides_the_capability(0);
    let covered = machine
        .deliver(machine.capability_receipt("alpha", linked(machine.skills().join("review"))));
    machine
        .script(0)
        .states
        .insert(covered.artifact.location(), AttachmentState::Drifted);
    let installed = machine.deliver(package_receipt("alpha", &machine.package, "current"));

    let planned = machine.plan_package(0);

    assert!(matches!(planned.route, DeliveryRoute::Package { .. }));
    assert_eq!(
        planned.capabilities[0].location,
        Some(installed.artifact.location())
    );
}

#[test]
fn the_plan_names_a_capability_delivered_on_its_own() {
    let machine = Machine::new("effective-named", &["alpha"]);
    names(&machine, "review", "review-2");

    let planned = machine.plan_package(0);

    assert_eq!(
        planned.capabilities[0].exposed_name.as_deref(),
        Some("review")
    );
}
