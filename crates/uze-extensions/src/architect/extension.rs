//! The architect surface as the host hosts every extension: one full-frame
//! surface, open on one checkout at a time, whose artifacts are read by a
//! job.

use std::path::PathBuf;

use super::{
    ArchitectOutcome, ArchitectPlace, ArchitectView, Artifacts, ArtifactsAnswer, CATALOG, drag_by,
    handle_command, handle_hover, handle_mouse, handle_scroll, pan, read_artifacts, scroll_to,
    text, view,
};
use crate::contract::{Checkout, Extension, Job, Opaque, Outcome, SurfaceInfo, SurfaceSlot};
use crate::view::{Command, Layout, ScrollDirection, ScrollTarget, Section, Size, View, ViewHit};

/// The one surface's id.
pub const SURFACE: &str = "board";

/// The checkout the surface is open on, and whether its read was asked.
struct Open {
    root: PathBuf,
    view: ArchitectView,
    asked: bool,
}

/// What a read of the artifacts answered, with the checkout it was asked
/// about: one landing after the viewer opened another is dropped.
pub struct ArtifactsRead {
    pub root: PathBuf,
    pub answer: ArtifactsAnswer,
}

#[derive(Default)]
pub struct ArchitectExtension {
    open: Option<Open>,
}

impl ArchitectExtension {
    fn view_mut(&mut self, surface: &str) -> Option<&mut ArchitectView> {
        (surface == SURFACE)
            .then_some(self.open.as_mut())
            .flatten()
            .map(|open| &mut open.view)
    }

    fn view_ref(&self, surface: &str) -> Option<&ArchitectView> {
        (surface == SURFACE)
            .then_some(self.open.as_ref())
            .flatten()
            .map(|open| &open.view)
    }
}

fn outcome(outcome: ArchitectOutcome) -> Outcome {
    match outcome {
        ArchitectOutcome::Stay => Outcome::Stay,
        ArchitectOutcome::Close => Outcome::Close,
        ArchitectOutcome::OpenPath { project, target } => Outcome::OpenPath { project, target },
    }
}

impl Extension for ArchitectExtension {
    fn identity(&self) -> String {
        CATALOG.id.to_owned()
    }

    fn surfaces(&self) -> Vec<SurfaceInfo> {
        vec![SurfaceInfo {
            id: SURFACE.to_owned(),
            slot: SurfaceSlot::View,
            title: CATALOG.name.to_owned(),
        }]
    }

    fn view(&self, surface: &str, space: Size) -> Option<View> {
        self.view_ref(surface).map(|state| view(state, space))
    }

    fn section(&self, _surface: &str, _collapsed: bool) -> Option<Section> {
        None
    }

    fn command(&mut self, surface: &str, command: Command, space: Size) -> Outcome {
        self.view_mut(surface).map_or(Outcome::Stay, |state| {
            outcome(handle_command(state, command, space))
        })
    }

    fn hit(&mut self, surface: &str, hit: ViewHit, space: Size) -> Outcome {
        self.view_mut(surface).map_or(Outcome::Stay, |state| {
            outcome(handle_mouse(state, Some(hit), space))
        })
    }

    fn scroll(
        &mut self,
        surface: &str,
        target: ScrollTarget,
        direction: ScrollDirection,
        space: Size,
    ) {
        if target == ScrollTarget::Content
            && let Some(state) = self.view_mut(surface)
        {
            handle_scroll(state, direction, space);
        }
    }

    fn missed(&mut self, surface: &str, space: Size) {
        if let Some(state) = self.view_mut(surface) {
            handle_mouse(state, None, space);
        }
    }

    fn open(&mut self, surface: &str, checkout: &Checkout) {
        if surface == SURFACE {
            self.open = Some(Open {
                root: checkout.root.clone(),
                view: ArchitectView::opening(checkout.display.clone()),
                asked: false,
            });
        }
    }

    /// The drawings and their layouts are large, and reopening reads the
    /// artifacts again anyway.
    fn close(&mut self, surface: &str) {
        if surface == SURFACE {
            self.open = None;
        }
    }

    fn layout(&self, _surface: &str) -> Layout {
        Layout::Board
    }

    fn hover(&mut self, surface: &str, hit: Option<ViewHit>) -> bool {
        self.view_mut(surface)
            .is_some_and(|state| handle_hover(state, hit))
    }

    fn drag(&mut self, surface: &str, columns: i32, rows: i32, space: Size) {
        if let Some(state) = self.view_mut(surface) {
            drag_by(state, columns, rows, space);
        }
    }

    fn pan(&mut self, surface: &str, columns: i32, space: Size) {
        if let Some(state) = self.view_mut(surface) {
            pan(state, columns, space);
        }
    }

    fn scroll_to(&mut self, surface: &str, first: usize, space: Size) {
        if let Some(state) = self.view_mut(surface) {
            scroll_to(state, first, space);
        }
    }

    fn text(&self, surface: &str, lines: std::ops::Range<usize>) -> Vec<String> {
        self.view_ref(surface)
            .map(|state| text(state, lines))
            .unwrap_or_default()
    }

    /// The artifacts' read, once per opening: a directory walk, a read per
    /// file and a call to Git, none of it the drawing thread's.
    fn jobs(&mut self) -> Vec<Job> {
        let Some(open) = self.open.as_mut().filter(|open| !open.asked) else {
            return Vec::new();
        };
        open.asked = true;
        let root = open.root.clone();
        let fallback_root = root.clone();
        vec![Job {
            run: Box::new(move |host| {
                let source = host.artifact_source(&root);
                let answer = read_artifacts(host, &root, source);
                Opaque::new(ArtifactsRead { root, answer })
            }),
            fallback: Box::new(move || {
                Opaque::new(ArtifactsRead {
                    root: fallback_root,
                    answer: ArtifactsAnswer {
                        branch: String::new(),
                        artifacts: Artifacts::Nothing {
                            text: "Reading the project's artifacts failed".to_owned(),
                            hint: "Close this and open it again.".to_owned(),
                        },
                    },
                })
            }),
        }]
    }

    fn absorb(&mut self, answer: Opaque) {
        let Some(read) = answer.downcast::<ArtifactsRead>() else {
            return;
        };
        if let Some(open) = self.open.as_mut().filter(|open| open.root == read.root) {
            open.view.absorb(read.answer);
        }
    }

    fn place(&self, surface: &str) -> Option<Opaque> {
        self.view_ref(surface)
            .map(|state| Opaque::new(state.place()))
    }

    fn resume(&mut self, surface: &str, place: Opaque) {
        if let (Some(state), Some(place)) =
            (self.view_mut(surface), place.downcast::<ArchitectPlace>())
        {
            state.resume(place);
        }
    }
}
