//! What the CLI says about the extensions installed plugins declare: `uze
//! status` and `uze inspect` report each one's state, and `uze install`
//! off a terminal leaves one that runs code disabled and says where to
//! enable it (`extension-consent`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use uze_testkit::process::{IsolatedHome, NotLaunchedByUze};

const BAND: &str = "gsd@studio/gsd-band";

/// A machine with the `gsd` fixture plugin's marketplace added, and a
/// project that is a Git repository of its own.
struct World {
    root: PathBuf,
    project: PathBuf,
}

impl World {
    /// `band` rewrites the `gsd-band` manifest before the marketplace is
    /// staged, as an author's next release would.
    fn new(label: &str, band: impl FnOnce(&str) -> String) -> Self {
        let root = uze_testkit::temp::scratch(label);
        let plugin = root.join("source/gsd");
        uze_testkit::fixtures::copy_tree(&uze_testkit::fixtures::canonical("gsd"), &plugin);
        let manifest = plugin.join("extensions/gsd-band/extension.kdl");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, band(&text)).unwrap();
        let marketplace = serde_json::json!({
            "name": "studio",
            "plugins": [{ "name": "gsd", "source": "./plugins/gsd", "description": "GSD." }],
        });
        uze_testkit::marketplace::stage(
            &root.join("market"),
            &marketplace.to_string(),
            &[("gsd".to_owned(), plugin)],
        );
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        uze_testkit::git::isolated_git_in(&project, &["init", "-q"]);
        let world = Self { root, project };
        let market = world.root.join("market");
        let added = world.uze(
            &world.root,
            &["market", "add", &market.display().to_string()],
        );
        assert!(added.status.success(), "{}", stderr(&added));
        world
    }

    fn uze(&self, cwd: &Path, args: &[&str]) -> Output {
        self.command(cwd, args).output().unwrap()
    }

    /// `uze` run from inside an agent's pane: carrying the stamp the
    /// agent's launch did.
    fn uze_as_an_agent(&self, cwd: &Path, args: &[&str]) -> Output {
        self.command(cwd, args)
            .env(uze_terminal::launch::AGENT_IDENTITY_VARIABLE, "a1")
            .output()
            .unwrap()
    }

    fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_uze"));
        command
            .current_dir(cwd)
            .env("UZE_HOME", self.root.join("uze"))
            .isolated_home(&self.root)
            .not_launched_by_uze()
            .env("PATH", uze_testkit::process::system_path())
            .args(args);
        command
    }

    fn installed(&self) -> Output {
        let installed = self.uze(&self.project, &["install", "gsd@studio"]);
        assert!(installed.status.success(), "{}", stderr(&installed));
        installed
    }

    fn status_json(&self) -> serde_json::Value {
        let status = self.uze(&self.project, &["status", "--format", "json"]);
        assert!(status.status.success(), "{}", stderr(&status));
        serde_json::from_slice(&status.stdout).unwrap()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn band_of(extensions: &serde_json::Value) -> serde_json::Value {
    extensions
        .as_array()
        .unwrap()
        .iter()
        .find(|extension| extension["id"] == "gsd-band")
        .cloned()
        .unwrap_or_else(|| panic!("no gsd-band in {extensions}"))
}

#[test]
fn an_install_off_a_terminal_leaves_a_script_extension_disabled_and_says_where_to_enable_it() {
    let world = World::new("cli-extensions-install", str::to_owned);
    let installed = world.installed();
    let said = format!("{}{}", stdout(&installed), stderr(&installed));
    assert!(
        said.contains(&format!(
            "{BAND} installed disabled; enable it from the workspace"
        )),
        "{said}"
    );
    assert!(
        !said.contains("gsd@studio/gsd-panel"),
        "a fixtures-only extension waits for nothing: {said}"
    );
    assert!(
        !world.root.join("uze/state/extensions.json").exists(),
        "nothing was consented to"
    );
}

#[test]
fn an_install_of_what_the_project_declares_says_the_same() {
    let world = World::new("cli-extensions-install-declared", str::to_owned);
    std::fs::write(
        world.project.join("agents.yaml"),
        format!(
            "marketplaces:\n  studio:\n    path: {}\n    plugins: [gsd]\n",
            world.root.join("market").display()
        ),
    )
    .unwrap();
    let installed = world.uze(&world.project, &["install"]);
    assert!(installed.status.success(), "{}", stderr(&installed));
    let said = format!("{}{}", stdout(&installed), stderr(&installed));
    assert!(
        said.contains(&format!(
            "{BAND} installed disabled; enable it from the workspace"
        )),
        "{said}"
    );
}

#[test]
fn an_install_from_an_agents_pane_says_an_agent_cannot_enable_it() {
    let world = World::new("cli-extensions-install-agent", str::to_owned);
    let installed = world.uze_as_an_agent(&world.project, &["install", "gsd@studio"]);
    assert!(installed.status.success(), "{}", stderr(&installed));
    let said = format!("{}{}", stdout(&installed), stderr(&installed));
    assert!(
        said.contains(&format!(
            "{BAND} installed disabled; an agent cannot enable an extension"
        )),
        "{said}"
    );
    assert!(
        !world.root.join("uze/state/extensions.json").exists(),
        "nothing was consented to"
    );
}

#[test]
fn status_and_inspect_report_each_extension_and_its_state() {
    let world = World::new("cli-extensions-status", str::to_owned);
    world.installed();

    let band = band_of(&world.status_json()["extensions"]);
    assert_eq!(band["condition"], "needs-consent");
    assert_eq!(band["grants"], serde_json::json!(["read:checkout"]));

    let text = world.uze(&world.project, &["status"]);
    let text = stdout(&text);
    assert!(text.contains("Extensions"), "{text}");
    let row = text
        .lines()
        .find(|line| line.contains(BAND))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(row.contains("needs consent"), "{row}");
    assert!(row.contains("wants read:checkout"), "{row}");
    assert!(
        text.lines()
            .any(|line| line.contains("gsd@studio/gsd-panel") && line.contains("runs nothing")),
        "{text}"
    );

    let inspect = world.uze(&world.project, &["inspect", "gsd", "--format", "json"]);
    assert!(inspect.status.success(), "{}", stderr(&inspect));
    let inspected: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(
        band_of(&inspected["extensions"])["condition"],
        "needs-consent"
    );
    assert!(inspected["extensions_project"].is_string(), "{inspected}");

    let machine = world.uze(&world.project, &["status", "-m"]);
    let machine = stdout(&machine);
    assert!(
        machine
            .lines()
            .any(|line| line.contains(BAND) && line.contains("needs consent")),
        "{machine}"
    );
}

#[test]
fn a_held_extension_is_listed_as_held_naming_the_new_grant() {
    let world = World::new("cli-extensions-held", |manifest| {
        manifest.replace(
            "wants \"read:checkout\"",
            "wants \"read:checkout\" \"clipboard\"",
        )
    });
    world.installed();
    let record = serde_json::json!({
        "schema_version": 1,
        "extensions": {
            "gsd@studio/gsd-band": {
                "accepted_grants": ["read:checkout"],
                "accepted_digest": "sha256:earlier",
                "accepted_at": 1,
                "enabled_in": [],
            }
        }
    });
    std::fs::write(
        world.root.join("uze/state/extensions.json"),
        record.to_string(),
    )
    .unwrap();

    let band = band_of(&world.status_json()["extensions"]);
    assert_eq!(band["condition"], "held");
    assert_eq!(band["new_grants"], serde_json::json!(["clipboard"]));
    let text = stdout(&world.uze(&world.project, &["status"]));
    let row = text
        .lines()
        .find(|line| line.contains(BAND))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(row.contains("held"), "{row}");
    assert!(row.contains("its update wants clipboard"), "{row}");
}
