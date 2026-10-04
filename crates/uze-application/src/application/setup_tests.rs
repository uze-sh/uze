//! `uze setup` against a harness that exists only once its official route
//! has run through the injected process runner: nothing here reaches a
//! network or a real vendor installer, and every world is its own
//! `UZE_HOME` and harness root under a scratch directory.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use uze_core::{
    capability::Resource,
    integration::{AttachmentReceipt, ManagedArtifact},
    provisioning::{ProcessResult, ProcessSpec},
};

use super::services::WorkspaceEntry;
use super::tests::fixture;
use super::*;

const INSTALLER: &str = "routed-official-installer";

/// What the fake runner has been asked to run, and the one fact about the
/// machine it can change: whether the harness executable exists.
#[derive(Clone, Default)]
struct Machine {
    installed: Arc<AtomicBool>,
    commands: Arc<Mutex<Vec<ProcessSpec>>>,
}

impl Machine {
    fn with_harness_installed() -> Self {
        let machine = Self::default();
        machine.installed.store(true, Ordering::SeqCst);
        machine
    }

    fn commands(&self) -> Vec<String> {
        self.commands
            .lock()
            .unwrap()
            .iter()
            .map(|spec| format!("{} {}", spec.program, spec.arguments.join(" ")))
            .collect()
    }
}

/// The installer puts the executable on the machine; a route named in
/// `failing` exits unsuccessfully, the way a vendor installer that lost its
/// network would.
struct FakeRunner {
    machine: Machine,
    failing: Option<&'static str>,
}

impl ProcessRunner for FakeRunner {
    fn run(&self, spec: &ProcessSpec) -> Result<ProcessResult> {
        self.machine.commands.lock().unwrap().push(spec.clone());
        let invocation = format!("{} {}", spec.program, spec.arguments.join(" "));
        if self.failing == Some(invocation.as_str()) {
            return Ok(ProcessResult {
                success: false,
                timed_out: false,
            });
        }
        if spec.program == INSTALLER {
            self.machine.installed.store(true, Ordering::SeqCst);
        }
        let ran = spec.program == INSTALLER || self.machine.installed.load(Ordering::SeqCst);
        Ok(ProcessResult {
            success: ran,
            timed_out: false,
        })
    }
}

/// A harness shaped like a peer integration: installed by an official
/// installer when absent, updated by its own verb when present, verified
/// by `--version`, and delivered to only once `install` has prepared it.
struct RoutedHarness {
    id: &'static str,
    home: UzeHome,
    launched_through_a_shim: bool,
    skills_dir: PathBuf,
    machine: Machine,
    provisioned: Arc<AtomicUsize>,
    prepared: Arc<AtomicUsize>,
}

impl RoutedHarness {
    fn new(id: &'static str, home: &UzeHome, root: &Path, machine: &Machine) -> Self {
        Self {
            id,
            home: home.clone(),
            launched_through_a_shim: false,
            skills_dir: root.join(id).join("skills"),
            machine: machine.clone(),
            provisioned: Arc::new(AtomicUsize::new(0)),
            prepared: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl IntegrationPort for RoutedHarness {
    fn id(&self) -> &'static str {
        self.id
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }

    fn supports_runtime_integration(&self) -> bool {
        self.launched_through_a_shim
    }

    fn detect(&self) -> HarnessDetection {
        let present = self.machine.installed.load(Ordering::SeqCst);
        HarnessDetection {
            present,
            version: present.then(|| "1.2.3".to_owned()),
        }
    }

    fn provision(&self, runner: &dyn ProcessRunner) -> Result<ProvisioningResult> {
        self.provisioned.fetch_add(1, Ordering::SeqCst);
        let (action, route) = if self.detect().present {
            (
                ProvisionAction::Update,
                ProcessSpec::new("routed", ["update"]),
            )
        } else {
            (
                ProvisionAction::Install,
                ProcessSpec::new(INSTALLER, ["--latest"]),
            )
        };
        if !runner.run(&route)?.success {
            return Ok(ProvisioningResult::failed(
                action,
                "official-test-route",
                "official installer exited unsuccessfully",
            ));
        }
        if !runner
            .run(&ProcessSpec::new("routed", ["--version"]))?
            .success
        {
            return Ok(ProvisioningResult::failed(
                action,
                "official-test-route",
                "installer finished but the executable could not be verified",
            ));
        }
        Ok(ProvisioningResult::verified(
            action,
            "official-test-route",
            self.detect(),
        ))
    }

    fn install(&self, home: &UzeHome, detection: &HarnessDetection) -> Result<()> {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        fs::create_dir_all(&self.skills_dir).map_err(UzeError::write(&self.skills_dir))?;
        state::record(
            home,
            self.id,
            state::IntegrationRecord {
                version: detection.version.clone(),
                strategy: "test-skills-dir".to_owned(),
            },
        )
    }

    fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
        ExposurePlan {
            route: CompatibilityRoute::Adaptable,
            mechanism: ExposureMechanism::Unsupported {
                rationale: "attachment is implemented directly".to_owned(),
            },
            evidence: "test".to_owned(),
        }
    }

    fn attach_receipt(&self, resource: &Resource) -> Result<Option<AttachmentReceipt>> {
        if !state::is_installed(&self.home, self.id) {
            return Ok(None);
        }
        let path = self
            .skills_dir
            .join(resource.identity().replace([':', '/'], "-"));
        if fs::read_link(&path).ok().as_ref() != Some(&resource.capability.path) {
            let _ = uze_platform::fs::remove_link(&path);
            uze_platform::fs::symlink(&resource.capability.path, &path).map_err(|source| {
                UzeError::Write {
                    path: path.clone(),
                    source,
                }
            })?;
        }
        Ok(Some(AttachmentReceipt {
            package_id: resource.package_id.as_str().to_owned(),
            resource_identity: Some(resource.identity()),
            integration: self.id.to_owned(),
            artifact: ManagedArtifact::SymlinkReference {
                path,
                target: resource.capability.path.clone(),
            },
        }))
    }
}

struct World {
    root: PathBuf,
    home: UzeHome,
    machine: Machine,
    provisioned: Arc<AtomicUsize>,
    prepared: Arc<AtomicUsize>,
    app: UzeApplication,
}

impl World {
    fn new(label: &str, machine: Machine, failing: Option<&'static str>) -> Self {
        Self::built(label, machine, failing, false)
    }

    /// A world whose harness the workspace launches through its shim.
    fn launched_through_a_shim(label: &str, machine: Machine) -> Self {
        Self::built(label, machine, None, true)
    }

    fn built(
        label: &str,
        machine: Machine,
        failing: Option<&'static str>,
        launched_through_a_shim: bool,
    ) -> Self {
        let root = uze_testkit::temp::scratch(label);
        let home = UzeHome::at(root.join("uze"));
        let mut harness = RoutedHarness::new("routed", &home, &root.join("home"), &machine);
        harness.launched_through_a_shim = launched_through_a_shim;
        let provisioned = harness.provisioned.clone();
        let prepared = harness.prepared.clone();
        let app = UzeApplication::new_with_runner(
            home.clone(),
            vec![Box::new(harness)],
            Box::new(FakeRunner {
                machine: machine.clone(),
                failing,
            }),
        );
        // Seeded now so the seeding every command runs first is already
        // settled when a test compares what setup itself changed.
        app.ensure_default_plugins().unwrap();
        Self {
            root,
            home,
            machine,
            provisioned,
            prepared,
            app,
        }
    }

    fn setup(&self) -> SetupResult {
        let mut results = self.app.setup(Some("routed")).unwrap();
        assert_eq!(results.len(), 1, "{results:?}");
        results.remove(0)
    }

    fn add_fixture(&self) {
        self.app
            .plugins()
            .add(PackageSource::local(fixture()), &trust::AlwaysTrust)
            .unwrap();
    }

    fn receipts(&self) -> Vec<AttachmentReceipt> {
        state::receipts(&self.home, None)
            .unwrap()
            .into_iter()
            .filter(|receipt| receipt.integration == "routed")
            .collect()
    }

    fn stored(&self) -> Vec<String> {
        self.app
            .store
            .package_ids()
            .unwrap()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_missing_harness_is_installed_verified_then_prepared() {
    let world = World::new("setup-missing-harness", Machine::default(), None);

    let result = world.setup();

    assert_eq!(
        world.machine.commands(),
        [
            format!("{INSTALLER} --latest"),
            "routed --version".to_owned()
        ],
        "the official install route, then its verification, and nothing else"
    );
    assert!(result.configured, "{result:?}");
    assert_eq!(result.provisioning.action, ProvisionAction::Install);
    assert_eq!(result.provisioning.status, ProvisionStatus::Verified);
    assert_eq!(result.detection.version.as_deref(), Some("1.2.3"));
    assert_eq!(world.provisioned.load(Ordering::SeqCst), 1);
    assert_eq!(world.prepared.load(Ordering::SeqCst), 1);
    assert!(state::is_installed(&world.home, "routed"));
    let record = state::provisioning(&world.home, "routed").unwrap().unwrap();
    assert_eq!(record.action, ProvisionAction::Install);
    assert_eq!(record.status, ProvisionStatus::Verified);
    assert_eq!(record.version.as_deref(), Some("1.2.3"));
}

#[test]
fn a_present_harness_is_updated_verified_then_prepared() {
    let world = World::new(
        "setup-present-harness",
        Machine::with_harness_installed(),
        None,
    );

    let result = world.setup();

    assert_eq!(
        world.machine.commands(),
        ["routed update", "routed --version"],
        "an installed harness takes its own update route, never the installer"
    );
    assert!(result.configured, "{result:?}");
    assert_eq!(result.provisioning.action, ProvisionAction::Update);
    assert_eq!(result.provisioning.status, ProvisionStatus::Verified);
    assert!(world.prepared.load(Ordering::SeqCst) >= 1);
    assert_eq!(
        state::provisioning(&world.home, "routed")
            .unwrap()
            .unwrap()
            .action,
        ProvisionAction::Update
    );
}

#[test]
fn a_failed_install_prepares_nothing_records_no_receipt_and_removes_nothing() {
    let world = World::new(
        "setup-failed-install",
        Machine::default(),
        Some("routed-official-installer --latest"),
    );
    world.add_fixture();
    let foreign = world.root.join("home/routed/skills/somebody-elses");
    fs::create_dir_all(&foreign).unwrap();
    let stored_before = world.stored();

    let result = world.setup();

    assert!(!result.configured, "{result:?}");
    assert_eq!(result.provisioning.status, ProvisionStatus::Failed);
    assert_eq!(
        world.machine.commands(),
        [format!("{INSTALLER} --latest")],
        "a failed installer is not followed by a verification"
    );
    assert_eq!(world.prepared.load(Ordering::SeqCst), 0);
    assert!(!state::is_installed(&world.home, "routed"));
    assert!(world.receipts().is_empty(), "{:?}", world.receipts());
    let stored_after = world.stored();
    assert!(
        stored_before.iter().all(|id| stored_after.contains(id)),
        "no package was removed: {stored_before:?} -> {stored_after:?}"
    );
    assert!(foreign.is_dir(), "nothing UZE does not own was touched");
    assert_eq!(
        state::provisioning(&world.home, "routed")
            .unwrap()
            .unwrap()
            .status,
        ProvisionStatus::Failed,
        "the attempt is history, and says it failed"
    );
}

#[test]
fn a_failed_update_blocks_preparation_and_keeps_what_was_delivered() {
    let world = World::new(
        "setup-failed-update",
        Machine::with_harness_installed(),
        Some("routed update"),
    );
    world.add_fixture();
    let delivered = world.receipts();
    assert!(!delivered.is_empty(), "a detected harness is delivered to");
    let stored_before = world.stored();

    let result = world.setup();

    assert!(!result.configured, "{result:?}");
    assert_eq!(result.provisioning.action, ProvisionAction::Update);
    assert_eq!(result.provisioning.status, ProvisionStatus::Failed);
    assert_eq!(
        world.machine.commands(),
        ["routed update"],
        "a failed update is not followed by a verification"
    );
    // The executable that was already there still works, so every command
    // keeps preparing it as a detected harness; what the failed update
    // withholds is setup's own claim that it verified one.
    let integration = world.app.integrations[0].as_ref();
    assert_eq!(
        integration.status(&world.home),
        uze_core::integration::IntegrationStatus::InstalledUnverified
    );
    assert_eq!(world.receipts(), delivered, "no receipt added or removed");
    let stored_after = world.stored();
    assert!(stored_before.iter().all(|id| stored_after.contains(id)));
    for receipt in &delivered {
        let ManagedArtifact::SymlinkReference { path, .. } = &receipt.artifact else {
            panic!("unexpected artifact {receipt:?}");
        };
        assert!(path.is_symlink(), "{} was removed", path.display());
    }
}

#[test]
fn a_package_added_before_the_harness_is_delivered_by_setup_exactly_once() {
    let world = World::new("setup-delivers-stored", Machine::default(), None);
    world.add_fixture();
    assert!(
        world.receipts().is_empty(),
        "an absent harness is not delivered to"
    );

    world.setup();
    let delivered = world.receipts();
    assert!(
        delivered
            .iter()
            .any(|receipt| receipt.package_id == "uze-agent-skill-conformance@local"),
        "setup delivers the package stored before the harness existed: {delivered:?}"
    );

    world.setup();
    assert_eq!(
        world.receipts(),
        delivered,
        "a second setup refreshes the same attachments rather than adding any"
    );
}

#[test]
fn add_never_provisions_even_when_every_harness_is_absent() {
    let root = uze_testkit::temp::scratch("add-never-provisions");
    let home = UzeHome::at(root.join("uze"));
    let machine = Machine::default();
    let harnesses = ["routed-a", "routed-b"]
        .map(|id| RoutedHarness::new(id, &home, &root.join("home"), &machine));
    let provisioned = harnesses
        .iter()
        .map(|harness| harness.provisioned.clone())
        .collect::<Vec<_>>();
    let app = UzeApplication::new_with_runner(
        home.clone(),
        harnesses
            .into_iter()
            .map(|harness| Box::new(harness) as Box<dyn IntegrationPort>)
            .collect(),
        Box::new(FakeRunner {
            machine: machine.clone(),
            failing: None,
        }),
    );

    app.plugins()
        .add(PackageSource::local(fixture()), &trust::AlwaysTrust)
        .unwrap();

    assert!(
        machine.commands().is_empty(),
        "add ran {:?}",
        machine.commands()
    );
    assert!(
        provisioned
            .iter()
            .all(|count| count.load(Ordering::SeqCst) == 0),
        "add never asks an integration to provision"
    );
    assert!(
        app.store
            .package_ids()
            .unwrap()
            .iter()
            .any(|id| id.as_str() == "uze-agent-skill-conformance@local"),
        "the package is stored once all the same"
    );
    assert!(state::provisioning(&home, "routed-a").unwrap().is_none());
    let _ = fs::remove_dir_all(root);
}

/// The environment a shim is placed in: the harness's executable on `PATH`
/// for the shim to resolve, and a shell with no startup file UZE knows, so
/// setup has none of the operator's to take a block back from.
fn with_the_executable_on_path(world: &World) -> uze_testkit::env::ProcessEnvGuard<'static> {
    let bin = world.root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = bin.join(uze_platform::executable::file_name("routed"));
    fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    uze_platform::executable::make_runnable(&executable).unwrap();
    let mut environment = uze_testkit::env::scope();
    environment.set("PATH", &bin);
    environment
}

#[test]
fn a_machine_with_no_harness_is_asked_which_to_provision() {
    let world = World::launched_through_a_shim("entry-no-harness", Machine::default());

    assert_eq!(world.app.workspace().entry(), WorkspaceEntry::Choose);
}

/// What every command prepares on its way through makes a harness able to
/// receive plugins, not one the workspace can launch — and what the
/// terminal runtime lays out under `state/` is not a machine meeting UZE
/// before: the two things that read as "set up" on a clean install.
#[test]
fn an_installed_harness_nobody_set_up_is_set_up_before_the_workspace_opens() {
    let world = World::launched_through_a_shim(
        "entry-installed-not-set-up",
        Machine::with_harness_installed(),
    );
    fs::create_dir_all(world.home.state_dir().join("terminal")).unwrap();

    assert_eq!(
        world.app.workspace().entry(),
        WorkspaceEntry::SetUp(vec!["routed".to_owned()])
    );
    assert!(
        !world.app.workspace().agent_identities()[0].configured,
        "not offered to launch before it is set up"
    );
}

#[test]
fn the_workspace_sets_up_what_is_installed_without_updating_it() {
    let world =
        World::launched_through_a_shim("entry-set-up-existing", Machine::with_harness_installed());
    let _environment = with_the_executable_on_path(&world);

    let mut results = world
        .app
        .setup_through(Some("routed"), ProvisionRoute::Existing)
        .unwrap();
    let result = results.remove(0);

    assert!(result.configured, "{result:?}");
    assert!(result.shim_error.is_none(), "{result:?}");
    assert!(
        world.machine.commands().is_empty(),
        "no vendor route was taken: {:?}",
        world.machine.commands()
    );
    assert_eq!(world.provisioned.load(Ordering::SeqCst), 0);
    assert_eq!(result.provisioning.action, ProvisionAction::None);
    assert!(
        world
            .home
            .shims_dir()
            .join(uze_platform::executable::file_name("routed"))
            .is_symlink()
    );
    assert_eq!(world.app.workspace().entry(), WorkspaceEntry::Ready);
    assert!(world.app.workspace().agent_identities()[0].configured);
    assert_eq!(world.app.workspace().launcher_names(), ["routed"]);
}

#[test]
fn a_launcher_gone_since_setup_is_set_up_again() {
    let world =
        World::launched_through_a_shim("entry-launcher-gone", Machine::with_harness_installed());
    let _environment = with_the_executable_on_path(&world);
    world
        .app
        .setup_through(Some("routed"), ProvisionRoute::Existing)
        .unwrap();

    fs::remove_file(
        world
            .home
            .shims_dir()
            .join(uze_platform::executable::file_name("routed")),
    )
    .unwrap();

    assert_eq!(
        world.app.workspace().entry(),
        WorkspaceEntry::SetUp(vec!["routed".to_owned()])
    );
    assert!(
        world.app.workspace().launcher_names().is_empty(),
        "a harness with no launcher cannot have bypassed one"
    );
}
