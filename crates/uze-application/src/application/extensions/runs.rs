//! What the runner did for each extension this session, kept for the
//! inspector and `uze doctor` (`add-extension-platform` D25, D28).
//!
//! Every question the runner answers is noted with what it was asked, how
//! long it took and how it ended; a run is one that executed a script or a
//! program, which is what runs per minute and the p95 count. What is kept
//! is bounded and in memory: the session's account, never a record.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use uze_contract::{Answer, Asking, Identity};

/// The most runs kept per extension for the inspector to list.
pub const RUNS_KEPT: usize = 64;

/// How far back runs per minute and the p95 look.
pub const COST_WINDOW: Duration = Duration::from_secs(5 * 60);

/// The longest an input is shown, in characters, before it is cut.
const INPUT_SHOWN: usize = 160;

/// D28's targets: an answer at p95, and what one loaded extension holds.
pub const P95_TARGET: Duration = Duration::from_millis(20);
pub const MEMORY_TARGET: usize = 1024 * 1024;

/// How a question ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunOutcome {
    /// A script or a program ran and answered.
    Answered,
    /// What an earlier run answered stood for it: nothing ran.
    Remembered,
    /// A fixture answered: nothing ran.
    Fixture,
    Failed(String),
    /// Nothing ran, and this is why.
    Withheld(String),
}

/// One question the runner answered.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionRun {
    /// The source or action asked.
    pub declaration: String,
    pub asking: Asking,
    /// The input it was asked with, as compact JSON, cut to a line.
    pub input: String,
    /// The first characters of the input's digest, telling two inputs
    /// apart where their text was cut.
    pub digest: String,
    pub took: Duration,
    pub outcome: RunOutcome,
    /// Whether a script or a program executed for it.
    pub ran: bool,
    pub at: Instant,
}

/// What an extension costs this session.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExtensionCost {
    /// Runs over the last [`COST_WINDOW`], per minute.
    pub runs_per_minute: f64,
    /// The 95th percentile of those runs' durations; `None` before one.
    pub p95: Option<Duration>,
    /// What the runner holds of its answers, in bytes.
    pub remembered_bytes: usize,
}

impl ExtensionCost {
    /// Whether its p95 is over D28's target.
    pub fn slow(&self) -> bool {
        self.p95.is_some_and(|p95| p95 > P95_TARGET)
    }
}

/// The account of one extension.
#[derive(Debug, Default)]
pub(super) struct RunLog {
    recent: VecDeque<ExtensionRun>,
    /// When each run in the window happened and how long it took.
    timed: VecDeque<(Instant, Duration)>,
}

/// Every extension's account.
#[derive(Debug, Default)]
pub(super) struct RunBook(HashMap<Identity, RunLog>);

impl RunBook {
    pub(super) fn note(&mut self, identity: &Identity, run: ExtensionRun) {
        let log = self.0.entry(identity.clone()).or_default();
        if run.ran {
            log.timed.push_back((run.at, run.took));
            while log
                .timed
                .front()
                .is_some_and(|(at, _)| run.at.saturating_duration_since(*at) > COST_WINDOW)
            {
                log.timed.pop_front();
            }
        }
        log.recent.push_front(run);
        log.recent.truncate(RUNS_KEPT);
    }

    /// Its runs, newest first.
    pub(super) fn runs(&self, identity: &Identity) -> Vec<ExtensionRun> {
        self.0
            .get(identity)
            .map(|log| log.recent.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Runs per minute and p95 at `now`, over the window.
    pub(super) fn cost(&self, identity: &Identity, now: Instant) -> ExtensionCost {
        let Some(log) = self.0.get(identity) else {
            return ExtensionCost::default();
        };
        let mut durations: Vec<Duration> = log
            .timed
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) <= COST_WINDOW)
            .map(|(_, took)| *took)
            .collect();
        let Some(first) = log
            .timed
            .iter()
            .find(|(at, _)| now.saturating_duration_since(*at) <= COST_WINDOW)
            .map(|(at, _)| *at)
        else {
            return ExtensionCost::default();
        };
        // A burst a second old is not sixty a minute: the rate is over at
        // least a minute, or the window once it has filled.
        let span = now
            .saturating_duration_since(first)
            .clamp(Duration::from_secs(60), COST_WINDOW);
        durations.sort();
        ExtensionCost {
            runs_per_minute: durations.len() as f64 * 60.0 / span.as_secs_f64(),
            p95: percentile(&durations, 95),
            remembered_bytes: 0,
        }
    }
}

/// The `pct`th percentile of `sorted`, by nearest rank.
pub(super) fn percentile(sorted: &[Duration], pct: usize) -> Option<Duration> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (sorted.len() * pct).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// What the account says of `answer`, given whether anything ran for it
/// and whether a fixture gave it.
pub(super) fn outcome_of(answer: &Answer, ran: bool, fixture: bool) -> RunOutcome {
    match answer {
        Answer::Answered { .. } if fixture => RunOutcome::Fixture,
        Answer::Answered { .. } if ran => RunOutcome::Answered,
        Answer::Answered { .. } => RunOutcome::Remembered,
        Answer::Failed { reason, .. } => RunOutcome::Failed(reason.clone()),
        Answer::Withheld { reason } => RunOutcome::Withheld(reason.clone()),
    }
}

/// `input` as one short line: compact JSON, cut where it runs long.
pub(super) fn input_line(input: &serde_json::Value) -> String {
    let text = serde_json::to_string(input).unwrap_or_default();
    match text.char_indices().nth(INPUT_SHOWN) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        Identity {
            marketplace: "studio".to_owned(),
            plugin: "gsd".to_owned(),
            id: "gsd-band".to_owned(),
        }
    }

    fn run(at: Instant, took_ms: u64, ran: bool) -> ExtensionRun {
        ExtensionRun {
            declaration: "state".to_owned(),
            asking: Asking::Source,
            input: "{}".to_owned(),
            digest: "abc".to_owned(),
            took: Duration::from_millis(took_ms),
            outcome: if ran {
                RunOutcome::Answered
            } else {
                RunOutcome::Remembered
            },
            ran,
            at,
        }
    }

    #[test]
    fn the_p95_is_the_nearest_rank() {
        let sorted: Vec<Duration> = (1..=20).map(Duration::from_millis).collect();
        assert_eq!(percentile(&sorted, 95), Some(Duration::from_millis(19)));
        assert_eq!(percentile(&sorted[..1], 95), Some(Duration::from_millis(1)));
        assert_eq!(percentile(&[], 95), None);
    }

    #[test]
    fn only_what_ran_is_counted_and_a_burst_is_a_rate_over_a_minute() {
        let mut book = RunBook::default();
        let start = Instant::now();
        for i in 0..10 {
            book.note(&identity(), run(start, 2 + i, true));
        }
        book.note(&identity(), run(start, 900, false));
        let cost = book.cost(&identity(), start + Duration::from_secs(1));
        assert!((cost.runs_per_minute - 10.0).abs() < 0.01, "{cost:?}");
        assert_eq!(cost.p95, Some(Duration::from_millis(11)));
        assert!(!cost.slow());
        assert_eq!(book.runs(&identity()).len(), 11, "every question is listed");
        assert!(!book.runs(&identity())[0].ran, "newest first");
    }

    #[test]
    fn runs_out_of_the_window_cost_nothing_and_the_list_is_bounded() {
        let mut book = RunBook::default();
        let start = Instant::now();
        for _ in 0..(RUNS_KEPT + 5) {
            book.note(&identity(), run(start, 30, true));
        }
        assert_eq!(book.runs(&identity()).len(), RUNS_KEPT);
        assert!(book.cost(&identity(), start).slow());
        let later = book.cost(&identity(), start + COST_WINDOW + Duration::from_secs(1));
        assert_eq!(later, ExtensionCost::default());
    }

    #[test]
    fn a_long_input_is_cut_to_a_line() {
        let long = serde_json::json!({ "text": "x".repeat(400) });
        let line = input_line(&long);
        assert!(line.ends_with('…'));
        assert_eq!(line.chars().count(), INPUT_SHOWN + 1);
        assert_eq!(input_line(&serde_json::json!({"a": 1})), r#"{"a":1}"#);
    }
}
