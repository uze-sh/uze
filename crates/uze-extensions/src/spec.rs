//! The spec surface: what a checkout *intends*, beside what it is (the
//! code surface) and what somebody described (the architect surface).
//!
//! # One surface, many tools
//!
//! Spec-driven-development tools differ in layout and agree in concept:
//! a unit of intent holding a why, a how, a list of steps and, in some, a
//! contract that outlives it. So this surface speaks in those
//! [`Role`]s, and a tool is a [`dialect::Dialect`] — a table, detected by
//! the marker the tool itself defines rather than declared in
//! `agents.yaml`, because where the tool keeps its files is the tool's
//! decision and not the project's. OpenSpec, Spec Kit, Superpowers and
//! GSD are the ones shipped, and they differ on screen only where their
//! tables do: which subjects exist, what counts as one unit of each, what
//! the files are called, how finished steps are counted, and whether a
//! finished change is put away.
//!
//! # Read from the checkout
//!
//! The files travel with the branch, so an agent's worktree is read as
//! its branch has them, and the units that checkout touched are its own
//! ([`ownership`]): the surface opens on the change the agent in front of
//! it is working on.
//!
//! # It reads; the code surface writes
//!
//! Nothing here writes. Activating an artifact hands it to the code
//! surface, which already holds the write grant and the rule about what
//! it may write (ADR-048) — a second editor would be a second place to
//! keep that rule.

mod catalog;
mod decision;
mod dialect;
mod ownership;
mod progress;
mod summary;
#[cfg(test)]
mod tests;

use std::{
    cell::{Ref, RefCell},
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    ArtifactSource, Host,
    registry::BuiltinExtension,
    shared::{checkout, highlight, markdown},
    view::{
        Command, Content, ContentLine, Layout, LineTone, MarkerSide, Medium, Mode, Navigator,
        NavigatorRow, Role as Tone, RowIcon, ScrollDirection, Size, Span, View, ViewHit,
    },
};

pub use catalog::{Artifact, Found, Unit};
pub use decision::Standing;
pub use dialect::{Role, Subject};
pub use progress::Progress;
pub use summary::{ChangeSummary, Summary, summary, summary_section};

pub const CATALOG: BuiltinExtension = BuiltinExtension {
    id: "spec",
    name: "Spec",
    description: "What a checkout intends: the proposals, designs, tasks and specs a spec-driven-development tool keeps, listed by change and read as documents.",
    surface: "Workspace TUI",
    usage: "Alt+X opens it over the workspace, as does its chip in the tab strip; the change the checkout is working on opens first, and Enter hands a document to the code surface to edit.",
};

/// What an event asked of the host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecOutcome {
    Stay,
    Close,
    /// An artifact was activated: the host opens the code surface on
    /// `target`, inside `project`.
    OpenPath {
        project: PathBuf,
        target: PathBuf,
    },
}

/// What reading a checkout produced — everything the surface needs to stop
/// saying "reading".
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecAnswer {
    /// The repository root the units were read from.
    pub root: PathBuf,
    pub branch: String,
    pub theme: String,
    pub found: Found,
    /// The subjects the detected dialects declare, whether or not each
    /// holds anything yet.
    pub subjects: Vec<Subject>,
}

/// Reads `checkout` for the shipped dialects, and `places` — where the
/// project declares its artifacts — for the decisions it keeps. `target` is
/// the branch the checkout delivers to, when the host knows one; what the
/// branch changed since it left it is part of what makes a unit this
/// checkout's own.
pub fn read_spec(
    host: &dyn Host,
    checkout: &Path,
    target: Option<&str>,
    places: &ArtifactSource,
) -> SpecAnswer {
    read_with(host, checkout, target, dialect::SHIPPED, places)
}

fn read_with(
    host: &dyn Host,
    checkout: &Path,
    target: Option<&str>,
    dialects: &[dialect::Dialect],
    places: &ArtifactSource,
) -> SpecAnswer {
    let root = host
        .repository_root(checkout)
        .unwrap_or_else(|_| checkout.to_path_buf());
    let mut found = catalog::read(host, &root, dialects);
    let decisions = decision::read(host, &root, places);
    if !decisions.is_empty() {
        match &mut found {
            Found::Units { units, .. } => units.extend(decisions),
            Found::NoLayout => {
                found = Found::Units {
                    dialects: Vec::new(),
                    units: decisions,
                }
            }
        }
    }
    let mut subjects = Vec::new();
    if let Found::Units {
        dialects: names,
        units,
    } = &mut found
    {
        mark_own(host, &root, target, units);
        subjects = Subject::ALL
            .into_iter()
            .filter(|subject| match subject {
                Subject::Decisions => units.iter().any(|unit| unit.subject == *subject),
                _ => dialects
                    .iter()
                    .filter(|dialect| names.contains(&dialect.name))
                    .any(|dialect| dialect.collections.iter().any(|c| c.subject == *subject)),
            })
            .collect();
    }
    SpecAnswer {
        branch: checkout::branch_of(host, &root),
        theme: host.syntax_theme(),
        root,
        found,
        subjects,
    }
}

/// Marks the units this checkout touched as its own.
fn mark_own(host: &dyn Host, root: &Path, target: Option<&str>, units: &mut [Unit]) {
    let touched = ownership::touched(host, root, target);
    for unit in units {
        unit.own = touched
            .iter()
            .any(|path| ownership::lies_in(path, &unit.relative));
    }
}

/// Where a change stands, for the bands the changes are listed in.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Band {
    Own,
    InProgress,
    /// Finished, in a dialect that puts finished units away.
    Ready,
    /// Finished, in a dialect that leaves them where they are.
    Done,
}

impl Band {
    const ALL: [Band; 4] = [Band::Own, Band::InProgress, Band::Ready, Band::Done];

    fn label(self) -> &'static str {
        match self {
            Band::Own => "this checkout",
            Band::InProgress => "in progress",
            Band::Ready => "ready to archive",
            Band::Done => "done",
        }
    }

    fn of(unit: &Unit) -> Self {
        match unit.progress {
            _ if unit.own => Band::Own,
            Some(progress) if progress.complete() && unit.archives => Band::Ready,
            Some(progress) if progress.complete() => Band::Done,
            _ => Band::InProgress,
        }
    }
}

/// One row of the navigator, which is also what its `id` names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Row {
    /// A band, and how many units stand in it.
    Band(Band, usize),
    /// Air before every band but the first.
    Gap,
    Unit(usize),
    Artifact(usize, usize),
}

/// What is selected: a unit, or one of its artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Unit(usize),
    Artifact(usize, usize),
}

impl Target {
    fn unit(self) -> usize {
        match self {
            Target::Unit(unit) | Target::Artifact(unit, _) => unit,
        }
    }

    /// The artifact on show: a unit shows the first of its own.
    fn artifact(self) -> usize {
        match self {
            Target::Unit(_) => 0,
            Target::Artifact(_, artifact) => artifact,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Showing {
    Preview,
    Source,
}

const MODES: [(Showing, &str); 2] = [(Showing::Preview, "Preview"), (Showing::Source, "Source")];

enum State {
    Reading,
    NoLayout,
    Found {
        units: Vec<Unit>,
        subjects: Vec<Subject>,
        dialects: Vec<&'static str>,
    },
}

/// Where a viewer was on a checkout's spec surface, by name: the fourth
/// change is not the same change an hour later, and `add-the-spec-surface`
/// is — until it is archived, which is the one case where starting over
/// is right.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecPlace {
    subject: Subject,
    unit: String,
    artifact: Option<String>,
}

impl SpecPlace {
    /// A change in flight, by name — where a click on it anywhere else
    /// sends the surface.
    pub fn change(name: &str) -> Self {
        Self {
            subject: Subject::Changes,
            unit: name.to_owned(),
            artifact: None,
        }
    }
}

pub struct SpecView {
    display_root: String,
    branch: String,
    root: PathBuf,
    theme: String,
    state: State,
    subject: Subject,
    selected: Option<Target>,
    /// Units opened to show their artifacts.
    expanded: BTreeSet<usize>,
    /// Bands folded shut.
    folded: BTreeSet<Band>,
    showing: Showing,
    scroll: usize,
    /// The artifact on show, as lines, for the way it is being shown —
    /// made when the selection, the showing or the width changes, never
    /// per frame. Behind a cell because the width is only known when the
    /// view is drawn, and drawing is through a shared reference.
    lines: RefCell<Shown>,
    resuming: Option<SpecPlace>,
}

/// Lines, and the width a rendered document was laid out at.
#[derive(Default)]
struct Shown {
    width: Option<usize>,
    lines: Vec<ContentLine>,
}

impl SpecView {
    /// The surface before the checkout has been read. `display_root` names
    /// the checkout from the first frame, as the other surfaces do.
    pub fn opening(display_root: String) -> Self {
        Self {
            display_root,
            branch: String::new(),
            root: PathBuf::new(),
            theme: String::new(),
            state: State::Reading,
            subject: Subject::Changes,
            selected: None,
            expanded: BTreeSet::new(),
            folded: BTreeSet::new(),
            showing: Showing::Preview,
            scroll: 0,
            lines: RefCell::default(),
            resuming: None,
        }
    }

    /// Where to put the viewer once the answer lands.
    pub fn resuming(mut self, place: SpecPlace) -> Self {
        self.resuming = Some(place);
        self
    }

    /// The change in flight the viewer is on, by name — what the sidebar's
    /// tasks keep lit while the surface is open on it.
    pub fn change_on_show(&self) -> Option<String> {
        self.place()
            .filter(|place| place.subject == Subject::Changes)
            .map(|place| place.unit)
    }

    /// Where the viewer is, for the host to hand back next time.
    pub fn place(&self) -> Option<SpecPlace> {
        if let Some(place) = &self.resuming {
            return Some(place.clone());
        }
        let target = self.selected?;
        let unit = self.units().get(target.unit())?;
        Some(SpecPlace {
            subject: self.subject,
            unit: unit.name.clone(),
            artifact: match target {
                Target::Unit(_) => None,
                Target::Artifact(_, artifact) => unit
                    .artifacts
                    .get(artifact)
                    .map(|artifact| artifact.relative.clone()),
            },
        })
    }

    pub fn absorb(&mut self, answer: SpecAnswer) {
        self.branch = answer.branch;
        self.root = answer.root;
        self.theme = answer.theme;
        self.state = match answer.found {
            Found::NoLayout => State::NoLayout,
            Found::Units { units, dialects } => State::Found {
                units,
                subjects: answer.subjects,
                dialects,
            },
        };
        let place = self.resuming.take();
        if !place.is_some_and(|place| self.restore(&place)) {
            self.land();
        }
        // A tool that puts nothing away only ever adds to what is done, so
        // that band opens folded — unless the viewer is being put in it.
        let selected_band = self
            .selected
            .map(|target| Band::of(&self.units()[target.unit()]));
        if selected_band != Some(Band::Done) {
            self.folded.insert(Band::Done);
        }
        self.redraw();
    }

    fn restore(&mut self, place: &SpecPlace) -> bool {
        let Some(unit) = self
            .units()
            .iter()
            .position(|unit| unit.subject == place.subject && unit.name == place.unit)
        else {
            return false;
        };
        self.subject = place.subject;
        let artifact = place.artifact.as_ref().and_then(|relative| {
            self.units()[unit]
                .artifacts
                .iter()
                .position(|artifact| &artifact.relative == relative)
        });
        self.selected = Some(match artifact {
            Some(artifact) => {
                self.expanded.insert(unit);
                Target::Artifact(unit, artifact)
            }
            None => Target::Unit(unit),
        });
        true
    }

    /// Where a viewer with no place to return to lands: on the first unit
    /// this checkout is working on, opened on its first document — the
    /// reason the surface was opened in an agent's worktree — else on the
    /// first unit there is.
    fn land(&mut self) {
        let subjects = self.subjects();
        self.subject = if subjects.contains(&Subject::Changes) {
            Subject::Changes
        } else {
            subjects.first().copied().unwrap_or(Subject::Changes)
        };
        let listed = self.listed(self.subject);
        let own = listed.iter().copied().find(|&unit| self.units()[unit].own);
        self.selected = match own {
            Some(unit) if !self.units()[unit].artifacts.is_empty() => {
                self.expanded.insert(unit);
                Some(Target::Artifact(unit, 0))
            }
            _ => listed.first().map(|&unit| Target::Unit(unit)),
        };
    }

    fn units(&self) -> &[Unit] {
        match &self.state {
            State::Found { units, .. } => units,
            _ => &[],
        }
    }

    fn subjects(&self) -> Vec<Subject> {
        match &self.state {
            State::Found { subjects, .. } => subjects.clone(),
            _ => Vec::new(),
        }
    }

    /// Whether a unit's tool has to be named beside it, which it does only
    /// where more than one tool shares the list.
    fn mixed(&self) -> bool {
        matches!(&self.state, State::Found { dialects, .. } if dialects.len() > 1)
    }

    /// The units of `subject` in the order they are listed: this
    /// checkout's first, and changes by where they stand.
    fn listed(&self, subject: Subject) -> Vec<usize> {
        let mut listed: Vec<usize> = (0..self.units().len())
            .filter(|&unit| self.units()[unit].subject == subject)
            .collect();
        let key = |unit: usize| {
            let unit = &self.units()[unit];
            match subject {
                Subject::Changes => Band::of(unit),
                _ if unit.own => Band::Own,
                _ => Band::InProgress,
            }
        };
        // Stable, so each band keeps the order the dialect read it in.
        listed.sort_by_key(|&unit| key(unit));
        listed
    }

    fn banded(&self) -> bool {
        self.subject == Subject::Changes
    }

    fn rows(&self) -> Vec<Row> {
        let listed = self.listed(self.subject);
        let mut rows = Vec::new();
        let push_unit = |rows: &mut Vec<Row>, unit: usize| {
            rows.push(Row::Unit(unit));
            if self.expanded.contains(&unit) {
                rows.extend(
                    (0..self.units()[unit].artifacts.len())
                        .map(|artifact| Row::Artifact(unit, artifact)),
                );
            }
        };
        if !self.banded() {
            for unit in listed {
                push_unit(&mut rows, unit);
            }
            return rows;
        }
        for band in Band::ALL {
            let members: Vec<usize> = listed
                .iter()
                .copied()
                .filter(|&unit| Band::of(&self.units()[unit]) == band)
                .collect();
            if members.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(Row::Gap);
            }
            rows.push(Row::Band(band, members.len()));
            if !self.folded.contains(&band) {
                for unit in members {
                    push_unit(&mut rows, unit);
                }
            }
        }
        rows
    }

    fn selected_row(&self) -> Option<usize> {
        let selected = self.selected?;
        self.rows().iter().position(|row| match (row, selected) {
            (Row::Unit(unit), Target::Unit(wanted)) => *unit == wanted,
            (Row::Artifact(unit, artifact), Target::Artifact(u, a)) => (*unit, *artifact) == (u, a),
            _ => false,
        })
    }

    fn select(&mut self, target: Option<Target>) {
        if self.selected != target {
            self.selected = target;
            self.scroll = 0;
            self.redraw();
        }
    }

    fn step(&mut self, forward: bool) {
        let rows = self.rows();
        let selectable: Vec<Target> = rows
            .iter()
            .filter_map(|row| match *row {
                Row::Band(..) | Row::Gap => None,
                Row::Unit(unit) => Some(Target::Unit(unit)),
                Row::Artifact(unit, artifact) => Some(Target::Artifact(unit, artifact)),
            })
            .collect();
        let at = self
            .selected
            .and_then(|selected| selectable.iter().position(|target| *target == selected));
        let next = match at {
            None => selectable.first(),
            Some(at) if forward => selectable.get(at + 1),
            Some(at) => at.checked_sub(1).and_then(|at| selectable.get(at)),
        };
        if let Some(&next) = next {
            self.select(Some(next));
        }
    }

    fn switch_subject(&mut self, subject: Subject) {
        if subject == self.subject {
            return;
        }
        self.subject = subject;
        let first = self.listed(subject).first().map(|&unit| Target::Unit(unit));
        self.select(first);
    }

    fn next_subject(&mut self) {
        let subjects = self.subjects();
        if let Some(at) = subjects.iter().position(|subject| *subject == self.subject) {
            self.switch_subject(subjects[(at + 1) % subjects.len()]);
        }
    }

    fn show(&mut self, showing: Showing) {
        if self.showing != showing {
            self.showing = showing;
            self.scroll = 0;
            self.redraw();
        }
    }

    fn toggle_showing(&mut self) {
        self.show(match self.showing {
            Showing::Preview => Showing::Source,
            Showing::Source => Showing::Preview,
        });
    }

    fn on_show(&self) -> Option<(&Unit, &Artifact)> {
        let target = self.selected?;
        let unit = self.units().get(target.unit())?;
        Some((unit, unit.artifacts.get(target.artifact())?))
    }

    fn redraw(&mut self) {
        let width = self.lines.get_mut().width;
        self.lay_out(width);
    }

    /// The lines on show, laid out for `width` when one is given and a
    /// rendered document was laid out for another.
    fn shown(&self, width: Option<usize>) -> Ref<'_, Vec<ContentLine>> {
        if width.is_some() && self.lines.borrow().width != width && self.renders_markdown() {
            self.lay_out(width);
        }
        Ref::map(self.lines.borrow(), |shown| &shown.lines)
    }

    fn renders_markdown(&self) -> bool {
        self.showing == Showing::Preview
            && self
                .on_show()
                .is_some_and(|(_, artifact)| catalog::is_markdown(&artifact.path))
    }

    fn lay_out(&self, width: Option<usize>) {
        let lines = match self.on_show() {
            Some((unit, artifact)) => match &artifact.text {
                Ok(text) => match self.showing {
                    Showing::Preview if catalog::is_markdown(&artifact.path) => {
                        let text = with_standing(unit.standing.as_ref(), text);
                        markdown::render(&text, &self.theme, width.unwrap_or(usize::MAX))
                    }
                    _ => source_lines(text, &artifact.path, &self.theme),
                },
                Err(_) => Vec::new(),
            },
            None => Vec::new(),
        };
        *self.lines.borrow_mut() = Shown { width, lines };
    }

    fn expand(&mut self) {
        match self.selected {
            Some(Target::Unit(unit)) if self.expanded.contains(&unit) => {
                if !self.units()[unit].artifacts.is_empty() {
                    self.select(Some(Target::Artifact(unit, 0)));
                }
            }
            Some(Target::Unit(unit)) => {
                self.expanded.insert(unit);
            }
            _ => {}
        }
    }

    fn collapse(&mut self) {
        match self.selected {
            Some(Target::Artifact(unit, _)) => {
                self.expanded.remove(&unit);
                self.select(Some(Target::Unit(unit)));
            }
            Some(Target::Unit(unit)) => {
                self.expanded.remove(&unit);
            }
            None => {}
        }
    }

    fn toggle_unit(&mut self, unit: usize) {
        if !self.expanded.remove(&unit) {
            self.expanded.insert(unit);
        }
    }

    fn activate(&mut self) -> SpecOutcome {
        match self.selected {
            Some(Target::Unit(unit)) => {
                self.toggle_unit(unit);
                SpecOutcome::Stay
            }
            Some(Target::Artifact(..)) => match self.on_show() {
                Some((_, artifact)) => SpecOutcome::OpenPath {
                    project: self.root.clone(),
                    target: artifact.path.clone(),
                },
                None => SpecOutcome::Stay,
            },
            None => SpecOutcome::Stay,
        }
    }

    fn scroll_by(&mut self, rows: isize) {
        let last = self.shown(None).len().saturating_sub(1);
        self.scroll = self.scroll.saturating_add_signed(rows).min(last);
    }
}

fn source_lines(text: &str, path: &Path, theme: &str) -> Vec<ContentLine> {
    highlight::lines(text, path, theme, usize::MAX)
        .into_iter()
        .zip(text.lines())
        .enumerate()
        .map(|(index, (pieces, text))| ContentLine {
            gutter: " ".to_owned(),
            number: (index + 1).to_string(),
            tone: LineTone::Neutral,
            spans: if pieces.is_empty() {
                vec![Span::new(text.to_owned(), Tone::Default)]
            } else {
                pieces
                    .into_iter()
                    .map(|(colour, piece)| Span::new(piece, Tone::Default).coloured(colour))
                    .collect()
            },
        })
        .collect()
}

/// What the surface shows, for the room it has.
pub fn view(state: &SpecView, space: Size) -> View {
    let base = View {
        title: checkout::name(CATALOG.name),
        caption: checkout::caption(&state.display_root, &state.branch),
        navigator: None,
        content: Content::Message {
            text: String::new(),
            hint: None,
            role: Tone::Muted,
        },
        // A message has nothing to ask, the way the other surfaces' own
        // messages stand alone; what there is to ask arrives with the specs.
        footer: Vec::new(),
        notice: None,
        confirm: None,
        modes: Vec::new(),
        subjects: Vec::new(),
        layout: Layout::Sidebar,
        trail: Vec::new(),
    };
    match &state.state {
        State::Reading => View {
            content: message("Reading the checkout's specs", None),
            ..base
        },
        State::NoLayout => View {
            content: message("No specs found", Some(supported_tools(dialect::SHIPPED))),
            ..base
        },
        State::Found { .. } => View {
            navigator: Some(navigator(state)),
            content: content(state, space),
            footer: vec![
                Command::SelectNext,
                Command::Expand,
                Command::Activate,
                Command::TogglePreview,
                Command::FocusNext,
                Command::Close,
            ],
            modes: MODES
                .iter()
                .map(|(showing, label)| Mode {
                    label: (*label).to_owned(),
                    active: *showing == state.showing,
                    icon: RowIcon::None,
                })
                .collect(),
            subjects: state
                .subjects()
                .into_iter()
                .map(|subject| Mode {
                    label: subject.label().to_owned(),
                    active: subject == state.subject,
                    icon: match subject {
                        Subject::Changes => RowIcon::InFlight,
                        Subject::Specs => RowIcon::Contract,
                        Subject::Archive => RowIcon::Finished,
                        Subject::Decisions => RowIcon::Decision,
                    },
                })
                .collect(),
            ..base
        },
    }
}

/// The tools this surface reads, one to a line with the directory that
/// gives each away, padded to one width: the host centres each line, and
/// only lines of one width stay lined up as a list when it does.
fn supported_tools(dialects: &[dialect::Dialect]) -> String {
    let markers: Vec<String> = dialects
        .iter()
        .map(|dialect| format!("{}/", dialect.marker))
        .collect();
    let name_width = dialects
        .iter()
        .map(|dialect| dialect.name.chars().count())
        .max()
        .unwrap_or(0);
    let marker_width = markers
        .iter()
        .map(|marker| marker.chars().count())
        .max()
        .unwrap_or(0);
    let tools: Vec<String> = dialects
        .iter()
        .zip(&markers)
        .map(|(dialect, marker)| {
            format!("{:<name_width$}   {:<marker_width$}", dialect.name, marker)
        })
        .collect();
    format!(
        "Shows any project laid out by one of these:\n\n{}",
        tools.join("\n")
    )
}

fn message(text: &str, hint: Option<String>) -> Content {
    Content::Message {
        text: text.to_owned(),
        hint,
        role: Tone::Muted,
    }
}

fn navigator(state: &SpecView) -> Navigator {
    let rows = state.rows();
    let selected = state.selected_row();
    let depth = usize::from(state.banded());
    Navigator {
        heading: state.subject.label().to_owned(),
        badge: state.listed(state.subject).len().to_string(),
        focused: true,
        rows: rows
            .iter()
            .enumerate()
            .map(|(id, row)| match *row {
                Row::Band(band, count) => NavigatorRow::Band {
                    id,
                    name: band.label().to_owned(),
                    count,
                    collapsed: state.folded.contains(&band),
                },
                Row::Gap => NavigatorRow::Gap,
                Row::Unit(unit) => {
                    let unit_record = &state.units()[unit];
                    NavigatorRow::Item {
                        id,
                        name: unit_record.name.clone(),
                        depth,
                        marker: match &unit_record.standing {
                            Some(standing) => standing_marker(standing),
                            None => progress_marker(unit_record.progress),
                        },
                        marker_side: MarkerSide::Trailing,
                        detail: [
                            (unit_record.own && !state.banded()).then_some(Band::Own.label()),
                            state.mixed().then_some(unit_record.dialect),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · "),
                        selected: selected == Some(id),
                        icon: if state.expanded.contains(&unit) {
                            RowIcon::DirectoryOpen
                        } else {
                            RowIcon::Directory
                        },
                    }
                }
                Row::Artifact(unit, artifact) => {
                    let artifact = &state.units()[unit].artifacts[artifact];
                    NavigatorRow::Item {
                        id,
                        name: artifact.name.clone(),
                        depth: depth + 1,
                        marker: role_marker(artifact.role),
                        marker_side: MarkerSide::Trailing,
                        detail: String::new(),
                        selected: selected == Some(id),
                        icon: RowIcon::Markup,
                    }
                }
            })
            .collect(),
        anchor: selected,
        choosing: None,
        menu: None,
    }
}

/// A decision's status as written, unless a later one took its place.
fn standing_marker(standing: &Standing) -> Span {
    if standing.superseded {
        Span::new("superseded", Tone::Faint)
    } else {
        Span::new(
            standing
                .status
                .as_deref()
                .unwrap_or_default()
                .to_lowercase(),
            Tone::Muted,
        )
    }
}

/// A decision's relations, said above its own text: the record names what
/// it replaced, and nothing in it can name what replaced it.
fn with_standing(standing: Option<&Standing>, text: &str) -> String {
    match standing {
        Some(standing) if !standing.notes.is_empty() => {
            let notes: Vec<String> = standing
                .notes
                .iter()
                .map(|note| format!("> {note}"))
                .collect();
            format!("{}\n\n{text}", notes.join("\n>\n"))
        }
        _ => text.to_owned(),
    }
}

fn progress_marker(progress: Option<Progress>) -> Span {
    match progress {
        Some(progress) => Span::new(
            format!("{}/{}", progress.done, progress.total),
            if progress.complete() {
                Tone::Success
            } else {
                Tone::Muted
            },
        ),
        None => Span::new(String::new(), Tone::Muted),
    }
}

/// The role, where the name alone does not say it: a contract is named by
/// its capability and a stray file by its path, and neither name says what
/// the file is for.
fn role_marker(role: Role) -> Span {
    match role {
        Role::Contract => Span::new("contract", Tone::Faint),
        Role::Decision => Span::new("decision", Tone::Faint),
        Role::Other => Span::new("other", Tone::Faint),
        Role::Why | Role::How | Role::Steps => Span::new(String::new(), Tone::Faint),
    }
}

/// Where a finished change went, told only as far as the checkout's
/// tools have somewhere for it to go.
fn where_finished(subjects: &[Subject]) -> Option<String> {
    if !subjects.contains(&Subject::Archive) {
        return None;
    }
    Some(
        if subjects.contains(&Subject::Specs) {
            "Finished changes are under Archive, and what they left under Specs."
        } else {
            "Finished changes are under Archive."
        }
        .to_owned(),
    )
}

fn content(state: &SpecView, space: Size) -> Content {
    if state.listed(state.subject).is_empty() {
        return match state.subject {
            Subject::Changes => message("Nothing in flight", where_finished(&state.subjects())),
            Subject::Specs => message("No specs yet", None),
            Subject::Archive => message("Nothing archived yet", None),
            Subject::Decisions => message("No decisions yet", None),
        };
    }
    let Some((unit, artifact)) = state.on_show() else {
        return match state.selected {
            Some(target) if state.units()[target.unit()].artifacts.is_empty() => {
                message("This unit holds no documents", None)
            }
            _ => message(
                "Nothing selected",
                Some("Pick a change on the left.".to_owned()),
            ),
        };
    };
    if let Err(reason) = &artifact.text {
        return Content::Message {
            text: reason.clone(),
            hint: None,
            role: Tone::Danger,
        };
    }
    let window = usize::from(space.height).saturating_mul(2);
    let shown = state.shown(Some(crate::view::prose_width(space)));
    let first = state.scroll.min(shown.len().saturating_sub(1));
    // A unit that is one file is named by where it lives; its name alone
    // repeats the file's.
    let heading = if state.root.join(&unit.relative) == artifact.path {
        unit.relative.clone()
    } else {
        format!("{}/{}", unit.name, artifact.relative)
    };
    Content::Lines {
        heading,
        scroll: u16::try_from(first).unwrap_or(u16::MAX),
        first,
        lines: shown.iter().skip(first).take(window).cloned().collect(),
        total: shown.len(),
        caret: None,
        medium: Medium::Text,
    }
}

/// Answers a command the host resolved from a key.
pub fn handle_command(state: &mut SpecView, command: Command, space: Size) -> SpecOutcome {
    let page = isize::try_from(space.height)
        .unwrap_or(isize::MAX)
        .saturating_sub(1)
        .max(1);
    match command {
        Command::Close => return SpecOutcome::Close,
        Command::SelectNext => state.step(true),
        Command::SelectPrevious => state.step(false),
        Command::Expand => state.expand(),
        Command::Collapse => state.collapse(),
        Command::Activate => return state.activate(),
        Command::TogglePreview | Command::NextMode => state.toggle_showing(),
        Command::FocusNext => state.next_subject(),
        Command::ScrollPageDown => state.scroll_by(page),
        Command::ScrollPageUp => state.scroll_by(-page),
        _ => {}
    }
    SpecOutcome::Stay
}

/// Answers a click the host resolved to one of this surface's hits.
pub fn handle_mouse(state: &mut SpecView, hit: Option<ViewHit>, _space: Size) -> SpecOutcome {
    match hit {
        Some(ViewHit::SelectItem(id)) => match state.rows().get(id).copied() {
            Some(Row::Unit(unit)) => {
                state.select(Some(Target::Unit(unit)));
                state.toggle_unit(unit);
            }
            Some(Row::Artifact(unit, artifact)) => {
                state.select(Some(Target::Artifact(unit, artifact)));
            }
            Some(Row::Band(..) | Row::Gap) | None => {}
        },
        Some(ViewHit::ToggleGroup(id)) => {
            if let Some(Row::Band(band, _)) = state.rows().get(id).copied()
                && !state.folded.remove(&band)
            {
                state.folded.insert(band);
            }
        }
        Some(ViewHit::SelectMode(index)) => {
            if let Some((showing, _)) = MODES.get(index) {
                state.show(*showing);
            }
        }
        Some(ViewHit::SelectSubject(index)) => {
            if let Some(subject) = state.subjects().get(index).copied() {
                state.switch_subject(subject);
            }
        }
        Some(ViewHit::Close) => return SpecOutcome::Close,
        _ => {}
    }
    SpecOutcome::Stay
}

/// The wheel over the content.
pub fn handle_scroll(state: &mut SpecView, direction: ScrollDirection) {
    state.scroll_by(match direction {
        ScrollDirection::Up => -3,
        ScrollDirection::Down => 3,
    });
}

/// Shows the content from line `first`, as a point on its scrollbar asks.
pub fn scroll_to(state: &mut SpecView, first: usize) {
    let last = state.shown(None).len().saturating_sub(1);
    state.scroll = first.min(last);
}

/// The text of `lines` of the document on show, as it reads rendered —
/// what the host copies when a reader marked them. A range past the end
/// gives back only the lines there are.
pub fn text(state: &SpecView, lines: std::ops::Range<usize>) -> Vec<String> {
    let count = lines.len();
    state
        .shown(None)
        .iter()
        .skip(lines.start)
        .take(count)
        .map(ContentLine::text)
        .collect()
}
