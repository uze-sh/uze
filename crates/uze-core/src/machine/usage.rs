//! How often each skill, agent and MCP server was used, read from the
//! records every harness already keeps of its sessions.
//!
//! An integration only parses its own record ([`Use`]); everything that does
//! not depend on which harness answered is decided here, once:
//! - which records to read again, and from where ([`Resume`]);
//! - what is kept between readings, in the `cache/` tier ([`Ledger`]);
//! - every use the records still hold, and the day of the last one.
//!
//! No hook counts anything: a use is read after the fact, so an agent's turn
//! pays nothing for it and the count covers the past from the first reading.
//! From a record only which resource was used, by whom and when is taken;
//! uses of anything the harness does not carry are dropped before they are
//! kept.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    Result,
    home::UzeHome,
    integration::IntegrationPort,
    inventory::{FoundResource, MachineOverview, ResourceKind},
    persistence::write_atomic,
};

const DAY: u64 = 86_400;

/// One session record a harness kept, as its metadata says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageRecord {
    /// The record's path, or the harness's own session id.
    pub id: String,
    pub stamp: Stamp,
}

/// What tells a record changed since it was last read without opening it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    /// Its length in bytes, or 0 where the harness gives none.
    pub length: u64,
    /// When it last changed, in seconds since the epoch.
    pub changed: u64,
}

impl Stamp {
    pub fn of(length: u64, changed: SystemTime) -> Self {
        Self {
            length,
            changed: unix_seconds(changed),
        }
    }
}

/// Where a reading of one record starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Resume {
    Start,
    /// A byte offset in a record only ever appended to.
    At(u64),
}

/// The uses one reading found, and where the next one continues.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageRead {
    pub uses: Vec<Use>,
    /// `None`: the record is not appended to, and is read whole next time.
    pub resume_at: Option<u64>,
}

/// Who asked for a use.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Invoker {
    /// The person typed it: a slash command, a `$` or `@` mention.
    Person,
    /// The model chose it: a tool call, the read of a skill's file.
    Model,
}

/// What a use names, in the form the same harness's inventory names it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Name(String),
    /// A definition file, joined to the resource defined by it.
    Definition(PathBuf),
}

/// One use of one resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Use {
    pub kind: ResourceKind,
    pub target: Target,
    pub at: SystemTime,
    pub by: Invoker,
    /// `false` when the record does not name the use but lets it be
    /// inferred from what the model wrote.
    pub exact: bool,
}

impl Use {
    pub fn named(kind: ResourceKind, name: impl Into<String>, at: SystemTime, by: Invoker) -> Self {
        Self {
            kind,
            target: Target::Name(name.into()),
            at,
            by,
            exact: true,
        }
    }

    pub fn defined_by(kind: ResourceKind, path: PathBuf, at: SystemTime, by: Invoker) -> Self {
        Self {
            kind,
            target: Target::Definition(path),
            at,
            by,
            exact: true,
        }
    }

    pub fn inferred(mut self) -> Self {
        self.exact = false;
        self
    }
}

/// The uses of one target on one day.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub kind: ResourceKind,
    pub target: Target,
    /// Days since the epoch.
    pub day: u64,
    pub by_person: u32,
    pub by_model: u32,
    /// Whether any of them was inferred.
    #[serde(default)]
    pub estimate: bool,
}

/// How often one resource was used in every record a harness still keeps.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Uses {
    pub total: u64,
    /// The day of the last use, in days since the epoch.
    pub last: Option<u64>,
    /// Some of them were inferred rather than recorded.
    pub estimate: bool,
    /// Every harness carrying the resource could be read.
    pub complete: bool,
}

impl Uses {
    /// Adds what another harness counted.
    pub fn add(&mut self, other: &Uses) {
        self.total += other.total;
        self.last = self.last.max(other.last);
        self.estimate |= other.estimate;
    }

    /// Adds one day's tally.
    pub fn count(&mut self, tally: &Tally) {
        self.total += u64::from(tally.by_person) + u64::from(tally.by_model);
        self.last = self.last.max(Some(tally.day));
        self.estimate |= tally.estimate;
    }
}

pub fn day_of(time: SystemTime) -> u64 {
    unix_seconds(time) / DAY
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|age| age.as_secs())
        .unwrap_or(0)
}

/// What was kept between readings, per harness and record: remembered, so
/// losing it costs one fresh reading and nothing else.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    harnesses: BTreeMap<String, BTreeMap<String, Kept>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Kept {
    stamp: Stamp,
    resume_at: Option<u64>,
    tallies: Vec<Tally>,
}

impl Ledger {
    pub fn load(home: &UzeHome) -> Self {
        std::fs::read(home.machine_usage_cache_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, home: &UzeHome) -> Result<()> {
        let path = home.machine_usage_cache_path();
        let payload = serde_json::to_vec(self).map_err(|source| crate::UzeError::Json {
            path: path.clone(),
            source,
        })?;
        write_atomic(&path, &payload)
    }

    /// Brings what is kept of `harness` up to date with its records and
    /// answers its tallies. Only records whose stamp changed are read, an
    /// appended one from where it was left. `None` when the harness cannot
    /// list its records or one of them cannot be read: its uses are then
    /// unknown, never zero. Stops at the next record once `cancelled`.
    pub fn read(
        &mut self,
        harness: &dyn IntegrationPort,
        carried: &[FoundResource],
        cancelled: &AtomicBool,
    ) -> Option<Vec<Tally>> {
        let records = harness.usage_records(UNIX_EPOCH)?;
        let known = Carried::of(carried);
        let kept = self.harnesses.entry(harness.id().to_owned()).or_default();
        let listed: BTreeSet<&str> = records.iter().map(|record| record.id.as_str()).collect();
        kept.retain(|id, _| listed.contains(id.as_str()));
        let mut complete = true;
        for record in &records {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            let previous = kept.get(&record.id);
            if previous.is_some_and(|previous| previous.stamp == record.stamp) {
                continue;
            }
            let from = match previous {
                Some(Kept {
                    stamp,
                    resume_at: Some(offset),
                    ..
                }) if record.stamp.length >= stamp.length => Resume::At(*offset),
                _ => Resume::Start,
            };
            let Some(read) = harness.read_usage(record, from) else {
                complete = false;
                continue;
            };
            let mut tallies = match (from, kept.remove(&record.id)) {
                (Resume::At(_), Some(previous)) => previous.tallies,
                _ => Vec::new(),
            };
            for found in read.uses.iter().filter(|found| known.carries(found)) {
                add(&mut tallies, found);
            }
            kept.insert(
                record.id.clone(),
                Kept {
                    stamp: record.stamp,
                    resume_at: read.resume_at,
                    tallies,
                },
            );
        }
        complete.then(|| {
            kept.values()
                .flat_map(|kept| kept.tallies.iter().cloned())
                .collect()
        })
    }
}

/// Gives every skill, agent and MCP server of `overview` its uses on each
/// harness carrying it, from that harness's tallies (`None`: its uses are
/// unknown, and it is left out).
pub fn count_uses(overview: &mut MachineOverview, tallies: &BTreeMap<String, Option<Vec<Tally>>>) {
    for report in &mut overview.harnesses {
        report.uses_known = tallies.get(&report.id).is_some_and(Option::is_some);
    }
    for resource in &mut overview.resources {
        if !COUNTED.contains(&resource.kind) {
            continue;
        }
        resource.uses.clear();
        for location in &resource.locations {
            let Some(Some(harness)) = tallies.get(&location.harness) else {
                continue;
            };
            let uses = resource
                .uses
                .entry(location.harness.clone())
                .or_insert(Uses {
                    complete: true,
                    ..Uses::default()
                });
            for tally in harness.iter().filter(|tally| {
                tally.kind == resource.kind
                    && match &tally.target {
                        Target::Name(name) => *name == resource.name,
                        Target::Definition(path) => *path == location.path,
                    }
            }) {
                uses.count(tally);
            }
        }
    }
}

/// The kinds a use is counted for: an instruction file is always on, and a
/// hook's runs are not recorded.
pub const COUNTED: [ResourceKind; 3] =
    [ResourceKind::Skill, ResourceKind::Agent, ResourceKind::Mcp];

/// What one harness carries, as a use can name it.
struct Carried<'a> {
    names: BTreeSet<(ResourceKind, &'a str)>,
    definitions: BTreeSet<&'a std::path::Path>,
}

impl<'a> Carried<'a> {
    fn of(found: &'a [FoundResource]) -> Self {
        Self {
            names: found
                .iter()
                .map(|resource| (resource.kind, resource.name.as_str()))
                .collect(),
            definitions: found
                .iter()
                .map(|resource| resource.definition.path().as_path())
                .collect(),
        }
    }

    fn carries(&self, found: &Use) -> bool {
        match &found.target {
            Target::Name(name) => self.names.contains(&(found.kind, name.as_str())),
            Target::Definition(path) => self.definitions.contains(path.as_path()),
        }
    }
}

fn add(tallies: &mut Vec<Tally>, found: &Use) {
    let day = day_of(found.at);
    let index = tallies
        .iter()
        .position(|tally| {
            tally.day == day && tally.kind == found.kind && tally.target == found.target
        })
        .unwrap_or_else(|| {
            tallies.push(Tally {
                kind: found.kind,
                target: found.target.clone(),
                day,
                by_person: 0,
                by_model: 0,
                estimate: false,
            });
            tallies.len() - 1
        });
    let tally = &mut tallies[index];
    match found.by {
        Invoker::Person => tally.by_person += 1,
        Invoker::Model => tally.by_model += 1,
    }
    tally.estimate |= !found.exact;
}

#[cfg(test)]
mod tests;
