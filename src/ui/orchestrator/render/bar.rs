//! The bar under the agent area: what plugins add to the workspace, kept
//! apart from the tab strip, whose doors are UZE's own. The tab strip's
//! mirror: a hairline toward the pane, then one row, on the same column.

use super::*;
use crate::ui::orchestrator::declared::{HostedSlot, drawn_tree, folded};
use tab_strip::{GroupButton, button_group, group_width};
use uze_extensions::contract::SurfaceSlot;
use uze_extensions::view::View;

/// The most an item of the bar takes, so one extension's line cannot push
/// every other off it.
const ITEM_MAX: u16 = 32;
/// The columns kept between the bar's groups and items.
const GAP: u16 = 1;
/// The most a plugin's door's name takes of the bar: a title is the
/// plugin's to make long.
const DOOR_LABEL: usize = 16;

/// Draws the bar into `area`: on its left the doors to the views and
/// drawers plugins declare, then the items that stand on the left; on its
/// right the items that stand on the right. What does not fit is left out
/// from the end inward. Answers whether anything in it moves with the
/// clock.
pub(super) fn render_bar(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let row = Rule::new(Edge::Top)
        .padding(Padding::new(0, 1, 0, 0))
        .render(frame, area);
    let mut left_x = row.x;
    let mut right_x = row.right();
    let mut animates = false;

    // A plugin's doors, lowercase whatever its manifest titles them: every
    // button UZE draws is, and a plugin's is drawn by UZE. A surface an
    // item opens has that item for its door, so it gets no second one.
    let opened = model.opened_by_items();
    let doors: Vec<GroupButton> = model
        .declared_views()
        .filter(|slot| !opened.contains(&slot.id))
        .map(|slot| GroupButton {
            hit: WorkspaceHit::OpenDeclared(slot.id),
            label: door_label(&slot.surface.title),
            hue: theme::color(Token::TextSecondary),
            strong: false,
            lit: model.open_slot == Some(slot.id),
            switch: true,
        })
        .chain(
            model
                .declared_drawers()
                .filter(|slot| !opened.contains(&slot.id))
                .map(|slot| GroupButton {
                    hit: WorkspaceHit::ToggleDrawer(slot.id),
                    label: door_label(&slot.surface.title),
                    hue: theme::color(Token::TextSecondary),
                    strong: false,
                    lit: model.open_drawer() == Some(slot.id),
                    switch: true,
                }),
        )
        .collect();
    if !doors.is_empty() {
        let width = group_width(&doors).min(row.width);
        let rect = Rect::new(left_x, row.y, width, 1);
        let (spans, group_hits) =
            button_group(model, &doors, Token::SurfaceRaised, (rect.x, rect.y));
        hits.extend(group_hits);
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        left_x = rect.right().saturating_add(GAP);
    }

    let items: Vec<(&HostedSlot, View)> = model
        .bar_items()
        .into_iter()
        .filter_map(|slot| {
            let view = model
                .remembered
                .declared
                .engine(&slot.owner)?
                .view(
                    &slot.surface.id,
                    uze_extensions::view::Size {
                        width: ITEM_MAX,
                        height: 1,
                    },
                )
                .and_then(drawn_tree)?;
            // One line: a text is cut where the item ends, never wrapped.
            Some((slot, folded(view)))
        })
        .collect();
    // What it asks for, and the inset the host draws a tree inside.
    let width_of = |view: &View| match &view.content {
        uze_extensions::view::Content::Tree { root, .. } => {
            crate::ui::extension_view::tree::tree_width(root)
                .saturating_add(uze_extensions::view::PROSE_INSET * 2)
                .clamp(1, ITEM_MAX)
        }
        _ => 0,
    };
    // The right end goes down first, from the edge inward, so an item on
    // the left can never take the room of one on the right.
    for (slot, view) in items
        .iter()
        .filter(|(slot, _)| matches!(slot.surface.slot, SurfaceSlot::Bar { right: true }))
        .rev()
    {
        let width = width_of(view);
        let x = right_x.saturating_sub(width);
        if x < left_x {
            break;
        }
        animates |= render_item(
            frame,
            Rect::new(x, row.y, width, 1),
            model,
            slot,
            view,
            hits,
        );
        right_x = x.saturating_sub(GAP);
    }
    for (slot, view) in items
        .iter()
        .filter(|(slot, _)| matches!(slot.surface.slot, SurfaceSlot::Bar { right: false }))
    {
        let width = width_of(view);
        if left_x.saturating_add(width) > right_x {
            break;
        }
        animates |= render_item(
            frame,
            Rect::new(left_x, row.y, width, 1),
            model,
            slot,
            view,
            hits,
        );
        left_x = left_x.saturating_add(width).saturating_add(GAP);
    }
    animates
}

/// One item: its view, then a target under it for what it opens, so a
/// button it draws answers first and the rest of it opens its surface.
fn render_item(
    frame: &mut ratatui::Frame<'_>,
    rect: Rect,
    model: &WorkspaceModel,
    slot: &HostedSlot,
    view: &View,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let animates = render_docked_slot(frame, rect, model, slot.id, view, hits);
    if let Some(target) = model.bar_target(slot) {
        hits.push((rect, target));
    }
    animates
}

/// A door's label: its surface's title in lowercase, cut to fit.
fn door_label(title: &str) -> String {
    text::elide(&title.to_lowercase(), DOOR_LABEL)
}
