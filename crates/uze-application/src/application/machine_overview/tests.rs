use std::sync::atomic::{AtomicBool, Ordering};

use uze_core::{
    ExposurePlan, Resource, UzeHome,
    integration::{HarnessDetection, IntegrationPort},
    inventory::{Definition, FoundResource, ResourceKind, SessionCounts},
    router::HarnessCapabilities,
};

use crate::UzeApplication;

/// A harness whose answers the test chooses.
struct Harness {
    id: &'static str,
    present: bool,
    found: Option<Vec<FoundResource>>,
    sessions: Option<SessionCounts>,
    panics: bool,
}

impl Harness {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            present: true,
            found: Some(Vec::new()),
            sessions: Some(SessionCounts {
                total: 2,
                oldest: None,
            }),
            panics: false,
        }
    }
}

impl IntegrationPort for Harness {
    fn id(&self) -> &'static str {
        self.id
    }
    fn display_name(&self) -> &'static str {
        self.id
    }
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }
    fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
        panic!("not used")
    }
    fn detect(&self) -> HarnessDetection {
        HarnessDetection {
            present: self.present,
            version: None,
        }
    }
    fn machine_inventory(&self) -> Option<Vec<FoundResource>> {
        assert!(!self.panics, "a reader that breaks");
        self.found.clone()
    }
    fn session_counts(&self) -> Option<SessionCounts> {
        self.sessions
    }
}

fn rule(home: &std::path::Path, name: &str) -> FoundResource {
    let path = home.join(name);
    std::fs::write(&path, "Be brief.\n").unwrap();
    FoundResource::new(ResourceKind::Rule, name, Definition::Whole(path))
}

#[test]
fn every_detected_harness_answers_and_the_result_is_remembered() {
    let root = uze_testkit::temp::TempDir::new("machine-overview-collect");
    let mut a = Harness::new("a");
    a.found = Some(vec![rule(root.path(), "A.md")]);
    let mut absent = Harness::new("absent");
    absent.present = false;
    let app = UzeApplication::new(
        UzeHome::at(root.join("uze")),
        vec![Box::new(a), Box::new(Harness::new("b")), Box::new(absent)],
    );
    let mut answers = 0;
    let overview = app
        .collect_machine_overview(&AtomicBool::new(false), |_| answers += 1)
        .unwrap();
    let ids: Vec<_> = overview.harnesses.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(overview.resources.len(), 1);
    assert_eq!(overview.harnesses[0].sessions.unwrap().total, 2);
    assert_eq!(
        answers, 5,
        "one answer per harness's inventory, one with the sessions, one per harness's uses"
    );
    let (remembered, _) = app.machine_overview_snapshot().unwrap();
    assert_eq!(remembered, overview);
}

#[test]
fn a_harness_whose_reader_panics_reads_as_unknown_and_the_others_answer() {
    let root = uze_testkit::temp::TempDir::new("machine-overview-panic");
    let mut broken = Harness::new("broken");
    broken.panics = true;
    let mut fine = Harness::new("fine");
    fine.found = Some(vec![rule(root.path(), "F.md")]);
    let app = UzeApplication::new(
        UzeHome::at(root.join("uze")),
        vec![Box::new(broken), Box::new(fine)],
    );
    let overview = app
        .collect_machine_overview(&AtomicBool::new(false), |_| {})
        .unwrap();
    assert!(!overview.harnesses[0].inventory_known);
    assert!(overview.harnesses[1].inventory_known);
    assert_eq!(overview.resources.len(), 1);
}

#[test]
fn a_cancelled_collection_stops_and_remembers_nothing() {
    let root = uze_testkit::temp::TempDir::new("machine-overview-cancel");
    let app = UzeApplication::new(
        UzeHome::at(root.join("uze")),
        vec![Box::new(Harness::new("a")), Box::new(Harness::new("b"))],
    );
    let cancelled = AtomicBool::new(false);
    let mut answers = 0;
    let result = app.collect_machine_overview(&cancelled, |_| {
        answers += 1;
        cancelled.store(true, Ordering::Relaxed);
    });
    assert!(result.is_none());
    assert_eq!(answers, 1);
    assert!(app.machine_overview_snapshot().is_none());
}

/// Cache is never required for correctness: with the snapshot gone, the
/// Overview is simply collected again and comes out the same.
#[test]
fn deleting_the_snapshot_costs_only_a_collection() {
    let root = uze_testkit::temp::TempDir::new("machine-overview-rebuild");
    let mut a = Harness::new("a");
    a.found = Some(vec![rule(root.path(), "A.md")]);
    let app = UzeApplication::new(UzeHome::at(root.join("uze")), vec![Box::new(a)]);
    let first = app
        .collect_machine_overview(&AtomicBool::new(false), |_| {})
        .unwrap();
    std::fs::remove_file(UzeHome::at(root.join("uze")).machine_overview_cache_path()).unwrap();
    assert!(app.machine_overview_snapshot().is_none());
    let again = app
        .collect_machine_overview(&AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(first, again);
}
