//! The machine Overview: what every detected harness carries, collected
//! on demand and remembered in the cache tier.
//!
//! Collection is read only and on demand. The caller decides when — the
//! workspace does so only while its Overview is in front — and it answers
//! as it goes, so a screen can fill in harness by harness. Nothing here is
//! reached by a CLI command, the runtime shim or a launch.

use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

use uze_core::{
    integration::IntegrationPort,
    UzeHome,
    inventory::{self, HarnessInventory, MachineOverview},
    usage,
};

use super::UzeApplication;

impl UzeApplication {
    /// The last Overview collected and when, if one can be read.
    pub fn machine_overview_snapshot(&self) -> Option<(MachineOverview, SystemTime)> {
        inventory::load_snapshot(&self.home)
    }

    /// Collects the Overview: every detected harness's inventory first,
    /// one at a time, then their session counts, then their uses from
    /// their session records. `answer` is handed the Overview after each
    /// harness's inventory, once more with the sessions, and after each
    /// harness's uses. Returns `None` when `cancelled` was set
    /// before the end; what is complete is then left unsaved.
    pub fn collect_machine_overview(
        &self,
        cancelled: &AtomicBool,
        mut answer: impl FnMut(&MachineOverview),
    ) -> Option<MachineOverview> {
        let receipts = uze_core::state::receipts(&self.home, None).unwrap_or_default();
        let detected: Vec<&dyn IntegrationPort> = self
            .integrations
            .iter()
            .map(AsRef::as_ref)
            .filter(|integration| self.detect_cached(*integration).present)
            .collect();
        let mut gathered: Vec<HarnessInventory> = Vec::new();
        for integration in &detected {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            gathered.push(HarnessInventory {
                id: integration.id().to_owned(),
                name: integration.display_name().to_owned(),
                found: answered(|| integration.machine_inventory()),
                sessions: None,
            });
            answer(&inventory::assemble(gathered.clone(), &receipts));
        }
        let now = SystemTime::now();
        for (index, integration) in detected.iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                return None;
            }
            gathered[index].sessions = answered(|| integration.session_counts(now));
        }
        let mut overview = inventory::assemble(gathered.clone(), &receipts);
        answer(&overview);
        let mut ledger = usage::Ledger::load(&self.home);
        let mut tallies = BTreeMap::new();
        for (index, integration) in detected.iter().enumerate() {
            let carried = gathered[index].found.as_deref().unwrap_or_default();
            let read = answered(|| Some(ledger.read(*integration, carried, now, cancelled)));
            if cancelled.load(Ordering::Relaxed) {
                save_ledger(&self.home, &ledger);
                return None;
            }
            tallies.insert(integration.id().to_owned(), read.flatten());
            usage::count_uses(&mut overview, &tallies, now);
            answer(&overview);
        }
        save_ledger(&self.home, &ledger);
        if let Err(error) = inventory::save_snapshot(&self.home, &overview, now) {
            tracing::warn!("machine overview not remembered: {error}");
        }
        Some(overview)
    }
}

/// What was read is kept even when the collection stops part way: each
/// record in it is whole, so the next reading continues from there.
fn save_ledger(home: &UzeHome, ledger: &usage::Ledger) {
    if let Err(error) = ledger.save(home) {
        tracing::warn!("machine usage not remembered: {error}");
    }
}

/// A harness whose reader panics answers "not known", and the rest of the
/// Overview is collected as if it had.
fn answered<T>(read: impl FnOnce() -> Option<T>) -> Option<T> {
    catch_unwind(AssertUnwindSafe(read)).unwrap_or_else(|_| {
        tracing::error!("a harness's machine inventory reader panicked");
        None
    })
}

#[cfg(test)]
mod tests;
