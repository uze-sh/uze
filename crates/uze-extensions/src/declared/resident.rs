//! A resident source: a script the host keeps running while a surface
//! showing it is in view (`extension-media-and-resident` M4, M5).
//!
//! The engine holds no script: the session is the host's, reached by the
//! source's id through [`crate::Host::answer`] with
//! [`Asking::Resident`], one call per job, on the extension's serial lane
//! so the calls arrive in the order they were made. What the engine keeps
//! is when to call: a session opens (`Start`) when a surface showing its
//! source comes into view and closes (`Stop`) when none is; a timer's tick
//! and the raw input of its focused surface are queued as `event` and
//! `input`; and `render` is asked only after one of them answered a redraw,
//! at most once per frame the host drew and [`RENDERS_PER_SECOND`].
//!
//! The rendered tree is a composed answer: validated against the catalog
//! when it is built, and drawn as one. Until a render arrives the last
//! frame stays; a call that failed stops the session, and its reason (the
//! bound it crossed) is what the surface says.

use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uze_contract::{Answer, Asking, Question, ResidentCall, Slot, SourceKind};

use super::{DeclaredExtension, STOPPED};
use crate::contract::{Job, Opaque, Outcome, RawInput};

/// The most renders a resident source is asked for in a second.
pub const RENDERS_PER_SECOND: u32 = 30;

/// The raw input a session holds while it answers slower than it is
/// typed at; what arrives past it is dropped.
pub const INPUT_QUEUED: usize = 64;

/// One open session, as the engine sees it.
#[derive(Debug)]
pub(super) struct Session {
    checkout: PathBuf,
    /// The surface it was opened for, whose size a render is asked at.
    surface: String,
    /// Numbers its calls' answers, so one from a closed session is dropped.
    number: u64,
    /// A call is out: the next waits for its answer.
    busy: bool,
    queue: VecDeque<(ResidentCall, Value)>,
    /// A call answered a redraw that no render has answered yet.
    redraw: bool,
    /// When the tick the script asked for is due.
    tick: Option<Instant>,
    /// When the last render was asked, on the host's clock.
    rendered_at: Option<Instant>,
    /// The frames drawn when the last render was asked.
    rendered_frame: Option<u64>,
    /// A call failed or was withheld: nothing more is asked of it.
    stopped: bool,
}

/// A resident call's answer, back from the host.
pub(super) struct ResidentAnswer {
    source: String,
    number: u64,
    call: ResidentCall,
    answer: Answer,
}

impl DeclaredExtension {
    pub(super) fn is_resident(&self, source: &str) -> bool {
        self.compiled
            .manifest
            .source(source)
            .is_some_and(|declared| matches!(declared.kind, SourceKind::Resident(_)))
    }

    /// Whether `surface` reads raw input when focused: a view or an
    /// overlay showing a resident source.
    pub(super) fn reads_raw(&self, surface: &str) -> bool {
        self.compiled.manifest.surface(surface).is_some_and(|decl| {
            matches!(decl.slot, Slot::View | Slot::Overlay) && self.is_resident(&decl.source)
        })
    }

    /// Opens a session for each resident source a surface in `visible`
    /// shows, closes each no surface in view shows, and queues the ticks
    /// that are due.
    pub(super) fn pass_residents(&mut self, visible: &[String], now: Instant) {
        let in_view = self.sources_in_view(visible);
        let shown: Vec<(String, PathBuf, String)> = self
            .shown_sources()
            .into_iter()
            .filter(|(source, ..)| in_view.contains(source) && self.is_resident(source))
            .map(|(source, checkout, _)| {
                let surface = self.surface_in_view(&source, visible);
                (source, checkout, surface)
            })
            .collect();
        let showing: BTreeSet<&String> = shown.iter().map(|(source, ..)| source).collect();
        let gone: Vec<String> = self
            .residents
            .keys()
            .filter(|source| !showing.contains(source))
            .cloned()
            .collect();
        for source in gone {
            self.close_resident(&source);
        }
        for (source, checkout, surface) in shown {
            let current = self
                .residents
                .get(&source)
                .is_some_and(|session| session.checkout == checkout);
            if !current {
                self.close_resident(&source);
                self.open_resident(source, checkout, surface);
            }
        }
        for session in self.residents.values_mut() {
            if !session.stopped && session.tick.is_some_and(|due| due <= now) {
                session.tick = None;
                session
                    .queue
                    .push_back((ResidentCall::Event, json!({ "name": "timer" })));
            }
        }
    }

    /// The first of `visible` showing `source`.
    fn surface_in_view(&self, source: &str, visible: &[String]) -> String {
        self.compiled
            .manifest
            .surfaces
            .iter()
            .find(|decl| decl.source == source && visible.contains(&decl.id))
            .map(|decl| decl.id.clone())
            .unwrap_or_default()
    }

    fn open_resident(&mut self, source: String, checkout: PathBuf, surface: String) {
        // A failure said by the last session is no longer the news; its
        // last frame stands until the new one renders.
        if matches!(self.data.get(&source), Some(Err(_))) {
            self.data.remove(&source);
            self.digests.remove(&source);
        }
        self.questions += 1;
        let space = self.last_space(&surface);
        let slot = self
            .compiled
            .manifest
            .surface(&surface)
            .map(|decl| decl.slot.name());
        let context = json!({
            "slot": slot,
            "locale": self.locale,
            "size": { "width": space.width, "height": space.height },
        });
        self.residents.insert(
            source,
            Session {
                checkout,
                surface,
                number: self.questions,
                busy: false,
                queue: VecDeque::from([(ResidentCall::Start, context)]),
                redraw: false,
                tick: None,
                rendered_at: None,
                // Its first render waits for a frame too, which is what
                // tells it the room it has.
                rendered_frame: Some(self.frames.get()),
                stopped: false,
            },
        );
    }

    /// Closes `source`'s session: its `Stop` goes out with the next jobs.
    fn close_resident(&mut self, source: &str) {
        if let Some(session) = self.residents.remove(source) {
            self.closing
                .push((source.to_owned(), session.checkout, session.number));
        }
    }

    /// Closes every session, to be opened again by the next pass that
    /// finds its surface in view: what answers them may have changed.
    pub(super) fn close_residents(&mut self) {
        let open: Vec<String> = self.residents.keys().cloned().collect();
        for source in open {
            self.close_resident(&source);
        }
    }

    /// The `Stop`s of the sessions closed since, then for each open session
    /// with no call out: its next queued call, or a render when one is
    /// owed, a frame was drawn since the last and the rate allows.
    pub(super) fn resident_jobs(&mut self) -> Vec<Job> {
        let mut jobs: Vec<Job> = std::mem::take(&mut self.closing)
            .into_iter()
            .map(|(source, checkout, number)| {
                resident_job(
                    self,
                    source,
                    checkout,
                    number,
                    ResidentCall::Stop,
                    json!({}),
                )
            })
            .collect();
        let now = self.now;
        let frames = self.frames.get();
        let interval = Duration::from_secs(1) / RENDERS_PER_SECOND;
        let sources: Vec<String> = self.residents.keys().cloned().collect();
        for source in sources {
            let space = match self.residents.get(&source) {
                Some(session) => self.last_space(&session.surface),
                None => continue,
            };
            let Some(session) = self.residents.get_mut(&source) else {
                continue;
            };
            if session.busy || session.stopped {
                continue;
            }
            let (call, input) = match session.queue.pop_front() {
                Some(queued) => queued,
                None => {
                    let drawn = session.rendered_frame.is_none_or(|at| frames > at);
                    let rested = match (session.rendered_at, now) {
                        (Some(at), Some(now)) => now.saturating_duration_since(at) >= interval,
                        _ => true,
                    };
                    if !(session.redraw && drawn && rested) {
                        continue;
                    }
                    session.redraw = false;
                    session.rendered_at = now;
                    session.rendered_frame = Some(frames);
                    (
                        ResidentCall::Render,
                        json!({ "width": space.width, "height": space.height }),
                    )
                }
            };
            session.busy = true;
            let (checkout, number) = (session.checkout.clone(), session.number);
            jobs.push(resident_job(self, source, checkout, number, call, input));
        }
        jobs
    }

    /// A key or the pointer on a focused resident surface, queued as an
    /// `input` for its session.
    pub(super) fn take_raw_input(&mut self, surface: &str, input: RawInput) {
        let Some(source) = self
            .compiled
            .manifest
            .surface(surface)
            .map(|decl| decl.source.clone())
        else {
            return;
        };
        let Some(session) = self.residents.get_mut(&source) else {
            return;
        };
        if session.stopped || session.queue.len() >= INPUT_QUEUED {
            return;
        }
        let input = match input {
            RawInput::Key { key, mods } => json!({ "key": key, "mods": mods }),
            RawInput::Pointer { x, y, button } => json!({ "x": x, "y": y, "button": button }),
        };
        session.queue.push_back((ResidentCall::Input, input));
    }

    /// A resident call's answer. One from a closed session is dropped; a
    /// render's tree is the source's data; a redraw or a tick asked for
    /// is kept for the next jobs; a failure stops the session and is said
    /// on its surfaces.
    pub(super) fn absorb_resident(&mut self, answered: ResidentAnswer) -> Outcome {
        let ResidentAnswer {
            source,
            number,
            call,
            answer,
        } = answered;
        let now = self.now.unwrap_or_else(Instant::now);
        let Some(session) = self
            .residents
            .get_mut(&source)
            .filter(|session| session.number == number)
        else {
            return Outcome::Stay;
        };
        session.busy = false;
        match answer {
            Answer::Answered { envelope, .. } => {
                if let Some(ms) = envelope.timer {
                    session.tick = Some(now + Duration::from_millis(ms));
                }
                if call != ResidentCall::Render && envelope.data == Value::Bool(true) {
                    session.redraw = true;
                }
                if call == ResidentCall::Render {
                    self.set_data(source.clone(), Ok(envelope.data.clone()));
                }
                let surfaces = self.surfaces_showing(&source);
                self.apply(&envelope, &surfaces, Some(&source))
            }
            Answer::Failed { reason, .. } => {
                session.stopped = true;
                session.queue.clear();
                self.set_data(source, Err(reason));
                Outcome::Stay
            }
            Answer::Withheld { reason } => {
                session.stopped = true;
                session.queue.clear();
                self.withhold(source, reason);
                Outcome::Stay
            }
        }
    }

    /// The open sessions' sources: what the inspector and a test read.
    pub fn resident_sessions(&self) -> Vec<String> {
        self.residents
            .iter()
            .filter(|(_, session)| !session.stopped)
            .map(|(source, _)| source.clone())
            .collect()
    }
}

/// The job making `call` into `source`'s session numbered `number`.
fn resident_job(
    engine: &DeclaredExtension,
    source: String,
    checkout: PathBuf,
    number: u64,
    call: ResidentCall,
    input: Value,
) -> Job {
    let question = Question {
        identity: engine.compiled.identity.clone(),
        declaration: source.clone(),
        asking: Asking::Resident(call),
        input,
        checkout: Some(checkout),
    };
    let stopped = source.clone();
    Job {
        run: Box::new(move |host| {
            Opaque::new(ResidentAnswer {
                source,
                number,
                call,
                answer: host.answer(question),
            })
        }),
        fallback: Box::new(move || {
            Opaque::new(ResidentAnswer {
                source: stopped,
                number,
                call,
                answer: Answer::Failed {
                    reason: STOPPED.to_owned(),
                    trace: Vec::new(),
                },
            })
        }),
    }
}
