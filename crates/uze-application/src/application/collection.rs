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
