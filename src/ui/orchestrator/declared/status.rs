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
        NodeKind::Text { spans, .. } => spans.iter().all(|span| span.text.trim().is_empty()),
        _ => false,
    }
}

/// `view` with every text cut at its end rather than wrapped: a status is
/// one row whatever it answers.
fn folded(mut view: View) -> View {
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
