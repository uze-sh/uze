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

/// What a right-click-opened [`ContextMenu`] targets — the space or tab its
/// the menu's actions act on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MenuTarget {
    Space(SpaceId),
    Tab(TabId),
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
    /// Index into `items` the keyboard's Up/Down currently highlights —
    /// same role [`AgentPicker::selected`] plays.
    pub(super) selected: usize,
    /// The right-clicked row's own rect — the popup anchors just under it,
    /// same placement rule [`AgentPicker::anchor`] uses.
    pub(super) anchor: Rect,
}
