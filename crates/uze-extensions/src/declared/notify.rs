//! What an answer asks to tell the operator outside the frame: a
//! notification, a cue, or both, under `notify` (S6).
//!
//! The engine lets one through only for an extension whose manifest holds
//! the grant, and only within its rate: one every [`INTERVAL`] and
//! [`PER_HOUR`] an hour. What it holds back is counted and said in the
//! notice of the surfaces the answer was about, so a burst is visible
//! where it came from rather than lost.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use uze_contract::{Clean, Envelope, Grant, Notification};

use super::DeclaredExtension;
use crate::contract::Notify;
use crate::view::{Role, Span};

/// The shortest time between two notifications of one extension.
pub const INTERVAL: Duration = Duration::from_secs(10);
/// The most notifications one extension sends in an hour.
pub const PER_HOUR: usize = 30;
const HOUR: Duration = Duration::from_secs(3600);

/// What the notice says of a request made without the grant.
pub(super) const UNGRANTED: &str = "not notified: needs the notify grant";

/// One extension's rate: when it last sent, within the hour, and how many
/// it was refused since it last got one through.
#[derive(Debug, Default)]
pub(super) struct Rate {
    sent: VecDeque<Instant>,
    dropped: usize,
}

impl Rate {
    /// Whether one more may go out at `now`, recording it when it may. No
    /// clock yet means nothing can be counted, so nothing goes.
    pub(super) fn admit(&mut self, now: Option<Instant>) -> bool {
        let Some(now) = now else {
            self.dropped += 1;
            return false;
        };
        while self
            .sent
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= HOUR)
        {
            self.sent.pop_front();
        }
        let rested = self
            .sent
            .back()
            .is_none_or(|last| now.saturating_duration_since(*last) >= INTERVAL);
        if rested && self.sent.len() < PER_HOUR {
            self.sent.push_back(now);
            self.dropped = 0;
            true
        } else {
            self.dropped += 1;
            false
        }
    }

    /// How many were refused since the last one went out.
    pub(super) fn dropped(&self) -> usize {
        self.dropped
    }
}

/// What the notice says once `dropped` were held back.
pub(super) fn dropped_notice(dropped: usize) -> String {
    let noun = if dropped == 1 {
        "notification"
    } else {
        "notifications"
    };
    format!("{dropped} {noun} dropped: at most 1 every 10 s and 30 an hour")
}

impl DeclaredExtension {
    /// The envelope's `notify` and `sound`, let through or said in the
    /// notice of `surfaces`, after what the answer itself said there.
    pub(super) fn ask_to_notify(&mut self, envelope: &Envelope, surfaces: &[String]) {
        if envelope.notify.is_none() && envelope.sound.is_none() {
            return;
        }
        let said = if !self.compiled.manifest.wants(&Grant::Notify) {
            UNGRANTED.to_owned()
        } else if self.rate.admit(self.now) {
            self.outgoing.push(Notify {
                notification: envelope.notify.as_deref().map(|asked| Notification {
                    title: Clean::label(&asked.title).into_string(),
                    body: Clean::label(&asked.body).into_string(),
                }),
                sound: envelope.sound,
            });
            return;
        } else {
            dropped_notice(self.rate.dropped())
        };
        for surface in surfaces {
            let text = match self.notices.get(surface) {
                Some(own) if !own.text.is_empty() => format!("{} · {said}", own.text),
                _ => said.clone(),
            };
            self.notices
                .insert(surface.clone(), Span::new(text, Role::Warning));
        }
    }

    /// What was let through since the host last asked.
    pub(super) fn take_outgoing(&mut self) -> Vec<Notify> {
        std::mem::take(&mut self.outgoing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_goes_out_every_ten_seconds_and_the_rest_are_counted() {
        let start = Instant::now();
        let mut rate = Rate::default();
        assert!(rate.admit(Some(start)));
        for second in 1..5 {
            assert!(!rate.admit(Some(start + Duration::from_secs(second))));
        }
        assert_eq!(rate.dropped(), 4);
        assert!(rate.admit(Some(start + INTERVAL)));
        assert_eq!(rate.dropped(), 0, "the count restarts once one goes out");
    }

    #[test]
    fn thirty_an_hour_and_then_none_until_the_first_is_an_hour_old() {
        let start = Instant::now();
        let mut rate = Rate::default();
        for sent in 0..PER_HOUR {
            assert!(rate.admit(Some(start + INTERVAL * sent as u32)));
        }
        let rested = start + INTERVAL * PER_HOUR as u32;
        assert!(!rate.admit(Some(rested)), "the thirty-first in the hour");
        assert!(rate.admit(Some(start + HOUR)), "the first left the hour");
    }

    #[test]
    fn without_a_clock_nothing_goes_out() {
        let mut rate = Rate::default();
        assert!(!rate.admit(None));
        assert_eq!(rate.dropped(), 1);
    }

    use crate::contract::{Extension, Pass};
    use crate::declared::source_tests::{Answering, checkout};
    use crate::declared::tests::{VIEW, compiled_view, extension, n};
    use crate::view::Size;
    use uze_contract::{Answer, NodeName, Sound};

    const SPACE: Size = Size {
        width: 60,
        height: 20,
    };

    /// An engine showing its source on an open view, wanting `wants`.
    fn showing(wants: Vec<Grant>) -> DeclaredExtension {
        let view = compiled_view(
            vec![n(NodeName::Text).arg("{count}").item()],
            Vec::new(),
            Vec::new(),
        );
        let mut compiled = extension(view);
        compiled.manifest.wants = wants;
        let mut engine = DeclaredExtension::new(compiled, "en");
        engine.focus(Some(&checkout("/work/a")));
        engine.open(VIEW, &checkout("/work/a"));
        engine
    }

    /// The engine's source answers `envelope` at `now`.
    fn answers(engine: &mut DeclaredExtension, envelope: Envelope, now: Instant) {
        engine.reconsider();
        engine.pass(&Pass {
            now,
            visible: &[VIEW.to_owned()],
            happened: &[],
        });
        let host = Answering::with(Answer::Answered {
            envelope,
            trace: Vec::new(),
        });
        for job in engine.jobs() {
            engine.absorb((job.run)(&host));
        }
    }

    fn notifying(title: &str) -> Envelope {
        Envelope {
            notify: Some(Box::new(Notification {
                title: title.to_owned(),
                body: "Phase \u{1b}[31m12".to_owned(),
            })),
            sound: Some(Sound::Done),
            ..Envelope::data(serde_json::json!({ "count": 1 }))
        }
    }

    fn notice(engine: &DeclaredExtension) -> Option<String> {
        engine
            .view(VIEW, SPACE)
            .and_then(|view| view.notice)
            .map(|span| span.text)
    }

    #[test]
    fn an_answer_holding_notify_is_handed_over_cleaned_and_once() {
        let mut engine = showing(vec![Grant::Notify]);
        answers(&mut engine, notifying("Done"), Instant::now());
        assert_eq!(
            engine.notifications(),
            [Notify {
                notification: Some(Notification {
                    title: "Done".to_owned(),
                    body: "Phase 12".to_owned(),
                }),
                sound: Some(Sound::Done),
            }]
        );
        assert!(engine.notifications().is_empty(), "taken once");
        assert_eq!(notice(&engine), None);
    }

    #[test]
    fn without_the_grant_nothing_is_handed_over_and_the_notice_names_it() {
        let mut engine = showing(Vec::new());
        answers(&mut engine, notifying("Done"), Instant::now());
        assert!(engine.notifications().is_empty());
        assert_eq!(notice(&engine).as_deref(), Some(UNGRANTED));
        assert_eq!(
            engine.data.get(crate::declared::tests::SOURCE).cloned(),
            Some(Ok(serde_json::json!({ "count": 1 }))),
            "the rest of the answer applies"
        );
    }

    #[test]
    fn a_burst_sends_one_and_the_notice_counts_the_rest() {
        let mut engine = showing(vec![Grant::Notify]);
        let start = Instant::now();
        for tenth in 0..5 {
            answers(
                &mut engine,
                notifying("Done"),
                start + Duration::from_millis(200 * tenth),
            );
        }
        assert_eq!(engine.notifications().len(), 1);
        assert_eq!(notice(&engine), Some(dropped_notice(4)));
    }

    #[test]
    fn the_answers_own_notice_is_kept_before_the_count() {
        let mut engine = showing(vec![Grant::Notify]);
        let start = Instant::now();
        answers(&mut engine, notifying("Done"), start);
        let said = Envelope {
            notice: Some("2 left".to_owned()),
            ..notifying("Done")
        };
        answers(&mut engine, said, start + Duration::from_secs(1));
        assert_eq!(
            notice(&engine),
            Some(format!("2 left · {}", dropped_notice(1)))
        );
    }

    #[test]
    fn the_notice_counts_what_was_dropped() {
        assert_eq!(
            dropped_notice(4),
            "4 notifications dropped: at most 1 every 10 s and 30 an hour"
        );
        assert!(dropped_notice(1).starts_with("1 notification dropped"));
    }
}
