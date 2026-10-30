//! Offering a project's extensions where it opens: the toast, the dialog
//! with the grants, and an answer that is remembered.

use super::WorkspaceHit;
use super::WorkspaceModel;
use super::support::*;
use crate::ui::orchestrator::{ExtensionOfferResolution, OfferStep};
use ratatui::layout::Rect;
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

/// Several reports of the world's one offer, under other names: what a
/// project whose plugins bring many extensions offers.
fn many(world: &World, names: &[&str]) -> Vec<uze_application::ExtensionReport> {
    let one = world.offers().remove(0);
    names
        .iter()
        .map(|name| uze_application::ExtensionReport {
            id: (*name).to_owned(),
            name: (*name).to_owned(),
            ..one.clone()
        })
        .collect()
}

/// The offer is said once a session, its toast opens the dialog, and the
/// dialog lists each extension with its plugin and, for the marked one,
/// every grant it wants and what it lets it do.
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
    assert!(screen.contains("gsd@studio"), "attributed: {screen}");
    assert!(screen.contains("read:checkout"), "{screen}");
    assert!(screen.contains("reads the checkout's files"), "{screen}");
    assert!(screen.contains("Enable 1"), "{screen}");
    assert!(screen.contains("this project"), "where it holds: {screen}");
}

/// Putting the dialog away answers nothing: no record is written, and the
/// project offers it again the next time it opens. This session does not
/// ask again.
#[test]
fn putting_the_offer_away_answers_nothing_and_offers_it_again() {
    let world = World::new("offer-not-now");
    let mut driven = driven(WorkspaceModel::default(), &world.home);
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), world.offers());
    driven.attach.model.review_extensions();
    driven.press_key(key(crossterm::event::KeyCode::Esc));
    assert!(driven.attach.model.extension_offer.is_none(), "closed");
    assert!(
        !driven.attach.model.offers_waiting(),
        "not again this session"
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(world.offers().len(), 1, "offered again at the next opening");
    assert!(matches!(
        band_state(&world, &world.project),
        uze_application::ExtensionState::NeedsConsent { .. }
    ));
}

/// The dialog's Enable is its Enter: a click consents and enables the
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
        .expect("the dialog's Enable is drawn");
    driven.press(allow.x, allow.y);
    assert!(
        driven.attach.model.extension_offer.is_none(),
        "the click answered, at {allow:?}"
    );
    wait_for_toast(&mut driven, "extension enabled");
    assert_eq!(
        band_state(&world, &world.project),
        uze_application::ExtensionState::Enabled
    );
    assert!(world.offers().is_empty(), "nothing waits any more");
}

fn wait_for_toast(driven: &mut Driven<'_>, said: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !format!("{:?}", driven.attach.model.toast_stack()).contains(said) {
        assert!(
            Instant::now() < deadline,
            "{said}: {:?}",
            driven.attach.model.toast_stack()
        );
        std::thread::sleep(Duration::from_millis(10));
        driven.pump();
    }
}

/// Every project is the field beside the answers, moved with the arrows:
/// the extension then runs in a project it was never named in.
#[test]
fn enabling_in_every_project_reaches_a_project_never_named() {
    let world = World::new("offer-everywhere");
    let other = world.root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let mut driven = driven(WorkspaceModel::default(), &world.home).on_a_roomy_terminal();
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), world.offers());
    driven.attach.model.review_extensions();
    driven.press_key(key(crossterm::event::KeyCode::Right));
    let screen = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(screen.contains("every project"), "{screen}");
    assert!(screen.contains("Enable 1 everywhere"), "{screen}");
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    wait_for_toast(&mut driven, "every project");
    assert_eq!(
        band_state(&world, &other),
        uze_application::ExtensionState::Enabled
    );
}

/// The rows are chosen with space and answered once: only the chosen are
/// enabled, an update wanting more starts left off, and one toast says
/// what happened.
#[test]
fn several_extensions_are_chosen_in_one_list_and_answered_once() {
    let world = World::new("offer-several");
    let mut offers = many(&world, &["alpha", "beta", "gamma"]);
    offers[2].condition = uze_application::ExtensionCondition::Held;
    let mut model = WorkspaceModel::default();
    model.extensions_offered(world.project.clone(), offers);
    model.review_extensions();
    let open = model.extension_offer.as_ref().unwrap();
    assert_eq!(open.chosen, [true, true, false], "held starts off");
    let screen = frame_rows_at(&mut model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(
        screen.contains("3 extensions from 1 plugin · 2 on"),
        "{screen}"
    );
    assert!(screen.contains("wants more"), "{screen}");

    let open = model.extension_offer.as_mut().unwrap();
    open.move_cursor(true);
    open.toggle(open.cursor);
    let answer = model.answer_extension_offer(true).expect("answered");
    assert_eq!(
        answer
            .enable
            .iter()
            .map(|offer| offer.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha"]
    );
    assert!(model.extension_offer.is_none());

    model.absorb_extension_offer(ExtensionOfferResolution {
        project: world.project.clone(),
        everywhere: false,
        enabled: vec!["alpha".to_owned(), "beta".to_owned()],
        kept_here: Vec::new(),
        failed: Vec::new(),
    });
    let stack = format!("{:?}", model.toast_stack());
    assert!(stack.contains("2 extensions enabled"), "one toast: {stack}");
}

/// A click on a row only marks it; a click on its mark switches it, the
/// way a list of choices answers a pointer.
#[test]
fn a_click_on_a_row_marks_it_and_one_on_its_mark_switches_it() {
    let world = World::new("offer-click");
    let mut driven = driven(WorkspaceModel::default(), &world.home).on_a_roomy_terminal();
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), many(&world, &["alpha", "beta"]));
    driven.attach.model.review_extensions();
    driven.frame();
    let at = |driven: &Driven<'_>, wanted: WorkspaceHit| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| *hit == wanted)
            .map(|(rect, _)| *rect)
            .expect("drawn")
    };
    let row = at(&driven, WorkspaceHit::ExtensionOfferRow(1));
    driven.press(row.x + row.width - 2, row.y);
    let open = driven.attach.model.extension_offer.as_ref().unwrap();
    assert_eq!(open.cursor, 1, "marked");
    assert_eq!(open.chosen, [true, true], "nothing switched");

    driven.frame();
    let mark = at(&driven, WorkspaceHit::ExtensionOfferMark(0));
    driven.press(mark.x, mark.y);
    let open = driven.attach.model.extension_offer.as_ref().unwrap();
    assert_eq!(open.chosen, [false, true], "the mark switched its row");

    driven.frame();
    let all = at(&driven, WorkspaceHit::ExtensionOfferAll);
    driven.press(all.x, all.y);
    let open = driven.attach.model.extension_offer.as_ref().unwrap();
    assert_eq!(open.chosen, [true, true], "switch all, as `a` does");
}

/// The wheel scrolls the list and leaves the cursor where the keys put it.
#[test]
fn the_wheel_scrolls_the_list_and_not_the_cursor() {
    let world = World::new("offer-wheel");
    let names: Vec<String> = (0..30).map(|index| format!("ext-{index}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut driven = driven(WorkspaceModel::default(), &world.home).on_a_roomy_terminal();
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), many(&world, &names));
    driven.attach.model.review_extensions();
    driven.frame();
    driven.attach.extension_offer_wheel(true);
    driven.attach.extension_offer_wheel(true);
    let open = driven.attach.model.extension_offer.as_ref().unwrap();
    assert_eq!((open.first, open.cursor), (2, 0));
    let rows = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40));
    let first = rows
        .iter()
        .find(|row| row.contains("gsd@studio"))
        .expect("a row of the list");
    assert!(first.contains("ext-2 "), "scrolled past two: {rows:#?}");
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
    driven.press_key(key(crossterm::event::KeyCode::Enter));
    wait_for_toast(&mut driven, "extension not enabled");
    assert!(matches!(
        band_state(&world, &world.project),
        uze_application::ExtensionState::NeedsConsent { .. }
    ));
}

/// The programs of every row chosen are asked about on one step after the
/// list, and only allowing that step answers; putting it away leaves those
/// rows off and enables the rest.
#[test]
fn an_offer_asks_about_programs_on_one_step_of_their_own() {
    let world = World::new("offer-programs");
    let mut offers = many(&world, &["gpu", "notes"]);
    offers[0].grants.push("exec:nvidia-smi".to_owned());
    offers[0].programs = vec![uze_application::ProgramPin {
        name: "nvidia-smi".to_owned(),
        path: Some(PathBuf::from("/usr/lib/wsl/lib/nvidia-smi")),
        unresolved: None,
        runs_anything: false,
        reads: vec!["/dev/dxg".to_owned()],
        packaged: false,
    }];
    offers[0].confined = Some(true);
    let mut model = WorkspaceModel::default();
    model.extensions_offered(world.project.clone(), offers.clone());
    model.review_extensions();
    let list = frame_rows_at(&mut model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(list.contains("runs nvidia-smi"), "{list}");
    assert!(
        model.answer_extension_offer(true).is_none(),
        "programs wait"
    );
    let programs = frame_rows_at(&mut model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(
        programs.contains("Allow these programs to run, confined?"),
        "{programs}"
    );
    assert!(
        programs.contains("nvidia-smi · /usr/lib/wsl/lib/nvidia-smi"),
        "{programs}"
    );
    assert!(
        programs.contains("Cancel leaves these off and enables the rest."),
        "{programs}"
    );
    let answer = model.answer_extension_offer(false).expect("the rest");
    assert_eq!(
        answer
            .enable
            .iter()
            .map(|offer| offer.name.as_str())
            .collect::<Vec<_>>(),
        ["notes"]
    );

    let mut model = WorkspaceModel::default();
    model.extensions_offered(world.project.clone(), offers);
    model.review_extensions();
    model.answer_extension_offer(true);
    assert_eq!(
        model
            .answer_extension_offer(true)
            .map(|answer| answer.enable.len()),
        Some(2),
        "allowing the programs answers for both"
    );
}

/// What a band costs is said beside the grants: every agent of the
/// project gives up its rows.
#[test]
fn an_offer_names_the_rows_a_band_takes_over_each_agent() {
    let world = World::new("offer-band");
    let mut offers = world.offers();
    offers[0].band_rows = Some(2);
    let mut driven = driven(WorkspaceModel::default(), &world.home);
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), offers);
    driven.attach.model.review_extensions();
    let screen = frame_rows(&mut driven.attach.model).join("\n");
    assert!(
        screen.contains("draws 2 rows over each agent's pane"),
        "{screen}"
    );
}

/// A row that asks for more than its detail holds says how many more, and
/// `i` shows them all in the list's place; esc comes back to the list
/// without answering.
#[test]
fn everything_a_row_asks_for_is_one_key_away_and_esc_comes_back() {
    let world = World::new("offer-whole");
    let mut offers = world.offers();
    offers[0].grants = [
        "read:checkout",
        "clipboard",
        "open-path",
        "store",
        "notify",
        "store:global",
        "resident",
    ]
    .map(str::to_owned)
    .to_vec();
    let mut driven = driven(WorkspaceModel::default(), &world.home).on_a_roomy_terminal();
    driven
        .attach
        .model
        .extensions_offered(world.project.clone(), offers);
    driven.attach.model.review_extensions();
    let list = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(list.contains("more · see all"), "{list}");
    driven.press_key(key(crossterm::event::KeyCode::Char('i')));
    let whole = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(whole.contains("all it asks for"), "{whole}");
    for _ in 0..4 {
        driven.press_key(key(crossterm::event::KeyCode::Down));
    }
    let scrolled = frame_rows_at(&mut driven.attach.model, Rect::new(0, 0, 120, 40)).join("\n");
    assert!(scrolled.contains("resident"), "scrolled to: {scrolled}");
    assert!(
        whole.contains("back"),
        "esc is named by what it does: {whole}"
    );
    driven.press_key(key(crossterm::event::KeyCode::Esc));
    assert_eq!(
        driven
            .attach
            .model
            .extension_offer
            .as_ref()
            .map(|open| open.step),
        Some(OfferStep::List),
        "back, not answered"
    );
}

/// On a terminal of the least size the dialog is drawn whole: the
/// answers are on screen.
#[test]
fn the_list_fits_a_small_terminal_with_its_answers() {
    let world = World::new("offer-small");
    let names: Vec<String> = (0..12).map(|index| format!("ext-{index}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut model = WorkspaceModel::default();
    model.extensions_offered(world.project.clone(), many(&world, &names));
    model.review_extensions();
    let screen = frame_rows(&mut model).join("\n");
    assert!(screen.contains("Cancel"), "{screen}");
    assert!(screen.contains("Enable 12"), "{screen}");
}

#[test]
fn the_review_is_reached_from_the_index_while_something_is_offered() {
    let world = World::new("orchestrator-offer-index");
    let mut model = agent_session_in(world.project.to_str().unwrap());
    let offers = world.offers();
    assert!(!offers.is_empty());
    assert!(!model.offers_waiting());
    model.extensions_offered(world.project.clone(), offers);
    assert!(model.offers_waiting(), "the index lists the review");
    model.review_extensions();
    assert!(model.extension_offer.is_some(), "and it opens the question");
}
