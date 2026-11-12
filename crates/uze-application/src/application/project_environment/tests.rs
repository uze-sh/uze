//! The project environment against a marketplace that is a local Git
//! repository: what `add`, `remove`, `update`, `install` and the reads
//! beside them leave in `agents.yaml`, `agents.lock` and the checkout.

use std::fs;
use std::path::{Path, PathBuf};

use uze_core::trust::AlwaysTrust;

use super::*;

const MARKET: &str = "acme";
const PLUGIN: &str = "flow";
/// A second plugin of the same marketplace.
const SECOND: &str = "tide";
const RULE: &str = "---\ntrigger: always_on\n---\nWrite tests first.\n";

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn built_in() -> &'static str {
    uze_core::manifest::BUILT_IN_MARKETPLACE
}

fn locked_at(marketplace: &str, integrity: Option<&str>) -> LockedPlugin {
    LockedPlugin::resolved(marketplace, integrity.map(str::to_owned))
}

fn manifest_declaring(plugins: &[(&str, &str)]) -> manifest::ProjectManifest {
    let mut manifest = manifest::ProjectManifest::default();
    for (plugin, marketplace) in plugins {
        manifest
            .marketplaces
            .entry((*marketplace).to_owned())
            .or_default()
            .plugins
            .push((*plugin).to_owned());
    }
    manifest
}

#[test]
fn a_linked_source_is_declared_relative_inside_the_project_and_absolute_outside() {
    let base = uze_testkit::temp::scratch("declared-path");
    let project = base.join("project");
    let inside = project.join("vendor").join("market");
    let outside = base.join("market");
    fs::create_dir_all(&inside).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let project = project.canonical().unwrap();

    assert_eq!(
        declared_path(inside, &project),
        Path::new("vendor").join("market")
    );
    assert_eq!(declared_path(project.clone(), &project), Path::new("."));
    assert_eq!(
        declared_path(outside.clone(), &project),
        outside.canonical().unwrap()
    );
    let _ = fs::remove_dir_all(base);
}

#[test]
fn only_a_relative_path_that_never_climbs_reproduces_in_a_clone() {
    assert!(stays_inside_the_project(Path::new("vendor/market")));
    assert!(stays_inside_the_project(Path::new(".")));
    assert!(!stays_inside_the_project(Path::new("../market")));
    assert!(!stays_inside_the_project(Path::new("vendor/../../market")));
    let absolute = std::env::temp_dir().join("market");
    assert!(!stays_inside_the_project(&absolute));
}

#[test]
fn a_locked_marketplace_is_declared_by_its_repository_and_a_linked_one_by_its_path() {
    let base = uze_testkit::temp::scratch("declared-marketplace");
    let project = base.join("project");
    let linked = project.join("market");
    fs::create_dir_all(&linked).unwrap();
    let project = project.canonical().unwrap();
    let mut lock = ProjectLock::default();
    lock.marketplaces.insert(
        "pinned".to_owned(),
        LockedMarketplace {
            git: "https://example.com/acme.git".to_owned(),
            r#ref: Some("main".to_owned()),
            subdirectory: None,
            revision: "abc".to_owned(),
        },
    );

    let pinned = declared_marketplace_for(&lock, "pinned", None, &project);
    assert_eq!(pinned.git.as_deref(), Some("https://example.com/acme.git"));
    assert_eq!(pinned.r#ref.as_deref(), Some("main"));
    assert_eq!(pinned.path, None);

    let linked = declared_marketplace_for(&lock, "linked", Some(linked), &project);
    assert_eq!(linked.git, None);
    assert_eq!(linked.path, Some(PathBuf::from("market")));
    let _ = fs::remove_dir_all(base);
}

#[test]
fn a_built_in_plugin_is_behind_unless_this_uze_locked_it() {
    let manifest = manifest_declaring(&[("workspace", built_in()), (PLUGIN, MARKET)]);
    let mut lock = ProjectLock::default();
    assert_eq!(
        built_ins_behind(&manifest, &lock),
        ["workspace"],
        "never locked"
    );

    lock.plugins.insert(
        "workspace".to_owned(),
        locked_at(built_in(), Some("sha256:a")),
    );
    lock.uze = Some("0.0.1".to_owned());
    assert_eq!(
        built_ins_behind(&manifest, &lock),
        ["workspace"],
        "locked by another uze"
    );

    lock.uze = Some(version().to_owned());
    assert!(built_ins_behind(&manifest, &lock).is_empty());

    lock.plugins
        .insert("workspace".to_owned(), locked_at(MARKET, None));
    assert_eq!(
        built_ins_behind(&manifest, &lock),
        ["workspace"],
        "locked from another marketplace"
    );
}

#[test]
fn the_uze_version_is_stamped_only_when_it_answers_for_every_built_in_plugin() {
    let mut lock = ProjectLock {
        uze: Some("0.0.1".to_owned()),
        ..ProjectLock::default()
    };
    lock.plugins
        .insert(PLUGIN.to_owned(), locked_at(MARKET, None));
    stamp_uze_version(&mut lock, &[]);
    assert_eq!(lock.uze, None, "no built-in plugin is locked");

    lock.plugins
        .insert("workspace".to_owned(), locked_at(built_in(), None));
    lock.plugins
        .insert("pingpong".to_owned(), locked_at(built_in(), None));
    lock.uze = Some("0.0.1".to_owned());
    stamp_uze_version(&mut lock, &["workspace"]);
    assert_eq!(
        lock.uze.as_deref(),
        Some("0.0.1"),
        "another build's version stays while one of its plugins is unresolved"
    );

    stamp_uze_version(&mut lock, &["workspace", "pingpong"]);
    assert_eq!(lock.uze.as_deref(), Some(version()));

    stamp_uze_version(&mut lock, &[]);
    assert_eq!(
        lock.uze.as_deref(),
        Some(version()),
        "a lock this uze already answers for keeps its version"
    );
}

#[test]
fn a_moved_package_is_named_by_its_new_source_or_else_its_content() {
    let revision = |content: Option<&str>, provenance: &str| InstalledRevision {
        content: content.map(str::to_owned),
        provenance: provenance.to_owned(),
    };
    let before = revision(Some("sha256:old"), "git@one");

    assert_eq!(
        revision(Some("sha256:new"), "git@two").named_against(&before),
        "git@two"
    );
    assert_eq!(
        revision(Some("sha256:new"), "git@one").named_against(&before),
        "new"
    );
    assert_eq!(revision(None, "git@one").named_against(&before), "git@one");
}

fn failed_delivery() -> HarnessDeliveryReport {
    HarnessDeliveryReport {
        integration: "harness".to_owned(),
        display_name: "Harness".to_owned(),
        outcome: HarnessDeliveryOutcome::Failed {
            error: "refused".to_owned(),
        },
    }
}

fn delivered() -> HarnessDeliveryReport {
    HarnessDeliveryReport {
        integration: "other".to_owned(),
        display_name: "Other".to_owned(),
        outcome: HarnessDeliveryOutcome::Delivered {
            route: DeliveryRoute::CapabilityByCapability {
                reason: "test".to_owned(),
            },
            attachments: Vec::new(),
            blocked: Vec::new(),
            shortfalls: Vec::new(),
        },
    }
}

fn update_report(outcomes: Vec<UpdateOutcome>) -> UpdateReport {
    UpdateReport {
        scope: UpdateScope::Project,
        outcomes,
        extensions: Vec::new(),
        rules: Vec::new(),
    }
}

#[test]
fn an_update_report_says_what_moved_what_was_blocked_and_what_went_undelivered() {
    let report = update_report(vec![
        UpdateOutcome::Moved {
            plugin: "moved".to_owned(),
            revision: "abc".to_owned(),
            deliveries: vec![failed_delivery(), delivered()],
        },
        UpdateOutcome::Blocked {
            plugin: "blocked".to_owned(),
            reason: "drifted".to_owned(),
        },
        UpdateOutcome::Held {
            plugin: "held".to_owned(),
            reason: "trust".to_owned(),
        },
    ]);

    assert!(report.moved());
    assert_eq!(report.blocked().collect::<Vec<_>>(), ["blocked"]);
    let undelivered: Vec<(&str, &str)> = report
        .undelivered()
        .map(|(plugin, delivery)| (plugin, delivery.integration.as_str()))
        .collect();
    assert_eq!(undelivered, [("moved", "harness")]);
    let plugins: Vec<&str> = report.outcomes.iter().map(UpdateOutcome::plugin).collect();
    assert_eq!(plugins, ["moved", "blocked", "held"]);

    let current = update_report(vec![UpdateOutcome::AlreadyCurrent {
        plugin: "current".to_owned(),
        deliveries: Vec::new(),
    }]);
    assert!(!current.moved());
    assert_eq!(current.blocked().count(), 0);
}

#[test]
fn an_install_report_names_each_harness_a_plugin_could_not_reach() {
    let report = InstallReport::Installed {
        plugins: vec![PLUGIN.to_owned()],
        removed: Vec::new(),
        skipped: Vec::new(),
        reconciled: false,
        undelivered: vec![(PLUGIN.to_owned(), failed_delivery())],
        rules: Vec::new(),
        extensions: Vec::new(),
    };
    let undelivered: Vec<&str> = report.undelivered().map(|(plugin, _)| plugin).collect();
    assert_eq!(undelivered, [PLUGIN]);
    assert_eq!(InstallReport::NoChanges.undelivered().count(), 0);
}

/// A marketplace holding one plugin that ships one rule, committed, and a
/// project in a repository of its own.
struct World {
    base: PathBuf,
    market: PathBuf,
    project: PathBuf,
}

impl World {
    fn new(label: &str) -> Self {
        let base = uze_testkit::temp::scratch(label);
        let world = Self {
            market: base.join("market"),
            project: base.join("project"),
            base,
        };
        let plugin = world.market.join(PLUGIN);
        fs::create_dir_all(plugin.join("skills").join("flow")).unwrap();
        fs::create_dir_all(plugin.join("rules")).unwrap();
        fs::write(
            world.market.join("marketplace.json"),
            r#"{"name": "acme", "plugins": [{"name": "flow", "source": "flow"}, {"name": "tide", "source": "tide"}]}"#,
        )
        .unwrap();
        let second = world.market.join(SECOND);
        fs::create_dir_all(second.join("rules")).unwrap();
        fs::write(second.join("plugin.json"), r#"{"name": "tide"}"#).unwrap();
        fs::write(second.join("rules").join("tide.md"), RULE).unwrap();
        fs::write(plugin.join("plugin.json"), r#"{"name": "flow"}"#).unwrap();
        fs::write(
            plugin.join("skills").join("flow").join("SKILL.md"),
            "---\nname: flow\ndescription: The flow.\n---\nBody\n",
        )
        .unwrap();
        fs::write(plugin.join("rules").join("tests.md"), RULE).unwrap();
        uze_testkit::git::commit_everything_in(&world.market);
        uze_testkit::git::mark_repository(&world.project);
        world
    }

    fn machine(&self, name: &str) -> UzeApplication {
        UzeApplication::new(UzeHome::at(self.base.join(name)), Vec::new())
    }

    /// A machine that registered the marketplace, as `uze market add` does.
    fn registered(&self, name: &str) -> UzeApplication {
        let app = self.machine(name);
        uze_core::state::marketplace_add(
            &app.home,
            MARKET,
            PackageSource::Local {
                path: self.market.clone(),
            },
        )
        .unwrap();
        app
    }

    fn add(&self, app: &UzeApplication) -> AddPluginReport {
        self.add_named(app, PLUGIN)
    }

    fn add_named(&self, app: &UzeApplication, plugin: &str) -> AddPluginReport {
        app.project()
            .add(
                plugin,
                MARKET,
                &self.project,
                &AlwaysTrust,
                &uze_core::naming::NoNameCollisionAuthority,
            )
            .unwrap()
    }

    fn add_built_in(&self, app: &UzeApplication) -> AddPluginReport {
        app.project()
            .add(
                "workspace",
                built_in(),
                &self.project,
                &AlwaysTrust,
                &uze_core::naming::NoNameCollisionAuthority,
            )
            .unwrap()
    }

    fn declare_at(&self, path: &Path) {
        fs::write(
            self.project.join("agents.yaml"),
            format!(
                "marketplaces:\n  {MARKET}:\n    path: {}\n    plugins:\n      - {PLUGIN}\n",
                path.display()
            ),
        )
        .unwrap();
    }

    fn lock(&self) -> Option<ProjectLock> {
        project_lock::load_lock(&self.project).unwrap()
    }

    fn declared(&self) -> Vec<(String, String)> {
        manifest::load(&self.project)
            .unwrap()
            .unwrap_or_default()
            .declared_plugins()
            .map(|(plugin, marketplace)| (plugin.to_owned(), marketplace.to_owned()))
            .collect()
    }

    fn copy(&self) -> PathBuf {
        self.project
            .join(".agents")
            .join("rules")
            .join("flow@acme")
            .join("tests.md")
    }

    fn move_market(&self) {
        fs::write(
            self.market.join(PLUGIN).join("rules").join("tests.md"),
            "---\ntrigger: always_on\n---\nWrite the test before the fix.\n",
        )
        .unwrap();
        fs::write(
            self.market.join(SECOND).join("rules").join("tide.md"),
            "---\ntrigger: always_on\n---\nThe tide moved.\n",
        )
        .unwrap();
        uze_testkit::git::commit_everything_in(&self.market);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn a_project_root_is_a_root_and_a_directory_inside_it_is_not() {
    let world = World::new("project-is-root");
    let app = world.machine("home");
    fs::create_dir_all(world.project.join("src")).unwrap();

    assert!(app.project().is_root(&world.project));
    assert!(!app.project().is_root(&world.project.join("src")));
    assert!(!app.project().is_root(&world.project.join("absent")));
}

#[test]
fn add_declares_pins_and_copies_and_remove_takes_all_three_back() {
    let world = World::new("project-add-remove");
    let app = world.registered("home");

    let added = world.add(&app);

    assert!(added.declared);
    assert_eq!(world.declared(), [(PLUGIN.to_owned(), MARKET.to_owned())]);
    let lock = world.lock().expect("add writes the lock");
    assert_eq!(lock.plugins[PLUGIN].marketplace, MARKET);
    assert!(lock.plugins[PLUGIN].integrity.is_some());
    assert!(!lock.marketplaces[MARKET].revision.is_empty());
    let written = added.rules.expect("the plugin's rule was copied").written;
    assert_eq!(written.len(), 1, "{written:?}");
    assert_eq!(fs::read_to_string(world.copy()).unwrap(), RULE);
    let plan = app.project().plan(&world.project).unwrap();
    assert!(!plan.has_changes, "{plan:?}");

    let elsewhere = app
        .project()
        .remove("flow@elsewhere", &world.project)
        .unwrap();
    assert!(
        matches!(elsewhere, RemoveProjectPluginReport::NotInLock { .. }),
        "{elsewhere:?}"
    );
    assert!(world.lock().is_some());

    let removed = app.project().remove(PLUGIN, &world.project).unwrap();
    match removed {
        RemoveProjectPluginReport::Removed { rules, .. } => {
            assert_eq!(rules.expect("the copy went").removed.len(), 1);
        }
        other => panic!("expected Removed, got {other:?}"),
    }
    assert!(!world.lock().unwrap().plugins.contains_key(PLUGIN));
    assert!(world.declared().is_empty());
    assert!(!world.copy().exists());

    let again = app.project().remove(PLUGIN, &world.project).unwrap();
    assert!(
        matches!(again, RemoveProjectPluginReport::NotInLock { .. }),
        "{again:?}"
    );
}

#[test]
fn removing_what_a_project_never_locked_finds_no_lock() {
    let world = World::new("project-remove-no-lock");
    let app = world.machine("home");

    let report = app.project().remove(PLUGIN, &world.project).unwrap();

    assert!(
        matches!(report, RemoveProjectPluginReport::NoLock),
        "{report:?}"
    );
}

#[test]
fn removing_a_plugin_the_lock_does_not_hold_says_so_and_keeps_the_rest() {
    let world = World::new("project-remove-unknown");
    let app = world.registered("home");
    world.add(&app);

    let unknown = app.project().remove("other", &world.project).unwrap();

    assert!(
        matches!(unknown, RemoveProjectPluginReport::NotInLock { .. }),
        "{unknown:?}"
    );
    assert!(world.lock().unwrap().plugins.contains_key(PLUGIN));
}

#[test]
fn a_plan_names_a_declaration_the_lock_does_not_answer_and_a_path_no_clone_has() {
    let world = World::new("project-plan-declared");
    fs::write(
        world.project.join("agents.yaml"),
        format!(
            "marketplaces:\n  {MARKET}:\n    path: {}\n    plugins:\n      - {PLUGIN}\n",
            world.market.display()
        ),
    )
    .unwrap();
    let app = world.machine("home");

    let plan = app.project().plan(&world.project).unwrap();

    assert_eq!(plan.unresolved, [PLUGIN]);
    assert_eq!(plan.unreproducible_marketplaces, [MARKET]);
    assert!(plan.missing.is_empty() && plan.surplus.is_empty());
    assert!(plan.has_changes);
}

#[test]
fn a_plan_trusts_a_path_inside_the_project_and_names_a_surplus_entry() {
    let world = World::new("project-plan-surplus");
    let app = world.registered("home");
    world.add(&app);
    fs::write(
        world.project.join("agents.yaml"),
        format!(
            "marketplaces:\n  {MARKET}:\n    path: {}\n    plugins: []\n  vendored:\n    path: vendor/market\n",
            world.market.display()
        ),
    )
    .unwrap();

    let plan = app.project().plan(&world.project).unwrap();

    assert_eq!(
        plan.unreproducible_marketplaces,
        [MARKET],
        "the vendored path reproduces"
    );
    assert_eq!(plan.surplus, [PLUGIN]);
    assert!(plan.unresolved.is_empty() && plan.missing.is_empty());
    assert!(plan.has_changes);
}

#[test]
fn a_plan_names_a_locked_plugin_this_machine_lacks() {
    let world = World::new("project-plan-missing");
    world.add(&world.registered("author"));
    let fresh = world.machine("fresh");

    let plan = fresh.project().plan(&world.project).unwrap();

    assert_eq!(plan.missing, [PLUGIN]);
    assert!(plan.has_changes);
}

#[test]
fn a_plan_names_a_region_an_earlier_release_left() {
    let world = World::new("project-plan-stale");
    let app = world.machine("home");
    uze_core::text_region::attach(
        &world.project.join("AGENTS.md"),
        "package:greet:tools:instructions",
        "hi\n",
    )
    .unwrap();

    let plan = app.project().plan(&world.project).unwrap();

    let stale = plan.stale_projection.expect("the region is named");
    assert_eq!(stale.projected_identity, "package:greet:tools:instructions");
    assert!(plan.has_changes);
}

#[test]
fn install_reproduces_the_lock_on_a_fresh_machine_and_then_has_nothing_to_do() {
    let world = World::new("project-install-fresh");
    world.add(&world.registered("author"));
    fs::remove_file(world.copy()).unwrap();
    let fresh = world.machine("fresh");

    let report = fresh
        .project()
        .install(&world.project, &AlwaysTrust)
        .unwrap();

    match report {
        InstallReport::Installed { plugins, rules, .. } => {
            assert_eq!(plugins, [PLUGIN]);
            assert_eq!(rules.len(), 1, "the deleted copy is restored: {rules:?}");
        }
        other => panic!("expected Installed, got {other:?}"),
    }
    assert_eq!(fs::read_to_string(world.copy()).unwrap(), RULE);
    match fresh.project().lock_status(&world.project) {
        ProjectLockStatus::Present { plugins } => {
            assert_eq!(plugins.len(), 1);
            assert!(plugins[0].installed);
            assert!(!plugins[0].differs_from_lock);
        }
        other => panic!("expected Present, got {other:?}"),
    }
    let again = fresh
        .project()
        .install(&world.project, &AlwaysTrust)
        .unwrap();
    assert!(matches!(again, InstallReport::NoChanges), "{again:?}");
}

#[test]
fn install_refuses_bytes_other_than_the_ones_the_lock_pinned() {
    let world = World::new("project-install-integrity");
    world.add(&world.registered("author"));
    let mut lock = world.lock().unwrap();
    lock.plugins.get_mut(PLUGIN).unwrap().integrity = Some("sha256:0000".to_owned());
    project_lock::save_lock(&world.project, &lock).unwrap();
    let fresh = world.machine("fresh");

    let refused = fresh.project().install(&world.project, &AlwaysTrust);

    assert!(
        matches!(refused, Err(UzeError::IntegrityMismatch { .. })),
        "{refused:?}"
    );
    assert!(fresh.plugins().list().unwrap().is_empty());
}

#[test]
fn install_resolves_what_only_the_manifest_declares_and_registers_its_marketplace() {
    let world = World::new("project-install-manifest");
    fs::write(
        world.project.join("agents.yaml"),
        format!(
            "marketplaces:\n  {MARKET}:\n    path: {}\n    plugins:\n      - {PLUGIN}\n",
            world.market.display()
        ),
    )
    .unwrap();
    let app = world.machine("home");

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    assert!(
        matches!(&report, InstallReport::Installed { plugins, .. } if plugins == &[PLUGIN]),
        "{report:?}"
    );
    let lock = world.lock().expect("install writes what it resolved");
    assert!(lock.plugins.contains_key(PLUGIN));
    assert!(lock.marketplaces.contains_key(MARKET));
    assert!(
        uze_core::state::marketplace_get(&app.home, MARKET)
            .unwrap()
            .is_some()
    );
}

#[test]
fn install_drops_from_the_lock_what_the_manifest_no_longer_declares() {
    let world = World::new("project-install-surplus");
    let app = world.registered("home");
    world.add(&app);
    fs::write(
        world.project.join("agents.yaml"),
        format!(
            "marketplaces:\n  {MARKET}:\n    path: {}\n    plugins: []\n",
            world.market.display()
        ),
    )
    .unwrap();

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    assert!(
        matches!(&report, InstallReport::Installed { removed, .. } if removed == &[PLUGIN]),
        "{report:?}"
    );
    assert!(!world.lock().unwrap().plugins.contains_key(PLUGIN));
}

#[test]
fn install_outside_a_project_has_nothing_to_bring_about() {
    let world = World::new("project-install-none");
    let outside = world.base.join("loose");
    fs::create_dir_all(&outside).unwrap();
    let app = world.machine("home");

    let report = app.project().install(&outside, &AlwaysTrust).unwrap();

    assert!(matches!(report, InstallReport::NoProject), "{report:?}");
}

#[test]
fn update_moves_the_pin_to_where_the_marketplace_is_now_and_the_copy_follows() {
    let world = World::new("project-update");
    let app = world.registered("home");
    world.add(&app);
    let before = world.lock().unwrap();
    world.move_market();

    let report = app
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();

    assert!(report.moved(), "{:?}", report.outcomes);
    let after = world.lock().unwrap();
    assert_ne!(
        after.marketplaces[MARKET].revision,
        before.marketplaces[MARKET].revision
    );
    assert_ne!(
        after.plugins[PLUGIN].integrity,
        before.plugins[PLUGIN].integrity
    );
    assert_eq!(report.rules.len(), 1, "{:?}", report.rules);
    assert!(
        fs::read_to_string(world.copy())
            .unwrap()
            .contains("before the fix")
    );

    let again = app
        .project()
        .update(&world.project, Some(PLUGIN), false, &AlwaysTrust)
        .unwrap();
    assert!(
        matches!(again.outcomes[..], [UpdateOutcome::AlreadyCurrent { .. }]),
        "{:?}",
        again.outcomes
    );
    assert!(again.rules.is_empty());
}

#[test]
fn updating_a_plugin_the_project_does_not_declare_is_refused() {
    let world = World::new("project-update-unknown");
    let app = world.registered("home");
    world.add(&app);

    let refused = app
        .project()
        .update(&world.project, Some("other"), false, &AlwaysTrust);

    assert!(
        matches!(refused, Err(UzeError::PluginNotUsedByProject { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_machine_past_the_lock_is_reported_as_differing_from_it() {
    let world = World::new("project-differs");
    let app = world.registered("home");
    world.add(&app);
    world.move_market();

    app.project()
        .update(&world.project, None, true, &AlwaysTrust)
        .unwrap();

    match app.project().lock_status(&world.project) {
        ProjectLockStatus::Present { plugins } => {
            assert!(plugins[0].installed);
            assert!(plugins[0].differs_from_lock, "{plugins:?}");
        }
        other => panic!("expected Present, got {other:?}"),
    }
}

#[test]
fn plugin_rules_judge_each_copy_and_name_a_namespace_nobody_declares() {
    let world = World::new("project-plugin-rules");
    let app = world.registered("home");
    world.add(&app);

    let clean = app.project().plugin_rules(&world.project);
    assert_eq!(clean.plugins.len(), 1);
    assert_eq!(clean.plugins[0].rules, Some(1));
    assert!(clean.plugins[0].files.is_empty(), "{clean:?}");
    assert!(clean.undeclared.is_empty());

    fs::write(world.copy(), "edited by hand\n").unwrap();
    let stray = world
        .project
        .join(".agents")
        .join("rules")
        .join("gone@acme");
    fs::create_dir_all(&stray).unwrap();
    fs::write(stray.join("old.md"), RULE).unwrap();

    let judged = app.project().plugin_rules(&world.project);
    let states: Vec<&str> = judged.plugins[0]
        .files
        .iter()
        .map(|finding| finding.state)
        .collect();
    assert_eq!(states, ["drifted"]);
    assert_eq!(judged.undeclared, ["gone@acme"]);
}

#[test]
fn adding_a_built_in_plugin_declares_it_and_locks_this_uzes_bytes() {
    let world = World::new("project-add-built-in");
    let app = world.machine("home");

    let report = app
        .project()
        .add(
            "workspace",
            built_in(),
            &world.project,
            &AlwaysTrust,
            &uze_core::naming::NoNameCollisionAuthority,
        )
        .unwrap();

    assert!(report.declared);
    assert_eq!(
        world.declared(),
        [("workspace".to_owned(), built_in().to_owned())]
    );
    let lock = world.lock().unwrap();
    assert_eq!(lock.plugins["workspace"].marketplace, built_in());
    assert!(lock.plugins["workspace"].integrity.is_some());
    assert_eq!(lock.uze.as_deref(), Some(version()));
    let plan = app.project().plan(&world.project).unwrap();
    assert!(plan.unresolved.is_empty(), "{plan:?}");

    let update = app
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();
    assert!(
        matches!(update.outcomes[..], [UpdateOutcome::AlreadyCurrent { .. }]),
        "{:?}",
        update.outcomes
    );

    let removed = app.project().remove("workspace", &world.project).unwrap();
    assert!(
        matches!(removed, RemoveProjectPluginReport::Removed { .. }),
        "{removed:?}"
    );
    assert!(world.lock().is_none(), "a lock left pinning nothing goes");
}

#[test]
fn a_built_in_plugin_another_marketplace_already_answers_for_is_refused() {
    let world = World::new("project-built-in-mismatch");
    let mut lock = ProjectLock::default();
    lock.plugins
        .insert("workspace".to_owned(), locked_at(MARKET, None));
    project_lock::save_lock(&world.project, &lock).unwrap();
    let app = world.machine("home");

    let refused = app.project().add(
        "workspace",
        built_in(),
        &world.project,
        &AlwaysTrust,
        &uze_core::naming::NoNameCollisionAuthority,
    );

    assert!(
        matches!(refused, Err(UzeError::MarketplaceMismatch { .. })),
        "{refused:?}"
    );
}

#[test]
fn install_settles_a_declared_built_in_plugin_and_says_so_once() {
    let world = World::new("project-install-built-in");
    fs::write(
        world.project.join("agents.yaml"),
        format!(
            "marketplaces:\n  {}:\n    plugins:\n      - workspace\n",
            built_in()
        ),
    )
    .unwrap();
    let app = world.machine("home");

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    assert!(
        matches!(&report, InstallReport::Installed { plugins, .. } if plugins == &["workspace"]),
        "{report:?}"
    );
    let lock = world.lock().unwrap();
    assert!(lock.plugins["workspace"].integrity.is_some());
    assert_eq!(lock.uze.as_deref(), Some(version()));
    let again = app.project().install(&world.project, &AlwaysTrust).unwrap();
    assert!(matches!(again, InstallReport::NoChanges), "{again:?}");
}

fn plugins_of(report: &UpdateReport) -> Vec<&str> {
    report.outcomes.iter().map(UpdateOutcome::plugin).collect()
}

#[test]
fn a_second_machine_installing_intact_copies_reports_the_plugins_it_installed() {
    let world = World::new("project-install-second");
    world.add(&world.registered("author"));
    let second = world.machine("second");

    let report = second
        .project()
        .install(&world.project, &AlwaysTrust)
        .unwrap();

    match report {
        InstallReport::Installed {
            plugins,
            rules,
            skipped,
            reconciled,
            ..
        } => {
            assert_eq!(plugins, [PLUGIN]);
            assert!(rules.is_empty() && skipped.is_empty() && !reconciled);
        }
        other => panic!("expected Installed, got {other:?}"),
    }
}

#[test]
fn a_marketplace_declared_where_this_machine_has_nothing_is_skipped_and_named() {
    let world = World::new("project-install-skipped");
    world.declare_at(&world.base.join("not-here"));
    let app = world.machine("home");

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    match report {
        InstallReport::Installed {
            plugins, skipped, ..
        } => {
            assert!(plugins.is_empty());
            assert_eq!(skipped.len(), 1);
            assert_eq!(skipped[0].plugin, PLUGIN);
            assert!(
                skipped[0].reason.contains("does not have"),
                "{}",
                skipped[0].reason
            );
        }
        other => panic!("expected Installed, got {other:?}"),
    }
}

// A directory another account owns is what this is about, and the root
// directory is one on Unix (Linux, macOS); Windows ownership is another model.
#[cfg(unix)]
#[test]
fn a_marketplace_declared_in_a_directory_another_account_owns_is_skipped() {
    let foreign = Path::new("/");
    if project_root::owned_here(foreign) {
        return;
    }
    let world = World::new("project-install-foreign");
    world.declare_at(foreign);
    let app = world.machine("home");

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    match report {
        InstallReport::Installed { skipped, .. } => {
            assert_eq!(skipped.len(), 1);
            assert!(
                skipped[0].reason.contains("another account"),
                "{}",
                skipped[0].reason
            );
        }
        other => panic!("expected Installed, got {other:?}"),
    }
}

#[test]
fn an_install_that_only_takes_out_an_earlier_region_says_it_reconciled() {
    let world = World::new("project-install-reconcile");
    let app = world.registered("home");
    world.add(&app);
    uze_core::text_region::attach(
        &world.project.join("AGENTS.md"),
        "package:greet:tools:instructions",
        "hi\n",
    )
    .unwrap();

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    match report {
        InstallReport::Installed {
            plugins,
            reconciled,
            ..
        } => {
            assert!(plugins.is_empty());
            assert!(reconciled);
        }
        other => panic!("expected Installed, got {other:?}"),
    }
}

#[test]
fn a_machine_update_says_what_moved_and_what_was_already_current() {
    let world = World::new("project-update-machine");
    let app = world.registered("home");
    world.add(&app);

    let current = app
        .project()
        .update(&world.project, None, true, &AlwaysTrust)
        .unwrap();
    assert_eq!(current.scope, UpdateScope::Machine);
    assert!(
        current
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, UpdateOutcome::AlreadyCurrent { .. })),
        "{:?}",
        current.outcomes
    );

    world.move_market();
    let moved = app
        .project()
        .update(&world.project, Some("flow@acme"), true, &AlwaysTrust)
        .unwrap();
    assert!(
        matches!(&moved.outcomes[..], [UpdateOutcome::Moved { plugin, .. }] if plugin == "flow@acme"),
        "{:?}",
        moved.outcomes
    );
}

#[test]
fn install_raises_a_machine_behind_the_lock_and_leaves_it_level_after() {
    let world = World::new("project-rise");
    let author = world.registered("author");
    world.add(&author);
    let behind = world.machine("behind");
    behind
        .project()
        .install(&world.project, &AlwaysTrust)
        .unwrap();
    world.move_market();
    author
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();

    let report = behind
        .project()
        .install(&world.project, &AlwaysTrust)
        .unwrap();

    assert!(
        matches!(&report, InstallReport::Installed { plugins, .. } if plugins == &[PLUGIN]),
        "{report:?}"
    );
    match behind.project().lock_status(&world.project) {
        ProjectLockStatus::Present { plugins } => assert!(!plugins[0].differs_from_lock),
        other => panic!("expected Present, got {other:?}"),
    }
    let again = behind
        .project()
        .install(&world.project, &AlwaysTrust)
        .unwrap();
    assert!(matches!(again, InstallReport::NoChanges), "{again:?}");
}

#[test]
fn two_plugins_of_one_marketplace_both_move_in_one_update() {
    let world = World::new("project-update-two");
    let app = world.registered("home");
    world.add(&app);
    world.add_named(&app, SECOND);
    world.move_market();

    let report = app
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();

    assert!(
        report
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, UpdateOutcome::Moved { .. })),
        "{:?}",
        report.outcomes
    );
    assert_eq!(report.outcomes.len(), 2);
}

#[test]
fn updating_one_plugin_considers_that_plugin_alone() {
    let world = World::new("project-update-one");
    let app = world.registered("home");
    world.add(&app);
    world.add_built_in(&app);

    let report = app
        .project()
        .update(&world.project, Some(PLUGIN), false, &AlwaysTrust)
        .unwrap();

    assert_eq!(plugins_of(&report), [PLUGIN]);
}

#[test]
fn a_built_in_plugin_locked_at_other_bytes_differs_and_is_brought_back() {
    let world = World::new("project-built-in-other-bytes");
    let app = world.machine("home");
    world.add_built_in(&app);
    let pinned = world.lock().unwrap().plugins["workspace"].integrity.clone();
    let tamper = || {
        let mut lock = world.lock().unwrap();
        lock.plugins.get_mut("workspace").unwrap().integrity = Some("sha256:other".to_owned());
        project_lock::save_lock(&world.project, &lock).unwrap();
    };
    tamper();

    match app.project().lock_status(&world.project) {
        ProjectLockStatus::Present { plugins } => assert!(plugins[0].differs_from_lock),
        other => panic!("expected Present, got {other:?}"),
    }
    let installed = app.project().install(&world.project, &AlwaysTrust).unwrap();
    assert!(
        matches!(&installed, InstallReport::Installed { plugins, .. } if plugins == &["workspace"]),
        "{installed:?}"
    );
    assert_eq!(world.lock().unwrap().plugins["workspace"].integrity, pinned);

    tamper();
    let updated = app
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();
    assert!(
        matches!(updated.outcomes[..], [UpdateOutcome::Moved { .. }]),
        "{:?}",
        updated.outcomes
    );
    assert_eq!(world.lock().unwrap().plugins["workspace"].integrity, pinned);
}

#[test]
fn a_bundled_plugin_is_installed_for_the_machine_and_never_declared() {
    let world = World::new("project-add-bundled");
    let app = world.machine("home");

    let report = app
        .project()
        .add(
            "uze",
            built_in(),
            &world.project,
            &AlwaysTrust,
            &uze_core::naming::NoNameCollisionAuthority,
        )
        .unwrap();

    assert!(!report.declared);
    assert!(world.declared().is_empty());
    assert!(world.lock().is_none());
}

#[test]
fn a_plugin_the_lock_draws_from_another_marketplace_is_not_added_over_it() {
    let world = World::new("project-add-mismatch");
    let app = world.registered("home");
    let mut lock = ProjectLock::default();
    lock.plugins
        .insert(PLUGIN.to_owned(), locked_at("elsewhere", None));
    project_lock::save_lock(&world.project, &lock).unwrap();

    let refused = app.project().add(
        PLUGIN,
        MARKET,
        &world.project,
        &AlwaysTrust,
        &uze_core::naming::NoNameCollisionAuthority,
    );

    assert!(
        matches!(refused, Err(UzeError::MarketplaceMismatch { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_marketplace_the_lock_records_at_another_repository_is_a_conflict() {
    let world = World::new("project-add-conflict");
    let app = world.registered("home");
    let mut lock = ProjectLock::default();
    lock.marketplaces.insert(
        MARKET.to_owned(),
        LockedMarketplace {
            git: "https://example.com/another.git".to_owned(),
            r#ref: None,
            subdirectory: None,
            revision: "abc".to_owned(),
        },
    );
    project_lock::save_lock(&world.project, &lock).unwrap();

    let refused = app.project().add(
        PLUGIN,
        MARKET,
        &world.project,
        &AlwaysTrust,
        &uze_core::naming::NoNameCollisionAuthority,
    );

    assert!(
        matches!(refused, Err(UzeError::MarketplaceSourceConflict { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_declaration_no_lock_answers_for_is_still_removed() {
    let world = World::new("project-remove-declared-only");
    world.declare_at(&world.market);
    let app = world.machine("home");

    let report = app.project().remove(PLUGIN, &world.project).unwrap();

    assert!(
        matches!(report, RemoveProjectPluginReport::Removed { .. }),
        "{report:?}"
    );
    assert!(world.declared().is_empty());
    assert!(world.lock().is_none());
}

#[test]
fn a_locked_plugin_the_manifest_no_longer_declares_is_still_removed() {
    let world = World::new("project-remove-locked-only");
    let app = world.registered("home");
    world.add(&app);
    fs::write(world.project.join("agents.yaml"), "").unwrap();

    let report = app.project().remove(PLUGIN, &world.project).unwrap();

    assert!(
        matches!(report, RemoveProjectPluginReport::Removed { .. }),
        "{report:?}"
    );
    assert!(!world.lock().unwrap().plugins.contains_key(PLUGIN));
}

/// A harness an earlier release bridged `AGENTS.md` into.
struct Bridged;

impl IntegrationPort for Bridged {
    fn id(&self) -> &'static str {
        "bridged"
    }

    fn capabilities(&self) -> uze_core::router::HarnessCapabilities {
        uze_core::router::HarnessCapabilities::default()
    }

    fn exposure_plan(
        &self,
        _resource: &uze_core::capability::Resource,
    ) -> uze_core::exposure::ExposurePlan {
        uze_core::exposure::ExposurePlan {
            route: uze_core::router::CompatibilityRoute::Adaptable,
            mechanism: uze_core::exposure::ExposureMechanism::Unsupported {
                rationale: "context only".to_owned(),
            },
            evidence: "test".to_owned(),
        }
    }

    fn attach_receipt(
        &self,
        _resource: &uze_core::capability::Resource,
    ) -> Result<Option<uze_core::integration::AttachmentReceipt>> {
        Ok(None)
    }

    fn context_delivery(&self) -> uze_core::integration::ContextDelivery {
        uze_core::integration::ContextDelivery::Bridge {
            file_name: "VENDOR.md",
        }
    }
}

#[test]
fn a_plan_names_a_bridge_left_in_a_harnesss_file_and_not_one_edited_by_hand() {
    let world = World::new("project-plan-bridge");
    let app = UzeApplication::new(
        UzeHome::at(world.base.join("home")),
        vec![Box::new(Bridged)],
    );
    let vendor = world.project.join("VENDOR.md");
    uze_core::text_region::attach(
        &vendor,
        uze_core::project_earlier::INSTRUCTION_BRIDGE,
        uze_core::project_earlier::INSTRUCTION_BRIDGE_CONTENT,
    )
    .unwrap();

    let plan = app.project().plan(&world.project).unwrap();
    let stale = plan.stale_projection.expect("the bridge is named");
    assert_eq!(
        stale.projected_identity,
        uze_core::project_earlier::INSTRUCTION_BRIDGE
    );

    let text = fs::read_to_string(&vendor).unwrap();
    fs::write(
        &vendor,
        text.replace("@AGENTS.md", "@AGENTS.md\n@docs/more.md"),
    )
    .unwrap();
    let edited = app.project().plan(&world.project).unwrap();
    assert!(edited.stale_projection.is_none(), "{edited:?}");
}

#[test]
fn a_linked_marketplace_is_followed_unpinned_and_its_pin_comes_back_after_unlink() {
    let world = World::new("project-linked");
    let app = world.registered("home");
    world.add(&app);
    let locked = world.lock().unwrap();
    app.marketplace().link(MARKET, &world.market).unwrap();

    let unchanged = app
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();
    assert!(
        matches!(unchanged.outcomes[..], [UpdateOutcome::Held { .. }]),
        "a working copy pins nothing: {:?}",
        unchanged.outcomes
    );

    fs::write(
        world
            .market
            .join(PLUGIN)
            .join("skills")
            .join("flow")
            .join("SKILL.md"),
        "---\nname: flow\ndescription: The flow.\n---\nEdited, never committed\n",
    )
    .unwrap();
    let edited = app
        .project()
        .update(&world.project, None, false, &AlwaysTrust)
        .unwrap();
    match &edited.outcomes[..] {
        [UpdateOutcome::FollowedLink { linked_source, .. }] => assert_eq!(
            uze_platform::path::canonical(linked_source).unwrap(),
            uze_platform::path::canonical(&world.market).unwrap()
        ),
        other => panic!("expected FollowedLink, got {other:?}"),
    }
    assert_eq!(world.lock().unwrap(), locked, "nothing was pinned");

    assert!(app.marketplace().unlink(MARKET).unwrap());
    let restored = app.project().install(&world.project, &AlwaysTrust).unwrap();
    assert!(
        matches!(&restored, InstallReport::Installed { plugins, .. } if plugins == &[PLUGIN]),
        "the commit pinned before the link is put back: {restored:?}"
    );
}

#[test]
fn a_machine_whose_built_in_bytes_are_not_this_uzes_is_brought_to_them() {
    let world = World::new("project-built-in-replaced");
    let app = world.machine("home");
    world.add_built_in(&app);
    let stored = app
        .plugins()
        .list()
        .unwrap()
        .into_iter()
        .find(|plugin| plugin.id == "workspace@uze")
        .expect("the built-in plugin is installed")
        .store_path;
    let manifest = stored.join("plugin.json");
    let original = fs::read(&manifest).unwrap();
    let mut permissions = fs::metadata(&manifest).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&manifest, permissions).unwrap();
    fs::write(
        &manifest,
        b"{\"name\": \"workspace\", \"description\": \"another build\"}",
    )
    .unwrap();

    let report = app.project().install(&world.project, &AlwaysTrust).unwrap();

    assert!(
        matches!(&report, InstallReport::Installed { plugins, .. } if plugins == &["workspace"]),
        "{report:?}"
    );
    let now = app
        .plugins()
        .list()
        .unwrap()
        .into_iter()
        .find(|plugin| plugin.id == "workspace@uze")
        .unwrap()
        .store_path;
    assert_eq!(fs::read(now.join("plugin.json")).unwrap(), original);
}
