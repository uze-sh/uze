//! One collector: every owner of a disposable path decides what of it is
//! still needed, and one registry runs them all (design D7 of
//! `doctor-repair`).
//!
//! The map of what UZE owns marks the paths UZE may sweep disposable and
//! holds no rule of its own. Each owner implements [`Collectable`] where
//! its rule is known — the projections by whether their project is still
//! there, the marketplace mirrors by the commit they serve, an extension's
//! scratch by whether the process that made it lives — and collects under
//! its own lock. The maintenance pass `doctor` and the workspace run calls
//! every collector, an owner's inline trigger calls its own, and every one
//! reports what it removed to the same report.
//!
//! `every_disposable_root_has_one_collector` holds every disposable path to
//! exactly one collector: a path added without one is how the dispersion
//! this replaces began.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use uze_core::home::Tier;

use super::services::Health;
use super::*;

/// The owner of one or more disposable paths, which knows what of them is
/// still needed.
pub trait Collectable: Send + Sync {
    /// What the report names it by.
    fn name(&self) -> &'static str;

    /// The disposable paths it answers for, by the builder that composes
    /// them (`UzeHome::owned_paths`).
    fn roots(&self) -> &'static [&'static str];

    /// Removes what its rule says nothing needs any more, under its own
    /// lock, and says what it removed. Never fails: what it cannot read or
    /// remove is left for the next pass.
    fn collect(&self, home: &UzeHome) -> Collected;
}

/// What one collector removed.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Collected {
    pub collector: &'static str,
    pub removed: Vec<PathBuf>,
    /// Why it did not run this time: its lock held by another process, or
    /// a check that stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

impl Collected {
    pub fn removed(collector: &'static str, removed: Vec<PathBuf>) -> Self {
        Self {
            collector,
            removed,
            skipped: None,
        }
    }

    pub fn skipped(collector: &'static str, reason: impl Into<String>) -> Self {
        Self {
            collector,
            removed: Vec::new(),
            skipped: Some(reason.into()),
        }
    }
}

/// The collectors registered from outside this crate: the binary's own,
/// for what only it writes (its log journals). Process-wide, since every
/// application a process builds sweeps the same home.
static REGISTERED: Mutex<Vec<Arc<dyn Collectable>>> = Mutex::new(Vec::new());

/// Registers a collector the application cannot name itself. One of a
/// name already registered replaces it.
pub fn register_collector(collector: Arc<dyn Collectable>) {
    let mut registered = REGISTERED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    registered.retain(|known| known.name() != collector.name());
    registered.push(collector);
}

/// A registered collector, borrowed for one pass.
struct Registered(Arc<dyn Collectable>);

impl Collectable for Registered {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn roots(&self) -> &'static [&'static str] {
        self.0.roots()
    }
    fn collect(&self, home: &UzeHome) -> Collected {
        self.0.collect(home)
    }
}

impl UzeApplication {
    /// Every collector: the application's own owners, then those
    /// registered from outside it.
    pub fn collectors(&self) -> Vec<Box<dyn Collectable + '_>> {
        let mut collectors = self.own_collectors();
        collectors.extend(
            REGISTERED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .map(|collector| {
                    Box::new(Registered(Arc::clone(collector))) as Box<dyn Collectable>
                }),
        );
        collectors
    }

    fn own_collectors(&self) -> Vec<Box<dyn Collectable + '_>> {
        vec![
            Box::new(super::maintenance::Projections),
            Box::new(super::maintenance::IntegrationProjections(
                &self.integrations,
            )),
            Box::new(super::rules_engine::RuleSessions),
            Box::new(super::extensions::CompiledViews(self)),
            Box::new(super::extensions::ExtensionScratch),
            Box::new(super::extensions::ExtensionPictures),
            Box::new(super::extensions::DeliveredPackages),
            Box::new(super::marketplace_catalogue::MarketplaceMirrors),
            Box::new(super::maintenance::ConversationSessions),
            Box::new(super::maintenance::AgentReports),
            Box::new(super::setup::Retired),
        ]
    }
}

impl Health<'_> {
    /// Runs every collector, each isolated from another's failure.
    #[tracing::instrument(name = "health.collect", skip_all)]
    pub fn collect(&self) -> Vec<Collected> {
        self.0
            .collectors()
            .iter()
            .map(|collector| {
                catch_unwind(AssertUnwindSafe(|| collector.collect(&self.0.home))).unwrap_or_else(
                    |_| Collected::skipped(collector.name(), "it stopped unexpectedly"),
                )
            })
            .collect()
    }
}

/// Removes `path`, a directory with everything in it or a file, answering
/// whether it is gone.
pub(crate) fn remove(path: &Path) -> bool {
    let removed = if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removed.is_ok()
}
/// What the last measure of the home found, remembered in
/// `cache/collection.json`: plain `doctor` shows it without walking
/// anything, `doctor --repair` measures it again.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionRecord {
    /// When it was measured, in seconds since the epoch.
    pub measured_at_unix: u64,
    /// The bytes each tier held, by the tier's label.
    pub tiers: Vec<(String, u64)>,
}

impl CollectionRecord {
    pub fn total(&self) -> u64 {
        self.tiers.iter().map(|(_, bytes)| bytes).sum()
    }

    /// The figure last recorded, when one was and it still reads.
    pub fn read(home: &UzeHome) -> Option<Self> {
        let bytes = std::fs::read(home.collection_record_path()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Measures every tier of `home` now and remembers it.
    pub(crate) fn measure(home: &UzeHome) -> Self {
        let record = Self {
            measured_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            tiers: Tier::ALL
                .iter()
                .map(|tier| (tier.label().to_owned(), tier_bytes(home, *tier)))
                .collect(),
        };
        if let Ok(payload) = serde_json::to_vec_pretty(&record) {
            let _ = uze_core::persistence::write_atomic_unsynced(
                &home.collection_record_path(),
                &payload,
            );
        }
        record
    }
}

/// The bytes the top-level entries of `home` in `tier` hold.
fn tier_bytes(home: &UzeHome, tier: Tier) -> u64 {
    std::fs::read_dir(home.root())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| Tier::of(home, path) == Some(tier))
        .map(|path| super::services::disk::measure(&path, &[]).0)
        .sum()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// A collector registered from outside, as the binary registers its
    /// journals': it removes nothing, so it disturbs no other test's pass.
    struct Outside;

    impl Collectable for Outside {
        fn name(&self) -> &'static str {
            "outside collector of the collection tests"
        }
        fn roots(&self) -> &'static [&'static str] {
            &["logs_dir"]
        }
        fn collect(&self, _home: &UzeHome) -> Collected {
            Collected::removed(self.name(), Vec::new())
        }
    }

    #[test]
    fn a_registered_collector_runs_with_the_applications_own_once() {
        register_collector(Arc::new(Outside));
        register_collector(Arc::new(Outside));
        let root = uze_testkit::temp::scratch("collection-registered");
        let app = UzeApplication::new(UzeHome::at(&root), Vec::new());
        let collectors = app.collectors();
        let outside: Vec<_> = collectors
            .iter()
            .filter(|collector| collector.name() == Outside.name())
            .collect();
        assert_eq!(outside.len(), 1, "registered again, it replaces itself");
        assert_eq!(outside[0].roots(), ["logs_dir"]);
        assert_eq!(
            outside[0].collect(&app.home).collector,
            Outside.name(),
            "its own pass answers"
        );
        assert!(
            collectors.len() > 1,
            "the application's own collectors run too"
        );

        let collected = app.health().collect();
        let names: Vec<&str> = collected.iter().map(|each| each.collector).collect();
        let expected: Vec<&str> = collectors.iter().map(|each| each.name()).collect();
        assert_eq!(names, expected, "every collector answers, once, in order");
        let _ = std::fs::remove_dir_all(root);
    }

    /// The crate's half of `every_disposable_root_has_one_collector`: the
    /// application's own collectors answer for every disposable path but
    /// the log journals, which only the binary writes and collects, and
    /// claim nothing else.
    #[test]
    fn the_applications_collectors_answer_for_every_disposable_path() {
        let root = uze_testkit::temp::scratch("collection-own");
        let app = UzeApplication::new(UzeHome::at(&root), Vec::new());
        let mut claimed: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for collector in app.own_collectors() {
            for path in collector.roots() {
                claimed.entry(path).or_default().push(collector.name());
            }
        }
        let uncollected: Vec<&str> = UzeHome::owned_paths()
            .iter()
            .filter(|owned| owned.disposable && owned.name != "logs_dir")
            .filter(|owned| !claimed.contains_key(owned.name))
            .map(|owned| owned.name)
            .collect();
        assert!(uncollected.is_empty(), "unclaimed: {uncollected:?}");
        let twice: Vec<_> = claimed.iter().filter(|(_, by)| by.len() > 1).collect();
        assert!(twice.is_empty(), "claimed twice: {twice:?}");
        let not_disposable: Vec<&&str> = claimed
            .keys()
            .filter(|name| !UzeHome::owned_path(name).is_some_and(|owned| owned.disposable))
            .collect();
        assert!(
            not_disposable.is_empty(),
            "not disposable: {not_disposable:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Each of the application's collectors reports under a name of its
    /// own, so what one removed is never read as another's.
    #[test]
    fn each_collector_reports_under_a_name_of_its_own() {
        let root = uze_testkit::temp::scratch("collection-names");
        let app = UzeApplication::new(UzeHome::at(&root), Vec::new());
        let names: Vec<&str> = app
            .own_collectors()
            .iter()
            .map(|collector| collector.name())
            .collect();
        assert!(names.iter().all(|name| !name.is_empty()), "{names:?}");
        assert_eq!(
            names.iter().collect::<BTreeSet<_>>().len(),
            names.len(),
            "{names:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_skipped_collection_removed_nothing_and_says_why() {
        let skipped = Collected::skipped("mirrors", "its lock is held");
        assert_eq!(skipped.collector, "mirrors");
        assert!(skipped.removed.is_empty());
        assert_eq!(skipped.skipped.as_deref(), Some("its lock is held"));
    }

    /// A measure gives every tier its bytes, the tier that holds something
    /// more than nothing and an empty tier nothing, and is remembered
    /// for the next plain `doctor`.
    #[test]
    fn a_measure_counts_each_tier_and_is_remembered() {
        let root = uze_testkit::temp::scratch("collection-measure");
        let home = UzeHome::at(&root);
        assert_eq!(CollectionRecord::read(&home), None);
        std::fs::create_dir_all(home.state_dir()).unwrap();
        std::fs::write(home.state_dir().join("record.json"), vec![b'x'; 8192]).unwrap();

        let record = CollectionRecord::measure(&home);
        assert!(record.measured_at_unix > 0);
        assert_eq!(record.tiers.len(), Tier::ALL.len());
        let bytes = |tier: Tier| {
            record
                .tiers
                .iter()
                .find(|(label, _)| label == tier.label())
                .map(|(_, bytes)| *bytes)
                .unwrap()
        };
        assert!(bytes(Tier::Record) > 0, "{record:?}");
        assert_eq!(bytes(Tier::Bytes), 0, "{record:?}");
        assert_eq!(bytes(Tier::Store), 0, "{record:?}");
        assert_eq!(record.total(), bytes(Tier::Record), "{record:?}");
        assert_eq!(CollectionRecord::read(&home), Some(record));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_records_total_is_every_tier_together() {
        let record = CollectionRecord {
            measured_at_unix: 1,
            tiers: vec![("store".to_owned(), 2), ("record".to_owned(), 3)],
        };
        assert_eq!(record.total(), 5);
    }
}
