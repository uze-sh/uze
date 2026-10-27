//! Consent to the `gsd` plugin's extensions, from install to removal, on
//! real files: the band wants `read:checkout` and declares a script, so it
//! asks for consent and then for enabling in each project; the panel draws
//! from fixtures alone and asks for nothing. Saves in the linked working
//! tree are updates, judged by grant set.

use std::fs;
use std::path::{Path, PathBuf};

use uze_application::{
    Asker, ExtensionIdentity, ExtensionState, InstalledExtension, UzeApplication, consent_key,
};
use uze_contract::Grant;
use uze_core::{UzeHome, trust::AlwaysTrust};
use uze_testkit::temp::TestEnvironment;

const MARKET: &str = "studio";

fn installed_from_a_linked_marketplace(env: &TestEnvironment) -> (UzeApplication, PathBuf) {
    let market = env.root().join("market");
    let manifest = serde_json::json!({
        "name": MARKET,
        "plugins": [{ "name": "gsd", "source": "./plugins/gsd", "description": "GSD band." }],
    });
    uze_testkit::marketplace::stage(
        &market,
        &manifest.to_string(),
        &[("gsd".to_owned(), uze_testkit::fixtures::canonical("gsd"))],
    );
    let app = UzeApplication::new(UzeHome::at(&env.uze_home), Vec::new());
    app.marketplace()
        .add(&market.display().to_string())
        .expect("the marketplace registers");
    app.marketplace()
        .link(MARKET, &market)
        .expect("the marketplace links to its working tree");
    app.marketplace()
        .install_plugin(&format!("gsd@{MARKET}"), &AlwaysTrust)
        .expect("the gsd plugin installs");
    (app, market.join("plugins/gsd/extensions/gsd-band"))
}

fn the(app: &UzeApplication, id: &str) -> InstalledExtension {
    app.extensions()
        .installed()
        .into_iter()
        .find(|extension| extension.identity.id == id)
        .unwrap_or_else(|| panic!("{id} is installed"))
}

fn project(env: &TestEnvironment, name: &str) -> PathBuf {
    let root = env.root().join(name);
    fs::create_dir_all(&root).unwrap();
    root
}

fn state(app: &UzeApplication, identity: &ExtensionIdentity, project: &Path) -> ExtensionState {
    app.extensions()
        .state(identity, project)
        .expect("the extension is judged")
}

/// Rewrites the band's `wants` line in the author's working tree.
fn wants(band: &Path, grants: &str) {
    let manifest = band.join("extension.kdl");
    let text = fs::read_to_string(&manifest).unwrap();
    let edited = text
        .lines()
        .map(|line| {
            if line.starts_with("wants ") {
                format!("wants {grants}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(manifest, edited + "\n").unwrap();
}

#[test]
fn a_fixtures_only_extension_needs_no_consent() {
    let env = TestEnvironment::isolated();
    let (app, _) = installed_from_a_linked_marketplace(&env);
    let panel = the(&app, "gsd-panel").identity;
    let here = project(&env, "a");

    assert_eq!(state(&app, &panel, &here), ExtensionState::FixturesOnly);
    assert!(app.extensions().consent(&panel, Asker::Operator).is_err());
    assert!(
        app.extensions()
            .enable(&panel, &here, Asker::Operator)
            .is_err()
    );
}

#[test]
fn a_script_runs_only_once_consented_to_and_enabled_in_its_project() {
    let env = TestEnvironment::isolated();
    let (app, _) = installed_from_a_linked_marketplace(&env);
    let band = the(&app, "gsd-band").identity;
    let (a, b) = (project(&env, "a"), project(&env, "b"));

    assert_eq!(
        state(&app, &band, &a),
        ExtensionState::NeedsConsent {
            grants: [Grant::ReadCheckout].into()
        }
    );
    assert!(
        app.extensions().enable(&band, &a, Asker::Operator).is_err(),
        "enabling is never the first answer"
    );

    let consented = app
        .extensions()
        .consent(&band, Asker::Operator)
        .expect("the operator consents");
    assert_eq!(consented.grants, [Grant::ReadCheckout].into());
    assert!(consented.digest.starts_with("sha256:"));
    assert_eq!(state(&app, &band, &a), ExtensionState::NeedsEnabling);

    let enabled = app
        .extensions()
        .enable(&band, &a, Asker::Operator)
        .expect("the operator enables it here");
    assert!(enabled.changed);
    assert_eq!(state(&app, &band, &a), ExtensionState::Enabled);
    assert!(state(&app, &band, &a).runs_scripts());
    assert_eq!(
        state(&app, &band, &b),
        ExtensionState::NeedsEnabling,
        "enabling does not cross projects"
    );

    let disabled = app.extensions().disable(&band, &a).expect("disabled");
    assert!(disabled.changed);
    assert_eq!(state(&app, &band, &a), ExtensionState::NeedsEnabling);
}

#[test]
fn an_agents_pane_can_neither_consent_nor_enable() {
    let env = TestEnvironment::isolated();
    let (app, _) = installed_from_a_linked_marketplace(&env);
    let band = the(&app, "gsd-band").identity;
    let here = project(&env, "a");

    let refused = app
        .extensions()
        .consent(&band, Asker::AgentPane)
        .expect_err("an agent cannot consent");
    assert!(refused.to_string().contains("an agent cannot"), "{refused}");
    assert!(matches!(
        state(&app, &band, &here),
        ExtensionState::NeedsConsent { .. }
    ));

    app.extensions().consent(&band, Asker::Operator).unwrap();
    assert!(
        app.extensions()
            .enable(&band, &here, Asker::AgentPane)
            .is_err()
    );
    assert_eq!(state(&app, &band, &here), ExtensionState::NeedsEnabling);
}

#[test]
fn an_update_is_held_by_a_new_grant_and_applied_under_the_same_or_fewer() {
    let env = TestEnvironment::isolated();
    let (app, band_dir) = installed_from_a_linked_marketplace(&env);
    let band = the(&app, "gsd-band").identity;
    let here = project(&env, "a");
    let home = UzeHome::at(&env.uze_home);
    app.extensions().consent(&band, Asker::Operator).unwrap();
    app.extensions()
        .enable(&band, &here, Asker::Operator)
        .unwrap();
    let digest = || {
        uze_workspace::extensions::consent::consent(&home, &consent_key(&band))
            .expect("recorded")
            .accepted_digest
    };
    let accepted = digest();

    fs::write(
        band_dir.join("state.rhai"),
        "fn answer(input, context) { #{} }\n",
    )
    .unwrap();
    assert_eq!(
        state(&app, &band, &here),
        ExtensionState::Enabled,
        "the same grants apply without asking"
    );
    let same_grants = digest();
    assert_ne!(same_grants, accepted, "and the new digest is recorded");

    wants(&band_dir, "\"read:checkout\" \"clipboard\"");
    assert_eq!(
        state(&app, &band, &here),
        ExtensionState::Held {
            new_grants: [Grant::Clipboard].into(),
            widened: Vec::new(),
        },
        "a widening holds it, naming what is new"
    );
    assert!(!state(&app, &band, &here).runs_scripts());
    assert!(
        app.extensions()
            .enable(&band, &here, Asker::Operator)
            .is_err()
    );
    assert_eq!(digest(), same_grants, "nothing was applied");

    app.extensions().consent(&band, Asker::Operator).unwrap();
    assert_eq!(
        state(&app, &band, &here),
        ExtensionState::Enabled,
        "consent to the new grant keeps the projects it was enabled in"
    );

    wants(&band_dir, "\"read:checkout\"");
    assert_eq!(state(&app, &band, &here), ExtensionState::Enabled);
    wants(&band_dir, "\"read:checkout\" \"clipboard\"");
    assert!(
        matches!(state(&app, &band, &here), ExtensionState::Held { .. }),
        "a grant the narrowing dropped is asked for again"
    );
}

/// Sets the band's `exec "sh"` to read `reads`, wanting `exec:sh`.
fn sh_reads(band: &Path, reads: &str) {
    wants(band, "\"read:checkout\" \"exec:sh\"");
    let manifest = band.join("extension.kdl");
    let text = fs::read_to_string(&manifest).unwrap();
    let text = text.split("exec \"sh\"").next().unwrap_or_default();
    fs::write(
        manifest,
        format!("{text}exec \"sh\" {{\n    reads {reads}\n}}\n"),
    )
    .unwrap();
}

#[test]
fn an_update_that_widens_what_a_program_reads_is_held() {
    let env = TestEnvironment::isolated();
    let (app, band_dir) = installed_from_a_linked_marketplace(&env);
    let band = the(&app, "gsd-band").identity;
    let here = project(&env, "a");
    sh_reads(&band_dir, "\"/etc/gsd\"");
    app.extensions().consent(&band, Asker::Operator).unwrap();
    app.extensions()
        .enable(&band, &here, Asker::Operator)
        .unwrap();

    sh_reads(&band_dir, "\"/etc/gsd\" \"/home/op/.ssh\"");
    assert_eq!(
        state(&app, &band, &here),
        ExtensionState::Held {
            new_grants: Default::default(),
            widened: vec!["exec:sh reads /home/op/.ssh".to_owned()],
        },
        "a read nobody accepted holds the update under the same grants"
    );

    sh_reads(&band_dir, "\"/etc/gsd\"");
    fs::write(
        band_dir.join("state.rhai"),
        "fn answer(input, context) { #{} }\n",
    )
    .unwrap();
    assert_eq!(
        state(&app, &band, &here),
        ExtensionState::Enabled,
        "the same reads or fewer apply"
    );
}

#[test]
fn removing_the_plugin_forgets_its_extensions() {
    let env = TestEnvironment::isolated();
    let (app, _) = installed_from_a_linked_marketplace(&env);
    let band = the(&app, "gsd-band").identity;
    let home = UzeHome::at(&env.uze_home);
    app.extensions().consent(&band, Asker::Operator).unwrap();
    assert!(uze_workspace::extensions::consent::consent(&home, &consent_key(&band)).is_some());

    let removed = app.plugins().remove("gsd").expect("the plugin is removed");
    assert!(matches!(
        removed,
        uze_application::application::RemovePluginReport::Removed { .. }
    ));
    assert_eq!(
        uze_workspace::extensions::consent::consent(&home, &consent_key(&band)),
        None,
        "installed again, it is consented to from nothing"
    );
}
