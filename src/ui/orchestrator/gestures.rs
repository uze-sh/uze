//! Gestures in flight: renaming a tab or a space, dragging a tab, a space, an edge or a diagram.

use super::*;

/// What [`WorkspaceModel::renaming`] is currently editing — a tab or a
/// space header both use the exact same inline-edit interaction (double-
/// click to enter, Enter/Esc/typing and the caret keys to edit, click-away to
/// discard), so one buffer serves both; this just says which request to
/// send on commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RenameTarget {
    Tab(TabId),
    Space(SpaceId),
}

/// The label being typed over a tab or a space, and where in it typing
/// lands. The caret is a byte offset that only ever sits on a character
/// boundary, so every edit is a plain `String` operation at it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct RenameBuffer {
    pub(super) text: String,
    pub(super) caret: usize,
}

impl RenameBuffer {
    /// A buffer holding `text`, with the caret after it — where someone
    /// who meant to append expects it, and one `home` from replacing.
    pub(super) fn new(text: String) -> Self {
        let caret = text.len();
        Self { text, caret }
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// The text either side of the caret.
    pub(super) fn split(&self) -> (&str, &str) {
        self.text.split_at(self.caret)
    }

    pub(super) fn insert(&mut self, character: char) {
        self.text.insert(self.caret, character);
        self.caret += character.len_utf8();
    }

    pub(super) fn insert_str(&mut self, text: &str) {
        self.text.insert_str(self.caret, text);
        self.caret += text.len();
    }

    pub(super) fn erase_back(&mut self) {
        if let Some(previous) = self.previous_boundary() {
            self.text.replace_range(previous..self.caret, "");
            self.caret = previous;
        }
    }

    pub(super) fn erase_forward(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.text.replace_range(self.caret..next, "");
        }
    }

    pub(super) fn left(&mut self) {
        if let Some(previous) = self.previous_boundary() {
            self.caret = previous;
        }
    }

    pub(super) fn right(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.caret = next;
        }
    }

    pub(super) fn home(&mut self) {
        self.caret = 0;
    }

    pub(super) fn end(&mut self) {
        self.caret = self.text.len();
    }

    pub(super) fn previous_boundary(&self) -> Option<usize> {
        self.text[..self.caret]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
    }

    pub(super) fn next_boundary(&self) -> Option<usize> {
        self.text[self.caret..]
            .chars()
            .next()
            .map(|character| self.caret + character.len_utf8())
    }
}

/// Which set of tabs a drag-to-reorder gesture is confined to — the exact
/// grouping `render.rs` already filters `Space.tabs` by to build the
/// sidebar's agent list (`agent_tabs`) and the tab strip (`strip`). A drag
/// never offers, nor accepts, a drop target from outside the group it
/// began in — see `tab_drag_group`/`tab_drag_group_members`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TabDragGroup {
    /// The agent rows of one space's sidebar list, by space id and by
    /// group: the column draws the agents in the space's own root above
    /// the isolated ones, and a row dragged out of its group would have
    /// to land somewhere the order it was dropped into does not exist.
    Agents(SpaceId, AgentGroup),
    /// The tabs of one strip: shells opened alongside one agent tab, or —
    /// when `None` — a space's own shells with no agent selected. Matches
    /// `Tab::agent`'s own vocabulary.
    Strip(SpaceId, Option<TabId>),
}

/// A press on the code surface's navigator edge, before it has said what
/// it is.
///
/// The edge is one line doing two jobs — the split moves sideways, the
/// list scrolls down — and a press carries no direction. So nothing is
/// decided when it lands: the first movement says which, by whichever of
/// the two distances is larger, and it stays said until release. A press
/// that never moves is a click, which on a scrollbar means "show me
/// here".
///
/// The same shape `DraggingTab` uses, and for the same reason: a gesture
/// that has not happened yet cannot be classified, and guessing early is
/// how a drag becomes the wrong one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DiagramGrab {
    pub(super) last: (u16, u16),
    pub(super) moved: bool,
    pub(super) click: ViewHit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EdgeDrag {
    pub(super) origin: (u16, u16),
    /// `None` until the pointer has moved.
    pub(super) intent: Option<EdgeIntent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EdgeIntent {
    Scroll,
    Resize,
}

impl EdgeDrag {
    pub(super) fn armed_at(column: u16, row: u16) -> Self {
        Self {
            origin: (column, row),
            intent: None,
        }
    }

    /// What this drag is, once the pointer has reached `(column, row)`.
    ///
    /// Ties go to scrolling: an exact diagonal is nobody's intention, and
    /// on a control that is mostly a scrollbar that is the likelier of
    /// the two.
    pub(super) fn decide(&mut self, column: u16, row: u16) -> Option<EdgeIntent> {
        if self.intent.is_none() {
            let sideways = column.abs_diff(self.origin.0);
            let along = row.abs_diff(self.origin.1);
            if sideways == 0 && along == 0 {
                return None;
            }
            self.intent = Some(match sideways > along {
                true => EdgeIntent::Resize,
                false => EdgeIntent::Scroll,
            });
        }
        self.intent
    }
}

#[cfg(test)]
mod edge_drag_tests {
    use super::{EdgeDrag, EdgeIntent};

    /// A press carries no direction, so it decides nothing. Guessing when
    /// it lands is how a resize becomes a scroll.
    #[test]
    fn a_press_that_has_not_moved_is_neither_gesture() {
        let mut drag = EdgeDrag::armed_at(40, 10);
        assert_eq!(drag.decide(40, 10), None);
        assert_eq!(drag.intent, None);
    }

    #[test]
    fn the_first_movement_says_which_gesture_it_is() {
        let mut sideways = EdgeDrag::armed_at(40, 10);
        assert_eq!(sideways.decide(44, 11), Some(EdgeIntent::Resize));

        let mut along = EdgeDrag::armed_at(40, 10);
        assert_eq!(along.decide(41, 16), Some(EdgeIntent::Scroll));
    }

    /// Once said, it stays said: a hand that wanders must not switch
    /// gestures halfway through one.
    #[test]
    fn a_decided_drag_does_not_change_its_mind() {
        let mut drag = EdgeDrag::armed_at(40, 10);
        assert_eq!(drag.decide(48, 10), Some(EdgeIntent::Resize));
        assert_eq!(
            drag.decide(40, 40),
            Some(EdgeIntent::Resize),
            "still a resize, however far down the pointer then goes"
        );
    }

    /// An exact diagonal is nobody's intention; on a control that is
    /// mostly a scrollbar, that is the likelier of the two.
    #[test]
    fn a_tie_scrolls() {
        let mut drag = EdgeDrag::armed_at(40, 10);
        assert_eq!(drag.decide(43, 13), Some(EdgeIntent::Scroll));
    }
}

/// A pending reorder drop position, in exactly the shape
/// [`ClientRequest::ReorderTab`] and [`ClientRequest::ReorderSpace`] expect
/// it: before a specific item, or at the end of the list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingDrop<Id = TabId> {
    Before(Id),
    End,
}

impl<Id: Copy> PendingDrop<Id> {
    pub(super) fn as_before(self) -> Option<Id> {
        match self {
            PendingDrop::Before(tab) => Some(tab),
            PendingDrop::End => None,
        }
    }
}

/// A tab being dragged for reordering. Armed once the pointer moves past
/// [`TAB_DRAG_THRESHOLD`] from where the press started, so a plain click —
/// still handled immediately and unchanged on `MouseEventKind::Down` — is
/// never mistaken for a drag. Cleared on release either way, and pruned
/// early if the dragged tab disappears from a `Session` update received
/// mid-drag (see `WorkspaceModel::prune_dragging_tab`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DraggingTab {
    pub(super) tab: TabId,
    pub(super) group: TabDragGroup,
    /// Row (`Agents`) or column (`Strip`) the press started at.
    pub(super) origin: u16,
    pub(super) armed: bool,
    /// Where `tab` would land if released right now. `None` while unarmed,
    /// or whenever the pointer isn't currently over a valid drop position
    /// for `group` — a release in either state is a no-op.
    pub(super) pending: Option<PendingDrop>,
}

impl DraggingTab {
    /// Whether `tab` — the last member of `group` when `is_last` — is
    /// where this drag's current pending drop would land. Used by the
    /// sidebar and tab-strip renderers to place the one insertion
    /// indicator each draws; `false` for a drag that isn't `armed`, is
    /// over a different group than the one being rendered, or has no
    /// pending drop right now (the pointer has left the group's area).
    pub(super) fn is_pending_drop_row(
        self,
        group: TabDragGroup,
        tab: TabId,
        is_last: bool,
    ) -> bool {
        if !self.armed || self.group != group {
            return false;
        }
        match self.pending {
            Some(PendingDrop::Before(before)) => before == tab,
            Some(PendingDrop::End) => is_last,
            None => false,
        }
    }
}

/// How far (rows for `Agents`, columns for `Strip`) the pointer must move
/// from a press before it's treated as a reorder drag rather than a plain
/// click — small enough that dragging still feels immediate, large enough
/// to rule out an ordinary click's own jitter.
pub(super) const TAB_DRAG_THRESHOLD: u16 = 2;

/// A space being dragged by its header to another place in the sidebar.
/// Armed, like [`DraggingTab`], only once the pointer has travelled
/// [`TAB_DRAG_THRESHOLD`] rows, so a click on the header stays a click.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DraggingSpace {
    pub(super) space: SpaceId,
    /// The row the press started at.
    pub(super) origin: u16,
    pub(super) armed: bool,
    pub(super) pending: Option<PendingDrop<SpaceId>>,
}

impl DraggingSpace {
    pub(super) fn armed_at(space: SpaceId, row: u16) -> Self {
        Self {
            space,
            origin: row,
            armed: false,
            pending: None,
        }
    }

    /// Where this drag would land with the pointer on `row`, among the
    /// space blocks the last frame drew.
    pub(super) fn follow(
        &mut self,
        row: u16,
        hits: &[(Rect, WorkspaceHit)],
        session: &Session,
        sidebar: Rect,
    ) {
        if !self.armed {
            self.armed = row.abs_diff(self.origin) >= TAB_DRAG_THRESHOLD;
        }
        self.pending = self
            .armed
            .then(|| {
                let members: Vec<(u16, u16, SpaceId)> = space_blocks(hits, session, sidebar)
                    .into_iter()
                    .filter(|(_, space)| *space != self.space)
                    .map(|(rect, space)| (rect.y, rect.bottom(), space))
                    .collect();
                pending_drop(&members, 2, row, self.origin)
            })
            .flatten();
    }
}

/// Each space's block in the sidebar as the last frame drew it — its
/// header and every row under it that selects the space or one of its
/// agents — top to bottom. Read off the frame's own hits, like
/// `tab_drag_group_members`, so a drop can never be offered where
/// nothing was drawn: a minimized space is its header alone.
pub(super) fn space_blocks(
    hits: &[(Rect, WorkspaceHit)],
    session: &Session,
    sidebar: Rect,
) -> Vec<(Rect, SpaceId)> {
    let mut blocks: std::collections::BTreeMap<SpaceId, Rect> = std::collections::BTreeMap::new();
    for (rect, hit) in hits {
        if rect.x >= sidebar.right() {
            continue;
        }
        let space = match hit {
            WorkspaceHit::SelectSpace(space) => Some(*space),
            WorkspaceHit::SelectTab(tab) => session
                .workspace
                .spaces
                .iter()
                .find(|space| space.tabs.iter().any(|candidate| candidate.id == *tab))
                .map(|space| space.id),
            _ => None,
        };
        if let Some(space) = space {
            blocks
                .entry(space)
                .and_modify(|merged| *merged = merged.union(*rect))
                .or_insert(*rect);
        }
    }
    let mut blocks: Vec<(Rect, SpaceId)> = blocks
        .into_iter()
        .map(|(space, rect)| (rect, space))
        .collect();
    blocks.sort_by_key(|(rect, _)| rect.y);
    blocks
}
