//! The spec surface as the host hosts every extension: a document view open
//! on one checkout at a time, and the sidebar's summary of the checkout in
//! front, each read by a job.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{
    CATALOG, Found, SpecAnswer, SpecOutcome, SpecPlace, SpecView, Summary, handle_command,
    handle_mouse, handle_scroll, read_spec, scroll_to, summary, summary_section, text, view,
};
use crate::contract::{Checkout, Entry, Extension, Job, Opaque, Outcome, SurfaceInfo, SurfaceSlot};
use crate::view::{Command, ScrollDirection, ScrollTarget, Section, Size, View, ViewHit};

/// The full-frame surface: a checkout's changes and specs, read as
/// documents.
pub const DOCUMENT: &str = "document";
/// The sidebar's section: the changes in flight in the checkout in front.
pub const SUMMARY: &str = "summary";

/// How long a summary stands before the checkout in front is counted
/// again: tasks get ticked while an agent works, and the section is how
/// that is seen without opening anything.
const SUMMARY_REFRESH: Duration = Duration::from_secs(5);

struct Open {
    root: PathBuf,
    view: SpecView,
    asked: bool,
}

/// What a read of the checkout answered, with the checkout it was asked
/// about: one landing after the viewer opened another is dropped.
pub struct SpecRead {
    pub root: PathBuf,
    pub answer: SpecAnswer,
}

/// What a count of the changes in flight answered, for the checkout it
/// was asked about.
pub struct SummaryRead {
    pub cwd: PathBuf,
    pub summary: Option<Summary>,
}

/// The summary last counted, and when.
struct Counted {
    cwd: PathBuf,
    summary: Option<Summary>,
    at: Instant,
}

#[derive(Default)]
pub struct SpecExtension {
    open: Option<Open>,
    /// Where the next opening goes, sent there from the summary: it wins
    /// over where the viewer last was.
    sent_to: Option<SpecPlace>,
    sent: bool,
    focus: Option<Checkout>,
    counted: Option<Counted>,
    counting: Option<PathBuf>,
    /// The last read, by the checkout it was about: a reopening draws it
    /// at once while the next read is out, since asking Git what a long
    /// branch touched can take a visible moment.
    last_read: Option<SpecRead>,
}

impl SpecExtension {
    fn view_mut(&mut self, surface: &str) -> Option<&mut SpecView> {
        (surface == DOCUMENT)
            .then_some(self.open.as_mut())
            .flatten()
            .map(|open| &mut open.view)
    }

    fn view_ref(&self, surface: &str) -> Option<&SpecView> {
        (surface == DOCUMENT)
            .then_some(self.open.as_ref())
            .flatten()
            .map(|open| &open.view)
    }

    fn summary(&self) -> Option<&Summary> {
        self.counted.as_ref()?.summary.as_ref()
    }

    fn read_job(open: &Open) -> Job {
        let root = open.root.clone();
        let fallback_root = root.clone();
        Job {
            run: Box::new(move |host| {
                let target = host.delivery_target(&root);
                let places = host.artifact_source(&root);
                let answer = read_spec(host, &root, target.as_deref(), &places);
                Opaque::new(SpecRead { root, answer })
            }),
            fallback: Box::new(move || {
                Opaque::new(SpecRead {
                    answer: SpecAnswer {
                        root: fallback_root.clone(),
                        branch: String::new(),
                        theme: String::new(),
                        found: Found::NoLayout,
                        subjects: Vec::new(),
                    },
                    root: fallback_root,
                })
            }),
        }
    }

    fn count_job(focus: &Checkout) -> Job {
        let cwd = focus.root.clone();
        let fallback_cwd = cwd.clone();
        let target = focus.target.clone();
        Job {
            run: Box::new(move |host| {
                let summary = summary(host, &cwd, target.as_deref());
                Opaque::new(SummaryRead { cwd, summary })
            }),
            fallback: Box::new(move || {
                Opaque::new(SummaryRead {
                    cwd: fallback_cwd,
                    summary: None,
                })
            }),
        }
    }

    /// Whether the summary held is about the checkout in front and young
    /// enough to stand.
    fn counted_fresh(&self, cwd: &PathBuf) -> bool {
        self.counted
            .as_ref()
            .is_some_and(|counted| &counted.cwd == cwd && counted.at.elapsed() < SUMMARY_REFRESH)
    }
}

fn outcome(outcome: SpecOutcome) -> Outcome {
    match outcome {
        SpecOutcome::Stay => Outcome::Stay,
        SpecOutcome::Close => Outcome::Close,
        SpecOutcome::OpenPath { project, target } => Outcome::OpenPath { project, target },
    }
}

impl Extension for SpecExtension {
    fn identity(&self) -> String {
        CATALOG.id.to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        vec![
            SurfaceInfo {
                opens: None,
                id: DOCUMENT.to_owned(),
                slot: SurfaceSlot::View,
                title: CATALOG.name.to_owned(),
            },
            SurfaceInfo {
                opens: None,
                id: SUMMARY.to_owned(),
                slot: SurfaceSlot::Section,
                title: "tasks".to_owned(),
            },
        ]
    }

    fn view(&self, surface: &str, space: Size) -> Option<View> {
        self.view_ref(surface).map(|state| view(state, space))
    }

    /// The changes in flight, the one the document view is on lit.
    fn section(&self, surface: &str, collapsed: bool) -> Option<Section> {
        if surface != SUMMARY {
            return None;
        }
        let summary = self.summary()?;
        let mut section = summary_section(summary, collapsed, 0);
        let on_show = self.view_ref(DOCUMENT).and_then(SpecView::change_on_show);
        section.lit = on_show
            .and_then(|name| {
                summary
                    .changes
                    .iter()
                    .position(|change| change.name == name)
            })
            .into_iter()
            .collect();
        Some(section)
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        self.view_mut(surface).map_or(Outcome::Stay, |state| {
            outcome(handle_command(state, command, space))
        })
    }

    /// On the summary, a change sends the document view to it; the fold
    /// of its header is the host's.
    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome {
        if surface == SUMMARY {
            let ViewHit::SelectItem(index) = hit else {
                return Outcome::Stay;
            };
            let Some(change) = self
                .summary()
                .and_then(|summary| summary.changes.get(index))
            else {
                return Outcome::Stay;
            };
            self.sent_to = Some(SpecPlace::change(&change.name));
            return Outcome::Show(DOCUMENT.to_owned());
        }
        self.view_mut(surface).map_or(Outcome::Stay, |state| {
            outcome(handle_mouse(state, Some(hit), space))
        })
    }

    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        _space: Size,
    ) {
        if target == ScrollTarget::Content
            && let Some(state) = self.view_mut(surface)
        {
            handle_scroll(state, direction);
        }
    }

    fn focus(&mut self, checkout: Option<&Checkout>) {
        self.focus = checkout.cloned();
        if self.focus.is_none() {
            self.counted = None;
        }
    }

    /// A return puts back what stood, never a change the summary pointed
    /// at earlier and nothing opened for.
    fn enter(&mut self, _surface: &str, entry: Entry) -> Outcome {
        if entry == Entry::Return {
            self.sent_to = None;
        }
        Outcome::Stay
    }

    fn open(&mut self, surface: &str, checkout: &Checkout) {
        if surface != DOCUMENT {
            return;
        }
        let mut view = SpecView::opening(checkout.display.clone());
        self.sent = self.sent_to.is_some();
        if let Some(place) = self.sent_to.take() {
            view.resume(place);
        }
        if let Some(last) = self
            .last_read
            .as_ref()
            .filter(|last| last.root == checkout.root)
        {
            view.absorb(last.answer.clone());
        }
        self.open = Some(Open {
            root: checkout.root.clone(),
            view,
            asked: false,
        });
    }

    fn close(&mut self, surface: &str) {
        if surface == DOCUMENT {
            self.open = None;
        }
    }

    fn scroll_to(&mut self, surface: &str, first: usize, _space: Size) {
        if let Some(state) = self.view_mut(surface) {
            scroll_to(state, first);
        }
    }

    fn text(&self, surface: &str, lines: std::ops::Range<usize>) -> Vec<String> {
        self.view_ref(surface)
            .map(|state| text(state, lines))
            .unwrap_or_default()
    }

    /// The document view's read, once per opening; and the summary of the
    /// checkout in front, again once it has stood for [`SUMMARY_REFRESH`]
    /// or the viewer moved to another, one count at a time.
    fn jobs(&mut self) -> Vec<Job> {
        let mut jobs = Vec::new();
        if let Some(open) = self.open.as_mut().filter(|open| !open.asked) {
            open.asked = true;
            jobs.push(Self::read_job(open));
        }
        if self.counting.is_none()
            && let Some(focus) = &self.focus
            && !self.counted_fresh(&focus.root)
        {
            self.counting = Some(focus.root.clone());
            jobs.push(Self::count_job(focus));
        }
        jobs
    }

    fn lanes(&self) -> crate::contract::Lanes {
        crate::contract::Lanes::Parallel
    }

    fn absorb(&mut self, answer: Opaque) -> Outcome {
        let answer = match answer.0.downcast::<SpecRead>() {
            Ok(read) => {
                if let Some(open) = self.open.as_mut().filter(|open| open.root == read.root) {
                    open.view.refresh(read.answer.clone());
                }
                self.last_read = Some(*read);
                return Outcome::Stay;
            }
            Err(other) => Opaque(other),
        };
        let Some(read) = answer.downcast::<SummaryRead>() else {
            return Outcome::Stay;
        };
        if self.counting.as_ref() == Some(&read.cwd) {
            self.counting = None;
        }
        if self.focus.as_ref().map(|focus| &focus.root) == Some(&read.cwd) {
            self.counted = Some(Counted {
                cwd: read.cwd,
                summary: read.summary,
                at: Instant::now(),
            });
        }
        Outcome::Stay
    }

    fn place(&self, surface: &str) -> Option<Opaque> {
        self.view_ref(surface)
            .and_then(SpecView::place)
            .map(Opaque::new)
    }

    /// Where the viewer last was, unless the summary sent them somewhere.
    fn resume(&mut self, surface: &str, place: Opaque) {
        if std::mem::take(&mut self.sent) {
            return;
        }
        if let (Some(state), Some(place)) = (self.view_mut(surface), place.downcast::<SpecPlace>())
        {
            state.resume(place);
        }
    }
}
