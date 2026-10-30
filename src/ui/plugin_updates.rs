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

static TOLD: Mutex<Told> = Mutex::new(Told {
    updated: Vec::new(),
    waiting: Vec::new(),
});
static PENDING: AtomicBool = AtomicBool::new(false);

/// What the pass did since the last call, once: the workspace asks every
/// frame, so the answer is one atomic load until there is something.
pub(crate) fn take() -> Option<Told> {
    if !PENDING.swap(false, Ordering::AcqRel) {
        return None;
    }
    let told = std::mem::take(&mut *TOLD.lock().unwrap_or_else(PoisonError::into_inner));
    (told != Told::default()).then_some(told)
}

fn publish(updated: Vec<String>, waiting: Vec<String>) {
    if updated.is_empty() && waiting.is_empty() {
        return;
    }
    let mut told = TOLD.lock().unwrap_or_else(PoisonError::into_inner);
    told.updated.extend(updated);
    told.waiting.extend(waiting);
    PENDING.store(true, Ordering::Release);
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
            publish(updated, waiting);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_done_is_taken_once() {
        assert_eq!(take(), None);
        publish(vec!["flow@mkt".to_owned()], Vec::new());
        publish(Vec::new(), vec!["review@mkt".to_owned()]);
        assert_eq!(
            take(),
            Some(Told {
                updated: vec!["flow@mkt".to_owned()],
                waiting: vec!["review@mkt".to_owned()],
            })
        );
        assert_eq!(take(), None);
    }
}
