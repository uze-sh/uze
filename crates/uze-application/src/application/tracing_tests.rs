//! What a call to the application looks like as a trace: one tree under
//! the caller's span, named for the service and the method, with the
//! error on the span when there is one.

use std::sync::{Arc, Mutex};

use tracing::{Subscriber, span};
use tracing_subscriber::{Layer, layer::Context, layer::SubscriberExt, registry::LookupSpan};

use super::*;

/// A span as this records it: its name, and its parent's name when it had
/// one.
type Span = (String, Option<String>);

/// Every span opened while the subscriber was current: its name, and its
/// parent's name when it had one.
#[derive(Clone, Default)]
struct Recorded {
    spans: Arc<Mutex<Vec<Span>>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl<S> Layer<S> for Recorded
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, context: Context<'_, S>) {
        let parent = context
            .span(id)
            .and_then(|span| span.parent().map(|parent| parent.name().to_owned()));
        self.spans
            .lock()
            .unwrap()
            .push((attrs.metadata().name().to_owned(), parent));
    }

    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if *event.metadata().level() == tracing::Level::ERROR {
            let mut message = String::new();
            event.record(
                &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                    if field.name() == "error" {
                        message = format!("{value:?}");
                    }
                },
            );
            self.errors.lock().unwrap().push(message);
        }
    }
}

fn recorded<T>(body: impl FnOnce() -> T) -> (T, Recorded) {
    let recorded = Recorded::default();
    let subscriber = tracing_subscriber::registry().with(recorded.clone());
    let result = tracing::subscriber::with_default(subscriber, body);
    (result, recorded)
}

#[test]
fn a_service_call_is_one_span_tree() {
    let root = uze_testkit::temp::scratch("tracing-tree");
    let app = UzeApplication::new(UzeHome::at(&root), Vec::new());
    let ((), recorded) = recorded(|| {
        let action = tracing::info_span!("action");
        let _entered = action.enter();
        let _ = app.machine_snapshot(&root);
    });
    let spans = recorded.spans.lock().unwrap().clone();
    let named = |name: &str| spans.iter().find(|(span, _)| span == name).cloned();
    assert_eq!(
        named("snapshot.machine"),
        Some(("snapshot.machine".to_owned(), Some("action".to_owned()))),
        "the snapshot hangs under the caller's span: {spans:?}"
    );
    for child in [
        "health.report",
        "marketplace.list",
        "marketplace.plugins",
        "context.inspect",
    ] {
        assert_eq!(
            named(child),
            Some((child.to_owned(), Some("snapshot.machine".to_owned()))),
            "{child} is a child of the snapshot: {spans:?}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_failed_service_call_records_its_error_on_the_span() {
    let root = uze_testkit::temp::scratch("tracing-error");
    let app = UzeApplication::new(UzeHome::at(&root), Vec::new());
    let (result, recorded) = recorded(|| app.plugins().inspect("no-such-plugin"));
    assert!(result.is_err());
    let spans = recorded.spans.lock().unwrap().clone();
    assert!(
        spans.iter().any(|(name, _)| name == "plugins.inspect"),
        "the call opened its span: {spans:?}"
    );
    let errors = recorded.errors.lock().unwrap().clone();
    assert!(
        errors
            .iter()
            .any(|message| message.contains("no-such-plugin")),
        "the error was recorded on the span: {errors:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Every value a span or an event carried while the subscriber was
/// current, as the text a layer would write.
#[derive(Clone, Default)]
struct Values(Arc<Mutex<Vec<String>>>);

impl Values {
    fn keep(&self, record: impl FnOnce(&mut dyn tracing::field::Visit)) {
        let mut seen = Vec::new();
        record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                seen.push(format!("{}={value:?}", field.name()));
            },
        );
        self.0.lock().unwrap().extend(seen);
    }
}

impl<S> Layer<S> for Values
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, _id: &span::Id, _context: Context<'_, S>) {
        self.keep(|visit| attrs.record(visit));
    }

    fn on_record(&self, _id: &span::Id, values: &span::Record<'_>, _context: Context<'_, S>) {
        self.keep(|visit| values.record(visit));
    }

    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        self.keep(|visit| event.record(visit));
    }
}

/// ENV-1: the token in a marketplace URL a person typed reached every
/// span that recorded the source, and from there the journal and any
/// collector.
#[test]
fn no_span_or_event_carries_the_credentials_of_a_url_the_operator_typed() {
    let root = uze_testkit::temp::scratch("tracing-credentials");
    let app = UzeApplication::new(UzeHome::at(&root), Vec::new());
    let values = Values::default();
    let subscriber = tracing_subscriber::registry().with(values.clone());
    tracing::subscriber::with_default(subscriber, || {
        let marketplace = app.marketplace();
        let source = "https://alice:ghp_SECRETTOKEN1234567890@127.0.0.1:9/org/market.git";
        assert!(marketplace.add(source).is_err());
        assert!(marketplace.register(source).is_err());
        let spec = "flow@https://alice:ghp_SECRETTOKEN1234567890@127.0.0.1:9/org/market.git";
        assert!(
            marketplace
                .install_plugin(spec, &uze_core::trust::AlwaysTrust)
                .is_err()
        );
        let _ = marketplace.inspect_plugin(source, "flow");
    });
    let values = values.0.lock().unwrap().clone();
    assert!(
        values
            .iter()
            .any(|value| value.contains("https://<redacted>@127.0.0.1:9/org/market.git")),
        "the source is still on the span, less its credentials: {values:?}"
    );
    let leaked: Vec<_> = values
        .iter()
        .filter(|value| value.contains("SECRETTOKEN") || value.contains("alice"))
        .collect();
    assert!(leaked.is_empty(), "credentials reached a trace: {leaked:?}");
    let _ = fs::remove_dir_all(&root);
}
