//! The bar under the agent area: what plugins add to the workspace, kept
//! apart from the tab strip, whose doors are UZE's own. The tab strip's
//! mirror: a hairline toward the pane, then one row, on the same column.

use super::*;
use crate::ui::orchestrator::declared::{HostedSlot, folded};
use tab_strip::{GroupButton, button_group, group_width};
use uze_extensions::contract::SurfaceSlot;
use uze_extensions::view::View;

/// The most an item of the bar takes, so one extension's line cannot push
/// every other off it.
const ITEM_MAX: u16 = super::BAR_ITEM_MAX;
/// The columns kept between the bar's groups and items.
const GAP: u16 = 1;
/// The air between two items, counted from what each draws rather than
/// from the room it asked for, whose inset doubled the gap.
const ITEM_GAP: u16 = 2;
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

    // The restart into an installed release takes the right end before
    // anything a plugin puts there: it is UZE's, and the one thing in the
    // bar that is news. Text on the bar's own ground, in the hue of what
    // went well: no fill behind it, and under the pointer a stronger
    // version of its own green, at the same weight.
    if let Some(version) = model.release_waiting() {
        let label = format!("{} v{version} · restart", theme::glyph(Symbol::ArrowUp));
        let width = (Span::raw(label.as_str()).width() as u16).min(row.width);
        let rect = Rect::new(right_x.saturating_sub(width), row.y, width, 1);
        let ink = match chip_state(model, WorkspaceHit::RestartIntoRelease) {
            ChipState::Resting => theme::color(Token::StateSuccess),
            ChipState::Hovered => theme::lifted(Token::StateSuccess),
            ChipState::Pressed => theme::lifted_ink(theme::lifted(Token::StateSuccess)),
        };
        frame.render_widget(
            Paragraph::new(Span::styled(label, Style::default().fg(ink))),
            rect,
        );
        hits.push((rect, WorkspaceHit::RestartIntoRelease));
        right_x = rect.x.saturating_sub(GAP);
    }

    // A plugin's doors, lowercase whatever its manifest titles them: every
    // button UZE draws is, and a plugin's is drawn by UZE. A surface an
    // item opens has that item for its door, so it gets no second one.
    let opened = model.opened_by_items();
    let doors: Vec<GroupButton> = model
        .bar_doors()
        .into_iter()
        .filter(|slot| !opened.contains(&slot.id))
        .map(|slot| {
            let (hit, lit) = match slot.is_drawer() {
                true => (
                    WorkspaceHit::ToggleDrawer(slot.id),
                    model.open_drawer() == Some(slot.id),
                ),
                false => (
                    WorkspaceHit::OpenDeclared(slot.id),
                    model.open_slot == Some(slot.id),
                ),
            };
            GroupButton {
                hit,
                label: door_label(&slot.surface.title),
                hue: theme::color(Token::TextSecondary),
                strong: false,
                lit,
                switch: true,
            }
        })
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
            let view = model.bar_item_view(slot)?;
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
        let rect = Rect::new(x, row.y, width, 1);
        animates |= render_item(frame, rect, model, slot, view, hits);
        let drawn_from = drawn_columns(frame, rect).map_or(x, |(first, _)| first);
        right_x = drawn_from.saturating_sub(ITEM_GAP);
    }
    for (slot, view) in items
        .iter()
        .filter(|(slot, _)| matches!(slot.surface.slot, SurfaceSlot::Bar { right: false }))
    {
        let width = width_of(view);
        if left_x.saturating_add(width) > right_x {
            break;
        }
        let rect = Rect::new(left_x, row.y, width, 1);
        animates |= render_item(frame, rect, model, slot, view, hits);
        let drawn_to = drawn_columns(frame, rect).map_or(rect.right(), |(_, last)| last + 1);
        left_x = drawn_to.saturating_add(ITEM_GAP);
    }
    animates
}

/// The first and last columns of `rect` an item drew something in.
fn drawn_columns(frame: &mut ratatui::Frame<'_>, rect: Rect) -> Option<(u16, u16)> {
    let buffer = frame.buffer_mut();
    let drawn = |x: u16| {
        buffer
            .cell((x, rect.y))
            .is_some_and(|cell| cell.symbol() != " ")
    };
    let first = (rect.x..rect.right()).find(|&x| drawn(x))?;
    let last = (rect.x..rect.right()).rev().find(|&x| drawn(x))?;
    Some((first, last))
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
    let drawn_from = hits.len();
    let animates = render_docked_slot(frame, rect, model, slot.id, view, hits);
    if matches!(slot.surface.slot, SurfaceSlot::Bar { right: true }) {
        flush_right(frame, rect, &mut hits[drawn_from..]);
    }
    if let Some(target) = model.bar_target(slot) {
        // The target is what the item drew, not the room it asked for: the
        // next item stands in that room's margin, and must answer there.
        let drawn = drawn_columns(frame, rect).map_or(rect, |(first, last)| {
            Rect::new(first, rect.y, last + 1 - first, 1)
        });
        // A door like the bar's own: held back at rest, lifted in its
        // own hues under the pointer.
        match model.hovered.as_ref() == Some(&target) {
            true => widget::lift(frame, drawn),
            false => widget::hold_back(frame, drawn),
        }
        hits.push((drawn, target));
    }
    animates
}

/// Moves what an item on the right drew against the bar's right end. Its
/// width is what it asks for, and a field that can grow (an `Elapsed`
/// keeping room for `1h04m`) asks for more than it draws now: the room it
/// keeps then stood at the end, a gap between the item and the edge. Kept
/// on the inside instead, the item never moves while its text grows into
/// it. Its targets move with it.
fn flush_right(frame: &mut ratatui::Frame<'_>, rect: Rect, hits: &mut [(Rect, WorkspaceHit)]) {
    let buffer = frame.buffer_mut();
    let row = rect.y;
    let last = (rect.x..rect.right()).rev().find(|&x| {
        buffer
            .cell((x, row))
            .is_some_and(|cell| cell.symbol() != " ")
    });
    let Some(last) = last else {
        return;
    };
    let shift = rect.right().saturating_sub(1).saturating_sub(last);
    if shift == 0 {
        return;
    }
    let blank = buffer[(rect.right() - 1, row)].clone();
    for x in (rect.x..=last).rev() {
        let cell = buffer[(x, row)].clone();
        buffer[(x + shift, row)] = cell;
    }
    for x in rect.x..rect.x + shift {
        buffer[(x, row)] = blank.clone();
    }
    for (hit, _) in hits {
        hit.x = hit
            .x
            .saturating_add(shift)
            .min(rect.right().saturating_sub(hit.width));
    }
}

/// A door's label: its surface's title in lowercase, cut to fit.
fn door_label(title: &str) -> String {
    text::elide(&title.to_lowercase(), DOOR_LABEL)
}
