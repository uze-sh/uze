//! The architect surface: a project's architecture, drawn in cells.
//!
//! Mermaid source (flowcharts, C4, sequences) becomes a diagram a
//! terminal can show *without* a graphics protocol, and owning the layout
//! buys what an image cannot: every box is addressable, so a click
//! selects it and lights what it connects to.
//!
//! Everything here is a file somebody wrote. What a project *is* —
//! where its lines are, which of them are hot — is measured rather than
//! written, and is the code surface's map
//! ([`crate::code`]): it looks like a diagram and is not one, and every
//! tile on it is a path the code surface already answers about.
//!
//! It describes and the host draws, like every surface here — but as a
//! [`Layout::Board`], because a drawing is not a document: it is larger
//! than the screen in both directions, it is moved rather than scrolled,
//! and the list of diagrams is how it is switched rather than something
//! to read beside it. What the board shows is a *screen* cut from the
//! whole drawing: this side decides which part, because it owns the
//! position; the host only draws the cells it is handed.

mod catalog;
mod extension;
mod layout;
mod mermaid;
mod minimap;
mod model;
mod notation;
mod paint;
mod route;
mod sequence;
mod zoom;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    Host,
    registry::BuiltinExtension,
    view::{
        Choosing, Command, Content, ContentLine, Layout, LineTone, MarkerSide, Medium, Mode,
        Navigator, NavigatorRow, PanDirection, Role, RowIcon, ScrollDirection, Size, Span,
        TrailChoice, TrailStep, View, ViewHit,
    },
};

use crate::shared::canvas::{Canvas, Frame, Glyphs};
use crate::shared::checkout;
use crate::shared::nearest;
use crate::shared::places::{ArtifactRoot, ArtifactSource};
use catalog::Catalog;

pub use catalog::Artifact;
pub use extension::{ArchitectExtension, ArtifactsRead, SURFACE};
use minimap::Minimap;
use model::Diagram;
use paint::{Leads, Scene};

pub const CATALOG: BuiltinExtension = BuiltinExtension {
    id: "architect",
    name: "Architect",
    description: "A project's architecture as diagrams drawn in the terminal: Mermaid flowcharts, C4 views and sequences, laid out in cells with no graphics protocol.",
    surface: "Workspace TUI",
    usage: "Alt+A opens it over the workspace, as does its chip in the tab strip; drag the board to move it, click a box to light what it connects to.",
};

const PAN_COLUMNS: i32 = 8;
const PAN_ROWS: i32 = 3;
/// The board's grid: a dot every so many cells, faint enough to read
/// through and regular enough that moving the board is visible even
/// where there is nothing drawn.
const GRID: (i32, i32) = (6, 3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Showing {
    Unicode,
    Ascii,
    Source,
}

const MODES: [(Showing, &str); 3] = [
    (Showing::Unicode, "Unicode"),
    (Showing::Ascii, "ASCII"),
    (Showing::Source, "Source"),
];

enum Drawing {
    Graph(Box<Scene>),
    Sequence(model::Sequence),
    Unreadable(String),
}

pub struct ArchitectView {
    catalog: Catalog,
    /// The checkout this surface is open on and the branch it is at, as
    /// the title says them — the same sentence the code surface's title
    /// is, because a reader comparing the two is asking one question.
    display_root: String,
    branch: String,
    selected: usize,
    showing: Showing,
    /// The board cell at the screen's top-left corner, once the board has
    /// been moved. `None` is *home* — the drawing in the middle of the
    /// screen — which cannot be a number here, because it depends on a
    /// screen size this side only learns when it is asked to draw.
    corner: Option<(i32, i32)>,
    picked: Option<usize>,
    drawing: Drawing,
    /// Which artifact `drawing` was made from, once one has been.
    drawn: Option<usize>,
    /// Drawings already laid out and routed, by artifact, for the ones not
    /// on show. Going back up a level or round the menu is the commonest
    /// thing done here, and laying a large board out again each time is
    /// the most expensive; the artifacts only change when they are read.
    laid_out: HashMap<usize, Drawing>,
    canvas: Option<Canvas>,
    /// Why there is nothing on the board, while there is nothing: the
    /// read still in flight, or what it found instead of artifacts.
    nothing: Option<(String, Option<String>)>,
    /// Which of the menu's two lists is open, and what is highlighted in
    /// it: an area — by the index of its first artifact, which is how an
    /// area is named everywhere here — or an artifact of the area on show.
    choosing: Option<Choosing>,
    /// What a box's link is relative to — where the artifacts were
    /// declared, which is not necessarily the checkout the title names.
    project: PathBuf,
    /// What is the matter with the declared artifacts, kept to be said
    /// beside the code map when that is all there is to show.
    trouble: Option<String>,
    /// Where the viewer was last time, waiting for the artifacts to be
    /// read: a place names diagrams and boxes, and neither exists until
    /// the host's answer lands.
    resuming: Option<ArchitectPlace>,
}

/// Where a viewer was on a checkout's architect surface, for the host to
/// hand back to [`ArchitectView::resuming`].
///
/// Named rather than numbered throughout. The artifacts are files in a
/// directory somebody is working in: the fourth diagram is not the same
/// diagram it was an hour ago, and the one called `crate-layering.mmd`
/// is. A name that no longer resolves is simply not restored — coming
/// back to a diagram that was deleted is the one case where starting at
/// the top is right.
///
/// What it holds is navigation and nothing else: which diagram, the box
/// that was selected and where the board was moved to. The way down to
/// the diagram is not kept: it is the diagram's own ancestors, read again
/// from the artifacts. Not the notation — Unicode, ASCII and Source are
/// how the drawing is being *read*, which is a question the viewer
/// answers again each time they arrive.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArchitectPlace {
    artifact: String,
    corner: Option<(i32, i32)>,
    picked: Option<String>,
}

/// How the project spells the directories it declared, for a sentence.
fn spelled(roots: &[ArtifactRoot]) -> String {
    roots
        .iter()
        .map(|root| format!("`{}`", root.declared))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every Mermaid file under every root. With more than one root an origin
/// carries its root's spelling, so `docs/overview.mmd` and
/// `design/overview.mmd` stay two artifacts rather than one tab.
fn read_roots(host: &dyn Host, roots: &[ArtifactRoot]) -> Result<Vec<Artifact>, (String, String)> {
    let mut artifacts = Vec::new();
    for root in roots {
        let mut found =
            catalog::read(host, &root.path).map_err(|reason| (root.declared.clone(), reason))?;
        for artifact in &mut found {
            artifact.path = root.path.join(&artifact.origin);
            if roots.len() > 1 {
                artifact.origin = format!("{}/{}", root.declared, artifact.origin);
            }
        }
        artifacts.extend(found);
    }
    Ok(artifacts)
}

/// What reading a checkout produced — everything the surface needs to
/// stop saying "reading".
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactsAnswer {
    /// The branch the checkout is at, for the title. Read with the
    /// artifacts rather than asked for separately: it is one read of one
    /// checkout, and a title filling its last part in a moment later
    /// reads as a rendering fault rather than as an answer arriving.
    pub branch: String,
    pub artifacts: Artifacts,
}

/// Whether there is anything to draw, and what to say where there is not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Artifacts {
    Found {
        artifacts: Vec<Artifact>,
        /// What a box's link is relative to.
        project: PathBuf,
    },
    /// Nothing to draw, in two levels: what is the matter, and what to do.
    Nothing { text: String, hint: String },
}

/// Reads a checkout: the artifacts its project declares, and the branch
/// it is at. Unbounded — a directory walk, a read per file and a call to
/// Git — so the host runs it off the thread that draws and hands the
/// answer to [`ArchitectView::absorb`].
pub fn read_artifacts(host: &dyn Host, checkout: &Path, source: ArtifactSource) -> ArtifactsAnswer {
    ArtifactsAnswer {
        branch: checkout::branch_of(host, checkout),
        artifacts: read_the_directory(host, source),
    }
}

/// Said once because the surface and `uze agent artifacts check` both say
/// it, and a project told two different things about the same state has
/// to work out which one to believe.
const UNDECLARED: &str = "No artifacts declared";
/// What to do about it differs, because the two are read by different
/// people: the check by an agent, which declares the directory itself,
/// and the surface by a person, who asks an agent to.
const DECLARE_ARTIFACTS: &str = "Point `workspace.artifacts` in agents.yaml\n\
                                 at a directory of Mermaid files (.mmd).";
const ASK_FOR_ARTIFACTS: &str = "Ask an agent to draw them:\n\n\
                                 `/uze:architect` diagram this project";

/// A declaration the host would not follow. The headline is short because
/// the surface sets it as a title; the reason, which names the key and the
/// file, is a sentence and goes under it.
const UNUSABLE: &str = "agents.yaml needs fixing";
const FIX_AND_REOPEN: &str = "Fix it and open this again.";

/// Why a declared directory gave nothing back. Same reason as above.
fn unreadable(declared: &str, reason: &str) -> (String, String) {
    (
        format!("`{declared}` could not be read"),
        format!("{reason}. It is declared in agents.yaml's `workspace.artifacts`."),
    )
}

fn read_the_directory(host: &dyn Host, source: ArtifactSource) -> Artifacts {
    let nothing = |text: String, hint: &str| Artifacts::Nothing {
        text,
        hint: hint.to_owned(),
    };
    match source {
        ArtifactSource::Undeclared => nothing(UNDECLARED.to_owned(), ASK_FOR_ARTIFACTS),
        ArtifactSource::Refused(reason) => nothing(
            UNUSABLE.to_owned(),
            &format!("{reason}\n\n{FIX_AND_REOPEN}"),
        ),
        ArtifactSource::Directories { roots, project } => match read_roots(host, &roots) {
            Ok(artifacts) if artifacts.is_empty() => nothing(
                format!("{} holds no Mermaid files yet", spelled(&roots)),
                "Add a .mmd file there: a diagram that starts with `C4Context`, \
                 `sequenceDiagram` or `flowchart` is drawn here.",
            ),
            Ok(artifacts) => Artifacts::Found { artifacts, project },
            Err((declared, reason)) => {
                let (text, hint) = unreadable(&declared, &reason);
                nothing(text, &hint)
            }
        },
    }
}

/// Whether a diagram survives being drawn — the only question a check can
/// answer for whoever wrote it. The grammar is not restated anywhere: the
/// parser is asked, and what it says is the answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    Drawn,
    /// Drawn, minus the edges the layout found no path for. Held apart
    /// from [`Verdict::Drawn`] because a diagram missing a relation still
    /// looks finished — nothing on the board says a line was meant to be
    /// there — which is the one failure an author cannot see by looking.
    Unrouted {
        edges: usize,
    },
    /// Not drawn, and why: the sentence the board shows in its place.
    Undrawable(String),
}

/// What checking one artifact found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checked {
    /// The file, as the project would say it.
    pub origin: String,
    /// The name the surface lists it under.
    pub name: String,
    /// The area its first word puts it in.
    pub area: &'static str,
    pub verdict: Verdict,
    /// Every box link that opens nothing: a path that leaves the project, or
    /// that names no file in it (a module's directory, a file that moved).
    /// The code surface opens a link as a file, so the box leads nowhere and
    /// nobody learns of it until they follow it.
    pub broken_links: Vec<String>,
    /// Where a C4 view departs from the model's notation: a box with no
    /// description, a line with no label, a view that zooms into nothing.
    /// Said, not failed — C4 is guidance a project may follow loosely.
    pub notes: Vec<String>,
}

/// What checking a project found: its artifacts, or why it has none to
/// check — the same two states the surface itself opens in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Checkup {
    Checked {
        /// The directories checked, how the project itself spells them.
        declared: String,
        /// Every artifact in it, in the order the menu lists them. Empty
        /// where the directory holds none, which is an answer and not a
        /// fault: a project may declare where its diagrams will go before
        /// it has drawn one.
        artifacts: Vec<Checked>,
    },
    /// Declared, and nothing the host will follow — the one state of the
    /// declaration itself that is somebody's mistake.
    Unusable { text: String, hint: String },
    /// Nothing declared, which is not a mistake.
    Nothing { text: String, hint: String },
}

/// Reads a project's artifacts and draws every one, reporting what that
/// found instead of a canvas.
///
/// The surface's own path — the same catalog, the same parser, the same
/// layout — because what an author needs is the answer *this build* would
/// give, not a second opinion about the grammar that can drift from it.
pub fn check(host: &dyn Host, source: ArtifactSource) -> Checkup {
    match source {
        ArtifactSource::Undeclared => Checkup::Nothing {
            text: UNDECLARED.to_owned(),
            hint: DECLARE_ARTIFACTS.to_owned(),
        },
        ArtifactSource::Refused(reason) => Checkup::Unusable {
            text: reason,
            hint: "Fix agents.yaml and run this again.".to_owned(),
        },
        ArtifactSource::Directories { roots, project } => match read_roots(host, &roots) {
            Ok(artifacts) => {
                let catalog = Catalog::of(artifacts, &project);
                let graphs: Vec<Option<model::Graph>> = catalog
                    .artifacts()
                    .iter()
                    .map(|artifact| match mermaid::parse(artifact.diagram()) {
                        Ok(Diagram::Graph(graph)) if graph.view.is_some() => Some(graph),
                        _ => None,
                    })
                    .collect();
                Checkup::Checked {
                    declared: roots
                        .iter()
                        .map(|root| root.declared.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    artifacts: catalog
                        .artifacts()
                        .iter()
                        .enumerate()
                        .map(|(index, artifact)| Checked {
                            notes: notation::notes(&catalog, &graphs, index),
                            ..checked(host, &project, artifact)
                        })
                        .collect(),
                }
            }
            Err((declared, reason)) => {
                let (text, hint) = unreadable(&declared, &reason);
                Checkup::Unusable { text, hint }
            }
        },
    }
}

fn checked(host: &dyn Host, project: &Path, artifact: &Artifact) -> Checked {
    let mut broken_links = Vec::new();
    let verdict = match mermaid::parse(artifact.diagram()) {
        Ok(Diagram::Graph(graph)) => {
            broken_links = graph
                .nodes
                .iter()
                .filter_map(|node| node.link.as_deref())
                .filter_map(|link| match zoom::project_path(link) {
                    None => Some(format!("`{link}` leaves the project")),
                    Some(path) => {
                        let target = project.join(path);
                        let missing =
                            host.read_file(&target).is_err() && !is_directory(host, &target);
                        missing.then(|| format!("`{link}` is not a file in the project"))
                    }
                })
                .collect();
            match Scene::of(graph).routes.unrouted {
                0 => Verdict::Drawn,
                edges => Verdict::Unrouted { edges },
            }
        }
        Ok(Diagram::Sequence(_)) => Verdict::Drawn,
        Err(reason) => Verdict::Undrawable(reason),
    };
    Checked {
        origin: artifact.origin.clone(),
        name: artifact.name.clone(),
        area: artifact.kind.name(),
        verdict,
        broken_links,
        notes: Vec::new(),
    }
}

/// Whether `path` is a directory, as its parent lists it — a component
/// may stand for a module's directory, which the code surface opens.
fn is_directory(host: &dyn Host, path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    host.list_dir(parent).is_ok_and(|entries| {
        entries
            .iter()
            .any(|entry| entry.directory && std::ffi::OsStr::new(&entry.name) == name)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchitectOutcome {
    Stay,
    Close,
    /// A box that stands for code was followed: the host shows `target`,
    /// which is where the board ends and the checkout begins. `project`
    /// is what the diagram wrote the path relative to.
    OpenPath {
        project: PathBuf,
        target: PathBuf,
    },
}

impl ArchitectView {
    /// The surface before its artifacts have been read.
    ///
    /// `display_root` is the checkout as the host spells it for a reader,
    /// the way [`crate::code::CodeView::opening`] takes it: the title
    /// names the checkout from the first frame, because which checkout
    /// is being looked at is not something to wait for a read to learn.
    pub fn opening(display_root: String) -> Self {
        Self {
            catalog: Catalog::default(),
            display_root,
            branch: String::new(),
            selected: 0,
            showing: Showing::Unicode,
            corner: None,
            picked: None,
            drawing: Drawing::Unreadable(String::new()),
            drawn: None,
            laid_out: HashMap::new(),
            canvas: None,
            nothing: Some(("Reading the project's artifacts".to_owned(), None)),
            choosing: None,
            project: PathBuf::new(),
            trouble: None,
            resuming: None,
        }
    }

    /// The same surface, to be put back where [`ArchitectPlace`] says the
    /// viewer left this checkout — once there is something to put back
    /// into. Nothing is resolved here: the artifacts have not been read
    /// yet, and a name can only be looked up among files that exist.
    pub fn resuming(mut self, place: ArchitectPlace) -> Self {
        self.resume(place);
        self
    }

    /// [`Self::resuming`], on a surface already open.
    pub fn resume(&mut self, place: ArchitectPlace) {
        self.resuming = Some(place);
    }

    /// Where the viewer is, for the host to keep.
    pub fn place(&self) -> ArchitectPlace {
        let named = |artifact: usize| {
            self.catalog
                .get(artifact)
                .map(|artifact| artifact.origin.clone())
        };
        let picked = match (&self.drawing, self.picked) {
            (Drawing::Graph(scene), Some(picked)) => Some(scene.graph.nodes[picked].id.clone()),
            _ => None,
        };
        ArchitectPlace {
            artifact: named(self.selected).unwrap_or_default(),
            corner: self.corner,
            picked,
        }
    }

    pub fn absorb(&mut self, answer: ArtifactsAnswer) {
        self.branch = answer.branch;
        self.drawn = None;
        self.laid_out.clear();
        match answer.artifacts {
            Artifacts::Found { artifacts, project } => {
                self.catalog = Catalog::of(artifacts, &project);
                self.project = project;
                self.nothing = None;
                self.trouble = None;
                self.restore();
            }
            Artifacts::Nothing { text, hint } => {
                self.catalog = Catalog::default();
                self.nothing = Some((text, Some(hint)));
            }
        }
    }

    /// Every area, as the index of its first artifact.
    fn areas(&self) -> Vec<usize> {
        let artifacts = self.catalog.artifacts();
        (0..artifacts.len())
            .filter(|&index| index == 0 || artifacts[index - 1].kind != artifacts[index].kind)
            .collect()
    }

    /// The area the artifact on show belongs to.
    fn area(&self) -> Option<usize> {
        self.areas()
            .into_iter()
            .take_while(|&first| first <= self.selected)
            .last()
    }

    /// What the artifact selector offers: the views that share the C4 view
    /// on show's parent, or else the artifacts of the area on show.
    fn choices(&self) -> Vec<usize> {
        match self.zoom().place(self.selected) {
            Some(_) => self.zoom().siblings(self.selected),
            None => self.area_artifacts(),
        }
    }

    fn zoom(&self) -> &zoom::Zoom {
        self.catalog.zoom()
    }

    /// The artifacts of the area on show, by index.
    fn area_artifacts(&self) -> Vec<usize> {
        let areas = self.areas();
        let Some(first) = self.area() else {
            return Vec::new();
        };
        let next = areas
            .iter()
            .copied()
            .find(|&area| area > first)
            .unwrap_or(self.catalog.artifacts().len());
        (first..next).collect()
    }

    /// Moves the highlight in whichever list is open, round and round.
    fn highlight(&mut self, step: isize) {
        let (list, at) = match self.choosing {
            Some(Choosing::Group(at)) => (self.areas(), at),
            Some(Choosing::Item(at)) => (self.choices(), at),
            None => return,
        };
        let Some(position) = list.iter().position(|&entry| entry == at) else {
            return;
        };
        let next = list[(position as isize + step).rem_euclid(list.len() as isize) as usize];
        self.choosing = Some(match self.choosing {
            Some(Choosing::Group(_)) => Choosing::Group(next),
            _ => Choosing::Item(next),
        });
    }

    /// A selector with one entry is not a choice: it opens onto the
    /// thing already on show, which is a list nobody can answer. The two
    /// of them stay shut, and the host draws them without the mark that
    /// says they open.
    fn choose_area(&mut self) {
        if self.areas().len() > 1 {
            self.choosing = self.area().map(Choosing::Group);
        }
    }

    fn choose_artifact(&mut self) {
        if self.choices().len() > 1 {
            self.choosing = Some(Choosing::Item(self.selected));
        }
    }

    /// One step out, and never more than one: what is selected is let go
    /// of, then the level that was entered is left, and only with neither
    /// of those left does the surface itself close. The list that may be
    /// open is a step too — the first — but it takes every key while it
    /// is, so it is left where that happens. A key that shuts everything
    /// at once is a key nobody can use to look around with.
    fn leave(&mut self, space: Size) -> ArchitectOutcome {
        if self.let_go() {
            return ArchitectOutcome::Stay;
        }
        match self.zoom().parent(self.selected) {
            None => ArchitectOutcome::Close,
            Some(_) => {
                self.up(space);
                ArchitectOutcome::Stay
            }
        }
    }

    /// Whether something was selected, and now is not.
    fn let_go(&mut self) -> bool {
        if self.picked.take().is_some() {
            self.repaint();
            return true;
        }
        false
    }

    /// The way down to the C4 view on show, for the menu to draw: every
    /// step by the level it is, the view on show included, which opens
    /// onto the views that share its parent by their titles. A title names
    /// a view's type and scope, which is a sentence, and a row of them is
    /// a row cut short; the title on show is the view's [`View::label`]
    /// instead. Empty off C4, where the area is a set to pick from.
    fn steps(&self) -> Vec<TrailStep> {
        let zoom = self.zoom();
        let Some(place) = zoom.place(self.selected) else {
            return Vec::new();
        };
        let mut steps: Vec<TrailStep> = zoom
            .path(self.selected)
            .into_iter()
            .filter(|&view| view != self.selected)
            .filter_map(|view| Some(TrailStep::new(zoom.place(view)?.name(), false)))
            .collect();
        let name = place.name();
        let siblings = zoom.siblings(self.selected);
        let choices = match siblings.len() > 1 {
            true => siblings
                .into_iter()
                .filter_map(|view| Some(TrailChoice::new(view, &self.catalog.get(view)?.name)))
                .collect(),
            false => Vec::new(),
        };
        steps.push(TrailStep::new(name, true).choosing(choices));
        steps
    }

    /// The title of the C4 view on show, which C4 asks every diagram to
    /// carry and the trail names only by its level.
    fn label(&self) -> String {
        match self.zoom().place(self.selected) {
            Some(_) => self
                .catalog
                .get(self.selected)
                .map(|artifact| artifact.name.clone())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    /// A step of the way down, pressed: an ancestor is gone back up to,
    /// standing on the box that leads down towards where the viewer was.
    fn go_to(&mut self, step: usize, space: Size) {
        let path = self.zoom().path(self.selected);
        if step + 1 < path.len() {
            self.up_to(path[step], path[step + 1], space);
        }
    }

    /// The pointer moved: while a list is open the highlight follows it,
    /// which is what makes the list read as a menu rather than as a
    /// keyboard-only one. Answers whether the highlight moved.
    fn hover(&mut self, hit: Option<ViewHit>) -> bool {
        let moved = match (hit, self.choosing) {
            (Some(ViewHit::ToggleGroup(entry)), Some(Choosing::Group(at))) if entry != at => {
                Choosing::Group(entry)
            }
            (Some(ViewHit::SelectItem(entry)), Some(Choosing::Item(at))) if entry != at => {
                Choosing::Item(entry)
            }
            _ => return false,
        };
        self.choosing = Some(moved);
        true
    }

    /// The first artifact, or wherever the viewer was when they left —
    /// whichever of the two the host asked for and the catalogue still
    /// answers to.
    fn restore(&mut self) {
        let Some(place) = self.resuming.take() else {
            self.open(0);
            return;
        };
        let named = |origin: &str| {
            self.catalog
                .artifacts()
                .iter()
                .position(|artifact| artifact.origin == origin)
        };
        let Some(artifact) = named(&place.artifact) else {
            self.open(0);
            return;
        };
        self.open(artifact);
        if let (Drawing::Graph(scene), Some(id)) = (&self.drawing, &place.picked) {
            self.picked = scene.graph.nodes.iter().position(|node| &node.id == id);
        }
        self.corner = place.corner;
        self.repaint();
    }

    fn open(&mut self, artifact: usize) {
        self.choosing = None;
        let count = self.catalog.artifacts().len().max(1);
        self.selected = artifact % count;
        self.corner = None;
        self.picked = None;
        if self.drawn != Some(self.selected) {
            let drawing = match self.laid_out.remove(&self.selected) {
                Some(drawing) => drawing,
                None => self.draw(self.selected),
            };
            let left = std::mem::replace(&mut self.drawing, drawing);
            if let Some(left_from) = self.drawn.replace(self.selected) {
                self.laid_out.insert(left_from, left);
            }
        }
        self.repaint();
    }

    fn draw(&self, artifact: usize) -> Drawing {
        match self.catalog.get(artifact) {
            Some(artifact) => match mermaid::parse(artifact.diagram()) {
                Ok(Diagram::Graph(graph)) => Drawing::Graph(Box::new(Scene::of(graph))),
                Ok(Diagram::Sequence(sequence)) => Drawing::Sequence(sequence),
                Err(reason) => Drawing::Unreadable(reason),
            },
            None => Drawing::Unreadable("there is nothing to draw".to_owned()),
        }
    }

    fn repaint(&mut self) {
        let glyphs = self.glyphs();
        self.canvas = match &self.drawing {
            Drawing::Graph(scene) => {
                let leads: Vec<Leads> = (0..scene.graph.nodes.len())
                    .map(|node| self.leads(scene, node))
                    .collect();
                Some(paint::paint(scene, glyphs, self.picked, &leads))
            }
            Drawing::Sequence(sequence) => Some(sequence::paint(sequence, glyphs)),
            Drawing::Unreadable(_) => None,
        };
    }

    fn glyphs(&self) -> Glyphs {
        match self.showing {
            Showing::Ascii => Glyphs::Ascii,
            Showing::Unicode | Showing::Source => Glyphs::Unicode,
        }
    }

    fn show(&mut self, showing: Showing) {
        self.showing = showing;
        self.corner = None;
        self.repaint();
    }

    fn source(&self) -> &str {
        self.catalog
            .get(self.selected)
            .map_or("", |artifact| artifact.source.as_str())
    }

    fn board_size(&self) -> (i32, i32) {
        match (self.showing, &self.canvas) {
            (Showing::Source, _) => (0, self.source().lines().count() as i32),
            (_, Some(canvas)) => (canvas.width, canvas.height),
            _ => (0, 0),
        }
    }

    /// How far the corner may go, each way. A board is not a document:
    /// its edge may be brought to the middle of the screen, from either
    /// side, because what is being read is as often at the edge of the
    /// drawing as in it — and a drawing pinned to the screen's border
    /// cannot be looked at the way its middle can. The source *is* a
    /// document, and scrolls like one.
    fn reach(&self, space: Size) -> ((i32, i32), (i32, i32)) {
        let board = self.board_size();
        let screen = (i32::from(space.width), i32::from(space.height));
        match self.showing {
            Showing::Source => ((0, 0), (0, (board.1 - screen.1).max(0))),
            _ => (
                (-screen.0 / 2, board.0 - screen.0 / 2),
                (-screen.1 / 2, board.1 - screen.1 / 2),
            ),
        }
    }

    /// Where the corner is when nothing has moved it: the drawing in the
    /// middle of the screen, or its top-left in view when it is larger.
    fn home(&self, space: Size) -> (i32, i32) {
        let board = self.board_size();
        let screen = (i32::from(space.width), i32::from(space.height));
        match self.showing {
            Showing::Source => (0, 0),
            _ => (
                ((board.0 - screen.0) / 2).min(0),
                ((board.1 - screen.1) / 2).min(0),
            ),
        }
    }

    fn corner(&self, space: Size) -> (i32, i32) {
        let (across, down) = self.reach(space);
        let corner = self.corner.unwrap_or_else(|| self.home(space));
        (
            corner.0.clamp(across.0, across.1.max(across.0)),
            corner.1.clamp(down.0, down.1.max(down.0)),
        )
    }

    fn move_by(&mut self, columns: i32, rows: i32, space: Size) {
        let corner = self.corner(space);
        self.corner = Some((corner.0 + columns, corner.1 + rows));
        self.corner = Some(self.corner(space));
    }

    fn minimap(&self, space: Size) -> Option<Minimap> {
        let screen = (i32::from(space.width), i32::from(space.height));
        match self.showing {
            Showing::Source => None,
            _ => Minimap::of(self.board_size(), screen),
        }
    }

    /// A click, given as the screen cell it landed on. The minimap
    /// answers first: it is drawn over the board.
    fn click(&mut self, x: i32, y: i32, space: Size) -> ArchitectOutcome {
        if let Some(target) = self.minimap(space).and_then(|map| map.board_cell_at(x, y)) {
            let half = (i32::from(space.width) / 2, i32::from(space.height) / 2);
            self.corner = Some((target.0 - half.0, target.1 - half.1));
            self.corner = Some(self.corner(space));
            return ArchitectOutcome::Stay;
        }
        let Drawing::Graph(scene) = &self.drawing else {
            return ArchitectOutcome::Stay;
        };
        let corner = self.corner(space);
        let at = (x + corner.0, y + corner.1);
        let node = scene.placement.node_at(at.0, at.1);
        // A box that leads somewhere is followed by a click on its mark,
        // or by a second click on the box — which is what a double click
        // is. One that leads nowhere is let go of by that second click,
        // so the diagram can be read whole again without a key.
        if let Some(node) = node {
            let leads = self.leads(scene, node);
            let frame = scene.placement.nodes[node];
            let on_code_mark = paint::code_mark_cells(frame, leads)
                .is_some_and(|cells| cells.contains(at.0, at.1));
            let on_mark = paint::mark_cells(frame).contains(at.0, at.1);
            if leads != Leads::Nowhere && on_code_mark {
                self.picked = Some(node);
                return self.to_code();
            }
            if leads != Leads::Nowhere && (on_mark || self.picked == Some(node)) {
                self.picked = Some(node);
                return self.enter();
            }
        }
        self.picked = if node == self.picked { None } else { node };
        self.repaint();
        ArchitectOutcome::Stay
    }

    fn leads(&self, scene: &Scene, node: usize) -> Leads {
        let node = &scene.graph.nodes[node];
        let inside = self.inside_of(node).is_some();
        let code = self.code_link(node).is_some();
        match (inside, code) {
            (true, true) => Leads::Both,
            (true, false) => Leads::Inside,
            (false, true) => Leads::ToCode,
            (false, false) => Leads::Nowhere,
        }
    }

    /// The view a box leads into: the first that zooms into it.
    fn inside_of(&self, node: &model::Node) -> Option<usize> {
        self.zoom().inside(self.selected, &node.id).first().copied()
    }

    /// The path a box's link names in the project, unless it names
    /// another view — that link is the way inside, not to the code.
    fn code_link(&self, node: &model::Node) -> Option<PathBuf> {
        let path = zoom::project_path(node.link.as_deref()?)?;
        let names_a_view = self
            .catalog
            .artifacts()
            .iter()
            .any(|artifact| self.project.join(&artifact.path) == self.project.join(&path));
        (!names_a_view).then(|| self.project.join(path))
    }

    /// Follows the picked box: into the view below it, or out to the code
    /// it names.
    fn enter(&mut self) -> ArchitectOutcome {
        let (Drawing::Graph(scene), Some(picked)) = (&self.drawing, self.picked) else {
            return ArchitectOutcome::Stay;
        };
        let node = &scene.graph.nodes[picked];
        if let Some(below) = self.inside_of(node) {
            self.open(below);
            return ArchitectOutcome::Stay;
        }
        self.to_code()
    }

    /// The code the picked box names, whatever else it leads to.
    fn to_code(&self) -> ArchitectOutcome {
        let (Drawing::Graph(scene), Some(picked)) = (&self.drawing, self.picked) else {
            return ArchitectOutcome::Stay;
        };
        match self.code_link(&scene.graph.nodes[picked]) {
            Some(target) => ArchitectOutcome::OpenPath {
                project: self.project.clone(),
                target,
            },
            None => ArchitectOutcome::Stay,
        }
    }

    /// Up one level, to the view the one on show zooms out to.
    fn up(&mut self, space: Size) {
        if let Some(parent) = self.zoom().parent(self.selected) {
            self.up_to(parent, self.selected, space);
        }
    }

    /// Opens `ancestor`, with the box that leads down towards `towards`
    /// picked and in the middle of the screen — where the viewer stood.
    fn up_to(&mut self, ancestor: usize, towards: usize, space: Size) {
        let through = self.zoom().through(towards).map(str::to_owned);
        self.open(ancestor);
        let Drawing::Graph(scene) = &self.drawing else {
            return;
        };
        let at = through.and_then(|id| scene.graph.nodes.iter().position(|node| node.id == id));
        if let Some(node) = at {
            self.picked = Some(node);
            self.bring_into_view(node, space, true);
            self.repaint();
        }
    }

    fn bring_into_view(&mut self, node: usize, space: Size, always: bool) {
        let Drawing::Graph(scene) = &self.drawing else {
            return;
        };
        let frame = scene.placement.nodes[node];
        let corner = self.corner(space);
        let screen = (i32::from(space.width), i32::from(space.height));
        let visible = frame.x >= corner.0
            && frame.y >= corner.1
            && frame.x + frame.w <= corner.0 + screen.0
            && frame.y + frame.h <= corner.1 + screen.1;
        if always || !visible {
            let centre = frame.center();
            self.corner = Some((centre.0 - screen.0 / 2, centre.1 - screen.1 / 2));
            self.corner = Some(self.corner(space));
        }
    }

    /// Picks the box that lies `direction` of the picked one — or, with
    /// nothing picked, the one nearest the middle of the screen.
    fn pick_toward(&mut self, direction: PanDirection, space: Size) {
        let Drawing::Graph(scene) = &self.drawing else {
            return;
        };
        let corner = self.corner(space);
        let from = match self.picked {
            Some(picked) => scene.placement.nodes[picked].center(),
            None => (
                corner.0 + i32::from(space.width) / 2,
                corner.1 + i32::from(space.height) / 2,
            ),
        };
        let candidates = scene
            .placement
            .nodes
            .iter()
            .enumerate()
            .filter(|(node, _)| Some(*node) != self.picked)
            .map(|(node, frame)| (node, frame.center()));
        let nearest = nearest::toward(from, direction, self.picked.is_some(), candidates);
        if let Some(node) = nearest {
            self.picked = Some(node);
            self.bring_into_view(node, space, false);
            self.repaint();
        }
    }

    fn heading(&self) -> String {
        match &self.drawing {
            Drawing::Graph(scene) => {
                let graph = &scene.graph;
                let mut heading =
                    format!("{} boxes · {} edges", graph.nodes.len(), graph.edges.len());
                if scene.routes.unrouted > 0 {
                    heading.push_str(&format!(" · {} not routed", scene.routes.unrouted));
                }
                if let Some(picked) = self.picked {
                    let into = graph.edges.iter().filter(|edge| edge.to == picked).count();
                    let out = graph
                        .edges
                        .iter()
                        .filter(|edge| edge.from == picked)
                        .count();
                    heading = format!("{} · {into} in · {out} out", graph.nodes[picked].title);
                }
                heading
            }
            Drawing::Sequence(sequence) => format!(
                "{} participants · {} steps",
                sequence.participants.len(),
                sequence.steps.len()
            ),
            Drawing::Unreadable(_) => String::new(),
        }
    }

    /// What the content says about itself, and which file it came from —
    /// the second half is what somebody needs to go and change it.
    fn caption(&self) -> String {
        let origin = self.catalog.get(self.selected).map(|a| a.origin.as_str());
        if let (Drawing::Graph(scene), Some(picked)) = (&self.drawing, self.picked) {
            let link = scene.graph.nodes[picked]
                .link
                .as_deref()
                .unwrap_or_default();
            let said = match self.leads(scene, picked) {
                Leads::Inside => Some("enter goes inside".to_owned()),
                Leads::ToCode => Some(format!("enter opens {link}")),
                Leads::Both => Some(format!("enter goes inside · its code mark opens {link}")),
                Leads::Nowhere => None,
            };
            if let Some(said) = said {
                return format!("{} · {said}", self.heading());
            }
        }
        match (self.heading(), origin) {
            (heading, Some(origin)) if heading.is_empty() => origin.to_owned(),
            (heading, Some(origin)) => format!("{heading} · {origin}"),
            (heading, None) => heading,
        }
    }

    /// The part of the board the screen is over, as the screen's own
    /// cells: the drawing, the grid behind it, the minimap over it.
    fn screen(&self, space: Size) -> Option<Canvas> {
        let board = self.canvas.as_ref()?;
        let (columns, rows) = (i32::from(space.width), i32::from(space.height));
        let corner = self.corner(space);
        let mut screen = Canvas::new(columns, rows, board.glyphs);
        let grid_dot = match board.glyphs {
            Glyphs::Unicode => '·',
            Glyphs::Ascii => '.',
        };
        let regions: &[Frame] = match &self.drawing {
            Drawing::Graph(scene) => &scene.placement.clusters,
            _ => &[],
        };
        for y in 0..rows {
            for x in 0..columns {
                let at = (x + corner.0, y + corner.1);
                let on_the_grid = at.0.rem_euclid(GRID.0) == 0 && at.1.rem_euclid(GRID.1) == 0;
                match board.cell(at.0, at.1) {
                    Some(cell) if cell.solid => screen.set(x, y, cell),
                    _ if on_the_grid && grounded(regions, at) => {
                        screen.put(x, y, grid_dot, Role::Faint, false);
                    }
                    _ => {}
                }
            }
        }
        if let Some(map) = self.minimap(space) {
            let boxes: &[Frame] = match &self.drawing {
                Drawing::Graph(scene) => &scene.placement.nodes,
                _ => &[],
            };
            let looking_at = Frame {
                x: corner.0,
                y: corner.1,
                w: columns,
                h: rows,
            };
            map.paint(&mut screen, boxes, looking_at);
        }
        Some(screen)
    }
}

/// Whether the board's own grid shows through at this cell.
///
/// It does not inside a region: the grid is the texture of a board with
/// nothing on it, and a region *is* something, so it stands on a ground
/// of its own. Nesting alternates — a region inside a region takes its
/// parent's ground back — which is what keeps two walls a cell apart
/// telling apart without a second kind of line.
fn grounded(regions: &[Frame], at: (i32, i32)) -> bool {
    regions
        .iter()
        .filter(|region| region.contains(at.0, at.1))
        .count()
        % 2
        == 0
}

pub fn view(state: &ArchitectView, space: Size) -> View {
    let mut rows = Vec::new();
    let mut area = None;
    for (index, artifact) in state.catalog.artifacts().iter().enumerate() {
        if area != Some(artifact.kind) {
            area = Some(artifact.kind);
            rows.push(NavigatorRow::Group {
                id: index,
                name: artifact.kind.name().to_owned(),
                depth: 0,
                collapsed: false,
                icon: RowIcon::None,
            });
        }
        rows.push(NavigatorRow::Item {
            id: index,
            name: artifact.name.clone(),
            depth: 1,
            marker: Span::default(),
            marker_side: MarkerSide::Leading,
            detail: String::new(),
            selected: index == state.selected,
            icon: RowIcon::None,
        });
    }
    View {
        title: checkout::name(CATALOG.name),
        caption: checkout::caption(&state.display_root, &state.branch),
        navigator: Some(Navigator {
            heading: "ARTIFACTS".to_owned(),
            badge: state.catalog.artifacts().len().to_string(),
            focused: false,
            rows,
            anchor: None,
            choosing: state.choosing,
            menu: None,
        }),
        content: content(state, space),
        footer: footer(state),
        notice: None,
        confirm: None,
        modes: modes(state),
        // Nothing to be about but the artifacts it draws: the trail is
        // how this surface is walked, and the selectors above it are
        // what stand where the code surface's own halves do.
        subjects: Vec::new(),
        layout: Layout::Board,
        trail: state.steps(),
        label: state.label(),
    }
}

/// How the board can be drawn. Offered only where there is a board: a
/// message has one way of being shown, and a selector over it switches
/// nothing.
fn modes(state: &ArchitectView) -> Vec<Mode> {
    if state.nothing.is_some() {
        return Vec::new();
    }
    MODES
        .iter()
        .map(|&(showing, label)| Mode {
            label: label.to_owned(),
            icon: RowIcon::None,
            active: showing == state.showing,
        })
        .collect()
}

/// What the surface can be asked right now. The way up is named only
/// where there is a level to leave — which is also where the key that
/// closes stops closing and starts going up. With nothing to draw there
/// is nothing to ask either, so a message stands alone.
fn footer(state: &ArchitectView) -> Vec<Command> {
    if state.nothing.is_some() {
        return Vec::new();
    }
    let mut commands = vec![Command::Close];
    if state.zoom().parent(state.selected).is_some() {
        commands.push(Command::Back);
    }
    commands.extend([Command::ChooseItem, Command::NextView, Command::NextMode]);
    commands
}

fn content(state: &ArchitectView, space: Size) -> Content {
    if let Some((text, hint)) = &state.nothing {
        return Content::Message {
            text: text.clone(),
            hint: hint.clone(),
            role: Role::Muted,
        };
    }
    if let Drawing::Unreadable(reason) = &state.drawing {
        return Content::Message {
            text: "This artifact could not be drawn".to_owned(),
            hint: Some(reason.clone()),
            role: Role::Warning,
        };
    }
    match state.showing {
        Showing::Source => {
            let source = state.source();
            let first = usize::try_from(state.corner(space).1).unwrap_or(0);
            Content::Lines {
                first,
                heading: state.caption(),
                scroll: first,
                total: source.lines().count(),
                // Twice the height for the reason the code surface's
                // windows are: only the host knows how many rows a
                // wrapped line takes.
                lines: source_lines(source, first, usize::from(space.height).saturating_mul(2)),
                caret: None,
                editable: false,
                medium: Medium::Text,
            }
        }
        // A board hands over its screen and nothing else: there is no
        // "scrolled past" on a surface that moves both ways, so there is
        // no scroll for the host to apply and no bar for it to draw.
        _ => {
            let lines = state.screen(space).map(|s| s.lines()).unwrap_or_default();
            Content::Lines {
                first: 0,
                heading: state.caption(),
                scroll: 0,
                total: lines.len(),
                lines,
                caret: None,
                editable: false,
                medium: Medium::Drawing,
            }
        }
    }
}

fn source_lines(source: &str, first: usize, count: usize) -> Vec<ContentLine> {
    source
        .lines()
        .enumerate()
        .skip(first)
        .take(count)
        .map(|(index, line)| ContentLine {
            gutter: " ".to_owned(),
            number: (index + 1).to_string(),
            tone: LineTone::Neutral,
            spans: vec![Span::new(line, Role::Default)],
        })
        .collect()
}

pub fn handle_command(
    state: &mut ArchitectView,
    command: Command,
    space: Size,
) -> ArchitectOutcome {
    let page = i32::from(space.height.saturating_sub(2).max(1));
    let count = state.catalog.artifacts().len().max(1);
    // An open list takes the keys that mean something in a list, and the
    // one that leaves: a list open over the board is what is being talked
    // to, and the board under it waits. Left and right step between the
    // two lists, which sit side by side on the menu.
    if let Some(choosing) = state.choosing {
        match (command, choosing) {
            (Command::Pan(PanDirection::Up), _) => state.highlight(-1),
            (Command::Pan(PanDirection::Down), _) => state.highlight(1),
            (Command::Pan(PanDirection::Left), Choosing::Item(_)) => state.choose_area(),
            (Command::Pan(PanDirection::Right), Choosing::Group(_)) => state.choose_artifact(),
            (Command::Activate, Choosing::Group(entry) | Choosing::Item(entry)) => {
                state.open(entry);
            }
            (Command::Close | Command::ChooseGroup | Command::ChooseItem, _) => {
                state.choosing = None;
            }
            _ => {}
        }
        return ArchitectOutcome::Stay;
    }
    match command {
        Command::Close => return state.leave(space),
        Command::ChooseGroup => state.choose_area(),
        Command::ChooseItem => state.choose_artifact(),
        Command::SelectToward(direction) => state.pick_toward(direction, space),
        Command::Activate => return state.enter(),
        Command::Back => state.up(space),
        // Nothing to move on a map that fits: the arrows walk its tiles.
        Command::NextView => state.open(state.selected + 1),
        Command::PreviousView => state.open(state.selected + count - 1),
        Command::Pan(PanDirection::Left) => state.move_by(-PAN_COLUMNS, 0, space),
        Command::Pan(PanDirection::Right) => state.move_by(PAN_COLUMNS, 0, space),
        Command::Pan(PanDirection::Up) => state.move_by(0, -PAN_ROWS, space),
        Command::Pan(PanDirection::Down) => state.move_by(0, PAN_ROWS, space),
        Command::ScrollPageDown => state.move_by(0, page, space),
        Command::ScrollPageUp => state.move_by(0, -page, space),
        Command::NextMode => {
            let current = MODES
                .iter()
                .position(|&(showing, _)| showing == state.showing)
                .unwrap_or(0);
            state.show(MODES[(current + 1) % MODES.len()].0);
        }
        _ => {}
    }
    ArchitectOutcome::Stay
}

/// The pointer moved over the surface, without pressing anything.
/// Answers whether the frame has to be drawn again.
pub fn handle_hover(state: &mut ArchitectView, hit: Option<ViewHit>) -> bool {
    state.hover(hit)
}

pub fn handle_mouse(
    state: &mut ArchitectView,
    hit: Option<ViewHit>,
    space: Size,
) -> ArchitectOutcome {
    // A click anywhere but on the list shuts it, and is spent doing so —
    // the same rule every menu over this client follows.
    let was_open = state.choosing.take();
    match hit {
        Some(ViewHit::Close) => return ArchitectOutcome::Close,
        // A selector pressed while its own list is open shuts it; pressed
        // while the other one is, it takes over.
        Some(ViewHit::ChooseGroup) if !matches!(was_open, Some(Choosing::Group(_))) => {
            state.choose_area();
        }
        Some(ViewHit::ChooseItem) if !matches!(was_open, Some(Choosing::Item(_))) => {
            state.choose_artifact();
        }
        Some(ViewHit::ToggleGroup(first)) => state.open(first),
        Some(ViewHit::SelectItem(artifact)) => state.open(artifact),
        _ if was_open.is_some() => {}
        Some(ViewHit::SelectMode(mode)) => {
            if let Some(&(showing, _)) = MODES.get(mode) {
                state.show(showing);
            }
        }
        Some(ViewHit::PlaceCaret { line, cell }) if state.showing != Showing::Source => {
            return state.click(cell as i32, line as i32, space);
        }
        Some(ViewHit::SelectTrail(step)) => state.go_to(step, space),
        _ => {}
    }
    ArchitectOutcome::Stay
}

/// The board, taken hold of and moved: it follows the pointer, so a drag
/// to the right brings what was off to the left into view — the way a
/// sheet of paper moves, not the way a scrollbar does.
pub fn drag_by(state: &mut ArchitectView, columns: i32, rows: i32, space: Size) {
    state.move_by(-columns, -rows, space);
}

/// A sideways wheel, where the terminal reports one.
pub fn pan(state: &mut ArchitectView, columns: i32, space: Size) {
    state.move_by(columns, 0, space);
}

pub fn handle_scroll(state: &mut ArchitectView, direction: ScrollDirection, space: Size) {
    let rows = match direction {
        ScrollDirection::Up => -PAN_ROWS,
        ScrollDirection::Down => PAN_ROWS,
    };
    state.move_by(0, rows, space);
}

pub fn scroll_to(state: &mut ArchitectView, first: usize, space: Size) {
    let corner = state.corner(space);
    state.corner = Some((corner.0, first as i32));
    state.corner = Some(state.corner(space));
}

/// The text of `lines` of the artifact's source, when the source is what
/// is on show — what the host copies when a reader marked them. A drawn
/// diagram has none: it is a [`Medium::Drawing`].
pub fn text(state: &ArchitectView, lines: std::ops::Range<usize>) -> Vec<String> {
    if state.showing != Showing::Source {
        return Vec::new();
    }
    let count = lines.len();
    state
        .source()
        .lines()
        .skip(lines.start)
        .take(count)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests;
