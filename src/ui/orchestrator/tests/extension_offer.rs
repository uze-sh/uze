//! Offering a project's extensions where it opens: the toast, the dialog
//! with the grants, and an answer that is remembered.

use super::WorkspaceHit;
use super::WorkspaceModel;
use super::support::*;
use std::path::{Path, PathBuf};

use std::time::{Duration, Instant};
use uze_core::UzeHome;

/// The `gsd` fixture plugin installed from a marketplace, and a project
/// whose `agents.yaml` declares it.
struct World {
    root: PathBuf,
    home: UzeHome,
    project: PathBuf,
}

impl World {
    fn new(label: &str) -> Self {
        let root = uze_testkit::temp::scratch(label);
        let home = UzeHome::at(root.join("uze"));
        let market = root.join("market");
        let manifest = serde_json::json!({
            "name": "studio",
            "plugins": [{ "name": "gsd", "source": "./plugins/gsd", "description": "GSD." }],
        });
        uze_testkit::marketplace::stage(
            &market,
            &manifest.to_string(),
            &[("gsd".to_owned(), uze_testkit::fixtures::canonical("gsd"))],
        );
        let app = uze_application::UzeApplication::new(home.clone(), Vec::new());
        app.marketplace()
            .add(&market.display().to_string())
            .unwrap();
        app.marketplace()
            .install_plugin("gsd@studio", &uze_application::AlwaysTrust)
            .unwrap();
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        uze_testkit::git::isolated_git_in(&project, &["init", "-q"]);
        std::fs::write(
            project.join("agents.yaml"),
            format!(
                "marketplaces:\n  studio:\n    path: {}\n    plugins: [gsd]\n",
                market.display()
            ),
        )
        .unwrap();
        let project = uze_platform::path::canonical(&project).unwrap();
        Self {
            root,
            home,
            project,
        }
    }

    fn app(&self) -> uze_application::UzeApplication {
        uze_application::UzeApplication::new(self.home.clone(), Vec::new())
    }

    /// What waits for the operator in the project, offer put away or not.
    /// (What the workspace reads where it opens also leaves out a checkout
    /// an extension may not read, as this one in the temporary directory
    /// is: `extensions::report`'s tests hold that half.)
    fn offers(&self) -> Vec<uze_application::ExtensionReport> {
        self.app()
            .extensions()
            .reports(Some(&self.project))
            .into_iter()
            .filter(|report| report.condition.awaits_the_operator() && !report.offer_declined)
            .collect()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
}

fn band_state(world: &World, project: &Path) -> uze_application::ExtensionState {
    let identity = uze_application::ExtensionIdentity {
        marketplace: "studio".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    };
    world.app().extensions().state(&identity, project).unwrap()
}

/// The offer is said once a session, its toast opens the dialog, and the
/// dialog names whose extension it is and every grant it wants.
#[test]
fn a_project_offers_its_extensions_once_with_their_grants() {
    let world = World::new("offer-ask");
    let offers = world.offers();
    assert_eq!(
        offers
            .iter()
            .map(|offer| offer.attribution())
            .collect::<Vec<_>>(),
        ["gsd-band · gsd@studio"],
        "a fixtures-only extension waits for nothing"
    );
    let mut driven = driven(WorkspaceModel::default(), &world.home);
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), offers.clone());
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), offers);
    let stack = driven.attach.model.toast_stack();
    assert_eq!(stack.len(), 1, "said once: {stack:?}");
    assert!(
        format!("{stack:?}").contains("extension to enable"),
        "{stack:?}"
    );
    assert_eq!(
        driven.attach.model.toast_offer(0),
        Some(WorkspaceHit::ReviewExtensions)
    );

    driven.attach.model.review_extensions();
    let screen = frame_rows(&mut driven.attach.model).join("\n");
    assert!(screen.contains("Enable GSD in"), "{screen}");
    assert!(
        screen.contains("gsd-band · gsd@studio"),
        "attributed: {screen}"
    );
    assert!(
        screen.contains("read:checkout · reads the files"),
        "{screen}"
    );
    assert!(screen.contains("Allow and enable"), "{screen}");
}

/// Putting the offer away enables nothing and is remembered on the
/// machine, so the next opening offers nothing.
#[test]
fn a_declined_offer_is_not_made_again() {
    let world = World::new("offer-decline");
    let mut driven = driven(WorkspaceModel::default(), &world.home);
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), world.offers());
    driven.attach.model.review_extensions();
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    assert!(driven.attach.model.extension_offer.is_none(), "closed");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !world.offers().is_empty() {
        assert!(Instant::now() < deadline, "the decline must be recorded");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(matches!(
        band_state(&world, &world.project),
        uze_application::ExtensionState::NeedsConsent { .. }
    ));
}

/// The dialog's Allow is its Enter: a click consents and enables the
/// extension in this project, and says so.
#[test]
fn clicking_allow_consents_and_enables_the_extension_here() {
    let world = World::new("offer-allow");
    let mut driven = driven(WorkspaceModel::default(), &world.home).on_a_roomy_terminal();
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), world.offers());
    driven.attach.model.review_extensions();
    driven.frame();
    let (allow, _) = *driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::ExtensionOfferAnswer(true))
        .expect("the dialog's Allow is drawn");
    driven.press(allow.x, allow.y);
    assert!(
        driven.attach.model.extension_offer.is_none(),
        "the click answered, at {allow:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !format!("{:?}", driven.attach.model.toast_stack()).contains("extension enabled") {
        assert!(
            Instant::now() < deadline,
            "{:?}",
            driven.attach.model.toast_stack()
        );
        std::thread::sleep(Duration::from_millis(10));
        driven.pump();
    }
    assert_eq!(
        band_state(&world, &world.project),
        uze_application::ExtensionState::Enabled
    );
    assert!(world.offers().is_empty(), "nothing waits any more");
}

/// A client an agent's pane started answers nothing for the operator:
/// allowing is refused, and says so.
#[test]
fn a_client_started_in_an_agents_pane_cannot_enable_an_extension() {
    let world = World::new("offer-agent");
    let mut driven = driven(WorkspaceModel::default(), &world.home);
    driven.attach.asker = uze_application::Asker::AgentPane;
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), world.offers());
    driven.attach.model.review_extensions();
    driven.press_key(key(crossterm::event::KeyCode::Right));
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !format!("{:?}", driven.attach.model.toast_stack()).contains("extension not enabled") {
        assert!(
            Instant::now() < deadline,
            "{:?}",
            driven.attach.model.toast_stack()
        );
        std::thread::sleep(Duration::from_millis(10));
        driven.pump();
    }
    assert!(matches!(
        band_state(&world, &world.project),
        uze_application::ExtensionState::NeedsConsent { .. }
    ));
}

/// An offer of an extension that runs programs asks about them on a step
/// of its own once its other grants are allowed, and only allowing that
/// step answers the offer.
#[test]
fn an_offer_asks_about_programs_on_their_own_step() {
    let world = World::new("offer-programs");
    let mut offers = world.offers();
    offers[0].grants.push("exec:nvidia-smi".to_owned());
    offers[0].programs = vec![uze_application::ProgramPin {
        name: "nvidia-smi".to_owned(),
        path: Some(PathBuf::from("/usr/lib/wsl/lib/nvidia-smi")),
        unresolved: None,
        runs_anything: false,
        reads: vec!["/dev/dxg".to_owned()],
    }];
    offers[0].confined = Some(true);
    let mut driven = driven(WorkspaceModel::default(), &world.home).on_a_roomy_terminal();
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), offers);
    driven.attach.model.review_extensions();
    let grants = frame_rows(&mut driven.attach.model).join("\n");
    assert!(grants.contains("Enable GSD in"), "{grants}");
    assert!(
        !grants.contains("exec:nvidia-smi"),
        "programs wait: {grants}"
    );
    driven.attach.model.answer_extension_offer(true);
    let programs = frame_rows(&mut driven.attach.model).join("\n");
    assert!(
        driven.attach.model.extension_offer.is_some(),
        "allowing the grants is not the answer yet"
    );
    assert!(
        programs.contains("Allow these programs to run, confined?"),
        "{programs}"
    );
    assert!(
        programs.contains("nvidia-smi · /usr/lib/wsl/lib/nvidia-smi"),
        "{programs}"
    );
    assert!(
        driven.attach.model.answer_extension_offer(true).is_some(),
        "allowing the programs answers the offer"
    );
}
