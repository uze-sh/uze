//! A plugin's rules in the projects that declare it (plugin-rules): copied
//! byte for byte into a folder of the plugin's own under the declaring
//! checkout's `.agents/rules/`, registered in `.agents/rules.json`, judged against the revision `agents.lock` pins, and
//! carried by add, install, update and remove without ever overwriting a
//! copy somebody edited.

use std::{
    fs,
    path::{Path, PathBuf},
};

use uze_application::{
    UzeApplication,
    application::{InstallReport, RemoveProjectPluginReport},
};
use uze_core::{UzeHome, project_lock, trust::AlwaysTrust};

const MARKET: &str = "acme";
const PLUGIN: &str = "acme-conventions";
const UI: &str = "---\ntrigger: glob\nglobs: **/ui/**\n---\nUI components live in ui/.\n";
const OWN: &str = "---\ntrigger: manual\n---\nThe project's own.\n";
const LEGACY: &str = "---\ntrigger: always_on\n---\nThis organization writes tests first.\n";

const MANIFEST: &str = ".agents/rules.json";

fn folder() -> String {
    format!(".agents/rules/{PLUGIN}@{MARKET}")
}

fn copy(rule: &str) -> String {
    format!("{}/{rule}", folder())
}

fn manifest(project: &Path) -> serde_json::Value {
    serde_json::from_str(&read(project, MANIFEST)).unwrap()
}

struct World {
    base: PathBuf,
    market: PathBuf,
}

impl World {
    fn new(label: &str) -> Self {
        let base = uze_testkit::temp::scratch(label);
        let world = Self {
            market: base.join("market"),
            base,
        };
        let plugin = world.market.join(PLUGIN);
        fs::create_dir_all(plugin.join("skills/conventions")).unwrap();
        fs::create_dir_all(plugin.join("rules")).unwrap();
        fs::write(
            world.market.join("marketplace.json"),
            format!(r#"{{"name": "{MARKET}", "plugins": [{{"name": "{PLUGIN}", "source": "{PLUGIN}"}}]}}"#),
        )
        .unwrap();
        fs::write(
            plugin.join("plugin.json"),
            format!(r#"{{"name": "{PLUGIN}"}}"#),
        )
        .unwrap();
        fs::write(
            plugin.join("skills/conventions/SKILL.md"),
            "---\nname: conventions\ndescription: The conventions.\n---\nBody\n",
        )
        .unwrap();
        fs::write(plugin.join("rules/ui.md"), UI).unwrap();
        fs::write(plugin.join("rules/legacy.md"), LEGACY).unwrap();
        uze_testkit::git::commit_everything_in(&world.market);
        world
    }

    fn project(&self, name: &str) -> PathBuf {
        let root = self.base.join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        root
    }

    fn machine(&self, name: &str) -> UzeApplication {
        UzeApplication::new(UzeHome::at(self.base.join(name)), Vec::new())
    }

    /// A machine that registered the marketplace, as `uze market add` does.
    fn registered_machine(&self, name: &str) -> UzeApplication {
        let app = self.machine(name);
        uze_core::state::marketplace_add(
            &UzeHome::at(self.base.join(name)),
            MARKET,
            uze_core::PackageSource::Local {
                path: self.market.clone(),
            },
        )
        .unwrap();
        app
    }

    fn add(
        &self,
        app: &UzeApplication,
        project: &Path,
    ) -> uze_application::application::AddPluginReport {
        app.project()
            .add(
                PLUGIN,
                MARKET,
                project,
                &AlwaysTrust,
                &uze_application::NoNameCollisionAuthority,
            )
            .unwrap()
    }

    fn change_rules(&self, edit: impl FnOnce(&Path)) {
        edit(&self.market.join(PLUGIN).join("rules"));
        uze_testkit::git::commit_everything_in(&self.market);
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn read(project: &Path, relative: &str) -> String {
    fs::read_to_string(project.join(relative)).unwrap()
}

fn plugin_status(
    app: &UzeApplication,
    project: &Path,
) -> uze_application::application::PluginRulesStatus {
    let report = app.project().plugin_rules(project);
    assert_eq!(report.plugins.len(), 1, "{report:?}");
    report.plugins.into_iter().next().unwrap()
}

#[test]
fn rules_land_byte_for_byte_in_the_declaring_project_only() {
    let world = World::new("plugin-rules-declaring-only");
    let app = world.registered_machine("home");
    let declaring = world.project("a");
    let other = world.project("b");
    fs::create_dir_all(declaring.join(".agents/rules")).unwrap();
    fs::write(declaring.join(".agents/rules/ui.md"), OWN).unwrap();

    let report = world.add(&app, &declaring);

    assert_eq!(read(&declaring, &copy("ui.md")), UI);
    assert_eq!(read(&declaring, &copy("legacy.md")), LEGACY);
    assert_eq!(
        read(&declaring, ".agents/rules/ui.md"),
        OWN,
        "the project's own rule with the same name is untouched"
    );
    let rules = report.rules.expect("the copies are reported");
    assert_eq!(rules.written, [copy("legacy.md"), copy("ui.md")]);
    assert_eq!(
        rules.manifest,
        uze_application::application::RulesManifestChange::Registered
    );
    assert_eq!(
        manifest(&declaring),
        serde_json::json!({"entries": [{"path": folder()}]}),
        "the folder is registered, and the manifest holds nothing else"
    );
    assert!(
        !other.join(".agents").exists(),
        "a project that does not declare the plugin receives nothing"
    );
    let discovered = app.project().rules(&declaring).unwrap();
    assert_eq!(
        discovered.rules.len(),
        3,
        "both are delivered: {discovered:?}"
    );
    assert_eq!(discovered.own().count(), 1);
    assert!(
        !fs::read_to_string(declaring.join(".git/info/exclude"))
            .unwrap_or_default()
            .contains(".agents"),
        "the copies are committed, never ignored"
    );
}

#[test]
fn a_machine_install_writes_into_no_project() {
    let world = World::new("plugin-rules-machine");
    let app = world.registered_machine("home");
    let project = world.project("a");
    let cwd = std::env::current_dir().unwrap();

    app.marketplace()
        .install_plugin_resolving(
            &format!("{PLUGIN}@{MARKET}"),
            &AlwaysTrust,
            &uze_application::NoNameCollisionAuthority,
        )
        .unwrap();

    assert!(!project.join(".agents").exists());
    assert!(!cwd.join(folder()).exists());
}

#[test]
fn a_fresh_machine_judges_a_cloned_project_without_installing() {
    let world = World::new("plugin-rules-fresh-clone");
    let project = world.project("a");
    world.add(&world.registered_machine("first"), &project);
    fs::write(project.join(copy("legacy.md")), "edited\n").unwrap();

    // What a clone carries: the committed files, in a checkout of its own.
    let clone = world.project("clone");
    for committed in ["agents.yaml", "agents.lock"] {
        fs::copy(project.join(committed), clone.join(committed)).unwrap();
    }
    uze_testkit::fixtures::copy_tree(&project.join(".agents"), &clone.join(".agents"));
    let project = clone;

    let fresh_home = world.base.join("fresh");
    let fresh = world.machine("fresh");
    assert!(fresh.plugins().list().unwrap().is_empty());
    let status = plugin_status(&fresh, &project);

    assert_eq!(status.plugin, format!("{PLUGIN}@{MARKET}"));
    assert_eq!(status.rules, Some(2));
    assert_eq!(status.unverified, None);
    assert_eq!(status.files.len(), 1, "{status:?}");
    assert_eq!(status.files[0].path, copy("legacy.md"));
    assert_eq!(status.files[0].state, "drifted");
    assert!(
        fresh.plugins().list().unwrap().is_empty(),
        "judging acquires the locked revision read-only and installs nothing"
    );
    let stored = UzeHome::at(&fresh_home).plugins_dir();
    assert!(
        fs::read_dir(&stored).map_or(true, |mut entries| entries.next().is_none()),
        "nothing reached the fresh machine's Store: {}",
        stored.display()
    );
}

#[test]
fn without_the_locked_revision_the_copies_are_unverified() {
    let world = World::new("plugin-rules-unverified");
    let project = world.project("a");
    world.add(&world.registered_machine("first"), &project);
    fs::remove_dir_all(&world.market).unwrap();

    let status = plugin_status(&world.machine("offline"), &project);

    assert!(status.unverified.is_some(), "{status:?}");
    assert_eq!(status.rules, None);
}

#[test]
fn a_hand_edit_survives_install_and_a_deleted_copy_is_restored() {
    let world = World::new("plugin-rules-install");
    let app = world.registered_machine("home");
    let project = world.project("a");
    world.add(&app, &project);
    fs::write(project.join(copy("ui.md")), "edited by hand\n").unwrap();
    fs::remove_file(project.join(copy("legacy.md"))).unwrap();

    let report = app.project().install(&project, &AlwaysTrust).unwrap();

    let InstallReport::Installed { rules, .. } = report else {
        panic!("a drifted and a missing copy are something to report: {report:?}");
    };
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].drifted, [copy("ui.md")]);
    assert_eq!(rules[0].written, [copy("legacy.md")]);
    assert_eq!(read(&project, &copy("ui.md")), "edited by hand\n");
    assert_eq!(read(&project, &copy("legacy.md")), LEGACY);
}

#[test]
fn an_update_is_a_reviewable_diff_of_the_copies() {
    let world = World::new("plugin-rules-update");
    let app = world.registered_machine("home");
    let project = world.project("a");
    world.add(&app, &project);
    let integrity_before = project_lock::load_lock(&project).unwrap().unwrap().plugins[PLUGIN]
        .integrity
        .clone();
    let changed = "---\ntrigger: glob\nglobs: **/ui/**\n---\nChanged.\n";
    world.change_rules(|rules| {
        fs::write(rules.join("ui.md"), changed).unwrap();
        fs::remove_file(rules.join("legacy.md")).unwrap();
        fs::write(rules.join("new.md"), LEGACY).unwrap();
    });

    let report = app
        .project()
        .update(&project, Some(PLUGIN), false, &AlwaysTrust)
        .unwrap();

    assert_eq!(report.rules.len(), 1, "{report:?}");
    assert_eq!(report.rules[0].removed, [copy("legacy.md")]);
    assert_eq!(report.rules[0].written, [copy("new.md"), copy("ui.md")]);
    assert_eq!(read(&project, &copy("ui.md")), changed);
    assert!(!project.join(copy("legacy.md")).exists());
    let integrity_after = project_lock::load_lock(&project).unwrap().unwrap().plugins[PLUGIN]
        .integrity
        .clone();
    assert_ne!(
        integrity_before, integrity_after,
        "the lock's integrity covers the rules"
    );
    let store = uze_core::UzeStore::new(UzeHome::at(world.base.join("home")));
    let id = store.find_by_active_name(PLUGIN).unwrap().unwrap();
    let stored = store.package(&id).unwrap();
    assert_eq!(
        fs::read_to_string(stored.root.join("rules/ui.md")).unwrap(),
        changed,
        "the Store keeps the rule's bytes verbatim"
    );
    assert!(plugin_status(&app, &project).files.is_empty());
}

#[test]
fn an_update_refuses_to_overwrite_or_remove_a_drifted_copy() {
    let world = World::new("plugin-rules-update-drift");
    let app = world.registered_machine("home");
    let project = world.project("a");
    world.add(&app, &project);
    fs::write(project.join(copy("ui.md")), "edited by hand\n").unwrap();
    fs::write(project.join(copy("legacy.md")), "also edited\n").unwrap();
    world.change_rules(|rules| {
        fs::write(rules.join("ui.md"), "---\ntrigger: manual\n---\nNew.\n").unwrap();
        fs::remove_file(rules.join("legacy.md")).unwrap();
    });

    let report = app
        .project()
        .update(&project, Some(PLUGIN), false, &AlwaysTrust)
        .unwrap();

    assert_eq!(
        report.rules[0].drifted,
        [copy("legacy.md"), copy("ui.md")],
        "{report:?}"
    );
    assert_eq!(read(&project, &copy("ui.md")), "edited by hand\n");
    assert_eq!(read(&project, &copy("legacy.md")), "also edited\n");
}

#[test]
fn removing_the_plugin_removes_its_rules_only() {
    let world = World::new("plugin-rules-remove");
    let app = world.registered_machine("home");
    let project = world.project("a");
    fs::create_dir_all(project.join(".agents/rules")).unwrap();
    fs::write(project.join(".agents/rules/own.md"), LEGACY).unwrap();
    world.add(&app, &project);
    fs::write(project.join(copy("mine.md")), "an extra file\n").unwrap();

    let report = app.project().remove(PLUGIN, &project).unwrap();

    let RemoveProjectPluginReport::Removed {
        rules: Some(rules), ..
    } = report
    else {
        panic!("the copies are reported: {report:?}");
    };
    assert_eq!(rules.removed, [copy("legacy.md"), copy("ui.md")]);
    assert_eq!(rules.extra, [copy("mine.md")]);
    assert!(!project.join(copy("ui.md")).exists());
    assert_eq!(read(&project, ".agents/rules/own.md"), LEGACY);
    assert!(
        project.join(MANIFEST).exists(),
        "a folder still holding a file keeps the entry that has it read"
    );
    assert_eq!(
        app.project().plugin_rules(&project).undeclared,
        [format!("{PLUGIN}@{MARKET}")],
        "the file left behind is named as an undeclared plugin's"
    );
}

#[test]
fn the_operators_entries_are_kept() {
    let world = World::new("plugin-rules-operator-manifest");
    let app = world.registered_machine("home");
    let project = world.project("a");
    fs::create_dir_all(project.join(".agents/rules/team")).unwrap();
    fs::write(project.join(".agents/rules/team/own.md"), OWN).unwrap();
    let operator = r#"{"entries": [{"path": ".agents/rules/team"}], "note": "the team's"}"#;
    fs::write(project.join(MANIFEST), operator).unwrap();

    world.add(&app, &project);

    let written = manifest(&project);
    assert_eq!(
        written["entries"][0],
        serde_json::json!({"path": ".agents/rules/team"})
    );
    assert_eq!(written["entries"][1], serde_json::json!({"path": folder()}));
    assert_eq!(written["note"], "the team's");
    let discovered = app.project().rules(&project).unwrap();
    assert_eq!(discovered.rules.len(), 3, "{discovered:?}");

    app.project().remove(PLUGIN, &project).unwrap();
    assert_eq!(
        manifest(&project),
        serde_json::from_str::<serde_json::Value>(operator).unwrap(),
        "only UZE's entry went"
    );
    assert!(!project.join(folder()).exists());
}

#[test]
fn an_unreadable_manifest_is_never_written() {
    let world = World::new("plugin-rules-broken-manifest");
    let app = world.registered_machine("home");
    let project = world.project("a");
    fs::create_dir_all(project.join(".agents")).unwrap();
    let broken = "{ \"entries\": [ { \"path\": \".agents/rules/team\" } ";
    fs::write(project.join(MANIFEST), broken).unwrap();

    let report = world.add(&app, &project);

    let rules = report.rules.expect("the manifest is reported");
    assert!(
        matches!(
            rules.manifest,
            uze_application::application::RulesManifestChange::Unreadable(_)
        ),
        "{rules:?}"
    );
    assert!(rules.written.is_empty());
    assert_eq!(read(&project, MANIFEST), broken, "its bytes are kept");
    assert!(!project.join(folder()).exists());
    let discovered = app.project().rules(&project).unwrap();
    assert!(discovered.unreadable_manifest().is_some());
}

#[test]
fn removing_the_last_plugin_takes_its_entry_back() {
    let world = World::new("plugin-rules-remove-last");
    let app = world.registered_machine("home");
    let project = world.project("a");
    world.add(&app, &project);
    assert!(project.join(MANIFEST).exists());

    app.project().remove(PLUGIN, &project).unwrap();

    assert!(!project.join(folder()).exists(), "the folder is gone");
    assert!(!project.join(MANIFEST).exists(), "and the manifest with it");
}
