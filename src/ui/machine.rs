//! The Overview screen's state: what the machine carries, as last
//! collected, and where the keyboard is in it.
//!
//! Collection is asked for only while the Overview is the screen in front,
//! once it has stood there a moment, and is cancelled the moment it is not:
//! another screen, or the modal closed. Every answer carries the question
//! it answers, and one for a collection no longer wanted is dropped.

use std::{
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

/// One row of the table the keyboard can stand on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// One of uze's own findings, by position in the health alerts.
    Finding(usize),
    /// A resource, by position in the Overview's resources.
    Resource(usize),
}

/// Which half of the screen the keyboard is in: the menu of sections, or
/// the table of the section chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pane {
    #[default]
    Menu,
    Table,
}

/// What the keyboard is on: the menu and table, one of the header's two
/// fields, or the table's column headings, which order it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Field {
    #[default]
    List,
    Harness,
    Period,
    Columns,
}

impl Field {
    const ORDER: [Field; 4] = [Self::List, Self::Harness, Self::Period, Self::Columns];

    pub(crate) fn next(self, forward: bool) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        let len = Self::ORDER.len();
        Self::ORDER[if forward {
            (at + 1) % len
        } else {
            (at + len - 1) % len
        }]
    }
}

/// A column of the table, which the table can be ordered by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Column {
    Name,
    From,
    PerSession,
    Uses,
    #[default]
    Status,
}

impl Column {
    pub(crate) const ALL: [Column; 5] = [
        Self::Name,
        Self::From,
        Self::PerSession,
        Self::Uses,
        Self::Status,
    ];

    /// The way a column is first read: names and origins from A, figures
    /// from the heaviest, status from what needs looking at.
    pub(crate) fn natural_ascending(self) -> bool {
        !matches!(self, Self::PerSession | Self::Uses)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::From => "from",
            Self::PerSession => "per session",
            Self::Uses => "uses",
            Self::Status => "status",
        }
    }
}

/// How the table is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sort {
    pub(crate) column: Column,
    pub(crate) ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            column: Column::Status,
            ascending: true,
        }
    }
}

impl Sort {
    /// A click on `column`'s heading: the same column turns around, another
    /// is read its natural way.
    pub(crate) fn by(self, column: Column) -> Self {
        if column == self.column {
            Self {
                column,
                ascending: !self.ascending,
            }
        } else {
            Self {
                column,
                ascending: column.natural_ascending(),
            }
        }
    }

    fn order(self, a: &OverviewResource, b: &OverviewResource, days: u32) -> std::cmp::Ordering {
        let primary = match self.column {
            Column::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Column::From => origin_word(&a.origin).cmp(&origin_word(&b.origin)),
            Column::PerSession => a.per_session.cmp(&b.per_session),
            Column::Uses => uses_within(a, days).cmp(&uses_within(b, days)),
            Column::Status => status_rank(a, days).cmp(&status_rank(b, days)),
        };
        let primary = if self.ascending {
            primary
        } else {
            primary.reverse()
        };
        primary
            .then_with(|| status_rank(a, days).cmp(&status_rank(b, days)))
            .then_with(|| b.per_session.cmp(&a.per_session))
            .then_with(|| a.name.cmp(&b.name))
    }
}

/// What needs looking at first: broken, duplicate, unused in the last
/// `days`, checked, unjudged.
fn status_rank(resource: &OverviewResource, days: u32) -> u8 {
    match resource.status {
        Status::Broken { .. } => 0,
        Status::Duplicate { .. } => 1,
        Status::Ok if resource.unused(days) => 2,
        Status::Ok if resource.checked => 3,
        Status::Ok => 4,
    }
}

fn uses_within(resource: &OverviewResource, days: u32) -> Option<u64> {
    resource.uses.map(|uses| uses.within(days))
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
    /// The menu's choice; `None` is the first section on offer.
    pub(crate) section: Option<Group>,
    pub(crate) pane: Pane,
    pub(crate) sort: Sort,
    /// The column the keyboard is on while it is on the headings.
    pub(crate) column: Column,
    /// The table's search and the row the keyboard is on.
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

    /// The menu: uze's own findings while there are any, then every kind,
    /// whether or not it has anything, so the menu never moves under the
    /// keyboard.
    pub(crate) fn sections(&self, findings: usize) -> Vec<Group> {
        let uze = (findings > 0).then_some(Group::Uze);
        uze.into_iter()
            .chain(ResourceKind::ALL.into_iter().map(Group::Kind))
            .collect()
    }

    /// The section the table shows.
    pub(crate) fn section(&self, findings: usize) -> Group {
        let sections = self.sections(findings);
        self.section
            .filter(|section| sections.contains(section))
            .unwrap_or(sections[0])
    }

    /// The resources of `kind` on screen, as positions in the Overview's.
    pub(crate) fn members(&self, kind: ResourceKind) -> Vec<usize> {
        let Some(overview) = self.overview.as_ref() else {
            return Vec::new();
        };
        let days = self.period_days();
        let mut members: Vec<usize> = overview
            .resources
            .iter()
            .enumerate()
            .filter(|(_, resource)| resource.kind == kind && self.shows(resource))
            .map(|(index, _)| index)
            .collect();
        members.sort_by(|a, b| {
            self.sort
                .order(&overview.resources[*a], &overview.resources[*b], days)
        });
        members
    }

    /// How many resources of `kind` there are with the harness chosen,
    /// whatever is typed in the search.
    pub(crate) fn count(&self, kind: ResourceKind) -> (usize, usize, usize) {
        let resources: Vec<&OverviewResource> = self
            .overview
            .iter()
            .flat_map(|overview| overview.resources.iter())
            .filter(|resource| resource.kind == kind)
            .filter(|resource| self.harness.as_ref().is_none_or(|h| resource.reaches(h)))
            .collect();
        let broken = resources
            .iter()
            .filter(|r| matches!(r.status, Status::Broken { .. }))
            .count();
        let duplicates = resources
            .iter()
            .filter(|r| matches!(r.status, Status::Duplicate { .. }))
            .count();
        (resources.len(), broken, duplicates)
    }

    /// Every row of the table, top to bottom, for the section chosen.
    pub(crate) fn lines(&self, findings: usize) -> Vec<Line> {
        match self.section(findings) {
            Group::Uze => (0..findings).map(Line::Finding).collect(),
            Group::Kind(kind) => self.members(kind).into_iter().map(Line::Resource).collect(),
        }
    }

    pub(crate) fn resource(&self, index: usize) -> Option<&OverviewResource> {
        self.overview.as_ref()?.resources.get(index)
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
            Field::Columns => {
                let at = Column::ALL
                    .iter()
                    .position(|column| *column == self.column)
                    .unwrap_or(0);
                let len = Column::ALL.len();
                self.column = Column::ALL[if forward {
                    (at + 1) % len
                } else {
                    (at + len - 1) % len
                }];
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
    /// The sections of the Overview's menu, as it is drawn now.
    pub(crate) fn machine_sections(&self) -> Vec<Group> {
        self.remembered.machine.sections(self.alerts().len())
    }

    pub(crate) fn machine_section(&self) -> Group {
        self.remembered.machine.section(self.alerts().len())
    }

    /// `↑↓`: the menu changes section, the table moves its row.
    pub(crate) fn machine_move(&mut self, delta: isize) -> Intent {
        self.remembered.machine.field = Field::List;
        match self.remembered.machine.pane {
            Pane::Menu => {
                let sections = self.machine_sections();
                let at = sections
                    .iter()
                    .position(|section| *section == self.machine_section())
                    .unwrap_or(0);
                let next = at
                    .saturating_add_signed(delta)
                    .min(sections.len().saturating_sub(1));
                self.remembered.machine.section = Some(sections[next]);
                self.remembered.machine.list.selected = 0;
            }
            Pane::Table => {
                let len = self.machine_lines().len();
                let list = &mut self.remembered.machine.list;
                list.selected = list
                    .selected
                    .saturating_add_signed(delta)
                    .min(len.saturating_sub(1));
            }
        }
        Intent::None
    }

    /// `→`: a field steps forward; the menu hands the keys to its table.
    pub(crate) fn machine_step_in(&mut self) -> Intent {
        let field = self.remembered.machine.field;
        if field != Field::List {
            self.remembered.machine.turn(field, true);
        } else if !self.machine_lines().is_empty() {
            self.remembered.machine.pane = Pane::Table;
        }
        Intent::None
    }

    /// `←`: a field steps back; the table gives the keys back to the menu.
    pub(crate) fn machine_step_back(&mut self) -> Intent {
        let field = self.remembered.machine.field;
        if field != Field::List {
            self.remembered.machine.turn(field, false);
        } else {
            self.remembered.machine.pane = Pane::Menu;
        }
        Intent::None
    }

    /// `tab`: the keyboard moves from the menu and table to the harness
    /// field, the period field, the table's headings, and on to the
    /// screens' strip, as it does from every other screen; `shift+tab`
    /// walks back.
    pub(crate) fn machine_next_field(&mut self, forward: bool) -> Intent {
        let screen = &mut self.remembered.machine;
        let leaves = matches!(
            (screen.field, forward),
            (Field::Columns, true) | (Field::List, false)
        );
        if leaves {
            screen.field = Field::List;
            self.focus = Focus::Sidebar;
        } else {
            screen.field = screen.field.next(forward);
            if screen.field == Field::Columns {
                screen.column = screen.sort.column;
            }
        }
        Intent::None
    }

    /// What Enter does where the keyboard is.
    pub(crate) fn machine_activate(&mut self) -> Intent {
        if self.remembered.machine.field == Field::Columns {
            let screen = &mut self.remembered.machine;
            screen.sort = screen.sort.by(screen.column);
            return Intent::None;
        }
        if self.remembered.machine.field != Field::List {
            return Intent::None;
        }
        if self.remembered.machine.pane == Pane::Menu {
            return self.machine_step_in();
        }
        match self.machine_line() {
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
        match self.remembered.machine.field {
            Field::Columns => return Some("reverse".to_owned()),
            Field::Harness | Field::Period => return None,
            Field::List => {}
        }
        if self.remembered.machine.pane == Pane::Menu {
            return (!self.machine_lines().is_empty()).then(|| "open".to_owned());
        }
        match self.machine_line()? {
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

    /// A click on a section of the menu: it is chosen, the keys on the menu.
    pub(crate) fn machine_choose(&mut self, section: Group) -> Intent {
        self.focus = Focus::Content;
        let screen = &mut self.remembered.machine;
        screen.field = Field::List;
        screen.pane = Pane::Menu;
        if screen.section != Some(section) {
            screen.section = Some(section);
            screen.list.selected = 0;
        }
        Intent::None
    }

    /// A click on a row of the table: it is chosen, the keys on the table.
    pub(crate) fn machine_click(&mut self, position: usize) -> Intent {
        self.focus = Focus::Content;
        let screen = &mut self.remembered.machine;
        screen.field = Field::List;
        screen.pane = Pane::Table;
        screen.list.selected = position;
        Intent::None
    }

    /// A click on a column's heading: the table is ordered by it, or the
    /// other way round when it already was.
    pub(crate) fn machine_sort(&mut self, column: Column) -> Intent {
        let screen = &mut self.remembered.machine;
        screen.column = column;
        screen.sort = screen.sort.by(column);
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
