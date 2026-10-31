//! The Overview screen's state: what the machine carries, as last
//! collected, and where the keyboard is in it.
//!
//! Collection is asked for only while the Overview is the screen in front,
//! once it has stood there a moment, and is cancelled the moment it is not:
//! another screen, or the modal closed. Every answer carries the question
//! it answers, and one for a collection no longer wanted is dropped.

use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use uze_application::machine_overview::{
    MachineOverview, Origin, OverviewResource, PERIODS, ResourceKind, Status,
};

use super::model::ListScreen;

/// How long a collection stands: opening the Overview inside it asks the
/// machine nothing again.
pub(crate) const STANDS_FOR: Duration = Duration::from_secs(60);

/// How long the Overview stands in front before collecting: passing over
/// it on the way to another screen asks nothing.
pub(crate) const SETTLE: Duration = Duration::from_millis(300);

/// One group of the tree: uze's own findings, or one kind of resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    Uze,
    Kind(ResourceKind),
}

impl Group {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Uze => "uze",
            Self::Kind(ResourceKind::Skill) => "skills",
            Self::Kind(ResourceKind::Agent) => "agents",
            Self::Kind(ResourceKind::Mcp) => "mcp servers",
            Self::Kind(ResourceKind::Hook) => "hooks",
            Self::Kind(ResourceKind::Rule) => "rules",
        }
    }
}

/// The word for one resource of `kind`.
pub(crate) fn kind_word(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Skill => "skill",
        ResourceKind::Agent => "agent",
        ResourceKind::Mcp => "mcp server",
        ResourceKind::Hook => "hook",
        ResourceKind::Rule => "rule",
    }
}

/// One line of the tree the keyboard can stand on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    Group(Group),
    /// One of uze's own findings, by position in the health alerts.
    Finding(usize),
    /// A resource, by position in the Overview's resources.
    Resource(usize),
}

/// What the keyboard is on: the tree, or one of the header's two fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Field {
    #[default]
    List,
    Harness,
    Period,
}

impl Field {
    pub(crate) fn next(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::List, true) | (Self::Period, false) => Self::Harness,
            (Self::Harness, true) | (Self::List, false) => Self::Period,
            (Self::Period, true) | (Self::Harness, false) => Self::List,
        }
    }
}

/// A collection's cancel switch, carried by the intent that starts it.
/// Two are the same switch when they are the same allocation.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cancel(pub(crate) Arc<AtomicBool>);

impl PartialEq for Cancel {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Cancel {}

#[derive(Default)]
pub(crate) struct MachineScreen {
    pub(crate) overview: Option<MachineOverview>,
    pub(crate) collected_at: Option<SystemTime>,
    /// The collection in flight, by its id.
    pub(crate) collecting: Option<u64>,
    cancel: Cancel,
    /// The one harness the screen is narrowed to, or every one.
    pub(crate) harness: Option<String>,
    /// A position in [`PERIODS`].
    pub(crate) period: usize,
    pub(crate) open: BTreeSet<Group>,
    pub(crate) uze_folded: bool,
    pub(crate) list: ListScreen,
    pub(crate) field: Field,
    in_front_since: Option<Instant>,
}

impl MachineScreen {
    pub(crate) fn period_days(&self) -> u32 {
        PERIODS[self.period.min(PERIODS.len() - 1)]
    }

    /// Asks for a collection when the Overview has stood in front long
    /// enough and the last one no longer stands; `force` asks regardless.
    pub(crate) fn wanted(&mut self, now: Instant, force: bool) -> Option<(u64, Cancel, bool)> {
        let since = *self.in_front_since.get_or_insert(now);
        if self.collecting.is_some() && !force {
            return None;
        }
        let stale = self
            .collected_at
            .is_none_or(|at| at.elapsed().map_or(true, |age| age >= STANDS_FOR));
        if !force && (now.duration_since(since) < SETTLE || !stale) {
            return None;
        }
        self.stop();
        static ISSUED: AtomicU64 = AtomicU64::new(1);
        let id = ISSUED.fetch_add(1, Ordering::Relaxed);
        self.cancel = Cancel(Arc::new(AtomicBool::new(false)));
        self.collecting = Some(id);
        Some((id, self.cancel.clone(), force))
    }

    /// The Overview is no longer in front: whatever it asked stops.
    pub(crate) fn leave(&mut self) {
        self.stop();
        self.in_front_since = None;
    }

    fn stop(&mut self) {
        if self.collecting.take().is_some() {
            self.cancel.0.store(true, Ordering::Relaxed);
        }
    }

    /// An answer: drawn when it is for the collection in flight, dropped
    /// otherwise.
    pub(crate) fn absorb(
        &mut self,
        id: u64,
        overview: Option<MachineOverview>,
        collected_at: Option<SystemTime>,
        done: bool,
    ) -> bool {
        if self.collecting != Some(id) {
            return false;
        }
        if let Some(overview) = overview {
            if let Some(harness) = &self.harness
                && !overview.harnesses.iter().any(|known| &known.id == harness)
            {
                self.harness = None;
            }
            self.overview = Some(overview);
        }
        if collected_at.is_some() {
            self.collected_at = collected_at;
        }
        if done {
            self.collecting = None;
        }
        true
    }

    /// Whether `resource` is on screen with the harness and filter chosen.
    fn shows(&self, resource: &OverviewResource) -> bool {
        if let Some(harness) = &self.harness
            && !resource.reaches(harness)
        {
            return false;
        }
        let filter = self.list.filter.trim().to_lowercase();
        filter.is_empty()
            || resource.name.to_lowercase().contains(&filter)
            || origin_word(&resource.origin)
                .to_lowercase()
                .contains(&filter)
            || status_word(&resource.status).starts_with(&filter)
    }

    pub(crate) fn filtering(&self) -> bool {
        !self.list.filter.trim().is_empty()
    }

    pub(crate) fn is_open(&self, group: Group) -> bool {
        match group {
            Group::Uze => !self.uze_folded,
            Group::Kind(_) => self.filtering() || self.open.contains(&group),
        }
    }

    pub(crate) fn set_open(&mut self, group: Group, open: bool) {
        match group {
            Group::Uze => self.uze_folded = !open,
            Group::Kind(_) if open => {
                self.open.insert(group);
            }
            Group::Kind(_) => {
                self.open.remove(&group);
            }
        }
    }

    /// The resources of `kind` on screen, as positions in the Overview's.
    pub(crate) fn members(&self, kind: ResourceKind) -> Vec<usize> {
        self.overview.as_ref().map_or_else(Vec::new, |overview| {
            overview
                .resources
                .iter()
                .enumerate()
                .filter(|(_, resource)| resource.kind == kind && self.shows(resource))
                .map(|(index, _)| index)
                .collect()
        })
    }

    /// Every line of the tree, top to bottom. `findings` is how many of
    /// uze's own there are; they head the tree while there are any and no
    /// search is on.
    pub(crate) fn lines(&self, findings: usize) -> Vec<Line> {
        let mut lines = Vec::new();
        if findings > 0 && !self.filtering() {
            lines.push(Line::Group(Group::Uze));
            if self.is_open(Group::Uze) {
                lines.extend((0..findings).map(Line::Finding));
            }
        }
        for kind in ResourceKind::ALL {
            let members = self.members(kind);
            if members.is_empty() {
                continue;
            }
            let group = Group::Kind(kind);
            lines.push(Line::Group(group));
            if self.is_open(group) {
                lines.extend(members.into_iter().map(Line::Resource));
            }
        }
        lines
    }

    pub(crate) fn resource(&self, index: usize) -> Option<&OverviewResource> {
        self.overview.as_ref()?.resources.get(index)
    }

    /// The group a line belongs to.
    pub(crate) fn group_of(&self, line: Line) -> Option<Group> {
        match line {
            Line::Group(group) => Some(group),
            Line::Finding(_) => Some(Group::Uze),
            Line::Resource(index) => self.resource(index).map(|r| Group::Kind(r.kind)),
        }
    }

    /// Moves the harness or the period field one step.
    pub(crate) fn turn(&mut self, field: Field, forward: bool) {
        match field {
            Field::List => {}
            Field::Harness => {
                let ids: Vec<Option<String>> = std::iter::once(None)
                    .chain(
                        self.overview
                            .iter()
                            .flat_map(|overview| overview.harnesses.iter())
                            .map(|harness| Some(harness.id.clone())),
                    )
                    .collect();
                let at = ids.iter().position(|id| *id == self.harness).unwrap_or(0);
                let next = if forward {
                    (at + 1) % ids.len()
                } else {
                    (at + ids.len() - 1) % ids.len()
                };
                self.harness = ids[next].clone();
                self.list.selected = 0;
            }
            Field::Period => {
                self.period = if forward {
                    (self.period + 1) % PERIODS.len()
                } else {
                    (self.period + PERIODS.len() - 1) % PERIODS.len()
                };
            }
        }
    }

    /// The name of the harness chosen, or "every harness".
    pub(crate) fn harness_label(&self) -> String {
        match &self.harness {
            None => "every harness".to_owned(),
            Some(id) => self
                .overview
                .as_ref()
                .and_then(|overview| overview.harnesses.iter().find(|h| &h.id == id))
                .map_or_else(|| id.clone(), |harness| harness.name.clone()),
        }
    }

    pub(crate) fn harness_name(&self, id: &str) -> String {
        self.overview
            .as_ref()
            .and_then(|overview| overview.harnesses.iter().find(|h| h.id == id))
            .map_or_else(|| id.to_owned(), |harness| harness.name.clone())
    }
}

/// Who manages a resource, in the word the list shows.
pub(crate) fn origin_word(origin: &Origin) -> String {
    match origin {
        Origin::Uze { .. } => "uze".to_owned(),
        Origin::Plugin { name } => name.clone(),
        Origin::BuiltIn => "built-in".to_owned(),
        Origin::You => "you".to_owned(),
    }
}

pub(crate) fn status_word(status: &Status) -> &'static str {
    match status {
        Status::Broken { .. } => "broken",
        Status::Duplicate { .. } => "duplicate",
        Status::Ok => "ok",
    }
}

/// A token figure as the screen prints it: always an estimate.
pub(crate) fn tokens(count: u32) -> String {
    if count < 1000 {
        format!("~{count}")
    } else {
        let thousands = f64::from(count) / 1000.0;
        let text = format!("{thousands:.1}");
        format!("~{}k", text.trim_end_matches(".0"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_is_always_an_estimate() {
        assert_eq!(tokens(140), "~140");
        assert_eq!(tokens(1000), "~1k");
        assert_eq!(tokens(4620), "~4.6k");
    }

    #[test]
    fn nothing_is_collected_before_the_screen_settles_or_while_one_stands() {
        let mut screen = MachineScreen::default();
        let start = Instant::now();
        assert!(screen.wanted(start, false).is_none(), "just arrived");
        assert!(screen.wanted(start + SETTLE, false).is_some(), "settled");
        assert!(
            screen.wanted(start + SETTLE * 2, false).is_none(),
            "one is in flight"
        );
        let id = screen.collecting.unwrap();
        assert!(screen.absorb(id, None, Some(SystemTime::now()), true));
        assert!(
            screen.wanted(start + SETTLE * 3, false).is_none(),
            "the last one still stands"
        );
        assert!(
            screen.wanted(start + SETTLE * 3, true).is_some(),
            "f5 asks again"
        );
    }

    #[test]
    fn leaving_cancels_and_a_late_answer_is_dropped() {
        let mut screen = MachineScreen::default();
        let start = Instant::now();
        screen.wanted(start, false);
        let (id, cancel, _) = screen.wanted(start + SETTLE, false).unwrap();
        screen.leave();
        assert!(cancel.0.load(Ordering::Relaxed));
        assert!(!screen.absorb(id, Some(MachineOverview::default()), None, true));
        assert!(screen.overview.is_none());
    }
}

// --- Gestures -----------------------------------------------------------

use super::model::{Focus, Overlay, Route, TuiModel};
use super::worker::Intent;

impl TuiModel {
    /// Moves the Overview's cursor by `delta` lines, back onto the tree.
    pub(crate) fn machine_move(&mut self, delta: isize) -> Intent {
        let len = self.machine_lines().len();
        let screen = &mut self.remembered.machine;
        screen.field = Field::List;
        screen.list.selected = screen
            .list
            .selected
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        Intent::None
    }

    /// `→`: a field steps forward; a folded group unfolds; an unfolded one
    /// steps onto its first line.
    pub(crate) fn machine_step_in(&mut self) -> Intent {
        let field = self.remembered.machine.field;
        if field != Field::List {
            self.remembered.machine.turn(field, true);
            return Intent::None;
        }
        if let Some(Line::Group(group)) = self.machine_line() {
            if self.remembered.machine.is_open(group) {
                return self.machine_move(1);
            }
            self.remembered.machine.set_open(group, true);
        }
        Intent::None
    }

    /// `←`: a field steps back; a line goes to its group; a group folds.
    pub(crate) fn machine_step_back(&mut self) -> Intent {
        let field = self.remembered.machine.field;
        if field != Field::List {
            self.remembered.machine.turn(field, false);
            return Intent::None;
        }
        let Some(line) = self.machine_line() else {
            return Intent::None;
        };
        match line {
            Line::Group(group) => self.remembered.machine.set_open(group, false),
            line => {
                let group = self.remembered.machine.group_of(line);
                if let Some(at) = self
                    .machine_lines()
                    .iter()
                    .position(|candidate| Some(*candidate) == group.map(Line::Group))
                {
                    self.remembered.machine.list.selected = at;
                }
            }
        }
        Intent::None
    }

    /// `tab`: the keyboard moves from the tree to the harness field, to the
    /// period field, and on to the screens' strip, as it does from every
    /// other screen; `shift+tab` walks back.
    pub(crate) fn machine_next_field(&mut self, forward: bool) -> Intent {
        let screen = &mut self.remembered.machine;
        let leaves = matches!(
            (screen.field, forward),
            (Field::Period, true) | (Field::List, false)
        );
        if leaves {
            screen.field = Field::List;
            self.focus = Focus::Sidebar;
        } else {
            screen.field = screen.field.next(forward);
        }
        Intent::None
    }

    /// What Enter does on the line the keyboard is on.
    pub(crate) fn machine_activate(&mut self) -> Intent {
        match self.machine_line() {
            Some(Line::Group(group)) => {
                let open = self.remembered.machine.is_open(group);
                self.remembered.machine.set_open(group, !open);
                Intent::None
            }
            Some(Line::Finding(_)) => {
                self.overlay = Overlay::Health;
                Intent::None
            }
            Some(Line::Resource(index)) => match self.machine_offer(index) {
                Some(MachineOffer::ShowPackage(package)) => self.show_package(&package),
                Some(MachineOffer::TypeRemoval(command)) => Intent::TypeInShell(command),
                None => Intent::None,
            },
            None => Intent::None,
        }
    }

    /// What Enter does, in the footer's word, or `None` where it does nothing.
    pub(crate) fn machine_activate_label(&self) -> Option<String> {
        match self.machine_line()? {
            Line::Group(group) if self.remembered.machine.filtering() => {
                let _ = group;
                None
            }
            Line::Group(group) => Some(
                if self.remembered.machine.is_open(group) {
                    "fold"
                } else {
                    "unfold"
                }
                .to_owned(),
            ),
            Line::Finding(_) => Some("details".to_owned()),
            Line::Resource(index) => match self.machine_offer(index)? {
                MachineOffer::ShowPackage(_) => Some("show plugin".to_owned()),
                MachineOffer::TypeRemoval(_) => Some("remove".to_owned()),
            },
        }
    }

    /// The one thing the Overview offers for a resource: the plugin uze
    /// installed it with, or the harness's own command that removes it,
    /// typed into a shell and never run.
    pub(crate) fn machine_offer(&self, index: usize) -> Option<MachineOffer> {
        let resource = self.remembered.machine.resource(index)?;
        match &resource.origin {
            Origin::Uze { package } if package.contains('@') => {
                Some(MachineOffer::ShowPackage(package.clone()))
            }
            Origin::Uze { .. } => None,
            _ => resource.removal.clone().map(MachineOffer::TypeRemoval),
        }
    }

    /// The Marketplace, on the installed package `id` (`name@market`).
    pub(crate) fn show_package(&mut self, id: &str) -> Intent {
        self.remembered.plugin_screen.filter.clear();
        let _ = self.set_route(Route::Plugins);
        self.focus = Focus::Content;
        let rows = self.marketplace_rows();
        let Some(position) = self
            .visible_indices_in(&rows)
            .iter()
            .position(|&raw| self.marketplace_plugin_id(&rows[raw]) == id)
        else {
            return Intent::None;
        };
        self.select_plugin_row(position, None);
        self.marketplace_inspect_intent()
    }

    /// A click on a line of the tree: it is chosen, and a group folds or
    /// unfolds at once — the first click acts.
    pub(crate) fn machine_click(&mut self, position: usize) -> Intent {
        self.focus = Focus::Content;
        self.remembered.machine.field = Field::List;
        self.remembered.machine.list.selected = position;
        if let Some(Line::Group(group)) = self.machine_line() {
            let open = self.remembered.machine.is_open(group);
            self.remembered.machine.set_open(group, !open);
        }
        Intent::None
    }

    /// A click on one of a field's arrows.
    pub(crate) fn machine_turn(&mut self, field: Field, forward: bool) -> Intent {
        self.remembered.machine.field = field;
        self.remembered.machine.turn(field, forward);
        Intent::None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MachineOffer {
    ShowPackage(String),
    TypeRemoval(String),
}

impl MachineScreen {
    /// Where `resource` sits in the Overview's resources.
    pub(crate) fn resource_index(&self, resource: &OverviewResource) -> Option<usize> {
        self.overview
            .as_ref()?
            .resources
            .iter()
            .position(|candidate| candidate.id == resource.id)
    }
}
