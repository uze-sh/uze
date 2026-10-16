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
    pub(super) command: Vec<String>,
    /// Said on the tab when the agent starts a conversation it will not be
    /// able to continue.
    pub(super) continuity_gap: Option<String>,
}

/// Open state of the "+ new agent" popup (`WorkspaceHit::NewAgentMenu`) —
/// built fresh each time it opens from `agent_options`, never persisted.
pub(super) struct AgentPicker {
    pub(super) options: Vec<AgentOption>,
    pub(super) selected: usize,
    /// The "new" button's own rect — the popup anchors just
    /// under it.
    pub(super) anchor: Rect,
    /// A preserved task to continue: placement answers with its slot, or
    /// gives it one again on its own branch, and the agent starts there.
    pub(super) resume: Option<ResumeTarget>,
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
    pub(super) key: SupportKey,
    /// The answer for `key`, held by the drawer itself. The client keeps
    /// one support answer, for whatever agent is in front, and replaces it
    /// whenever that agent's harness or directory changes; read from
    /// there, an open drawer stopped being drawn the moment the agent ran
    /// something, while it still held the keyboard.
    pub(super) support: Option<crate::ui::agent_support::AgentSupport>,
    /// The agent UZE launched in the tab, which is what "this agent's
    /// prompts" is matched on. `None` for a harness started by hand, whose
    /// drawer can only offer the space's.
    pub(super) agent: Option<String>,
    /// The space's root: the history is kept per space, keyed on it.
    pub(super) space_root: PathBuf,
    /// The agent's directory as the operator reads it, taken when the
    /// drawer opened.
    pub(super) path: String,
    pub(super) scope: PromptScope,
    /// Index into the prompts `scope` shows, newest first.
    pub(super) selected: usize,
    /// `x` asked whether to clear the space's history; a second `x`
    /// answers, anything else withdraws the question.
    pub(super) clearing: bool,
}

impl AgentSupportDropdown {
    /// The prompts `scope` shows, newest first.
    pub(super) fn prompts<'a>(
        &self,
        history: &'a [uze_application::PromptEntry],
    ) -> Vec<&'a uze_application::PromptEntry> {
        history
            .iter()
            .filter(|entry| match self.scope {
                PromptScope::Space => true,
                PromptScope::Agent => self.agent.is_some() && entry.agent == self.agent,
            })
            .collect()
    }
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
    /// The harnesses behind the [`Action::SwitchHarness`] entry, beside
    /// `items`; `None` for every other entry. One entry opens them as a
    /// submenu rather than one entry standing for each: the harnesses a
    /// machine has are not something a keymap can name, and a group keeps
    /// the menu the same length however many there are.
    pub(super) groups: Vec<Option<Vec<HarnessChoice>>>,
    /// The open group's highlighted harness, when the highlighted entry's
    /// submenu is open.
    pub(super) submenu: Option<usize>,
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

    /// Inserts the "Continue in…" entry at `index`, opening `harnesses`;
    /// nothing when there are none to continue in.
    pub(super) fn offering_switches(mut self, index: usize, harnesses: Vec<HarnessChoice>) -> Self {
        if !harnesses.is_empty() {
            self.items.insert(index, Action::SwitchHarness);
            self.labels
                .insert(index, format!("{}…", Action::SwitchHarness.label()));
            self.groups.insert(index, Some(harnesses));
        }
        self
    }

    /// The group entry `index` opens, if it opens one.
    pub(super) fn group(&self, index: usize) -> Option<&[HarnessChoice]> {
        self.groups.get(index)?.as_deref()
    }

    /// The open submenu's harnesses, as they read.
    pub(super) fn submenu_labels(&self) -> Option<Vec<&str>> {
        self.submenu?;
        let group = self.group(self.selected)?;
        Some(
            group
                .iter()
                .map(|choice| choice.display_name.as_str())
                .collect(),
        )
    }

    /// The harness highlighted in the open submenu.
    pub(super) fn submenu_choice(&self) -> Option<HarnessChoice> {
        self.group(self.selected)?.get(self.submenu?).cloned()
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
