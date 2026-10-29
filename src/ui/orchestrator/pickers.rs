//! The agent picker, the agent's support dropdown and the context menu: what each offers and what it targets.

use super::*;

/// A second `Down(Left)` on the same hit within this window counts as a
/// double-click (enters tab rename); slower than this, it's just another
/// single click.
pub(super) const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// One selectable row of the agent picker: what to show, the `argv` to
/// launch in the new pane if chosen, and what that launch will not be able
/// to do.
pub(super) struct AgentOption {
    pub(super) display_name: String,
    /// The integration the harness belongs to — what an agent is recorded
    /// as running.
    pub(super) integration: String,
    pub(super) command: TabCommand,
    /// Said on the tab when the agent starts a conversation it will not be
    /// able to continue.
    pub(super) continuity_gap: Option<String>,
    /// What its launch takes beyond what UZE composes.
    pub(super) overrides: uze_application::OverrideCatalog,
}

/// Open state of the harness picker a resume asks with when the harness its
/// agent ran with is gone — built fresh each time it opens from
/// `agent_options`, never persisted.
pub(super) struct AgentPicker {
    pub(super) options: Vec<AgentOption>,
    pub(super) selected: usize,
    /// The rect of what asked for it — the popup anchors just under it.
    pub(super) anchor: Rect,
    pub(super) launch: AgentLaunch,
}

/// What a harness is being chosen for.
#[derive(Clone)]
pub(super) enum AgentLaunch {
    /// A new agent, where the new agent's popup put it.
    New(NewPlace),
    /// A preserved task to continue: placement answers with its slot, or
    /// gives it one again on its own branch, and the agent starts there.
    Resume(ResumeTarget),
}

/// Where a new agent goes: the space whose `new` was pressed, which the
/// operator may leave before its checkout is ready, and the directory
/// chosen for it.
#[derive(Clone)]
pub(super) struct NewPlace {
    pub(super) space: SpaceId,
    pub(super) directory: PathBuf,
    /// The words the operator added to this agent's launch for its harness;
    /// none is the launch the workspace makes by default.
    pub(super) overrides: Vec<String>,
}

/// Open state of the "+ new agent" popup (`WorkspaceHit::NewAgentMenu`):
/// the directory the agent runs in, chosen from a listing, and the harness
/// it runs, a field offering the one used last — one step, since the
/// harness is rarely the thing that changes.
pub(super) struct DirectoryStep {
    pub(super) picker: RootPicker,
    pub(super) space: SpaceId,
    /// The "new" button's own rect, which the popup anchors beside.
    pub(super) anchor: Rect,
    /// The listing's rows, fixed when the popup opens so it never resizes
    /// under the typing.
    pub(super) rows: u16,
    /// The harnesses set up here; empty, Enter leads to setting one up.
    pub(super) harnesses: Vec<AgentOption>,
    /// Index into `harnesses` of the one the agent will run.
    pub(super) harness: usize,
    /// The overrides step, while it stands in the listing's place.
    pub(super) overrides: Option<OverridesStep>,
    /// The overrides added in this popup, by harness: moving the harness
    /// shows that harness's, never another's flags.
    pub(super) drafts: BTreeMap<String, Vec<OverrideEntry>>,
    /// The entries used lately, as last read.
    pub(super) recent: Option<RecentOverridesRead>,
    /// The read of them that is out, so a step opened again before it
    /// answers asks nothing more.
    pub(super) recent_asked: Option<(String, PathBuf)>,
}

impl AgentSupportDropdown {
    /// Whose recorded overrides the section names: an agent UZE launched,
    /// on the harness it runs.
    pub(super) fn overrides_of(&self) -> Option<OverridesOf> {
        Some(OverridesOf {
            prompts: self.prompts.clone()?,
            harness: self.key.as_ref()?.0.clone(),
        })
    }
}

impl DirectoryStep {
    /// The harness the agent will run, when one is set up.
    pub(super) fn chosen_harness(&self) -> Option<&AgentOption> {
        self.harnesses.get(self.harness)
    }

    /// Moves the harness on by `step`, round the ones set up. On the
    /// overrides view the entries, the catalog and the sets used lately
    /// follow it, each kept per harness; what was marked in the list, and
    /// why a token was refused, were the other harness's and go.
    pub(super) fn cycle_harness(&mut self, step: isize) {
        let count = self.harnesses.len();
        if count > 0 {
            self.harness = (self.harness as isize + step).rem_euclid(count as isize) as usize;
        }
        if let Some(over) = self.overrides.as_mut() {
            over.marked = None;
            over.top.set(0);
            over.refusal = None;
        }
    }
}

/// A task to put back in a checkout, named by its repository and id.
#[derive(Clone)]
pub(super) struct ResumeTarget {
    pub(super) primary: PathBuf,
    pub(super) task: String,
    /// The agent tab whose checkout was removed from under it, when the
    /// resume was asked for from that row rather than from the preserved
    /// list — the one the revived agent replaces.
    pub(super) replacing: Option<TabId>,
    /// The integration the agent last ran with, when it is known: resuming
    /// launches it again rather than asking, since its conversation is
    /// that harness's.
    pub(super) harness: Option<String>,
}

/// Open state for the agent drawer: what the agent in front runs on, and
/// the prompts it was given. The `(harness, cwd)` key keeps it tied to the
/// exact live agent it was opened over, rather than to a mutable display
/// label or process name — and makes a resolution for some other pane
/// unrenderable here.
pub(super) struct AgentSupportDropdown {
    /// The agent in front, when there is one: what its support answer is
    /// read for.
    pub(super) key: Option<SupportKey>,
    /// The answer for `key`, held by the drawer itself. The client keeps
    /// one support answer, for whatever agent is in front, and replaces it
    /// whenever that agent's harness or directory changes; read from
    /// there, an open drawer stopped being drawn the moment the agent ran
    /// something, while it still held the keyboard.
    pub(super) support: Option<crate::ui::agent_support::AgentSupport>,
    /// The agent's tab, which names it in the overview's header.
    pub(super) tab: TabId,
    /// Whose prompts are listed: an agent UZE launched, in the project it
    /// works in. `None` for a harness started by hand, which no prompt is
    /// recorded against.
    pub(super) prompts: Option<PromptsOf>,
    /// The agent's directory as the operator reads it.
    pub(super) path: String,
    /// What the agent was launched with on its harness beyond what UZE
    /// composes, once read.
    pub(super) overrides: Option<Vec<String>>,
    /// Index into the agent's prompts, newest first.
    pub(super) selected: usize,
}

/// One agent's prompts: the project whose history keeps them, and the agent
/// they were given to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PromptsOf {
    pub(super) project: PathBuf,
    pub(super) agent: String,
}

/// What a right-click-opened [`ContextMenu`] targets — the space or tab
/// the menu's actions act on, or recent's selected row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MenuTarget {
    Space(SpaceId),
    Tab(TabId),
    /// The row recent has selected: a right-click selects it first, and
    /// the menu offers what its keys would, so each action is the key's
    /// own, taking away included with its question.
    Recent,
}

/// Open state of the right-click action menu a space header or agent tab
/// raises. Closing a space/tab is never one click any more — right-click,
/// then confirm the menu's own "close" row — deliberately two steps, so an
/// accidental click can't kill a running agent or an entire space's worth
/// of them; other, non-destructive actions this menu grows (like `rename`)
/// don't need that same two-step guard, but share the same
/// open/navigate/confirm mechanics.
/// Built fresh on each right-click, never persisted.
pub(super) struct ContextMenu {
    pub(super) target: MenuTarget,
    pub(super) items: Vec<Action>,
    /// What each of `items` reads as, in order.
    pub(super) labels: Vec<String>,
    /// Index into `items` the keyboard's Up/Down currently highlights —
    /// same role [`AgentPicker::selected`] plays.
    pub(super) selected: usize,
    /// The right-clicked row's own rect — the popup anchors just under it,
    /// same placement rule [`AgentPicker::anchor`] uses.
    pub(super) anchor: Rect,
    /// The submenu each of `items` opens, beside it; `None` for an entry
    /// that acts.
    pub(super) groups: Vec<Option<MenuGroup>>,
    /// The open group's highlighted choice, when the highlighted entry's
    /// submenu is open.
    pub(super) submenu: Option<usize>,
}

/// What a menu entry opens instead of acting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum MenuGroup {
    /// The harnesses behind [`Action::SwitchHarness`]. One entry opens them
    /// rather than one entry standing for each: the harnesses a machine has
    /// are not something a keymap can name, and a group keeps the menu the
    /// same length however many there are.
    Harnesses(Vec<HarnessChoice>),
    /// What [`Action::RenameSelection`] renames on an agent's row: the
    /// agent, or the project it works in. One entry rather than two
    /// renames side by side, which read as the same word twice.
    Rename,
}

/// A choice made in an open submenu.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SubmenuChoice {
    Harness(HarnessChoice),
    RenameAgent,
    RenameProject,
}

const RENAME_CHOICES: [&str; 2] = ["Agent", "Project"];

impl MenuGroup {
    fn labels(&self) -> Vec<&str> {
        match self {
            Self::Harnesses(harnesses) => harnesses
                .iter()
                .map(|choice| choice.display_name.as_str())
                .collect(),
            Self::Rename => RENAME_CHOICES.to_vec(),
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Harnesses(harnesses) => harnesses.len(),
            Self::Rename => RENAME_CHOICES.len(),
        }
    }

    fn choice(&self, index: usize) -> Option<SubmenuChoice> {
        match self {
            Self::Harnesses(harnesses) => harnesses.get(index).cloned().map(SubmenuChoice::Harness),
            Self::Rename => match index {
                0 => Some(SubmenuChoice::RenameAgent),
                1 => Some(SubmenuChoice::RenameProject),
                _ => None,
            },
        }
    }
}

/// A harness an agent can continue in, as a menu entry offers it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HarnessChoice {
    pub(super) integration: String,
    pub(super) display_name: String,
    pub(super) continuity_gap: Option<String>,
}

impl ContextMenu {
    /// A menu whose entries read as their actions' own labels.
    pub(super) fn new(target: MenuTarget, items: Vec<Action>, anchor: Rect) -> Self {
        let labels = items.iter().map(|action| action.label()).collect();
        let groups = vec![None; items.len()];
        Self {
            target,
            items,
            labels,
            selected: 0,
            anchor,
            groups,
            submenu: None,
        }
    }

    /// Inserts the "Continue in" entry at `index`, opening `harnesses`;
    /// nothing when there are none to continue in.
    pub(super) fn offering_switches(mut self, index: usize, harnesses: Vec<HarnessChoice>) -> Self {
        if !harnesses.is_empty() {
            self.items.insert(index, Action::SwitchHarness);
            self.labels.insert(index, Action::SwitchHarness.label());
            self.groups
                .insert(index, Some(MenuGroup::Harnesses(harnesses)));
        }
        self
    }

    /// Makes the menu's rename entry open a choice of the agent or its
    /// project, when `project_known`; it stays a plain rename otherwise.
    pub(super) fn offering_project_rename(mut self, project_known: bool) -> Self {
        let rename = self
            .items
            .iter()
            .position(|action| *action == Action::RenameSelection);
        if let Some(index) = rename.filter(|_| project_known) {
            self.groups[index] = Some(MenuGroup::Rename);
        }
        self
    }

    /// The group entry `index` opens, if it opens one.
    pub(super) fn group(&self, index: usize) -> Option<&MenuGroup> {
        self.groups.get(index)?.as_ref()
    }

    /// The open submenu's choices, as they read.
    pub(super) fn submenu_labels(&self) -> Option<Vec<&str>> {
        self.submenu?;
        Some(self.group(self.selected)?.labels())
    }

    /// The choice highlighted in the open submenu.
    pub(super) fn submenu_choice(&self) -> Option<SubmenuChoice> {
        self.group(self.selected)?.choice(self.submenu?)
    }

    /// Opens entry `index`'s submenu on its first harness; `false` for an
    /// entry that opens none.
    pub(super) fn open_submenu(&mut self, index: usize) -> bool {
        if self.group(index).is_none() {
            return false;
        }
        self.selected = index;
        self.submenu = Some(0);
        true
    }

    /// A menu whose entries read as the target calls each action, the way
    /// recent's foot does: a row resumes or joins, discards or forgets.
    /// An entry that asks before acting says so with an ellipsis.
    pub(super) fn worded(target: MenuTarget, offers: Vec<(Action, &str)>, anchor: Rect) -> Self {
        let labels = offers
            .iter()
            .map(|(action, word)| {
                let mut letters = word.chars();
                let first = letters.next().map(|c| c.to_uppercase().to_string());
                let ellipsis = if action.destructive() { "…" } else { "" };
                format!(
                    "{}{}{ellipsis}",
                    first.unwrap_or_default(),
                    letters.as_str()
                )
            })
            .collect();
        Self {
            target,
            groups: vec![None; offers.len()],
            items: offers.into_iter().map(|(action, _)| action).collect(),
            labels,
            selected: 0,
            anchor,
            submenu: None,
        }
    }
}
