//! Keeping plugins current while the workspace is open.
//!
//! Opening the workspace applies the pending updates once (see
//! `worker::spawn_startup`); a workspace stays open for days, so this
//! repeats it on the release check's wake. Each wake fetches the
//! marketplaces whose answer has gone stale, one process per machine, and
//! applies what needs no new trust. What it did is held here, process-wide
//! like the release notice, until the workspace takes it and says so.

use std::{
    collections::BTreeSet,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use uze_application::UzeHome;

use super::{orchestrator::answered_or, tui_application};

/// What the background pass did since the workspace last asked.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Told {
    /// Plugins updated, by `name@marketplace`.
    pub(crate) updated: Vec<String>,
    /// Plugins with an update the pass would not apply alone, each told
    /// once while it waits.
    pub(crate) waiting: Vec<String>,
}

/// What the pass did, waiting for the workspace to take it.
struct Mailbox {
    told: Mutex<Told>,
    pending: AtomicBool,
}

impl Mailbox {
    const fn new() -> Self {
        Self {
            told: Mutex::new(Told {
                updated: Vec::new(),
                waiting: Vec::new(),
            }),
            pending: AtomicBool::new(false),
        }
    }

    fn take(&self) -> Option<Told> {
        if !self.pending.swap(false, Ordering::AcqRel) {
            return None;
        }
        let told = std::mem::take(&mut *self.told.lock().unwrap_or_else(PoisonError::into_inner));
        (told != Told::default()).then_some(told)
    }

    fn publish(&self, updated: Vec<String>, waiting: Vec<String>) {
        if updated.is_empty() && waiting.is_empty() {
            return;
        }
        let mut told = self.told.lock().unwrap_or_else(PoisonError::into_inner);
        told.updated.extend(updated);
        told.waiting.extend(waiting);
        self.pending.store(true, Ordering::Release);
    }
}

static MAILBOX: Mailbox = Mailbox::new();

/// What the pass did since the last call, once: the workspace asks every
/// frame, so the answer is one atomic load until there is something.
pub(crate) fn take() -> Option<Told> {
    MAILBOX.take()
}

/// Starts the pass for this process, on a thread of its own. The first
/// wake is a period away: opening already ran it.
pub(crate) fn watch(home: UzeHome) {
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let mut waiting_told = BTreeSet::new();
        loop {
            thread::sleep(crate::self_update::jittered(crate::self_update::WAKE_EVERY));
            let _span = tracing::debug_span!(parent: &parent, "plugins.watch").entered();
            let outcomes = answered_or(
                || {
                    tui_application(home.clone())
                        .map(|app| app.plugins().keep_current())
                        .unwrap_or_default()
                },
                Vec::new(),
            );
            let (applied, held): (Vec<_>, Vec<_>) =
                outcomes.into_iter().partition(|outcome| outcome.applied);
            let updated: Vec<String> = applied.into_iter().map(|outcome| outcome.plugin).collect();
            let held: BTreeSet<String> = held.into_iter().map(|outcome| outcome.plugin).collect();
            let waiting = held.difference(&waiting_told).cloned().collect();
            // Forgetting what no longer waits is what lets one that waits
            // again be told again.
            waiting_told = held;
            if !updated.is_empty() {
                super::extension_switch::plugins_changed();
            }
            MAILBOX.publish(updated, waiting);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mailbox of its own: the workspace's tests drain the process's
    /// one on every pump, and would take what this one publishes.
    #[test]
    fn what_was_done_is_taken_once() {
        let mailbox = Mailbox::new();
        assert_eq!(mailbox.take(), None);
        mailbox.publish(vec!["flow@mkt".to_owned()], Vec::new());
        mailbox.publish(Vec::new(), vec!["review@mkt".to_owned()]);
        assert_eq!(
            mailbox.take(),
            Some(Told {
                updated: vec!["flow@mkt".to_owned()],
                waiting: vec!["review@mkt".to_owned()],
            })
        );
        assert_eq!(mailbox.take(), None);
    }
}
