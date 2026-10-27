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

/// The most events a source delivered `each` is asked with per window;
/// the rest are counted as dropped in the next one's input.
pub const EACH_PER_WINDOW: usize = 16;

/// What a source is asked with as `input.event`.
pub(super) fn event_value(happened: &CheckoutEvent) -> serde_json::Value {
    match &happened.event {
        EventName::PathsChanged { paths } => serde_json::json!({
            "name": EventName::PATHS_CHANGED,
            "paths": paths,
            "checkout": happened.checkout.display().to_string(),
        }),
        EventName::CheckoutChanged => serde_json::json!({
            "name": EventName::CHECKOUT_CHANGED,
            "checkout": happened.checkout.display().to_string(),
        }),
    }
}

/// The events waiting to be handed to the sources that asked for them.
#[derive(Debug, Default)]
pub(super) struct Delivery {
    /// A source delivered `latest`: the last event that asked it.
    pub(super) latest: BTreeMap<String, serde_json::Value>,
    /// A source delivered `each`: the events not yet handed over, in order.
    pub(super) queued: BTreeMap<String, std::collections::VecDeque<serde_json::Value>>,
    /// How many events each such source missed, said in its next input.
    pub(super) dropped: BTreeMap<String, u64>,
}

impl Delivery {
    /// Keeps `event` for `source`, delivered as `each` says.
    pub(super) fn keep(&mut self, source: &str, event: serde_json::Value, each: bool) {
        if !each {
            self.latest.insert(source.to_owned(), event);
            return;
        }
        let queue = self.queued.entry(source.to_owned()).or_default();
        if queue.len() < EACH_PER_WINDOW {
            queue.push_back(event);
        } else {
            *self.dropped.entry(source.to_owned()).or_default() += 1;
        }
    }

    /// The event `source` is asked with next, its dropped count said.
    pub(super) fn take(&mut self, source: &str) -> Option<serde_json::Value> {
        let mut event = match self
            .queued
            .get_mut(source)
            .and_then(|queue| queue.pop_front())
        {
            Some(event) => event,
            None => self.latest.remove(source)?,
        };
        if let Some(dropped) = self.dropped.remove(source)
            && let serde_json::Value::Object(fields) = &mut event
        {
            fields.insert("dropped".to_owned(), serde_json::Value::from(dropped));
        }
        Some(event)
    }

    /// Whether `source` still has events to be asked with one by one.
    pub(super) fn waiting(&self, source: &str) -> bool {
        self.queued
            .get(source)
            .is_some_and(|queue| !queue.is_empty())
    }
}

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

    /// Whether `source` is gathered in the window still open.
    pub(super) fn holds(&self, source: &str) -> bool {
        self.sources.contains(source)
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
