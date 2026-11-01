//! A plugin's status rows, one line each in the sidebar's header for the
//! checkout in front, in the order the operator gave them on the
//! Extensions screen.
//!
//! A row is folded to the column, never wrapped: every text in it is cut
//! with an ellipsis where the column ends. An answer that draws nothing
//! frees its row rather than leaving a blank one.

use super::*;
use uze_extensions::view::{Node, NodeKind};

impl WorkspaceModel {
    /// The order the operator gave the status rows: the modal's own while
    /// it is open, so a move there is drawn behind it at once.
    fn status_order(&self) -> &[String] {
        match &self.manage {
            Some(manage) => &manage.status_order,
            None => &self.management_layout.status_order,
        }
    }

    /// Each status row the header draws, `width` columns wide, in the
    /// operator's order: an extension's rows together, in the order it
    /// declares them. An answer with nothing to draw takes no row.
    pub(in crate::ui::orchestrator) fn declared_statuses(&self, width: u16) -> Vec<(SlotId, View)> {
        let declared = &self.remembered.declared;
        // A row is about the checkout in front, so it stands only where
        // its extension is enabled, as a band does.
        let in_front = self.project_of_pane(self.focused_pane());
        let shown: Vec<&HostedSlot> = declared
            .slots
            .iter()
            .filter(|slot| {
                slot.surface.slot == SurfaceSlot::Status
                    && slot.owner.declared().is_some()
                    && self.offers_declared(&slot.owner)
                    && in_front
                        .as_ref()
                        .is_some_and(|project| self.enabled_in(&slot.owner, project))
            })
            .collect();
        let mut owners: Vec<String> = Vec::new();
        for slot in &shown {
            let key = slot.owner.switch_id();
            if !owners.contains(&key) {
                owners.push(key);
            }
        }
        let space = Size { width, height: 1 };
        crate::ui::status_order::arranged(self.status_order(), owners.iter().map(String::as_str))
            .into_iter()
            .flat_map(|owner| {
                shown
                    .iter()
                    .filter(move |slot| slot.owner.switch_id() == owner)
            })
            .filter_map(|slot| {
                let view = declared
                    .engine(&slot.owner)?
                    .view(&slot.surface.id, space)
                    .and_then(drawn_tree)?;
                Some((slot.id, folded(view)))
            })
            .collect()
    }
}

/// `view`, when what it answered draws something: a waiting, a refusal or
/// a tree with nothing in it is no row and no band content.
pub(in crate::ui::orchestrator) fn drawn_tree(view: View) -> Option<View> {
    match &view.content {
        uze_extensions::view::Content::Tree { root, .. } if !draws_nothing(root) => Some(view),
        _ => None,
    }
}

fn draws_nothing(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Stack { .. } | NodeKind::Row { .. } | NodeKind::Grid { .. } => {
            node.children.iter().all(draws_nothing)
        }
        // A ground or a border is drawn whatever the box holds.
        NodeKind::Box(layout) if layout.ground.is_none() && layout.border.is_none() => {
            node.children.iter().all(draws_nothing)
        }
        NodeKind::Text { spans, .. } => spans.iter().all(|span| span.text.trim().is_empty()),
        // An icon the glyph set has no glyph for, holding no room of its own.
        NodeKind::Icon { glyph, .. } => {
            node.min_width == 0 && !node.grow && crate::ui::theme::icon(glyph).1 == 0
        }
        _ => false,
    }
}

/// `view` with every text cut at its end rather than wrapped: a status is
/// one row whatever it answers.
pub(in crate::ui::orchestrator) fn folded(mut view: View) -> View {
    if let uze_extensions::view::Content::Tree { root, .. } = &mut view.content {
        cut_texts(root);
    }
    view
}

fn cut_texts(node: &mut Node) {
    if let NodeKind::Text { truncate, .. } = &mut node.kind {
        *truncate = true;
    }
    node.children.iter_mut().for_each(cut_texts);
}

#[cfg(test)]
mod tests {
    use super::drawn_tree;
    use uze_extensions::view::{Content, IconGlyph, Layout, Node, NodeKind, Role, View};

    fn answering(root: Node) -> View {
        View {
            tabs: Vec::new(),
            title: Vec::new(),
            caption: Vec::new(),
            navigator: None,
            content: Content::Tree {
                heading: String::new(),
                scroll: 0,
                root,
            },
            footer: Vec::new(),
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: Layout::Sidebar,
            trail: Vec::new(),
            label: String::new(),
        }
    }

    /// An answer whose only mark is an icon the glyph set draws nothing
    /// for is no row: a status of nothing but a blank icon would hold an
    /// empty slot, separators and all.
    #[test]
    fn an_answer_of_only_a_blank_icon_takes_no_row() {
        let mut row = Node::new(
            0,
            NodeKind::Row {
                gap: 1,
                align: Default::default(),
            },
        );
        row.children = vec![Node::new(
            1,
            NodeKind::Icon {
                glyph: IconGlyph::Unknown,
                role: Role::Info,
            },
        )];
        assert!(drawn_tree(answering(row)).is_none());
    }
}
