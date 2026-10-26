//! What asks a source again beside its own surfaces: the clock of its
//! `every`, and the events its `refresh-on` names, taken together when
//! they arrive close, so a burst of saves asks once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use uze_contract::EventName;

use crate::contract::CheckoutEvent;

/// How long after an event others are taken with it.
pub const COALESCE: Duration = Duration::from_millis(250);

/// The sources events asked again, gathered from the first event until
/// [`COALESCE`] has passed.
#[derive(Debug, Default)]
pub(super) struct Window {
    opened: Option<Instant>,
    sources: BTreeSet<String>,
}

impl Window {
    pub(super) fn add(&mut self, source: String, now: Instant) {
        self.opened.get_or_insert(now);
        self.sources.insert(source);
    }

    /// The sources gathered, once the window has stood for [`COALESCE`];
    /// nothing before.
    pub(super) fn take(&mut self, now: Instant) -> BTreeSet<String> {
        match self.opened {
            Some(opened) if now.saturating_duration_since(opened) >= COALESCE => {
                self.opened = None;
                std::mem::take(&mut self.sources)
            }
            _ => BTreeSet::new(),
        }
    }
}

/// Whether what `happened` is a reason a source `declared`: the same
/// event, and for `paths.changed` one of the same paths.
pub(super) fn is_reason(declared: &EventName, happened: &EventName) -> bool {
    match (declared, happened) {
        (EventName::CheckoutChanged, EventName::CheckoutChanged) => true,
        (
            EventName::PathsChanged { paths: declared },
            EventName::PathsChanged { paths: changed },
        ) => declared.iter().any(|path| changed.contains(path)),
        _ => false,
    }
}

/// What to watch, one entry per checkout and event, each path once and in
/// order.
pub(super) fn merged(
    watched: impl IntoIterator<Item = (PathBuf, EventName)>,
) -> Vec<CheckoutEvent> {
    let mut checkouts: BTreeMap<PathBuf, (bool, BTreeSet<String>)> = BTreeMap::new();
    for (checkout, event) in watched {
        let entry = checkouts.entry(checkout).or_default();
        match event {
            EventName::CheckoutChanged => entry.0 = true,
            EventName::PathsChanged { paths } => entry.1.extend(paths),
        }
    }
    let mut events = Vec::new();
    for (checkout, (changed, paths)) in checkouts {
        if changed {
            events.push(CheckoutEvent {
                checkout: checkout.clone(),
                event: EventName::CheckoutChanged,
            });
        }
        if !paths.is_empty() {
            events.push(CheckoutEvent {
                checkout,
                event: EventName::PathsChanged {
                    paths: paths.into_iter().collect(),
                },
            });
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(paths: &[&str]) -> EventName {
        EventName::PathsChanged {
            paths: paths.iter().map(|path| (*path).to_owned()).collect(),
        }
    }

    #[test]
    fn a_burst_inside_the_window_is_taken_once_when_it_closes() {
        let start = Instant::now();
        let mut window = Window::default();
        window.add("board".to_owned(), start);
        window.add("board".to_owned(), start + Duration::from_millis(100));
        window.add("log".to_owned(), start + Duration::from_millis(200));
        assert!(window.take(start + Duration::from_millis(249)).is_empty());
        let taken = window.take(start + COALESCE);
        assert_eq!(
            taken.into_iter().collect::<Vec<_>>(),
            ["board".to_owned(), "log".to_owned()]
        );
        assert!(window.take(start + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn an_event_after_the_window_closed_opens_another() {
        let start = Instant::now();
        let mut window = Window::default();
        window.add("board".to_owned(), start);
        assert_eq!(window.take(start + COALESCE).len(), 1);
        let later = start + Duration::from_secs(1);
        window.add("board".to_owned(), later);
        assert!(window.take(later + Duration::from_millis(10)).is_empty());
        assert_eq!(window.take(later + COALESCE).len(), 1);
    }

    #[test]
    fn a_path_event_is_a_reason_only_for_a_path_it_names() {
        assert!(is_reason(&paths(&[".planning", "docs"]), &paths(&["docs"])));
        assert!(!is_reason(&paths(&[".planning"]), &paths(&["docs"])));
        assert!(!is_reason(&paths(&["docs"]), &EventName::CheckoutChanged));
        assert!(is_reason(
            &EventName::CheckoutChanged,
            &EventName::CheckoutChanged
        ));
    }

    #[test]
    fn what_is_watched_is_one_entry_per_checkout_and_event() {
        let a = PathBuf::from("/a");
        let watched = merged([
            (a.clone(), paths(&["docs", ".planning"])),
            (a.clone(), EventName::CheckoutChanged),
            (a.clone(), paths(&["docs"])),
            (PathBuf::from("/b"), EventName::CheckoutChanged),
        ]);
        assert_eq!(
            watched,
            vec![
                CheckoutEvent {
                    checkout: a.clone(),
                    event: EventName::CheckoutChanged,
                },
                CheckoutEvent {
                    checkout: a,
                    event: paths(&[".planning", "docs"]),
                },
                CheckoutEvent {
                    checkout: PathBuf::from("/b"),
                    event: EventName::CheckoutChanged,
                },
            ]
        );
    }
}
