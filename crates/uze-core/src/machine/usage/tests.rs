use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Mutex, atomic::AtomicBool},
    time::{Duration, SystemTime},
};

use super::*;
use crate::{
    ExposurePlan, Resource,
    integration::HarnessDetection,
    inventory::{
        Definition, HarnessReport, Location, MachineOverview, Origin, OverviewResource, Status,
    },
    router::HarnessCapabilities,
};

/// A harness whose records are lists of uses, one byte per use, so an
/// offset is how many were read.
#[derive(Default)]
struct Harness {
    records: BTreeMap<String, Vec<Use>>,
    unreadable: Option<String>,
    reads: Mutex<Vec<(String, Resume)>>,
}

impl Harness {
    fn write(&mut self, record: &str, uses: Vec<Use>) {
        self.records.insert(record.to_owned(), uses);
    }
}

impl IntegrationPort for Harness {
    fn id(&self) -> &'static str {
        "probe"
    }
    fn display_name(&self) -> &'static str {
        "Probe"
    }
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }
    fn exposure_plan(&self, _resource: &Resource) -> ExposurePlan {
        panic!("not used")
    }
    fn detect(&self) -> HarnessDetection {
        HarnessDetection {
            present: true,
            version: None,
        }
    }
    fn usage_records(&self, _since: SystemTime) -> Option<Vec<UsageRecord>> {
        Some(
            self.records
                .iter()
                .map(|(id, uses)| UsageRecord {
                    id: id.clone(),
                    stamp: Stamp {
                        length: uses.len() as u64,
                        changed: 1,
                    },
                })
                .collect(),
        )
    }
    fn read_usage(&self, record: &UsageRecord, from: Resume) -> Option<UsageRead> {
        self.reads.lock().unwrap().push((record.id.clone(), from));
        if self.unreadable.as_deref() == Some(record.id.as_str()) {
            return None;
        }
        let uses = &self.records[&record.id];
        let start = match from {
            Resume::Start => 0,
            Resume::At(offset) => offset as usize,
        };
        Some(UsageRead {
            uses: uses[start..].to_vec(),
            resume_at: Some(uses.len() as u64),
        })
    }
}

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(400 * DAY)
}

fn days_ago(days: u64) -> SystemTime {
    now() - Duration::from_secs(days * DAY)
}

fn skill(name: &str) -> FoundResource {
    FoundResource::new(
        ResourceKind::Skill,
        name,
        Definition::Markdown(PathBuf::from(format!("/skills/{name}/SKILL.md"))),
    )
}

fn typed(name: &str) -> Use {
    Use::named(ResourceKind::Skill, name, days_ago(1), Invoker::Person)
}

fn total(tallies: &[Tally]) -> u32 {
    tallies.iter().map(|t| t.by_person + t.by_model).sum()
}

fn read(ledger: &mut Ledger, harness: &Harness, carried: &[FoundResource]) -> Option<Vec<Tally>> {
    ledger.read(harness, carried, &AtomicBool::new(false))
}

#[test]
fn a_record_that_did_not_change_is_not_read_again() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review")]);
    let carried = [skill("review")];
    let mut ledger = Ledger::default();
    assert_eq!(total(&read(&mut ledger, &harness, &carried).unwrap()), 1);
    assert_eq!(total(&read(&mut ledger, &harness, &carried).unwrap()), 1);
    assert_eq!(harness.reads.lock().unwrap().len(), 1);
}

#[test]
fn an_appended_record_is_read_from_where_it_was_left() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review")]);
    let carried = [skill("review")];
    let mut ledger = Ledger::default();
    read(&mut ledger, &harness, &carried);
    harness.write("a", vec![typed("review"), typed("review"), typed("review")]);
    let tallies = read(&mut ledger, &harness, &carried).unwrap();
    assert_eq!(total(&tallies), 3);
    assert_eq!(
        harness.reads.lock().unwrap()[1],
        ("a".to_owned(), Resume::At(1))
    );
}

#[test]
fn a_record_that_shrank_is_read_whole_and_replaces_what_was_kept() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review"), typed("review")]);
    let carried = [skill("review")];
    let mut ledger = Ledger::default();
    read(&mut ledger, &harness, &carried);
    harness.write("a", vec![typed("review")]);
    let tallies = read(&mut ledger, &harness, &carried).unwrap();
    assert_eq!(total(&tallies), 1);
    assert_eq!(harness.reads.lock().unwrap()[1].1, Resume::Start);
}

#[test]
fn a_record_no_longer_listed_is_forgotten() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review")]);
    harness.write("b", vec![typed("review")]);
    let carried = [skill("review")];
    let mut ledger = Ledger::default();
    read(&mut ledger, &harness, &carried);
    harness.records.remove("a");
    assert_eq!(total(&read(&mut ledger, &harness, &carried).unwrap()), 1);
}

#[test]
fn a_use_of_something_the_harness_does_not_carry_is_never_kept() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review"), typed("clear")]);
    let mut ledger = Ledger::default();
    let tallies = read(&mut ledger, &harness, &[skill("review")]).unwrap();
    assert_eq!(tallies.len(), 1);
    assert_eq!(tallies[0].target, Target::Name("review".into()));
}

#[test]
fn a_record_that_cannot_be_read_makes_the_harness_unknown_never_zero() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review")]);
    harness.write("b", vec![]);
    harness.unreadable = Some("b".into());
    let mut ledger = Ledger::default();
    assert!(read(&mut ledger, &harness, &[skill("review")]).is_none());
}

#[test]
fn a_cancelled_reading_answers_nothing() {
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review")]);
    let mut ledger = Ledger::default();
    let cancelled = AtomicBool::new(true);
    assert!(
        ledger
            .read(&harness, &[skill("review")], &cancelled)
            .is_none()
    );
    assert!(harness.reads.lock().unwrap().is_empty());
}

#[test]
fn losing_what_was_kept_costs_only_a_fresh_reading() {
    let root = uze_testkit::temp::TempDir::new("machine-usage-ledger");
    let home = UzeHome::at(root.path());
    let mut harness = Harness::default();
    harness.write("a", vec![typed("review"), typed("review")]);
    let carried = [skill("review")];
    let mut ledger = Ledger::load(&home);
    let first = read(&mut ledger, &harness, &carried).unwrap();
    ledger.save(&home).unwrap();
    let mut kept = Ledger::load(&home);
    assert_eq!(read(&mut kept, &harness, &carried).unwrap(), first);
    std::fs::write(home.machine_usage_cache_path(), b"{not json").unwrap();
    let mut lost = Ledger::load(&home);
    assert_eq!(read(&mut lost, &harness, &carried).unwrap(), first);
}

fn row(name: &str, harnesses: &[&str]) -> OverviewResource {
    OverviewResource {
        id: name.into(),
        kind: ResourceKind::Skill,
        name: name.into(),
        origin: Origin::You,
        locations: harnesses
            .iter()
            .map(|harness| Location {
                harness: (*harness).into(),
                path: PathBuf::from(format!("/{harness}/{name}/SKILL.md")),
            })
            .collect(),
        description: None,
        per_session: None,
        when_used: None,
        status: Status::Ok,
        checked: true,
        removal: None,
        uses: BTreeMap::new(),
    }
}

fn overview(rows: Vec<OverviewResource>) -> MachineOverview {
    MachineOverview {
        harnesses: ["a", "b"]
            .into_iter()
            .map(|id| HarnessReport {
                id: id.into(),
                name: id.into(),
                inventory_known: true,
                sessions: None,
                uses_known: false,
            })
            .collect(),
        resources: rows,
    }
}

fn tally(target: Target, age: u64, person: u32, model: u32) -> Tally {
    Tally {
        kind: ResourceKind::Skill,
        target,
        day: day_of(now()) - age,
        by_person: person,
        by_model: model,
        estimate: false,
    }
}

#[test]
fn uses_are_summed_per_period_by_name_or_by_definition() {
    let mut machine = overview(vec![row("review", &["a", "b"])]);
    let tallies = BTreeMap::from([
        (
            "a".to_owned(),
            Some(vec![tally(Target::Name("review".into()), 2, 1, 1)]),
        ),
        (
            "b".to_owned(),
            Some(vec![tally(
                Target::Definition("/b/review/SKILL.md".into()),
                20,
                0,
                3,
            )]),
        ),
    ]);
    count_uses(&mut machine, &tallies);
    let uses = machine.resources[0].uses_on(None).unwrap();
    assert_eq!(uses.total, 5);
    assert_eq!(uses.last, Some(day_of(now()) - 2), "the latest use's day");
    assert!(uses.complete);
    let on_a = machine.resources[0].uses_on(Some("a")).unwrap();
    assert_eq!(on_a.total, 2, "one harness's figure is its own");
    assert!(machine.harnesses.iter().all(|h| h.uses_known));
}

#[test]
fn unused_needs_every_carrying_harness_read() {
    let mut machine = overview(vec![row("quiet", &["a"]), row("half", &["a", "b"])]);
    let tallies = BTreeMap::from([("a".to_owned(), Some(Vec::new())), ("b".to_owned(), None)]);
    count_uses(&mut machine, &tallies);
    assert!(machine.resources[0].unused(None));
    assert!(!machine.resources[1].unused(None));
    assert!(
        machine.resources[1].unused(Some("a")),
        "narrowed to the harness that was read"
    );
    assert_eq!(machine.resources[1].uses_on(None).unwrap().total, 0);
}

#[test]
fn a_harness_that_cannot_be_read_leaves_its_resources_unknown() {
    let mut machine = overview(vec![row("only-b", &["b"])]);
    let tallies = BTreeMap::from([("b".to_owned(), None)]);
    count_uses(&mut machine, &tallies);
    assert_eq!(machine.resources[0].uses_on(None), None);
}

#[test]
fn an_inferred_use_makes_the_count_an_estimate() {
    let mut harness = Harness::default();
    harness.write(
        "a",
        vec![Use::named(ResourceKind::Skill, "review", days_ago(1), Invoker::Model).inferred()],
    );
    let mut ledger = Ledger::default();
    let tallies = read(&mut ledger, &harness, &[skill("review")]).unwrap();
    let mut machine = overview(vec![row("review", &["a"])]);
    count_uses(
        &mut machine,
        &BTreeMap::from([("a".to_owned(), Some(tallies))]),
    );
    assert!(machine.resources[0].uses_on(None).unwrap().estimate);
}
