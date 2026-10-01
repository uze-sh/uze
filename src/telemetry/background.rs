//! A pass nobody asked for is written only when it changed something,
//! failed, or ran slow.
//!
//! A timer's root span is `debug`, but the application services it calls
//! open `info` spans, because the same methods answer a person's gesture,
//! and `tracing` judges each span by its own level. So every refresh wrote
//! its services into the journal anyway, hung on the nearest span that was
//! shown: seven reads at least every 20 s, all day, were 95% of a 12 MB day
//! and said nothing a reader could act on. Lowering the services would
//! have lost the other half, a gesture's own breakdown.
//!
//! A pass is marked where it starts, by [`background_pass!`], rather than
//! inferred from the levels above it: a `debug` span also wraps every key
//! the client handles, and a gesture under it is exactly what must stay.
//! While a pass whose root no output shows is entered on a thread, no span
//! opened on that thread is shown either. Events keep their own level, so
//! a pass that changed something says so at `info`, and a failure is a
//! failure whoever asked. Whatever shows the root (`UZE_LOG=debug`, or a
//! directive naming its module) shows the whole pass.
//!
//! [`background_pass!`]: crate::telemetry::background_pass

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

/// A refresh the operator never asked for has this long before it is
/// worth a line. Above every pass's steady state, measured: the slowest,
/// the task evaluation, settles under half a second on a busy repository.
const SLOW_AFTER: Duration = Duration::from_secs(1);

thread_local! {
    /// How many silenced passes are entered on this thread. A filter is
    /// asked about a new span before it exists, so where it will sit can
    /// only be read from what this thread is inside.
    static SILENCED: Cell<usize> = const { Cell::new(0) };
}

/// Whether a span opened here now belongs to a pass no output shows.
pub(super) fn silenced() -> bool {
    SILENCED.with(Cell::get) > 0
}

/// A timer's pass, entered for as long as this lives. `!Send` through the
/// span it holds, so it is left on the thread that counted it.
#[must_use = "a pass ends when this is dropped"]
pub struct Pass {
    name: &'static str,
    started: Instant,
    slow_after: Duration,
    silenced: bool,
    _span: tracing::span::EnteredSpan,
}

impl Pass {
    #[doc(hidden)]
    pub fn enter(name: &'static str, span: tracing::Span) -> Self {
        Self::reported_after(SLOW_AFTER, name, span)
    }

    fn reported_after(slow_after: Duration, name: &'static str, span: tracing::Span) -> Self {
        let silenced = span.is_disabled();
        if silenced {
            SILENCED.with(|depth| depth.set(depth.get() + 1));
        }
        Self {
            name,
            started: Instant::now(),
            slow_after,
            silenced,
            _span: span.entered(),
        }
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        if !self.silenced {
            return;
        }
        SILENCED.with(|depth| depth.set(depth.get() - 1));
        // A routine pass that started costing seconds is the one fact
        // about it somebody should act on. A shown pass already says what
        // it cost on its own close line.
        let took = self.started.elapsed();
        if took >= self.slow_after {
            tracing::warn!(pass = self.name, ?took, "a background pass ran slow");
        }
    }
}

/// Enters a timer's pass: `let _pass = background_pass!("tui.git_read");`.
/// Its root span is `debug`, and nothing under it is written unless that
/// root is.
macro_rules! background_pass {
    ($name:literal) => {
        $crate::telemetry::background::Pass::enter($name, ::tracing::debug_span!($name))
    };
}
pub(crate) use background_pass;

#[cfg(test)]
mod tests {
    use tracing_subscriber::{
        EnvFilter, Layer,
        filter::{FilterExt, dynamic_filter_fn},
        layer::SubscriberExt,
    };

    use super::*;

    /// What a journal at `directives` holds after `body`, through the
    /// filter `init` composes, beside the steps layer it always carries.
    fn journal_of(directives: &str, body: impl FnOnce()) -> String {
        let scratch = uze_testkit::temp::scratch("telemetry-background");
        let appender = tracing_appender::rolling::never(&scratch, "probe.log");
        let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
            .lossy(false)
            .finish(appender);
        let text = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_writer(writer)
            .with_filter(
                EnvFilter::new(directives).and(dynamic_filter_fn(|metadata, _| {
                    !metadata.is_span() || !silenced()
                })),
            );
        let subscriber = tracing_subscriber::registry()
            .with(crate::steps::layer())
            .with(text);
        tracing::subscriber::with_default(subscriber, body);
        drop(guard);
        let written = std::fs::read_to_string(scratch.join("probe.log")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(scratch);
        written
    }

    fn evaluation() {
        let _pass = background_pass!("tui.task_evaluation");
        let _service = tracing::info_span!("workspace.evaluate_tasks").entered();
        let _read = tracing::info_span!("workspace.primary_of").entered();
    }

    /// The incident this exists for: a timer's `debug` pass calling a
    /// service whose span is `info` wrote the service into the journal,
    /// every few seconds, all day.
    #[test]
    fn nothing_under_a_silenced_pass_is_written() {
        let written = journal_of("info", evaluation);
        assert!(!written.contains("workspace."), "{written}");
    }

    /// The other half: a gesture keeps its whole tree, even under the
    /// `debug` span the client opens around every event it handles.
    #[test]
    fn a_gesture_under_a_debug_span_is_written_with_its_children() {
        let written = journal_of("info", || {
            let _event = tracing::debug_span!("tui.event").entered();
            let _gesture = tracing::info_span!("tui.gesture").entered();
            let _service = tracing::info_span!("plugins.list").entered();
        });
        assert!(written.contains("tui.gesture:plugins.list"), "{written}");
    }

    /// Whether a span is shown depends on where it is opened, so its
    /// callsite's answer can never be cached: the same service first under
    /// a gesture, then under a timer, is written once.
    #[test]
    fn the_same_service_is_judged_where_it_is_opened() {
        fn service() {
            let _span = tracing::info_span!("workspace.primary_of").entered();
        }
        let written = journal_of("info", || {
            {
                let _gesture = tracing::info_span!("tui.gesture").entered();
                service();
            }
            let _pass = background_pass!("tui.task_evaluation");
            service();
        });
        assert_eq!(
            written.matches("workspace.primary_of").count(),
            1,
            "{written}"
        );
    }

    /// A pass that changed something says so: an event keeps its own
    /// level whoever asked.
    #[test]
    fn an_event_in_a_silenced_pass_keeps_its_own_level() {
        let written = journal_of("info", || {
            let _pass = background_pass!("tui.task_evaluation");
            let _service = tracing::info_span!("workspace.evaluate_tasks").entered();
            tracing::info!(agent = "a1", "an agent's work changed");
        });
        assert!(
            written.contains("an agent's work changed agent=\"a1\""),
            "{written}"
        );
        assert!(!written.contains("workspace.evaluate_tasks"), "{written}");
    }

    /// Raising one module to `debug` brings that module back, not every
    /// timer: the pass is silenced by whether its own root is shown.
    #[test]
    fn a_directive_for_another_module_leaves_passes_silenced() {
        let written = journal_of("info,uze_git=debug", evaluation);
        assert!(!written.contains("workspace."), "{written}");
    }

    /// Whatever shows the root shows the pass.
    #[test]
    fn a_shown_root_shows_its_whole_pass() {
        let written = journal_of("debug", evaluation);
        assert!(
            written.contains("tui.task_evaluation:workspace.evaluate_tasks:workspace.primary_of"),
            "{written}"
        );
    }

    /// A pass spawned onto a worker the way the client spawns them: the
    /// thread enters its parent, then the pass.
    #[test]
    fn a_pass_on_a_worker_thread_is_silenced_there() {
        let written = journal_of("info", || {
            let dispatch = tracing::dispatcher::get_default(Clone::clone);
            let session = tracing::info_span!("tui.session");
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    tracing::dispatcher::with_default(&dispatch, || {
                        let _parent = session.enter();
                        evaluation();
                    });
                });
            });
        });
        assert!(written.contains("tui.session"), "{written}");
        assert!(!written.contains("workspace."), "{written}");
    }

    /// A routine pass that stops being cheap is the one line it earns.
    #[test]
    fn a_silenced_pass_that_ran_slow_is_reported_once() {
        let written = journal_of("info", || {
            let _pass = Pass::reported_after(
                Duration::ZERO,
                "tui.task_evaluation",
                tracing::debug_span!("tui.task_evaluation"),
            );
            let _service = tracing::info_span!("workspace.evaluate_tasks").entered();
        });
        assert_eq!(written.matches("ran slow").count(), 1, "{written}");
        assert!(
            written.contains("pass=\"tui.task_evaluation\""),
            "{written}"
        );
    }

    /// A shown pass already says what it cost on its own close line.
    #[test]
    fn a_shown_pass_is_never_reported_slow() {
        let written = journal_of("debug", || {
            let _pass = Pass::reported_after(
                Duration::ZERO,
                "tui.task_evaluation",
                tracing::debug_span!("tui.task_evaluation"),
            );
        });
        assert!(written.contains("tui.task_evaluation"), "{written}");
        assert!(!written.contains("ran slow"), "{written}");
    }
}
