//! A space's overview: the screen selecting a space shows where its pane
//! is, saying what state the project is in, section by section, and
//! letting the operator configure its workspace.
//!
//! It belongs to the space it was opened for, never to the tab in front:
//! the broadcast confirming the space selection changes the tab in front,
//! and an overview tied to it would close on its own arrival. Only the
//! operator choosing something else puts it away.

use super::*;
use crate::ui::widget::alerts::{Alert, Severity};
use uze_application::application::{StatusHarness, memory::Entry as MemoryEntry};
use uze_application::{
    ActionEntry, ActionState, CheckoutOwner, CheckoutView, CheckoutsView, DeclarationApplied,
    DeclarationEntry, DeclarationKey, DeclarationProblem, DeclarationView, DeclaredValue, Edit,
    ProjectSpace, ShellCommand, SpaceOverview,
};

/// The overview's sections, in the order the bar shows them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Section {
    /// The agent in front: what it was asked, and what reaches it.
    Agent,
    /// The `workspace:` section of `agents.yaml`, as a form.
    Settings,
    Plugins,
    Memory,
    /// The checkouts UZE made for the space's agents and their subagents.
    Worktrees,
}

impl Section {
    // The agent in front first, then what the project declares, then what
    // follows from it. Worktrees close the bar: they are what the agents
    // left on disk, and last keeps the number each earlier section is
    // reached by.
    pub(super) const ALL: [Self; 5] = [
        Self::Agent,
        Self::Settings,
        Self::Plugins,
        Self::Memory,
        Self::Worktrees,
    ];

    pub(super) const COUNT: usize = Self::ALL.len();

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Plugins => "Plugins",
            Self::Settings => "Settings",
            Self::Memory => "Memory",
            Self::Worktrees => "Worktrees",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }

    /// The section `step` places after this one along the bar, wrapping.
    fn after(self, step: usize) -> Self {
        Self::ALL[(self.index() + step) % Self::ALL.len()]
    }

    fn next(self) -> Self {
        self.after(1)
    }

    fn previous(self) -> Self {
        self.after(Self::ALL.len() - 1)
    }
}

/// What a section's parts add up to, worst first when compared.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Health {
    Fine,
    Loading,
    Warning,
    Failing,
}

/// One space's overview: the last answer, and where the operator is in it.
pub(super) struct SpaceOverviewView {
    pub(super) space: SpaceId,
    pub(super) root: PathBuf,
    /// The tab it stands in front of, as asked of the server.
    behind: TabId,
    /// Whether the server has said that tab is in front. Until it has,
    /// the selection it reports is the one being left; once it has, any
    /// other selection is the operator going somewhere else.
    settled: bool,
    pub(super) section: Section,
    /// Where each section's list is scrolled to: moved by the wheel, and
    /// pulled to the selection only when the keys move it.
    pub(super) scroll: [crate::ui::extension_view::NavigatorScroll; Section::COUNT],
    /// Where the drawer's text is scrolled to: see [`DrawerScroll`].
    drawer_scroll: DrawerScroll,
    /// The selected row of each section, by section index: a card, in the
    /// Worktrees section.
    pub(super) rows: [usize; Section::COUNT],
    /// Whether the drawer stands over the section, where there is no room
    /// for both side by side.
    pub(super) drawer: bool,
    /// The read whose answer is awaited: an answer to any earlier one is
    /// dropped.
    pub(super) asked: u64,
    pub(super) pending: bool,
    /// The answer held is older than what is on disk: read again, drawing
    /// it meanwhile.
    pub(super) stale: bool,
    pub(super) answer: Option<std::result::Result<SpaceOverview, String>>,
    /// A fact's body, read for the drawer, by name.
    pub(super) fact: Option<(String, Option<std::result::Result<String, String>>)>,
    pub(super) edit: WorkspaceEdit,
    /// The agent in front when the overview opened, and its prompts' state.
    pub(super) agent: Option<AgentSupportDropdown>,
    pub(super) worktrees: WorktreesRead,
    /// What the overview asks before taking something away, while it
    /// stands.
    pub(super) question: Option<OverviewQuestion>,
    /// A removal is out: one at a time, since a second would judge a slot
    /// the first is taking away.
    pub(super) removing: bool,
    /// The space's issues are listed over the overview.
    pub(super) issues_open: bool,
    /// The words the Memory section's facts are narrowed by, matched
    /// against each fact's name, description, kind and origin.
    pub(super) memory_filter: String,
    /// Whether that search is taking what is typed.
    pub(super) filtering: bool,
}

/// How far the drawer is scrolled, and the row that was in it then.
///
/// Kept with the row as well as reset where the keys move: the selection
/// moves from a dozen places, a read that shrinks the list among them,
/// and any of them forgetting the reset would open the next row's text
/// partway down.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DrawerScroll {
    /// The section's index and its row.
    on: (usize, usize),
    rows: u16,
}

/// Where the overview's two scrolled parts settled on the frame just
/// drawn: the section in front, and the drawer beside it.
#[derive(Clone, Copy, Debug)]
pub(super) struct OverviewScroll {
    pub(super) section: crate::ui::extension_view::NavigatorScroll,
    pub(super) drawer: u16,
}

/// The space's worktrees as last measured. Measuring walks every file of
/// every checkout, so it is asked only while the section is in front, and
/// read again on a refresh.
#[derive(Default)]
pub(super) struct WorktreesRead {
    /// The read whose answer is awaited: an answer to any earlier one is
    /// dropped.
    pub(super) asked: u64,
    pub(super) pending: bool,
    /// The answer held is older than what is on disk: read again, drawing
    /// it meanwhile.
    pub(super) stale: bool,
    /// `Some(None)` outside a repository.
    pub(super) answer: Option<Option<CheckoutsView>>,
}

pub(super) struct WorktreesResolution {
    pub(super) space: SpaceId,
    /// The project the read was for: a space holds overviews of several.
    pub(super) root: PathBuf,
    pub(super) asked: u64,
    pub(super) view: Option<CheckoutsView>,
}

/// Where a worktree's work stands, from what the client knows of it: its
/// record, a tab working there and the last evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorktreeState {
    /// Its work waits to be delivered.
    Ready,
    /// An agent works there: a tab of this workspace, or one its record
    /// says is live.
    Working,
    /// A subagent's work, its agent gone, waiting to be joined.
    ToJoin,
    /// Work its agent left unfinished, kept for it.
    Kept,
    /// Held by an agent that is neither working nor kept.
    Idle,
    /// No agent holds it: a slot waiting for the next.
    Free,
    /// UZE's, with a record this build cannot read.
    Unreadable,
}

impl WorktreeState {
    /// Where a card of this state stands among the others: what asks
    /// something of the operator first, what only takes disk last.
    fn rank(self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::ToJoin => 1,
            Self::Kept => 2,
            Self::Unreadable => 3,
            Self::Working => 4,
            Self::Idle => 5,
            Self::Free => 6,
        }
    }
}

impl WorktreeState {
    pub(super) fn words(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Working => "working",
            Self::ToJoin => "to join",
            Self::Kept => "kept",
            Self::Idle => "idle",
            Self::Free => "free",
            Self::Unreadable => "unreadable",
        }
    }
}

/// One worktree as its card shows it.
pub(super) struct WorktreeCard<'a> {
    pub(super) checkout: &'a CheckoutView,
    pub(super) state: WorktreeState,
    /// The label of the tab working there, in this workspace.
    pub(super) tab: Option<&'a str>,
}

/// Whether UZE made `checkout` for an agent: a person's and a harness's
/// own are theirs, and not the space's worktrees.
fn made_for_an_agent(checkout: &CheckoutView) -> bool {
    matches!(
        checkout.owner,
        CheckoutOwner::Agent { .. } | CheckoutOwner::Subagent { .. } | CheckoutOwner::Unreadable
    )
}

/// Uncommitted work no agent is there to commit: what the section warns
/// about, since nothing but the operator will pick it up.
pub(super) fn left_unattended(checkout: &CheckoutView) -> bool {
    let attended = match &checkout.owner {
        CheckoutOwner::Agent { live, .. } => *live,
        CheckoutOwner::Subagent { topic, .. } => topic.is_some(),
        _ => false,
    };
    checkout.dirty && !attended
}

/// The `workspace:` edits not yet in `agents.yaml`. Each is written as
/// soon as it is made, so one stays here only while its save is out, or
/// once the file refused it.
#[derive(Default)]
pub(super) struct WorkspaceEdit {
    /// A key's new value; `None` unsets it.
    pub(super) values: BTreeMap<DeclarationKey, Option<DeclaredValue>>,
    /// An action's new instruction; `None` takes it out.
    pub(super) actions: BTreeMap<ActionState, Option<String>>,
    pub(super) field: Option<EditField>,
    /// An action's instruction being written, over as many lines as it
    /// takes.
    pub(super) text: Option<(ActionState, RenameBuffer)>,
    /// An edit was made since the last save was sent.
    pub(super) unsent: bool,
    /// The edits the save in flight carries.
    pub(super) saving: Option<SentEdits>,
    /// Edits written, standing for the file until the overview read after
    /// them answers, with the overview read that was last asked when they
    /// landed: an answer to that one may have been read before them.
    pub(super) landed: Option<(u64, SentEdits)>,
    /// What the file refused in the edits last sent, each under its key.
    pub(super) refused: Vec<DeclarationProblem>,
    /// Why the last save failed outright.
    pub(super) refusal: Option<String>,
    /// What the last write put in the file, marked beside each key until
    /// the next edit: the form says it was kept rather than a toast.
    pub(super) saved: SentEdits,
}

impl WorkspaceEdit {
    /// How many keys and actions the edits change.
    pub(super) fn pending(&self) -> usize {
        self.values.len() + self.actions.len()
    }

    /// The edits are held back by the file's refusal rather than on their
    /// way to it.
    pub(super) fn is_refused(&self) -> bool {
        self.pending() > 0
            && self.saving.is_none()
            && (!self.refused.is_empty() || self.refusal.is_some())
    }

    fn snapshot(&self) -> SentEdits {
        SentEdits {
            values: self.values.clone(),
            actions: self.actions.clone(),
        }
    }

    /// Drops the edits `sent` carried that still read as they were sent:
    /// one changed since is the operator's newer word, and goes next.
    fn written(&mut self, sent: &SentEdits) {
        self.values
            .retain(|key, value| sent.values.get(key) != Some(value));
        self.actions
            .retain(|state, text| sent.actions.get(state) != Some(text));
    }
}

/// The edits one save carries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SentEdits {
    pub(super) values: BTreeMap<DeclarationKey, Option<DeclaredValue>>,
    pub(super) actions: BTreeMap<ActionState, Option<String>>,
}

impl SentEdits {
    /// The edits as the application takes them.
    pub(super) fn edits(&self) -> Vec<Edit> {
        self.values
            .iter()
            .map(|(key, value)| match value {
                Some(value) => Edit::Set {
                    key: *key,
                    value: value.clone(),
                },
                None => Edit::Unset { key: *key },
            })
            .chain(self.actions.iter().map(|(state, text)| Edit::Action {
                state: *state,
                text: text.clone(),
            }))
            .collect()
    }
}

/// A value being typed: a key's text, or one item of its list.
pub(super) struct EditField {
    pub(super) key: DeclarationKey,
    /// The list item, `Some(len)` for a new one; `None` for the key itself.
    pub(super) item: Option<usize>,
    pub(super) buffer: RenameBuffer,
    /// The offered path the keys are on, when the key offers paths to
    /// choose from rather than to type.
    pub(super) picked: usize,
}

impl EditField {
    /// A field over `key`, or over item `item` of its list, holding `text`.
    fn new(key: DeclarationKey, item: Option<usize>, text: String) -> Self {
        Self {
            key,
            item,
            buffer: RenameBuffer::new(text),
            picked: 0,
        }
    }
}

/// One line of the Workspace section, which the keys walk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkspaceRow {
    Key(DeclarationKey),
    Item(DeclarationKey, usize),
    Action(ActionState),
}

impl WorkspaceRow {
    pub(super) fn key(self) -> Option<DeclarationKey> {
        match self {
            Self::Key(key) | Self::Item(key, _) => Some(key),
            Self::Action(_) => None,
        }
    }
}

/// An action as the form names it.
pub(super) fn action_label(state: ActionState) -> &'static str {
    match state {
        ActionState::OnChange => "onChange",
        ActionState::OnPublish => "onPublish",
        ActionState::OnConflict => "onConflict",
        ActionState::OnFail => "onFail",
    }
}

/// When an action's instruction is sent, in words.
pub(super) fn action_moment(state: ActionState) -> &'static str {
    match state {
        ActionState::OnChange => "Sent when an agent's checkout holds uncommitted work",
        ActionState::OnPublish => "Sent when an agent's branch has been published",
        ActionState::OnConflict => "Sent when rebasing onto the target paused on a conflict",
        ActionState::OnFail => "Sent when the project's checks failed on the rebased branch",
    }
}

pub(super) struct SpaceOverviewResolution {
    pub(super) space: SpaceId,
    /// The project the read was for: a space holds overviews of several,
    /// each counting its reads from its own start.
    pub(super) root: PathBuf,
    pub(super) asked: u64,
    pub(super) answer: std::result::Result<SpaceOverview, String>,
}

pub(super) struct FactBodyResolution {
    pub(super) space: SpaceId,
    pub(super) root: PathBuf,
    pub(super) name: String,
    pub(super) body: std::result::Result<String, String>,
}

pub(super) struct DeclarationSaveResolution {
    pub(super) space: SpaceId,
    pub(super) root: PathBuf,
    pub(super) sent: SentEdits,
    pub(super) outcome: std::result::Result<DeclarationApplied, String>,
}

/// Whether `needle`, already lowercased, is in anything the fact's row or
/// drawer says of it: its name, its description, its kind, its scope or
/// its origin.
fn fact_matches(entry: &MemoryEntry, needle: &str) -> bool {
    [
        entry.name.as_str(),
        entry.description.as_str(),
        entry.kind.label(),
        entry.scope.label(),
        entry.origin.as_str(),
    ]
    .into_iter()
    .any(|said| said.to_lowercase().contains(needle))
}

/// Whether the overview answers `action` itself. Everything else is the
/// workspace's — walking spaces, opening a tab — and reaches it over the
/// overview, which does not seal.
pub(super) fn overview_answers(action: Action) -> bool {
    matches!(
        action,
        Action::Dismiss
            | Action::FocusNext
            | Action::FocusPrevious
            | Action::SelectNext
            | Action::SelectPrevious
            | Action::SelectSection(_)
            | Action::StartFilter
            | Action::Activate
            | Action::Refresh
            | Action::AddItem
            | Action::RemoveItem
            | Action::MoveItemUp
            | Action::MoveItemDown
            | Action::DiscardChanges
            | Action::ReviewCommands
            | Action::NextValue
            | Action::PreviousValue
            | Action::SaveFile
            | Action::ClearPromptHistory
            | Action::EraseBack
            | Action::EraseForward
            | Action::CaretLeft
            | Action::CaretRight
            | Action::CaretLineStart
            | Action::CaretLineEnd
            | Action::ScrollPageDown
            | Action::ScrollPageUp
    )
}

/// The Workspace section's keys, grouped by what they decide about, in
/// the order the section shows them.
pub(super) const GROUPS: [(&str, &[DeclarationKey]); 3] = [
    (
        "checkouts",
        &[
            DeclarationKey::Worktree,
            DeclarationKey::Slots,
            DeclarationKey::IdleDays,
            // Lists last: they grow by an entry a line, and push down only
            // what follows the group.
            DeclarationKey::Link,
            DeclarationKey::Setup,
        ],
    ),
    (
        "delivery",
        &[
            DeclarationKey::Target,
            DeclarationKey::Delivery,
            DeclarationKey::Branch,
            DeclarationKey::Gate,
        ],
    ),
    ("project", &[DeclarationKey::Artifacts]),
];

/// A key as the form names it.
pub(super) fn key_label(key: DeclarationKey) -> &'static str {
    match key {
        DeclarationKey::Worktree => "Worktree",
        DeclarationKey::Target => "Target branch",
        DeclarationKey::Delivery => "Delivery",
        DeclarationKey::Branch => "Branch names",
        DeclarationKey::Link => "Linked files",
        DeclarationKey::Setup => "Setup",
        DeclarationKey::Gate => "Gate",
        DeclarationKey::Slots => "Slots",
        DeclarationKey::IdleDays => "Idle days",
        DeclarationKey::Artifacts => "Artifacts",
    }
}

/// Whether the key is a list of entries, one per line.
pub(super) fn is_listed(key: DeclarationKey, view: &SpaceOverviewView) -> bool {
    is_list(key) || listed_branch(key, view)
}

/// The option a choice key holds as it would be saved.
pub(super) fn chosen(key: DeclarationKey, value: Option<&DeclaredValue>) -> String {
    match value {
        None => key.default_spelled().to_owned(),
        Some(DeclaredValue::List(_)) => "types".to_owned(),
        Some(value) => scalar(value),
    }
}

/// Whether the key is a number, stepped with − and +.
pub(super) fn is_number(key: DeclarationKey) -> bool {
    matches!(key, DeclarationKey::Slots | DeclarationKey::IdleDays)
}

/// The choices a key offers, when it is one choice among a few.
pub(super) fn choices(key: DeclarationKey) -> &'static [&'static str] {
    match key {
        DeclarationKey::Worktree => &["manual", "always"],
        DeclarationKey::Delivery => &["none", "merge", "push"],
        DeclarationKey::Branch => &["conventional", "gitflow", "flat", "agent", "types"],
        _ => &[],
    }
}

fn is_list(key: DeclarationKey) -> bool {
    matches!(
        key,
        DeclarationKey::Link
            | DeclarationKey::Setup
            | DeclarationKey::Gate
            | DeclarationKey::Artifacts
    )
}

/// How many lines a list's items take: [`items`]' length, counted
/// without spelling them.
pub(super) fn item_count(value: &DeclaredValue) -> usize {
    match value {
        DeclaredValue::List(items) => items.len(),
        DeclaredValue::Commands(commands) => commands.len(),
        DeclaredValue::Text(_) | DeclaredValue::Number(_) => 1,
    }
}

/// A list's items as the section shows them, one line each.
pub(super) fn items(value: &DeclaredValue) -> Vec<String> {
    match value {
        DeclaredValue::List(items) => items.clone(),
        DeclaredValue::Commands(commands) => commands.iter().map(command_line).collect(),
        DeclaredValue::Text(text) => vec![text.clone()],
        DeclaredValue::Number(number) => vec![number.to_string()],
    }
}

/// A command as one line: its POSIX spelling, or both when it has two.
pub(super) fn command_line(command: &ShellCommand) -> String {
    match command {
        ShellCommand::Line(line) => line.clone(),
        ShellCommand::PerPlatform(spellings) => match (&spellings.posix, &spellings.windows) {
            (Some(posix), Some(windows)) => format!("{posix} | windows: {windows}"),
            (Some(posix), None) => posix.clone(),
            (None, Some(windows)) => format!("windows: {windows}"),
            (None, None) => String::new(),
        },
    }
}

/// A value as one word or line, for a key that is not a list.
pub(super) fn scalar(value: &DeclaredValue) -> String {
    match value {
        DeclaredValue::Text(text) => text.clone(),
        DeclaredValue::Number(number) => number.to_string(),
        DeclaredValue::List(items) => items.join(", "),
        DeclaredValue::Commands(commands) => commands
            .iter()
            .map(command_line)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

impl SpaceOverviewView {
    fn new(space: SpaceId, root: PathBuf, behind: TabId, settled: bool) -> Self {
        Self {
            space,
            root,
            behind,
            settled,
            section: Section::Agent,
            rows: [0; Section::COUNT],
            scroll: Default::default(),
            drawer_scroll: DrawerScroll::default(),
            drawer: false,
            asked: 0,
            pending: false,
            stale: false,
            answer: None,
            fact: None,
            edit: WorkspaceEdit::default(),
            agent: None,
            worktrees: WorktreesRead::default(),
            question: None,
            removing: false,
            issues_open: false,
            memory_filter: String::new(),
            filtering: false,
        }
    }

    /// The worktrees UZE made for the space's agents, in the order Git
    /// lists them.
    pub(super) fn worktrees(&self) -> Vec<&CheckoutView> {
        let Some(Some(view)) = &self.worktrees.answer else {
            return Vec::new();
        };
        view.checkouts
            .iter()
            .filter(|checkout| made_for_an_agent(checkout))
            .collect()
    }

    /// Whether an answer asked for `root` in `space` is this overview's:
    /// the same space and the same project, since one space holds the
    /// overviews of every project its agents work in.
    pub(super) fn answers(&self, space: SpaceId, root: &Path) -> bool {
        self.space == space && self.root == root
    }

    pub(super) fn project(&self) -> Option<&ProjectSpace> {
        match self.answer.as_ref()? {
            Ok(SpaceOverview::Project(project)) => Some(project),
            _ => None,
        }
    }

    pub(super) fn declaration(&self) -> Option<&DeclarationView> {
        self.project()?.workspace.as_ref().ok()
    }

    pub(super) fn row(&self) -> usize {
        self.rows[self.section.index()]
    }

    /// The rows the drawer is scrolled past: none on a row other than the
    /// one it was scrolled on.
    pub(super) fn drawer_scroll(&self) -> u16 {
        match self.drawer_scroll.on == self.drawer_row() {
            true => self.drawer_scroll.rows,
            false => 0,
        }
    }

    /// Scrolls the drawer by `rows`, past its end only until the next
    /// frame settles it on its last screenful.
    pub(super) fn scroll_drawer(&mut self, rows: i16) {
        self.drawer_scroll = DrawerScroll {
            on: self.drawer_row(),
            rows: self.drawer_scroll().saturating_add_signed(rows),
        };
    }

    /// Opens the drawer at its top: what the page keys did while it was
    /// closed was never seen.
    fn open_drawer(&mut self) {
        self.drawer = true;
        self.drawer_scroll = DrawerScroll::default();
    }

    fn drawer_row(&self) -> (usize, usize) {
        (self.section.index(), self.row())
    }

    /// The facts the Memory search lets through, the most recently
    /// updated first. One with no date goes last, and a tie keeps the
    /// order they are kept in: the project's, then the machine's.
    pub(super) fn memory_facts(&self) -> Vec<&MemoryEntry> {
        let Some(project) = self.project() else {
            return Vec::new();
        };
        let needle = self.memory_filter.trim().to_lowercase();
        let mut facts: Vec<&MemoryEntry> = project
            .memory
            .project
            .iter()
            .chain(&project.memory.machine)
            .filter(|entry| needle.is_empty() || fact_matches(entry, &needle))
            .collect();
        // `updated` is `YYYY-MM-DD`, so its text sorts as its date does.
        facts.sort_by(|left, right| right.updated.cmp(&left.updated));
        facts
    }

    /// The search's words changed: the selection starts over on what they
    /// let through, and a body read for a fact that may be gone is let go.
    fn narrowed(&mut self) {
        self.rows[Section::Memory.index()] = 0;
        self.scroll[Section::Memory.index()] = Default::default();
        // Row 0 again may be another fact.
        self.drawer_scroll = DrawerScroll::default();
        self.fact = None;
    }

    pub(super) fn type_into_search(&mut self, character: char) {
        self.memory_filter.push(character);
        self.narrowed();
    }

    /// A key's value as it would be saved: the edit when there is one,
    /// else what the file declares.
    pub(super) fn effective(&self, key: DeclarationKey) -> Option<DeclaredValue> {
        self.value(key).cloned()
    }

    /// [`Self::effective`], borrowed.
    pub(super) fn value(&self, key: DeclarationKey) -> Option<&DeclaredValue> {
        if let Some(edited) = self.edit.values.get(&key) {
            return edited.as_ref();
        }
        self.held(key)
    }

    /// What the file holds for `key`: an edit just written to it, until
    /// the overview reads it back, else what the overview read.
    fn held(&self, key: DeclarationKey) -> Option<&DeclaredValue> {
        if let Some((_, landed)) = &self.edit.landed
            && let Some(written) = landed.values.get(&key)
        {
            return written.as_ref();
        }
        self.entry(key)?.declared.as_ref()
    }

    /// [`Self::held`], for an action's instruction.
    fn held_action(&self, state: ActionState) -> Option<String> {
        if let Some((_, landed)) = &self.edit.landed
            && let Some(written) = landed.actions.get(&state)
        {
            return written.clone();
        }
        self.action_entry(state)?.declared.clone()
    }

    /// What the file declares about `key`.
    pub(super) fn entry(&self, key: DeclarationKey) -> Option<&DeclarationEntry> {
        self.declaration()?
            .entries
            .iter()
            .find(|entry| entry.key == key)
    }

    /// What the file declares about the action at `state`.
    pub(super) fn action_entry(&self, state: ActionState) -> Option<&ActionEntry> {
        self.declaration()?
            .actions
            .iter()
            .find(|entry| entry.state == state)
    }

    /// What the workspace refuses: in the edits the file last refused,
    /// while they stand, else in the file as it is.
    pub(super) fn problems(&self) -> &[DeclarationProblem] {
        if self.edit.pending() > 0 && !self.edit.refused.is_empty() {
            return &self.edit.refused;
        }
        self.declaration()
            .map_or(&[], |declaration| &declaration.problems)
    }

    pub(super) fn editable(&self) -> bool {
        self.declaration()
            .is_some_and(|declaration| declaration.editable.is_ok())
    }

    /// The Workspace section's lines: each key, and under a list its items.
    pub(super) fn workspace_rows(&self) -> Vec<WorkspaceRow> {
        let mut rows = Vec::new();
        for key in GROUPS.iter().flat_map(|(_, keys)| keys.iter().copied()) {
            rows.push(WorkspaceRow::Key(key));
            if is_listed(key, self) {
                let count = self.value(key).map_or(0, item_count);
                rows.extend((0..count).map(|index| WorkspaceRow::Item(key, index)));
            }
        }
        rows.extend(ActionState::ALL.into_iter().map(WorkspaceRow::Action));
        rows
    }

    /// How many rows the section in front has.
    pub(super) fn row_count(&self) -> usize {
        let Some(project) = self.project() else {
            return 0;
        };
        match self.section {
            Section::Plugins => project.plugins.plugins.len(),
            Section::Settings => self.workspace_rows().len(),
            Section::Memory => self.memory_facts().len(),
            Section::Worktrees => self.worktrees().len(),
            // Its prompts are counted where they are held, by the session.
            Section::Agent => 0,
        }
    }

    pub(super) fn selected_workspace_row(&self) -> Option<WorkspaceRow> {
        self.workspace_rows()
            .get(self.rows[Section::Settings.index()])
            .copied()
    }

    fn select(&mut self, delta: isize) {
        let count = self.row_count();
        let index = self.section.index();
        if count == 0 {
            self.rows[index] = 0;
            return;
        }
        let next = (self.rows[index] as isize + delta).clamp(0, count as isize - 1);
        if self.rows[index] != next as usize {
            self.drawer_scroll = DrawerScroll::default();
        }
        self.rows[index] = next as usize;
        self.fact = None;
    }

    fn switch(&mut self, section: Section) {
        self.section = section;
        self.drawer_scroll = DrawerScroll::default();
        self.filtering = false;
        self.drawer = false;
        self.fact = None;
        self.edit.field = None;
    }

    /// Sets a choice key to its option at `index`. Choosing the project's
    /// own branch types starts the list with the two every vocabulary has.
    pub(super) fn choose(&mut self, key: DeclarationKey, index: usize) {
        let Some(option) = choices(key).get(index) else {
            return;
        };
        if chosen(key, self.value(key)) == *option {
            return;
        }
        let value = match (key, *option) {
            (DeclarationKey::Delivery, "none") => None,
            (DeclarationKey::Branch, "types") => Some(DeclaredValue::List(vec![
                "feat".to_owned(),
                "fix".to_owned(),
            ])),
            (_, option) => Some(DeclaredValue::Text(option.to_owned())),
        };
        self.set(key, value);
    }

    /// Steps a number by `delta`. Leaving it out is its default: slots
    /// below one is no limit, and idle days start from the three the
    /// workspace applies.
    pub(super) fn step(&mut self, key: DeclarationKey, delta: i64) {
        let current = match self.value(key) {
            Some(DeclaredValue::Number(number)) => *number as i64,
            Some(DeclaredValue::Text(text)) => text.trim().parse().unwrap_or(0),
            _ if key == DeclarationKey::IdleDays => 3,
            _ => 0,
        };
        let next = (current + delta).max(0) as u64;
        let value = match key {
            DeclarationKey::Slots if next == 0 => None,
            _ => Some(DeclaredValue::Number(next)),
        };
        self.set(key, value);
    }

    /// What the field being typed offers to choose from: for a link, what
    /// the repository ignores; for artifacts, the directories it tracks;
    /// for the target, its branches; for a choice, its options. Those
    /// already in a list are left out, those the typed text heads first,
    /// then those it appears in.
    pub(super) fn picker_options(&self) -> Vec<&str> {
        let Some(field) = &self.edit.field else {
            return Vec::new();
        };
        let Some(offered) = self.declaration().map(|declaration| &declaration.choices) else {
            return Vec::new();
        };
        let (read, fixed): (&[String], &[&str]) = match field.key {
            DeclarationKey::Link => (&offered.links, &[]),
            DeclarationKey::Artifacts => (&offered.directories, &[]),
            DeclarationKey::Target => (&offered.branches, &[]),
            key if is_combo(key) => (&[], choices(key)),
            _ => return Vec::new(),
        };
        let taken = self
            .value(field.key)
            .filter(|_| field.item.is_some())
            .map(items)
            .unwrap_or_default();
        let editing = field.item.and_then(|item| taken.get(item));
        let typed = field.buffer.text().trim().to_lowercase();
        let mut heads = Vec::new();
        let mut inside = Vec::new();
        let source = read.iter().map(String::as_str).chain(fixed.iter().copied());
        for option in source.filter(|option| {
            editing.is_some_and(|item| item == option) || !taken.iter().any(|item| item == option)
        }) {
            let lowered = option.to_lowercase();
            if lowered.starts_with(&typed) {
                heads.push(option);
            } else if lowered.contains(&typed) {
                inside.push(option);
            }
        }
        heads.extend(inside);
        heads
    }

    /// An action's instruction as it would be saved.
    pub(super) fn effective_action(&self, state: ActionState) -> Option<String> {
        if let Some(edited) = self.edit.actions.get(&state) {
            return edited.clone();
        }
        self.held_action(state)
    }

    // An edit back to what the file holds is no edit, unless a save in
    // flight is about to write something else there.
    fn set_action(&mut self, state: ActionState, text: Option<String>) {
        self.edit.saved = SentEdits::default();
        let text = text.filter(|text| !text.trim().is_empty());
        let in_flight =
            (self.edit.saving.as_ref()).is_some_and(|sent| sent.actions.contains_key(&state));
        if text == self.held_action(state) && !in_flight {
            self.edit.actions.remove(&state);
        } else {
            self.edit.actions.insert(state, text);
        }
        self.edit.unsent = true;
    }

    /// The health of each section, from the answer as it stands.
    pub(super) fn health(&self, section: Section) -> Health {
        let Some(project) = self.project() else {
            return if self.answer.is_none() {
                Health::Loading
            } else {
                Health::Fine
            };
        };
        match section {
            Section::Plugins => {
                let plugins = &project.plugins;
                if plugins.lock_problem.is_some() {
                    Health::Failing
                } else if plugins.plugins.iter().any(|plugin| !plugin.installed)
                    || !plugins.drift.unresolved.is_empty()
                    || !plugins.drift.surplus.is_empty()
                    || !plugins.drift.missing.is_empty()
                {
                    Health::Warning
                } else {
                    Health::Fine
                }
            }
            Section::Settings => {
                let failing = !project.status.steps_not_spelled_here.is_empty()
                    || project
                        .workspace
                        .as_ref()
                        .map_or(true, |declaration| !declaration.problems.is_empty());
                if failing {
                    Health::Failing
                } else if project.status.commands_awaiting_approval.is_some() {
                    Health::Warning
                } else {
                    Health::Fine
                }
            }
            Section::Agent => Health::Fine,
            Section::Worktrees => {
                let worktrees = self.worktrees();
                if self.worktrees.answer.is_none() && self.worktrees.pending {
                    Health::Loading
                } else if worktrees.iter().any(|checkout| {
                    left_unattended(checkout) || checkout.owner == CheckoutOwner::Unreadable
                }) {
                    Health::Warning
                } else {
                    Health::Fine
                }
            }
            Section::Memory => {
                if project.memory.unreadable.is_empty() {
                    Health::Fine
                } else {
                    Health::Warning
                }
            }
        }
    }

    /// Everything in the space worth checking, each what it is about and
    /// what to do: what the section bar marks section by section, and what
    /// reaches the harnesses, which has no section of its own since the
    /// Agent's drawer says it for the agent in front.
    pub(super) fn issues(&self) -> Vec<Alert> {
        let Some(project) = self.project() else {
            return Vec::new();
        };
        let alert = |severity, label: String, detail: String| Alert {
            severity,
            label,
            detail,
        };
        let mut issues = Vec::new();
        let report = &project.status.report;
        if !report.instructions.exists {
            issues.push(alert(
                Severity::Low,
                "No AGENTS.md".to_owned(),
                "Harnesses get no project context here.".to_owned(),
            ));
        }
        for harness in &report.harnesses {
            for issue in harness_issues(harness) {
                issues.push(alert(
                    Severity::Medium,
                    harness.display_name.clone(),
                    issue.to_owned(),
                ));
            }
        }
        for issue in &report.issues {
            issues.push(alert(Severity::Medium, "Context".to_owned(), issue.clone()));
        }
        let plugins = &project.plugins;
        if let Some(problem) = &plugins.lock_problem {
            issues.push(alert(
                Severity::High,
                "agents.lock is malformed".to_owned(),
                problem.clone(),
            ));
        }
        for plugin in plugins.plugins.iter().filter(|plugin| !plugin.installed) {
            issues.push(alert(
                Severity::Medium,
                format!("{} is not installed", plugin.plugin),
                "`uze install` installs what agents.lock records.".to_owned(),
            ));
        }
        let drift = &plugins.drift;
        for (label, names) in [
            ("Declared, not locked yet", &drift.unresolved),
            ("Locked, no longer declared", &drift.surplus),
            ("Locked, not in the store", &drift.missing),
        ] {
            if !names.is_empty() {
                issues.push(alert(
                    Severity::Medium,
                    label.to_owned(),
                    format!("{}. `uze install` puts it right.", names.join(", ")),
                ));
            }
        }
        if drift.stale_projection {
            issues.push(alert(
                Severity::Medium,
                "AGENTS.md is stale".to_owned(),
                "A plugin's region says other than it should. `uze install` puts it right."
                    .to_owned(),
            ));
        }
        for step in &project.status.steps_not_spelled_here {
            issues.push(alert(
                Severity::High,
                format!("`{}` never runs here", step.command),
                format!("It has no {} spelling in agents.yaml.", step.platform),
            ));
        }
        match &project.workspace {
            Err(reason) => issues.push(alert(
                Severity::High,
                "agents.yaml could not be read".to_owned(),
                reason.clone(),
            )),
            Ok(declaration) => {
                for problem in &declaration.problems {
                    issues.push(alert(
                        Severity::High,
                        problem.key.as_deref().map_or_else(
                            || "agents.yaml".to_owned(),
                            |key| format!("workspace.{key}"),
                        ),
                        problem.reason.clone(),
                    ));
                }
            }
        }
        if let Some(awaiting) = &project.status.commands_awaiting_approval {
            issues.push(alert(
                Severity::Medium,
                match awaiting.commands.len() {
                    1 => "1 command waiting for approval".to_owned(),
                    count => format!("{count} commands waiting for approval"),
                },
                "Settings shows them; approving lets agents run them here.".to_owned(),
            ));
        }
        for unreadable in &project.memory.unreadable {
            issues.push(alert(
                Severity::Medium,
                "A memory file cannot be read".to_owned(),
                format!("{}: {}", unreadable.path.display(), unreadable.reason),
            ));
        }
        for checkout in self.worktrees() {
            let name = checkout
                .branch
                .clone()
                .unwrap_or_else(|| checkout.path.display().to_string());
            if checkout.owner == CheckoutOwner::Unreadable {
                issues.push(alert(
                    Severity::Medium,
                    format!("{name}: record unreadable"),
                    "This build cannot read the record of who owns this worktree.".to_owned(),
                ));
            } else if left_unattended(checkout) {
                issues.push(alert(
                    Severity::Medium,
                    format!("{name}: uncommitted work"),
                    "No agent is working in it. Worktrees shows it.".to_owned(),
                ));
            }
        }
        issues
    }

    fn set(&mut self, key: DeclarationKey, value: Option<DeclaredValue>) {
        self.edit.saved = SentEdits::default();
        let value = value.filter(|value| {
            !items(value).iter().all(String::is_empty) || matches!(value, DeclaredValue::Number(_))
        });
        let in_flight =
            (self.edit.saving.as_ref()).is_some_and(|sent| sent.values.contains_key(&key));
        if value.as_ref() == self.held(key) && !in_flight {
            self.edit.values.remove(&key);
        } else {
            self.edit.values.insert(key, value);
        }
        self.edit.unsent = true;
    }

    /// Drops the edits the file refused, and why; edits on their way to it
    /// are not the operator's to take back.
    fn discard_refused(&mut self) {
        if !self.edit.is_refused() {
            return;
        }
        self.edit.values.clear();
        self.edit.actions.clear();
        self.edit.refused.clear();
        self.edit.refusal = None;
        self.edit.unsent = false;
    }
}

/// What keeps `harness` from reading the project's context here: its
/// context unread, an agent context it reads instead, an outdated version.
pub(super) fn harness_issues(harness: &StatusHarness) -> impl Iterator<Item = &str> {
    [
        &harness.context_unread,
        &harness.agent_context,
        &harness.outdated,
    ]
    .into_iter()
    .filter_map(|issue| issue.as_deref())
}

/// Whether a field for `key` offers something to choose from, as
/// [`SpaceOverviewView::picker_options`] lists it: what the repository
/// ignores, the directories it tracks, or a combo's options.
pub(super) fn offers_choices(key: DeclarationKey) -> bool {
    matches!(key, DeclarationKey::Link | DeclarationKey::Artifacts) || is_combo(key)
}

/// The most options a choice draws side by side; past it, they go behind
/// a list.
const SIDE_BY_SIDE: usize = 3;

/// A choice whose options are all drawn side by side.
pub(super) fn is_segmented(key: DeclarationKey) -> bool {
    (2..=SIDE_BY_SIDE).contains(&choices(key).len())
}

/// A key chosen from a list it offers rather than typed: its options, or
/// the repository's branches.
pub(super) fn is_combo(key: DeclarationKey) -> bool {
    !choices(key).is_empty() || key == DeclarationKey::Target
}

/// `branch` is a list once it holds the project's own types.
fn listed_branch(key: DeclarationKey, view: &SpaceOverviewView) -> bool {
    key == DeclarationKey::Branch && matches!(view.value(key), Some(DeclaredValue::List(_)))
}

impl WorkspaceModel {
    /// Opens the overview of the project `root` an agent of `space` works
    /// in, in front of `behind`, or keeps the one already open for it.
    /// `settled` says the server already has that tab in front.
    pub(super) fn open_space_overview(
        &mut self,
        space: SpaceId,
        behind: TabId,
        root: PathBuf,
        settled: bool,
    ) {
        if let Some(overview) = self
            .space_overview
            .as_mut()
            .filter(|overview| overview.space == space && overview.root == root)
        {
            overview.behind = behind;
            overview.settled = settled;
            return;
        }
        self.put_away_space_overview();
        self.close_extension();
        let overview = match self.put_away_overviews.remove(&space) {
            // Its answer is read again on opening; what was drawn from the
            // last one stands until the new one arrives.
            Some(mut kept) if kept.root == root => {
                kept.behind = behind;
                kept.settled = settled;
                kept.drawer = false;
                kept.stale = true;
                kept.worktrees.stale = true;
                kept
            }
            _ => SpaceOverviewView::new(space, root, behind, settled),
        };
        self.space_overview = Some(overview);
        self.dirty = true;
    }

    /// Follows the agent the overview is about as the selection moves,
    /// however that came about: a shell beside the same agent keeps it,
    /// another agent turns it to that agent (see
    /// [`WorkspaceModel::overview_follows`]), and anything that is no
    /// agent's context — the space's own row, its shell, a space of
    /// shells — puts it away.
    pub(super) fn put_away_space_overview_left_behind(&mut self, identities: &[AgentIdentity]) {
        let Some(overview) = self.space_overview.as_mut() else {
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        let space = session.selected_space();
        if space.id == overview.space && space.selected_tab == overview.behind {
            overview.settled = true;
            return;
        }
        // Opened ahead of the broadcast that puts its tab in front: what
        // the server still says is the selection before the one asked for.
        if !overview.settled && session.space(overview.space).is_some() {
            return;
        }
        let in_front = space_context_agent(space, identities);
        if in_front.is_some() && in_front == overview.agent.as_ref().map(|agent| agent.tab) {
            overview.space = space.id;
            overview.behind = space.selected_tab;
        } else if in_front.is_some() {
            self.overview_follows = true;
        } else {
            self.put_away_space_overview();
        }
    }

    /// Forgets the overviews put away for spaces the session no longer
    /// has: a space's id is never given to another, so nothing could open
    /// them again.
    pub(super) fn forget_overviews_of_closed_spaces(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        self.put_away_overviews
            .retain(|space, _| session.space(*space).is_some());
    }

    /// The space's worktrees as cards: each with where its work stands and
    /// the tab working there, from the tabs this client holds and its last
    /// evaluation, so a card moves with the sidebar. Ordered by where they
    /// stand, which is the order the keys walk them in.
    pub(super) fn worktree_cards<'a>(
        &'a self,
        overview: &'a SpaceOverviewView,
        project: &ProjectSpace,
    ) -> Vec<WorktreeCard<'a>> {
        let tabs: BTreeMap<&str, &str> = self
            .tabs()
            .filter_map(|tab| launched_agent_id(tab).map(|id| (id, tab.label.as_str())))
            .collect();
        let primary = evaluation_key(&project.status.report.root);
        let ready: BTreeSet<&str> =
            self.remembered
                .tasks
                .get(&primary)
                .map_or_else(BTreeSet::new, |tasks| {
                    tasks
                        .iter()
                        .filter(|task| matches!(task.state, uze_application::WorkStateView::Ready))
                        .map(|task| task.id.as_str())
                        .collect()
                });
        let kept: BTreeSet<&str> = project
            .preserved
            .iter()
            .map(|work| work.id.as_str())
            .collect();
        let mut cards = overview
            .worktrees()
            .into_iter()
            .map(|checkout| {
                let task = checkout.task.as_deref();
                let tab = task.and_then(|id| tabs.get(id).copied());
                let state = match &checkout.owner {
                    CheckoutOwner::Unreadable => WorktreeState::Unreadable,
                    _ if task.is_some_and(|id| ready.contains(id)) => WorktreeState::Ready,
                    _ if tab.is_some() => WorktreeState::Working,
                    CheckoutOwner::Agent { live: true, .. } => WorktreeState::Working,
                    CheckoutOwner::Subagent { joinable: true, .. } => WorktreeState::ToJoin,
                    CheckoutOwner::Subagent { topic: Some(_), .. } => WorktreeState::Working,
                    _ if task.is_some_and(|id| kept.contains(id)) => WorktreeState::Kept,
                    CheckoutOwner::Agent {
                        holder: Some(_), ..
                    } => WorktreeState::Idle,
                    _ => WorktreeState::Free,
                };
                WorktreeCard {
                    checkout,
                    state,
                    tab,
                }
            })
            .collect::<Vec<_>>();
        // By where each stands, the most recently changed first within one.
        cards.sort_by_key(|card| {
            (
                card.state.rank(),
                std::cmp::Reverse(card.checkout.last_changed),
            )
        });
        cards
    }

    /// A click on a row of the section in front.
    pub(super) fn select_overview_row(&mut self, index: usize) {
        if let Some(overview) = self.space_overview.as_mut()
            && index < overview.row_count()
        {
            let section = overview.section.index();
            if overview.rows[section] != index {
                overview.fact = None;
            }
            overview.rows[section] = index;
            self.dirty = true;
        }
    }

    /// Keeps where the section in front settled on the frame just drawn.
    pub(super) fn absorb_overview_frame(&mut self, scroll: Option<OverviewScroll>) {
        if let (Some(scroll), Some(overview)) = (scroll, self.space_overview.as_mut()) {
            overview.scroll[overview.section.index()] = scroll.section;
            overview.drawer_scroll = DrawerScroll {
                on: overview.drawer_row(),
                rows: scroll.drawer,
            };
        }
    }

    pub(super) fn put_away_space_overview(&mut self) {
        if let Some(mut overview) = self.space_overview.take() {
            // An answer in flight is for the overview open now; one opened
            // again asks afresh.
            overview.pending = false;
            overview.worktrees.pending = false;
            overview.question = None;
            overview.edit.field = None;
            overview.edit.text = None;
            self.put_away_overviews.insert(overview.space, overview);
            self.dirty = true;
        }
    }

    pub(super) fn absorb_space_overview(&mut self, resolution: SpaceOverviewResolution) -> bool {
        let Some(overview) = self
            .space_overview
            .as_mut()
            .filter(|overview| overview.answers(resolution.space, &resolution.root))
        else {
            return false;
        };
        if resolution.asked != overview.asked {
            return false;
        }
        overview.pending = false;
        overview.answer = Some(resolution.answer);
        if overview
            .edit
            .landed
            .as_ref()
            .is_some_and(|(asked, _)| resolution.asked > *asked)
        {
            overview.edit.landed = None;
        }
        let count = overview.row_count();
        let section = overview.section.index();
        overview.rows[section] = overview.rows[section].min(count.saturating_sub(1));
        true
    }

    /// The space's worktrees, measured: kept only while they answer the
    /// last read the overview open now asked.
    pub(super) fn absorb_space_worktrees(&mut self, resolution: WorktreesResolution) -> bool {
        let section = Section::Worktrees.index();
        // The cards are ordered by where each stands, which a new answer
        // can change: the selection follows its worktree, not its place.
        let chosen = self.selected_worktree();
        let Some(overview) = self
            .space_overview
            .as_mut()
            .filter(|overview| overview.answers(resolution.space, &resolution.root))
        else {
            return false;
        };
        let read = &mut overview.worktrees;
        if resolution.asked != read.asked {
            return false;
        }
        read.pending = false;
        read.answer = Some(resolution.view);
        let count = overview.worktrees().len();
        overview.rows[section] = overview.rows[section].min(count.saturating_sub(1));
        if let Some(path) = chosen
            && let Some(at) = self.worktree_position(&path)
            && let Some(overview) = self.space_overview.as_mut()
        {
            overview.rows[section] = at;
        }
        true
    }

    /// The path of the worktree card the keys are on.
    fn selected_worktree(&self) -> Option<PathBuf> {
        let overview = self.space_overview.as_ref()?;
        let project = overview.project()?;
        self.worktree_cards(overview, project)
            .get(overview.rows[Section::Worktrees.index()])
            .map(|card| card.checkout.path.clone())
    }

    /// Where the card for the worktree at `path` stands now.
    fn worktree_position(&self, path: &Path) -> Option<usize> {
        let overview = self.space_overview.as_ref()?;
        let project = overview.project()?;
        self.worktree_cards(overview, project)
            .iter()
            .position(|card| card.checkout.path == path)
    }

    pub(super) fn absorb_fact_body(&mut self, resolution: FactBodyResolution) -> bool {
        let Some(overview) = self
            .space_overview
            .as_mut()
            .filter(|overview| overview.answers(resolution.space, &resolution.root))
        else {
            return false;
        };
        match &mut overview.fact {
            Some((name, body)) if *name == resolution.name => {
                *body = Some(resolution.body);
                true
            }
            _ => false,
        }
    }

    /// A save answered. Written, the edits it carried leave the pending
    /// ones, the overview is read again and a toast says what changed, and
    /// the project's root is returned for its approvals to be asked about
    /// again. Refused, they stay, each problem under its key. Answered for
    /// an overview put away meanwhile, it is that one's.
    pub(super) fn absorb_declaration_save(
        &mut self,
        resolution: DeclarationSaveResolution,
    ) -> Option<PathBuf> {
        let open = self
            .space_overview
            .as_mut()
            .filter(|overview| overview.answers(resolution.space, &resolution.root));
        let overview = match open {
            Some(overview) => overview,
            None => self
                .put_away_overviews
                .get_mut(&resolution.space)
                .filter(|overview| overview.root == resolution.root)?,
        };
        self.dirty = true;
        let edit = &mut overview.edit;
        edit.saving = None;
        match resolution.outcome {
            Ok(DeclarationApplied::Written { .. }) => {}
            Ok(DeclarationApplied::Refused { problems }) => {
                edit.refused = problems;
                edit.refusal = None;
                return None;
            }
            Err(reason) => {
                edit.refusal = Some(reason.clone());
                self.raise_toast(
                    crate::ui::widget::ToastKind::Failed,
                    "agents.yaml not saved",
                    reason,
                    None,
                );
                return None;
            }
        };
        let sent = resolution.sent;
        edit.written(&sent);
        edit.refused.clear();
        edit.refusal = None;
        let landed = edit
            .landed
            .get_or_insert_with(|| (overview.asked, SentEdits::default()));
        landed.0 = overview.asked;
        landed.1.values.extend(sent.values.clone());
        landed.1.actions.extend(sent.actions.clone());
        overview.stale = true;
        // Commands waiting for approval again are said where they always
        // are: the cause line and the drawer's Review.
        overview.edit.saved = sent;
        Some(overview.root.clone())
    }
}

impl Attach<'_> {
    /// The agent button: the overview of the agent in front, over the tab
    /// in front, or put away when it is already up.
    pub(super) fn toggle_space_overview(&mut self) {
        if self.model.space_overview.is_some() {
            self.model.put_away_space_overview();
            return;
        }
        self.open_agent_overview();
    }

    /// Opens the overview on the agent whose context is in front — nothing
    /// with none: an overview is an agent's — reading the project it works
    /// in, what reaches it and the prompts it was given.
    pub(super) fn open_agent_overview(&mut self) {
        let Some(agent) = agent_panel_in_front(&self.model, &self.identities) else {
            self.model.put_away_space_overview();
            return;
        };
        let Some((space, behind, root)) = self.model.session.as_ref().and_then(|session| {
            let space = session.selected_space();
            let directory = named_checkout(&self.model.tab(agent.tab)?.pane.cwd);
            // The checkout the agent works in: what its Settings read and
            // write, so a change made in an isolated checkout is that
            // checkout's work, committed and delivered with it.
            let root = uze_application::isolated_checkout_root(&directory)
                .or_else(|| self.model.projects.get(&directory).cloned())?;
            Some((space.id, space.selected_tab, root))
        }) else {
            return;
        };
        self.model.open_space_overview(space, behind, root, true);
        // The agent is the overview's first section. Opening re-reads what
        // reaches it and what it was asked: `AGENTS.md` and `.agents/`
        // change under an open workspace, and this is the moment somebody
        // is looking.
        let remembered = &mut self.model.remembered;
        if let Some(key) = &agent.key
            && remembered.agent_support_pending.as_ref() != Some(key)
        {
            remembered.agent_support_pending = Some(key.clone());
            spawn_support_refresh(self.home, key.clone(), self.channels.support.sender.clone());
        }
        if let Some(of) = &agent.prompts
            && remembered.prompt_history_pending.as_ref() != Some(of)
        {
            remembered.prompt_history_pending = Some(of.clone());
            spawn_prompt_history(self.home, of.clone(), self.channels.prompts.sender.clone());
        }
        if let Some(overview) = self.model.space_overview.as_mut() {
            // The prompt it was on stays chosen when the agent is the same.
            let selected = overview
                .agent
                .as_ref()
                .filter(|kept| kept.prompts == agent.prompts)
                .map(|kept| kept.selected);
            overview.agent = Some(agent);
            if let (Some(agent), Some(selected)) = (overview.agent.as_mut(), selected) {
                agent.selected = selected;
            }
        }
        self.schedule_space_overview();
    }

    /// Reads the overview open now, unless its answer is awaited already.
    pub(super) fn schedule_space_overview(&mut self) {
        self.schedule_space_worktrees();
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        if overview.pending || overview.answer.is_some() && !overview.stale {
            return;
        }
        overview.asked += 1;
        overview.pending = true;
        overview.stale = false;
        spawn_space_overview(
            self.home,
            overview.space,
            overview.asked,
            overview.root.clone(),
            self.channels.space_overviews.sender.clone(),
        );
    }

    /// Measures the space's worktrees while their section is in front,
    /// unless a measure is out or the one held is current.
    fn schedule_space_worktrees(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let read = &mut overview.worktrees;
        if overview.section != Section::Worktrees
            || read.pending
            || read.answer.is_some() && !read.stale
        {
            return;
        }
        read.asked += 1;
        read.pending = true;
        read.stale = false;
        spawn_space_worktrees(
            self.home,
            overview.space,
            read.asked,
            overview.root.clone(),
            self.channels.space_worktrees.sender.clone(),
        );
    }

    /// Reads it again, drawing the last answer until the new one lands.
    pub(super) fn refresh_space_overview(&mut self) {
        if let Some(overview) = self.model.space_overview.as_mut() {
            overview.stale = true;
            overview.worktrees.stale = true;
        }
        self.schedule_space_overview();
    }

    /// Takes in whatever the overview's reads answered, and asks what it
    /// now needs.
    pub(super) fn absorb_space_overview_answers(&mut self) {
        while let Ok(resolution) = self.channels.space_overviews.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_space_overview(resolution);
        }
        while let Ok(resolution) = self.channels.space_worktrees.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_space_worktrees(resolution);
        }
        self.absorb_worktree_removals();
        while let Ok(resolution) = self.channels.fact_bodies.receiver.try_recv() {
            self.model.dirty |= self.model.absorb_fact_body(resolution);
        }
        while let Ok(resolution) = self.channels.declaration_saves.receiver.try_recv() {
            // What waits for approval is a digest of the lines, so a
            // written edit to them is asked about again.
            if let Some(root) = self.model.absorb_declaration_save(resolution) {
                spawn_commands_awaiting(
                    self.home,
                    vec![root],
                    self.channels.commands_awaiting.sender.clone(),
                );
            }
        }
        self.schedule_space_overview();
        self.schedule_declaration_save();
    }

    /// Sends the edits made since the last save, unless one is out: what
    /// is made meanwhile goes once it answers, so two saves never race for
    /// the file.
    pub(super) fn schedule_declaration_save(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let edit = &mut overview.edit;
        if !edit.unsent || edit.saving.is_some() {
            return;
        }
        edit.unsent = false;
        if edit.pending() == 0 {
            return;
        }
        let sent = edit.snapshot();
        edit.saving = Some(sent.clone());
        spawn_declaration_save(
            self.home,
            overview.space,
            overview.root.clone(),
            sent,
            self.channels.declaration_saves.sender.clone(),
        );
    }

    /// One action, while the overview has the keyboard: the instruction or
    /// the value being typed takes it first, then the section in front.
    pub(super) fn space_overview_action(&mut self, action: Action) {
        let Some(overview) = self.model.space_overview.as_ref() else {
            return;
        };
        self.model.dirty = true;
        if action == Action::ReviewCommands {
            self.review_overview_commands();
        } else if overview.edit.text.is_some() {
            self.instruction_action(action);
        } else if overview.filtering {
            self.memory_search_action(action);
        } else if overview.edit.field.is_some() {
            self.field_action(action);
        } else if overview.section == Section::Agent
            && matches!(
                action,
                Action::SelectNext
                    | Action::SelectPrevious
                    | Action::Activate
                    | Action::ClearPromptHistory
            )
        {
            self.drawer_action(action);
        } else {
            self.section_action(action);
            // A section just brought to front may have its own read to ask.
            self.schedule_space_worktrees();
        }
    }

    /// An action's instruction being written.
    fn instruction_action(&mut self, action: Action) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let Some((state, buffer)) = overview.edit.text.as_mut() else {
            return;
        };
        let state = *state;
        match action {
            Action::Dismiss => overview.edit.text = None,
            // Enter is a line break here: an instruction runs to as
            // many lines as it needs, and is done with the save chord.
            Action::Activate => buffer.insert('\n'),
            Action::SaveFile => {
                let text = buffer.text().to_owned();
                overview.edit.text = None;
                overview.set_action(state, Some(text));
                self.schedule_declaration_save();
            }
            Action::EraseBack => buffer.erase_back(),
            Action::EraseForward => buffer.erase_forward(),
            Action::CaretLeft => buffer.left(),
            Action::CaretRight => buffer.right(),
            Action::CaretLineStart => buffer.home(),
            Action::CaretLineEnd => buffer.end(),
            _ => {}
        }
    }

    /// What a key does while the Memory search takes text: the arrows walk
    /// what it let through, enter leaves it on the words and reads the
    /// fact, and esc leaves it with the words gone.
    fn memory_search_action(&mut self, action: Action) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        match action {
            Action::EraseBack => {
                overview.memory_filter.pop();
                overview.narrowed();
            }
            Action::Dismiss => {
                overview.filtering = false;
                overview.memory_filter.clear();
                overview.narrowed();
            }
            Action::SelectNext => overview.select(1),
            Action::SelectPrevious => overview.select(-1),
            Action::Activate => {
                overview.filtering = false;
                self.activate_overview_row();
            }
            _ => {}
        }
    }

    /// A value being typed, and the options it narrows.
    fn field_action(&mut self, action: Action) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let offered = if action == Action::SelectNext {
            overview.picker_options().len()
        } else {
            0
        };
        let Some(field) = overview.edit.field.as_mut() else {
            return;
        };
        match action {
            Action::Dismiss => overview.edit.field = None,
            Action::SelectNext if offered > 0 => {
                field.picked = (field.picked + 1).min(offered - 1);
            }
            Action::SelectPrevious => field.picked = field.picked.saturating_sub(1),
            Action::Activate => self.commit_field(),
            Action::EraseBack => {
                field.buffer.erase_back();
                field.picked = 0;
            }
            Action::EraseForward => field.buffer.erase_forward(),
            Action::CaretLeft => field.buffer.left(),
            Action::CaretRight => field.buffer.right(),
            Action::CaretLineStart => field.buffer.home(),
            Action::CaretLineEnd => field.buffer.end(),
            _ => {}
        }
    }

    /// The section in front, its rows and the bar over it.
    fn section_action(&mut self, action: Action) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        match action {
            Action::Dismiss if overview.drawer => overview.drawer = false,
            Action::Dismiss if overview.fact.is_some() => overview.fact = None,
            // Esc takes the words away before it takes the overview away.
            Action::Dismiss
                if overview.section == Section::Memory && !overview.memory_filter.is_empty() =>
            {
                overview.memory_filter.clear();
                overview.narrowed();
            }
            Action::Dismiss => self.model.put_away_space_overview(),
            Action::StartFilter if overview.section == Section::Memory => {
                overview.filtering = true;
            }
            Action::SelectSection(position) => {
                if let Some(section) = Section::ALL.get(usize::from(position).saturating_sub(1)) {
                    overview.switch(*section);
                }
            }
            Action::FocusNext => overview.switch(overview.section.next()),
            Action::FocusPrevious => overview.switch(overview.section.previous()),
            Action::SelectNext => overview.select(1),
            Action::SelectPrevious => overview.select(-1),
            Action::Refresh => self.refresh_space_overview(),
            Action::Activate => self.activate_overview_row(),
            Action::AddItem => self.add_workspace_item(),
            Action::RemoveItem if overview.section == Section::Worktrees => {
                self.ask_worktree_removal();
            }
            Action::RemoveItem => self.remove_workspace_row(),
            Action::MoveItemUp => self.move_workspace_item(-1),
            Action::MoveItemDown => self.move_workspace_item(1),
            Action::NextValue | Action::PreviousValue => {
                let delta = if action == Action::NextValue { 1 } else { -1 };
                let row = overview.rows[Section::Settings.index()];
                if overview.section == Section::Settings {
                    self.step_overview_value(row, delta);
                }
            }
            Action::DiscardChanges => overview.discard_refused(),
            Action::ScrollPageDown => overview.scroll_drawer(crate::ui::input::PREVIEW_PAGE),
            Action::ScrollPageUp => overview.scroll_drawer(-crate::ui::input::PREVIEW_PAGE),
            _ => {}
        }
    }

    /// Enter on the row in focus: the drawer where it stands over the
    /// section, a fact's body, the approval question, or an edit.
    fn activate_overview_row(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        match overview.section {
            Section::Settings => {
                let Some(row) = overview.selected_workspace_row() else {
                    return;
                };
                if !overview.editable() {
                    return;
                }
                self.edit_workspace_row(row);
            }
            Section::Memory => {
                let Some((scope, name)) = overview
                    .memory_facts()
                    .get(overview.row())
                    .map(|entry| (entry.scope, entry.name.clone()))
                else {
                    return;
                };
                overview.open_drawer();
                overview.fact = Some((name.clone(), None));
                spawn_fact_body(
                    self.home,
                    overview.space,
                    overview.root.clone(),
                    scope,
                    name,
                    self.channels.fact_bodies.sender.clone(),
                );
            }
            Section::Plugins | Section::Worktrees => overview.open_drawer(),
            // Its keys are the prompts', answered before this.
            Section::Agent => {}
        }
    }

    /// A click on a row: it is selected, and in the Workspace form a value,
    /// an entry, a list's add or a choice's list is opened, as Enter would.
    /// A number's or a segmented choice's row waits for a click on one of
    /// its buttons.
    pub(super) fn click_overview_row(&mut self, index: usize) {
        // On the row being typed into, the click is the typing's own.
        if self.model.space_overview.as_ref().is_some_and(|overview| {
            (overview.edit.field.is_some() || overview.edit.text.is_some())
                && overview.row() == index
        }) {
            return;
        }
        // Known by what it is rather than where: finishing what is open
        // can add a line above it.
        let clicked = self
            .model
            .space_overview
            .as_ref()
            .filter(|overview| overview.section == Section::Settings)
            .and_then(|overview| overview.workspace_rows().get(index).copied());
        let index = match clicked {
            Some(row) => {
                self.finish_editing();
                self.model
                    .space_overview
                    .as_ref()
                    .and_then(|overview| overview.workspace_rows().iter().position(|at| *at == row))
                    .unwrap_or(index)
            }
            None => index,
        };
        self.model.select_overview_row(index);
        let Some(overview) = self.model.space_overview.as_ref() else {
            return;
        };
        if overview.section != Section::Settings {
            return;
        }
        // A number and a segmented choice answer only on their own
        // buttons: the rest of their line selects them and nothing more.
        let own_buttons = matches!(
            overview.selected_workspace_row(),
            Some(WorkspaceRow::Key(key)) if is_number(key) || is_segmented(key)
        );
        if !own_buttons {
            self.space_overview_action(Action::Activate);
        }
    }

    /// Leaves whatever is being typed, as a click elsewhere in the form
    /// does: prose is kept and saved, a typed entry is taken, and a list
    /// of options closes without choosing — the one highlighted was never
    /// picked.
    pub(in crate::ui::orchestrator) fn finish_editing(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        // The search keeps its words; only the typing ends.
        overview.filtering = false;
        if let Some((state, buffer)) = overview.edit.text.take() {
            overview.set_action(state, Some(buffer.text().to_owned()));
            self.schedule_declaration_save();
            return;
        }
        let Some(field) = &overview.edit.field else {
            return;
        };
        let offers = is_combo(field.key) || !overview.picker_options().is_empty();
        if offers || field.buffer.text().trim().is_empty() {
            overview.edit.field = None;
        } else {
            self.commit_field();
        }
    }

    /// A click on a path a list's field offers: it is taken, as enter
    /// takes the one the keys are on.
    pub(super) fn pick_overview_option(&mut self, option: usize) {
        let Some(field) = self
            .model
            .space_overview
            .as_mut()
            .and_then(|overview| overview.edit.field.as_mut())
        else {
            return;
        };
        field.picked = option;
        self.model.dirty = true;
        self.commit_field();
    }

    /// Steps the number on `row` by `delta`, selecting it: its − and +
    /// clicked, or the keys that stand for them.
    pub(super) fn step_overview_value(&mut self, row: usize, delta: i64) {
        self.finish_editing();
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        if !overview.editable() {
            return;
        }
        let Some(WorkspaceRow::Key(key)) = overview.workspace_rows().get(row).copied() else {
            return;
        };
        if !is_number(key) {
            return;
        }
        overview.rows[Section::Settings.index()] = row;
        overview.step(key, delta);
        self.model.dirty = true;
        self.schedule_declaration_save();
    }

    /// A click on one of a segmented choice's options: selects its row and
    /// sets it.
    pub(super) fn choose_overview_option(&mut self, row: usize, option: usize) {
        self.finish_editing();
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        if !overview.editable() {
            return;
        }
        let Some(WorkspaceRow::Key(key)) = overview.workspace_rows().get(row).copied() else {
            return;
        };
        if !is_segmented(key) {
            return;
        }
        overview.rows[Section::Settings.index()] = row;
        overview.choose(key, option);
        self.model.dirty = true;
        self.schedule_declaration_save();
    }

    /// Opens the approval question about this project's commands.
    pub(super) fn review_overview_commands(&mut self) {
        let Some(project) = self
            .model
            .space_overview
            .as_ref()
            .and_then(|overview| overview.project())
            .and_then(|project| project.status.commands_awaiting_approval.as_ref())
            .map(|awaiting| awaiting.project.clone())
        else {
            return;
        };
        self.model.review_commands_for(&project);
    }

    fn edit_workspace_row(&mut self, row: WorkspaceRow) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let key = match row {
            WorkspaceRow::Action(state) => {
                let text = overview.effective_action(state).unwrap_or_default();
                overview.edit.text = Some((state, RenameBuffer::new(text)));
                return;
            }
            WorkspaceRow::Key(key) | WorkspaceRow::Item(key, _) => key,
        };
        let value = overview.effective(key);
        match row {
            WorkspaceRow::Item(_, index) => {
                let text = value
                    .as_ref()
                    .map(items)
                    .and_then(|items| items.get(index).cloned())
                    .unwrap_or_default();
                overview.edit.field = Some(EditField::new(key, Some(index), text));
            }
            // Opened on the value it holds, with nothing typed: every
            // option is on offer until typing narrows them.
            // Every option is on screen, so enter takes the next one.
            WorkspaceRow::Key(key) if is_segmented(key) => {
                let options = choices(key);
                let current = chosen(key, value.as_ref());
                let at = options.iter().position(|option| *option == current);
                overview.choose(key, at.map_or(0, |at| (at + 1) % options.len()));
                self.schedule_declaration_save();
            }
            WorkspaceRow::Key(key) if is_combo(key) => {
                let current = match choices(key) {
                    [] => value.as_ref().map(scalar).unwrap_or_default(),
                    _ => chosen(key, value.as_ref()),
                };
                overview.edit.field = Some(EditField::new(key, None, String::new()));
                let picked = overview
                    .picker_options()
                    .iter()
                    .position(|option| *option == current)
                    .unwrap_or(0);
                if let Some(field) = overview.edit.field.as_mut() {
                    field.picked = picked;
                }
            }
            WorkspaceRow::Key(key) if is_listed(key, overview) => {
                let len = value.as_ref().map_or(0, item_count);
                overview.edit.field = Some(EditField::new(key, Some(len), String::new()));
            }
            WorkspaceRow::Key(key) => {
                let text = value.as_ref().map(scalar).unwrap_or_default();
                overview.edit.field = Some(EditField::new(key, None, text));
            }
            // Answered above, before the key was read.
            WorkspaceRow::Action(_) => {}
        }
    }

    /// The typed value, taken: into the key, or into its list.
    fn commit_field(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        let offered = {
            let options = overview.picker_options();
            let picked = overview.edit.field.as_ref().map_or(0, |field| field.picked);
            options
                .get(picked.min(options.len().saturating_sub(1)))
                .map(|option| (*option).to_owned())
        };
        let Some(field) = overview.edit.field.take() else {
            return;
        };
        // A path on offer is taken as offered; with none matching, what
        // was typed stands, and the check says whether it may.
        let typed = offered.unwrap_or_else(|| field.buffer.text().trim().to_owned());
        let key = field.key;
        if !choices(key).is_empty() && field.item.is_none() {
            if let Some(index) = choices(key).iter().position(|option| *option == typed) {
                overview.choose(key, index);
                self.schedule_declaration_save();
            }
            return;
        }
        let value = match field.item {
            None if typed.is_empty() => None,
            None => Some(match key {
                DeclarationKey::Slots | DeclarationKey::IdleDays => typed
                    .parse::<u64>()
                    .map_or(DeclaredValue::Text(typed), DeclaredValue::Number),
                _ => DeclaredValue::Text(typed),
            }),
            Some(index) => {
                let mut list = overview.effective(key);
                with_item(&mut list, key, index, (!typed.is_empty()).then_some(typed));
                list
            }
        };
        overview.set(key, value);
        if let Some(index) = field.item {
            let rows = overview.workspace_rows();
            let row = rows
                .iter()
                .position(|row| *row == WorkspaceRow::Item(key, index))
                .or_else(|| rows.iter().position(|row| *row == WorkspaceRow::Key(key)));
            if let Some(row) = row {
                overview.rows[Section::Settings.index()] = row;
            }
        }
        self.schedule_declaration_save();
    }

    fn add_workspace_item(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        if overview.section != Section::Settings || !overview.editable() {
            return;
        }
        let Some(key) = overview
            .selected_workspace_row()
            .and_then(WorkspaceRow::key)
        else {
            return;
        };
        if !is_listed(key, overview) {
            return;
        }
        let len = overview.value(key).map_or(0, item_count);
        overview.edit.field = Some(EditField::new(key, Some(len), String::new()));
    }

    fn remove_workspace_row(&mut self) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        if overview.section != Section::Settings || !overview.editable() {
            return;
        }
        let Some(row) = overview.selected_workspace_row() else {
            return;
        };
        match row {
            WorkspaceRow::Item(key, index) => {
                let mut list = overview.effective(key);
                with_item(&mut list, key, index, None);
                overview.set(key, list);
            }
            // A list goes one entry at a time: a stray `d` on its heading
            // must not take every entry with it.
            WorkspaceRow::Key(key) if is_listed(key, overview) => return,
            WorkspaceRow::Key(key) => overview.set(key, None),
            WorkspaceRow::Action(state) => overview.set_action(state, None),
        }
        let count = overview.row_count();
        let index = Section::Settings.index();
        overview.rows[index] = overview.rows[index].min(count.saturating_sub(1));
        self.schedule_declaration_save();
    }

    fn move_workspace_item(&mut self, delta: isize) {
        let Some(overview) = self.model.space_overview.as_mut() else {
            return;
        };
        if overview.section != Section::Settings || !overview.editable() {
            return;
        }
        let Some(WorkspaceRow::Item(key, index)) = overview.selected_workspace_row() else {
            return;
        };
        let Some(mut value) = overview.effective(key) else {
            return;
        };
        let target = index as isize + delta;
        let moved = match &mut value {
            DeclaredValue::List(items) if target >= 0 && (target as usize) < items.len() => {
                items.swap(index, target as usize);
                true
            }
            DeclaredValue::Commands(commands)
                if target >= 0 && (target as usize) < commands.len() =>
            {
                commands.swap(index, target as usize);
                true
            }
            _ => false,
        };
        if !moved {
            return;
        }
        overview.set(key, Some(value));
        let selected = &mut overview.rows[Section::Settings.index()];
        *selected = (*selected as isize + delta) as usize;
        self.schedule_declaration_save();
    }
}

/// Sets, replaces or removes item `index` of a list value; a value the
/// list does not yet have is started.
fn with_item(
    value: &mut Option<DeclaredValue>,
    key: DeclarationKey,
    index: usize,
    item: Option<String>,
) {
    let commands = matches!(key, DeclarationKey::Setup | DeclarationKey::Gate);
    let mut list = match value.take() {
        Some(DeclaredValue::Commands(commands)) => Items::Commands(commands),
        Some(DeclaredValue::List(items)) => Items::Strings(items),
        Some(other) if commands => Items::Commands(vec![ShellCommand::Line(scalar(&other))]),
        Some(other) => Items::Strings(vec![scalar(&other)]),
        None if commands => Items::Commands(Vec::new()),
        None => Items::Strings(Vec::new()),
    };
    match (&mut list, item) {
        (Items::Strings(items), Some(item)) if index < items.len() => items[index] = item,
        (Items::Strings(items), Some(item)) => items.push(item),
        (Items::Strings(items), None) if index < items.len() => {
            items.remove(index);
        }
        (Items::Commands(commands), Some(item)) if index < commands.len() => {
            commands[index] = match &commands[index] {
                ShellCommand::PerPlatform(spellings) => {
                    let mut spellings = spellings.clone();
                    spellings.posix = Some(item);
                    ShellCommand::PerPlatform(spellings)
                }
                ShellCommand::Line(_) => ShellCommand::Line(item),
            };
        }
        (Items::Commands(commands), Some(item)) => commands.push(ShellCommand::Line(item)),
        (Items::Commands(commands), None) if index < commands.len() => {
            commands.remove(index);
        }
        _ => {}
    }
    *value = match list {
        Items::Strings(items) if items.is_empty() => None,
        Items::Strings(items) => Some(DeclaredValue::List(items)),
        Items::Commands(commands) if commands.is_empty() => None,
        Items::Commands(commands) => Some(DeclaredValue::Commands(commands)),
    };
}

enum Items {
    Strings(Vec<String>),
    Commands(Vec<ShellCommand>),
}

#[cfg(test)]
mod tests {
    use super::{Section, SpaceOverviewView, WorkspaceModel};
    use std::path::PathBuf;
    use uze_terminal::{ClientEvent, Session, SpaceId, SpaceSeat, TabId};

    /// An overview put away for a space that has since closed can never be
    /// opened again: the session's word that the space is gone forgets it,
    /// and the one of a space still open is kept.
    #[test]
    fn a_closed_space_takes_its_put_away_overview_with_it() {
        let root = PathBuf::from("/project");
        let session = Session::new(
            SpaceSeat {
                project: root.clone(),
            },
            80,
            24,
        );
        let open = session.selected_space().id;
        let closed = SpaceId(open.0 + 1);
        let mut model = WorkspaceModel::default();
        for space in [open, closed] {
            let overview = SpaceOverviewView::new(space, root.clone(), TabId(0), true);
            model.put_away_overviews.insert(space, overview);
        }
        model.apply(ClientEvent::SessionUpdated { session }, &[]);
        assert!(model.put_away_overviews.contains_key(&open));
        assert!(!model.put_away_overviews.contains_key(&closed));
    }

    /// Back from any section is the one before it on the bar, wrapping
    /// from the first to the last, and forward undoes it.
    #[test]
    fn stepping_back_lands_on_the_section_before() {
        assert_eq!(Section::Agent.previous(), Section::Worktrees);
        for section in Section::ALL {
            assert_eq!(section.next().previous(), section);
        }
    }
}
