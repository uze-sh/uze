//! A plugin's source asked again by the host's clock and by the checkout
//! watch, through the real declared engine: its `every` only while a
//! surface showing it is in view, a burst of saves under a declared path
//! once, and nothing watched that no source declared.

use super::support::*;
use super::{
    EngineAnswer, ExtensionAnswer, ExtensionWatch, ExtensionWatchRead, HostedExtensions,
    LoadedExtension, WorkspaceModel,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use uze_application::{CompiledExtension, ExtensionIdentity, InstalledExtension, UzeHome};
use uze_contract::EventName;
use uze_extensions::declared::COALESCE;

fn identity() -> ExtensionIdentity {
    ExtensionIdentity {
        marketplace: "market".to_owned(),
        plugin: "gsd".to_owned(),
        id: "gsd-band".to_owned(),
    }
}

/// A section showing a source that `every` and `refresh_on` ask again.
fn compiled(every: Option<Duration>, refresh_on: Vec<EventName>) -> CompiledExtension {
    let view = uze_contract::CompiledView {
        file: "band.kdl".to_owned(),
        items: Vec::new(),
        templates: BTreeMap::new(),
        lookups: Vec::new(),
    };
    let mut source = uze_contract::SourceDecl::new(
        "board",
        uze_contract::SourceKind::Fixture("fixtures/board.json".to_owned()),
    );
    source.every = every;
    source.refresh_on = refresh_on;
    CompiledExtension {
        identity: identity(),
        manifest: uze_contract::Manifest {
            protocol: 1,
            id: "gsd-band".to_owned(),
            name: "GSD".to_owned(),
            description: String::new(),
            surfaces: vec![uze_contract::SurfaceDecl {
                band: None,
                id: "band".to_owned(),
                slot: uze_contract::Slot::Section,
                view: "band.kdl".to_owned(),
                source: "board".to_owned(),
                title: None,
                applies_when: None,
            }],
            wants: Vec::new(),
            sources: vec![source],
            actions: Vec::new(),
            exec: Vec::new(),
            preferences: Vec::new(),
            sprites: Vec::new(),
        },
        views: BTreeMap::from([("band.kdl".to_owned(), view)]),
    }
}

/// An agent in `root` and the extension hosted, the built-ins off so
/// only its questions are counted.
fn hosting(root: &Path, compiled: CompiledExtension) -> WorkspaceModel {
    let mut model = agent_session_in(root.to_str().unwrap());
    model.follow_extension_switch(
        [
            uze_extensions::code::CATALOG.id.to_owned(),
            uze_extensions::spec::CATALOG.id.to_owned(),
        ]
        .into(),
    );
    model.remembered.declared = HostedExtensions {
        discovered_at: Some(Instant::now()),
        ..HostedExtensions::default()
    };
    let installed = InstalledExtension {
        identity: identity(),
        dir: PathBuf::from("/market/plugins/gsd/extensions/gsd-band"),
        linked: false,
    };
    assert!(model.absorb_extension_answer(ExtensionAnswer {
        preferences: BTreeMap::new(),
        discovery: true,
        discovered: Some(vec![installed.clone()]),
        recompiled: Vec::new(),
        records: Default::default(),
        loaded: vec![LoadedExtension {
            installed,
            stamp: 1,
            compiled: Ok(compiled),
            data: BTreeMap::new(),
        }],
    }));
    model
}

fn fold_the_section(model: &mut WorkspaceModel, folded: bool) {
    for slot in &mut model.remembered.declared.slots {
        if slot.owner.declared().is_some() {
            slot.collapsed = folded;
        }
    }
}

/// What the watch thread reads for `watch`, read here.
fn read(watch: &ExtensionWatch) -> ExtensionWatchRead {
    ExtensionWatchRead {
        checkouts: Vec::new(),
        paths: watch
            .paths
            .iter()
            .map(|(root, path)| {
                let digest = uze_application::paths_digest(root, path);
                ((root.clone(), path.clone()), digest)
            })
            .collect(),
        reports: Vec::new(),
    }
}

fn save(file: &Path, seconds: u64) {
    std::fs::write(file, format!("saved at {seconds}")).unwrap();
    std::fs::File::options()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
}

/// Hands out the jobs due at `now` and takes each one's answer back, as the
/// workspace does: a source with a question out is not asked again.
fn asked(
    model: &mut WorkspaceModel,
    home: &UzeHome,
    sender: &std::sync::mpsc::Sender<EngineAnswer>,
    answers: &std::sync::mpsc::Receiver<EngineAnswer>,
    now: Instant,
) -> usize {
    let handed = model.schedule_engine_jobs_at(home, sender, now);
    for _ in 0..handed {
        let answer = answers
            .recv_timeout(Duration::from_secs(10))
            .expect("every job handed out answers");
        model.absorb_engine_answer(answer);
    }
    handed
}

#[test]
fn every_asks_only_while_a_surface_showing_the_source_is_in_view() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-every-home"));
    let root = PathBuf::from("/repo");
    let mut model = hosting(&root, compiled(Some(Duration::from_secs(10)), Vec::new()));
    fold_the_section(&mut model, true);
    let (sender, answers) = std::sync::mpsc::channel();
    let start = Instant::now();
    assert_eq!(asked(&mut model, &home, &sender, &answers, start), 1);
    let later = start + Duration::from_secs(30);
    assert_eq!(
        asked(&mut model, &home, &sender, &answers, later),
        0,
        "folded, nothing showing it is in view"
    );
    fold_the_section(&mut model, false);
    assert_eq!(asked(&mut model, &home, &sender, &answers, later), 1);
    let soon = later + Duration::from_secs(5);
    assert_eq!(asked(&mut model, &home, &sender, &answers, soon), 0);
    let due = later + Duration::from_secs(10);
    assert_eq!(asked(&mut model, &home, &sender, &answers, due), 1);
}

#[test]
fn a_burst_of_saves_under_a_declared_path_asks_the_source_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-paths-home"));
    let root = uze_testkit::temp::scratch("orchestrator-paths-checkout");
    std::fs::create_dir_all(root.join(".planning")).unwrap();
    save(&root.join(".planning/PLAN.md"), 1);
    let planning = EventName::PathsChanged {
        paths: vec![".planning".to_owned()],
    };
    let mut model = hosting(&root, compiled(None, vec![planning]));
    let (sender, answers) = std::sync::mpsc::channel();
    let start = Instant::now();
    assert_eq!(asked(&mut model, &home, &sender, &answers, start), 1);

    let watch = model.extension_watch();
    assert_eq!(watch.paths, [(root.clone(), ".planning".to_owned())]);
    model.absorb_extension_watch(read(&watch));
    let read_once = start + Duration::from_secs(1);
    assert_eq!(
        asked(&mut model, &home, &sender, &answers, read_once),
        0,
        "a first read only sets what the next is compared with"
    );

    save(&root.join(".planning/PLAN.md"), 2);
    model.absorb_extension_watch(read(&watch));
    let saved = start + Duration::from_secs(2);
    assert_eq!(asked(&mut model, &home, &sender, &answers, saved), 0);
    save(&root.join(".planning/STATE.md"), 3);
    model.absorb_extension_watch(read(&watch));
    let burst = saved + Duration::from_millis(100);
    assert_eq!(asked(&mut model, &home, &sender, &answers, burst), 0);
    let closed = saved + COALESCE;
    assert_eq!(
        asked(&mut model, &home, &sender, &answers, closed),
        1,
        "the burst asks once its window closes"
    );
    model.absorb_extension_watch(read(&watch));
    let quiet = closed + Duration::from_secs(1);
    assert_eq!(asked(&mut model, &home, &sender, &answers, quiet), 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_checkout_whose_stamp_moved_asks_a_source_declaring_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-checkout-home"));
    let root = PathBuf::from("/repo");
    let mut model = hosting(&root, compiled(None, vec![EventName::CheckoutChanged]));
    let (sender, answers) = std::sync::mpsc::channel();
    let start = Instant::now();
    assert_eq!(asked(&mut model, &home, &sender, &answers, start), 1);
    assert_eq!(
        model.extension_watch().checkouts,
        std::slice::from_ref(&root)
    );
    let stamped = |stamp| ExtensionWatchRead {
        checkouts: vec![(root.clone(), Some(stamp))],
        paths: Vec::new(),
        reports: Vec::new(),
    };
    model.absorb_extension_watch(stamped(1));
    model.absorb_extension_watch(stamped(1));
    assert_eq!(
        asked(&mut model, &home, &sender, &answers, start + COALESCE),
        0
    );
    model.absorb_extension_watch(stamped(2));
    let moved = start + Duration::from_secs(1);
    asked(&mut model, &home, &sender, &answers, moved);
    assert_eq!(
        asked(&mut model, &home, &sender, &answers, moved + COALESCE),
        1
    );
}

#[test]
fn an_extension_without_refresh_on_has_nothing_watched() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-unwatched-home"));
    let root = PathBuf::from("/repo");
    let (sender, _answers) = std::sync::mpsc::channel();
    let mut quiet = hosting(&root, compiled(Some(Duration::from_secs(10)), Vec::new()));
    quiet.schedule_engine_jobs(&home, &sender);
    assert_eq!(quiet.extension_watch(), ExtensionWatch::default());

    let planning = EventName::PathsChanged {
        paths: vec![".planning".to_owned()],
    };
    let mut watched = hosting(&root, compiled(None, vec![planning]));
    watched.schedule_engine_jobs(&home, &sender);
    assert_ne!(watched.extension_watch(), ExtensionWatch::default());
    watched.follow_extension_switch(
        [
            uze_extensions::code::CATALOG.id.to_owned(),
            uze_extensions::spec::CATALOG.id.to_owned(),
            super::switch_id(&identity()),
        ]
        .into(),
    );
    assert_eq!(
        watched.extension_watch(),
        ExtensionWatch::default(),
        "a switched-off extension is not watched"
    );
}

/// The agent the session placed in `root`, as the bridge sees it.
fn the_agent(model: &mut WorkspaceModel) -> super::BridgedAgent {
    model.note_space_projects(&identities_fixture());
    let tab = model.tabs().next().unwrap();
    super::BridgedAgent {
        id: "a1".to_owned(),
        harness: "harness".to_owned(),
        label: tab.label.clone(),
        space: "repo".to_owned(),
        checkout: PathBuf::from("/repo"),
        pane: tab.pane.id,
        skill_prefix: Some("/"),
    }
}

fn turn_finished() -> uze_application::AgentReport {
    uze_application::AgentReport {
        event: "turn.finished".to_owned(),
        at: 1,
        ..Default::default()
    }
}

#[test]
fn an_agents_report_asks_the_source_of_an_extension_observing_agents() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-bridge-home"));
    let root = PathBuf::from("/repo");
    let finished = EventName::Agent(uze_contract::AgentEvent::TurnFinished);
    let mut observing = compiled(None, vec![finished.clone()]);
    observing.manifest.wants = vec![uze_contract::Grant::AgentObserve];
    let mut model = hosting(&root, observing);
    let (sender, answers) = std::sync::mpsc::channel();
    let start = Instant::now();
    assert_eq!(asked(&mut model, &home, &sender, &answers, start), 1);

    let agent = the_agent(&mut model);
    let project = model.projects.get(&root).cloned().expect("its project");
    model
        .remembered
        .declared
        .extensions
        .get_mut(&identity())
        .expect("hosted")
        .record = Some(Some(uze_application::ExtensionConsent {
        enabled_in: [project].into(),
        ..Default::default()
    }));
    model.see_agents(vec![agent]);
    assert_eq!(
        model.extension_watch().journals,
        [("a1".to_owned(), None)],
        "a journal never read is read first only to set its offset"
    );
    let cursor = |offset| uze_application::AgentReportCursor { offset, last: None };
    let read = |reports, offset| ExtensionWatchRead {
        checkouts: Vec::new(),
        paths: Vec::new(),
        reports: vec![("a1".to_owned(), reports, cursor(offset))],
    };
    model.absorb_extension_watch(read(None, 40));
    assert_eq!(
        model.extension_watch().journals,
        [("a1".to_owned(), Some(cursor(40)))]
    );
    let read_once = start + Duration::from_secs(1);
    assert_eq!(asked(&mut model, &home, &sender, &answers, read_once), 0);

    model.absorb_extension_watch(read(Some(vec![turn_finished()]), 80));
    let reported = start + Duration::from_secs(2);
    asked(&mut model, &home, &sender, &answers, reported);
    assert_eq!(
        asked(&mut model, &home, &sender, &answers, reported + COALESCE),
        1,
        "the agent's finished turn asks the source again"
    );

    let mut blind = hosting(&root, compiled(None, vec![finished]));
    let agent = the_agent(&mut blind);
    blind.see_agents(vec![agent]);
    assert!(
        blind.extension_watch().journals.is_empty(),
        "no journal is read for an extension without agent:observe"
    );
}

#[test]
fn an_observer_not_enabled_in_the_agents_project_sees_nothing_of_it() {
    let root = PathBuf::from("/repo");
    let finished = EventName::Agent(uze_contract::AgentEvent::TurnFinished);
    let mut observing = compiled(None, vec![finished]);
    observing.manifest.wants = vec![uze_contract::Grant::AgentObserve];
    let mut model = hosting(&root, observing);
    model
        .remembered
        .declared
        .extensions
        .get_mut(&identity())
        .expect("hosted")
        .per_project = true;
    let agent = the_agent(&mut model);
    model.see_agents(vec![agent]);
    assert!(
        model.extension_watch().journals.is_empty(),
        "allowed nowhere, it observes no one and no journal is read for it"
    );
    let project = model.projects.get(&root).cloned().expect("its project");
    model
        .remembered
        .declared
        .extensions
        .get_mut(&identity())
        .expect("hosted")
        .record = Some(Some(uze_application::ExtensionConsent {
        enabled_in: [project].into(),
        ..Default::default()
    }));
    assert_eq!(
        model.extension_watch().journals,
        [("a1".to_owned(), None)],
        "enabled in its project, it does"
    );
}

/// An observing extension, enabled in `/repo`'s project, with the agent
/// `a1` seen there.
fn observed(root: &Path) -> (WorkspaceModel, super::BridgedAgent) {
    let finished = EventName::Agent(uze_contract::AgentEvent::TurnFinished);
    let mut observing = compiled(None, vec![finished]);
    observing.manifest.wants = vec![uze_contract::Grant::AgentObserve];
    let mut model = hosting(root, observing);
    model
        .remembered
        .declared
        .extensions
        .get_mut(&identity())
        .expect("hosted")
        .per_project = true;
    let agent = the_agent(&mut model);
    observe(&mut model, root, true);
    model.see_agents(vec![agent.clone()]);
    (model, agent)
}

/// Enables the observer in `root`'s project, or in none.
fn observe(model: &mut WorkspaceModel, root: &Path, observing: bool) {
    let project = model.projects.get(root).cloned().expect("its project");
    model
        .remembered
        .declared
        .extensions
        .get_mut(&identity())
        .expect("hosted")
        .record = Some(Some(uze_application::ExtensionConsent {
        enabled_in: if observing {
            [project].into()
        } else {
            Default::default()
        },
        ..Default::default()
    }));
}

fn journal_read(
    reports: Option<Vec<uze_application::AgentReport>>,
    offset: u64,
) -> ExtensionWatchRead {
    ExtensionWatchRead {
        checkouts: Vec::new(),
        paths: Vec::new(),
        reports: vec![(
            "a1".to_owned(),
            reports,
            uze_application::AgentReportCursor { offset, last: None },
        )],
    }
}

fn working(model: &mut WorkspaceModel, agent: &super::BridgedAgent, working: bool) {
    model
        .remembered
        .agent_activity
        .entry(agent.pane)
        .or_default()
        .working_until = working.then(|| Instant::now() + Duration::from_secs(60));
}

#[test]
fn an_agent_that_reported_has_its_turns_painted_again_once_its_journal_is_not_read() {
    let root = PathBuf::from("/repo");
    let (mut model, agent) = observed(&root);
    model.absorb_extension_watch(journal_read(None, 40));
    let started = uze_application::AgentReport {
        event: "turn.started".to_owned(),
        at: 1,
        ..Default::default()
    };
    model.absorb_extension_watch(journal_read(Some(vec![started, turn_finished()]), 80));
    let turns = |model: &WorkspaceModel| model.remembered.declared.bridge.turns.get("a1").copied();
    assert_eq!(turns(&model), Some(1), "the report's turn");

    observe(&mut model, &root, false);
    assert!(
        model.extension_watch().journals.is_empty(),
        "nobody reads it"
    );
    working(&mut model, &agent, true);
    model.see_agents(vec![agent.clone()]);
    working(&mut model, &agent, false);
    model.see_agents(vec![agent.clone()]);
    working(&mut model, &agent, true);
    model.see_agents(vec![agent.clone()]);
    assert_eq!(
        turns(&model),
        Some(3),
        "its turns go on from the paint, so one prompt a turn does not freeze"
    );
}

#[test]
fn resuming_observation_reads_an_agents_journal_from_its_end() {
    let root = PathBuf::from("/repo");
    let (mut model, agent) = observed(&root);
    model.absorb_extension_watch(journal_read(None, 40));
    assert_eq!(
        model.extension_watch().journals,
        [(
            "a1".to_owned(),
            Some(uze_application::AgentReportCursor {
                offset: 40,
                last: None
            })
        )]
    );

    observe(&mut model, &root, false);
    model.see_agents(vec![agent.clone()]);
    observe(&mut model, &root, true);
    model.see_agents(vec![agent]);
    assert_eq!(
        model.extension_watch().journals,
        [("a1".to_owned(), None)],
        "what it did unobserved is not replayed: the next read only sets the offset"
    );
}
