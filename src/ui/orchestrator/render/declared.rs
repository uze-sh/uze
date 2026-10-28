//! What the host draws around a plugin's view: the error over its last
//! good draw, the inspector beside it, a plugin's drawer docked beside
//! the pane, its status rows in the sidebar's header, its bands over the
//! agent's pane and its overlay over the frame.

use super::*;
use crate::ui::extension_view::{SECTION_AIR, SECTION_CHROME};

/// The pane, split for the inspector when it is open beside a declared
/// view: the view keeps what the drawer leaves.
pub(super) fn declared_split(model: &WorkspaceModel, pane: Rect) -> (Rect, Option<Rect>) {
    if model.open_slot.is_none() || !model.inspecting {
        return (pane, None);
    }
    let width = widget::drawer::width(model.overview_drawer_width, pane.width);
    let room = Rect::new(pane.x, pane.y, pane.width - width, pane.height);
    let drawer = Rect::new(room.right(), pane.y, width, pane.height);
    // The size the author tries the surface at, within the room it has.
    let surface = match model.inspected_size() {
        Some((columns, rows)) => Rect {
            width: room.width.min(columns),
            height: room.height.min(rows),
            ..room
        },
        None => room,
    };
    (surface, Some(drawer))
}

/// The open declared view's error, over the bottom of its content and
/// above its footer, and the inspector in the room `declared_split` left.
pub(super) fn render_declared_overlays(
    frame: &mut ratatui::Frame<'_>,
    surface: Rect,
    inspector: Option<Rect>,
    model: &WorkspaceModel,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let Some(slot) = model.slot_in_pane() else {
        return;
    };
    let errors = model.remembered.declared.errors_of(&slot.owner);
    if !errors.is_empty() {
        let content = Rect {
            height: surface.height.saturating_sub(1),
            ..surface
        };
        widget::banner::render(frame, content, &error_title(errors.len()), errors);
    }
    if let Some(area) = inspector {
        render_inspector(frame, area, model, &slot.surface.title, errors, hits);
    }
}

/// The open drawer, in the column `compute_layout` docked beside the pane:
/// a raised card, the strip naming whose it is across its top where nothing
/// it draws can cover it, and its view under that. Answers whether anything
/// in it moves with the clock.
pub(super) fn render_declared_drawer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let Some(slot) = model.open_drawer_slot() else {
        return false;
    };
    let note = model.dock_notice.as_ref().map(|notice| {
        Line::from(Span::styled(
            format!(" {} ", notice.text),
            theme::fg(Token::TextMuted),
        ))
    });
    let (inner, handle) = widget::drawer::render_docked(frame, area, model.dragging_dock, note);
    let content = match slot.owner.declared() {
        Some(identity) => crate::ui::extension_view::tree::render_attribution(
            frame,
            inner,
            &crate::ui::extension_view::tree::attribution(identity),
        ),
        None => inner,
    };
    let space = uze_extensions::view::Size {
        width: content.width,
        height: content.height,
    };
    model.dock_space.set(space);
    let view = model.surface_view(slot, space);
    let hovered = match model.hovered {
        Some(WorkspaceHit::Extension(ExtensionHit { slot: hovered, hit }))
            if hovered == slot.id =>
        {
            Some(hit)
        }
        _ => None,
    };
    let mut view_hits = Vec::new();
    let docked = crate::ui::graphics::in_slot(slot.id, || {
        crate::ui::extension_view::render_docked(frame, &view, content, hovered, &mut view_hits)
    });
    model.dock_bar.set(docked.bar);
    let animates = docked.animates;
    let errors = model.remembered.declared.errors_of(&slot.owner);
    if !errors.is_empty() {
        widget::banner::render(frame, content, &error_title(errors.len()), errors);
    }
    // The edge first, so it answers before the rows beside it; the card
    // last, behind everything it holds.
    hits.push((handle, WorkspaceHit::ResizeDock));
    hits.extend(view_hits.into_iter().map(|(rect, hit)| {
        (
            rect,
            WorkspaceHit::Extension(ExtensionHit { slot: slot.id, hit }),
        )
    }));
    hits.push((area, WorkspaceHit::DockBody));
    animates
}

/// The status rows, one under another from where `rows` stands, each
/// inset as the header's own name is and folded to what is left of the
/// column. Answers whether anything in them moves with the clock.
pub(super) fn render_status_rows(
    frame: &mut ratatui::Frame<'_>,
    rows: &mut crate::ui::Rows,
    model: &WorkspaceModel,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let width = rows.width.saturating_sub(1 + TRAILING_PAD);
    let mut animates = false;
    for (slot, view) in model.declared_statuses(width) {
        let Some(row) = rows.next(1) else {
            break;
        };
        let rect = Rect::new(row.x + 1, row.y, width, 1);
        animates |= render_docked_slot(frame, rect, model, slot, &view, hits);
    }
    animates
}

/// A band in the rows the layout took for it: its extension's id, muted,
/// against the right end of its first row, where the host keeps the
/// column it needs so nothing the band draws can cover it, and the band's
/// view in the rest. A band with nothing to draw leaves its rows blank.
/// Answers whether anything in it moves with the clock.
pub(super) fn render_band(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    slot: SlotId,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let Some(id) = model
        .remembered
        .declared
        .slot(slot)
        .and_then(|slot| slot.owner.declared())
        .map(|identity| identity.id.clone())
    else {
        return false;
    };
    let mark = text::elide(&id, usize::from(area.width / 2));
    let mark_width = u16::try_from(text::columns(&mark)).unwrap_or(u16::MAX);
    let content = Rect {
        width: area.width.saturating_sub(mark_width + 1),
        ..area
    };
    frame.render_widget(
        Paragraph::new(Span::styled(mark, theme::fg(Token::TextMuted))),
        Rect::new(content.right() + 1, area.y, mark_width, 1),
    );
    let space = uze_extensions::view::Size {
        width: content.width,
        height: content.height,
    };
    match model.band_view(slot, space) {
        Some(view) => render_docked_slot(frame, content, model, slot, &view, hits),
        None => false,
    }
}

/// The share of the frame an overlay's card takes each way, in percent.
const OVERLAY_SHARE: u16 = 70;
/// The fewest columns and rows an overlay's card is given, the frame
/// allowing.
const OVERLAY_MIN: (u16, u16) = (40, 10);

/// Where an overlay's card stands on `frame`: centred, at
/// [`OVERLAY_SHARE`] of it each way and never under [`OVERLAY_MIN`]
/// where the frame has that much.
pub(in crate::ui::orchestrator) fn overlay_card(frame: Rect) -> Rect {
    let share = |whole: u16, least: u16| {
        let part =
            u16::try_from(u32::from(whole) * u32::from(OVERLAY_SHARE) / 100).unwrap_or(whole);
        part.max(least).min(whole)
    };
    let width = share(frame.width, OVERLAY_MIN.0);
    let height = share(frame.height, OVERLAY_MIN.1);
    Rect::new(
        frame.x + (frame.width - width) / 2,
        frame.y + (frame.height - height) / 2,
        width,
        height,
    )
}

/// The overlay standing over the frame: the scrim, a floating card with
/// the mark that closes it, the strip naming whose it is across its top,
/// where nothing it draws can cover it, and its view under that. Its hits
/// go ahead of everything drawn before it. Answers whether anything in it
/// moves with the clock.
pub(super) fn render_overlay(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    let Some(slot) = model.overlay_slot() else {
        return false;
    };
    widget::scrim::render(frame, area);
    let card = overlay_card(area);
    frame.render_widget(ratatui::widgets::Clear, card);
    let inner = Surface::card().render(frame, card);
    let close = theme::glyph(Symbol::MarkClose);
    let close_width = theme::width(Symbol::MarkClose);
    let close_rect = Rect::new(
        card.right().saturating_sub(1 + TRAILING_PAD + close_width),
        card.y,
        close_width,
        1,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            close,
            theme::fg(widget::mark::close_tone(chip_state(
                model,
                WorkspaceHit::CloseOverlay,
            ))),
        )),
        close_rect,
    );
    let content = match slot.owner.declared() {
        Some(identity) => crate::ui::extension_view::tree::render_attribution(
            frame,
            inner,
            &crate::ui::extension_view::tree::attribution(identity),
        ),
        None => inner,
    };
    let space = crate::ui::extension_view::content_space(content, model.navigator_width);
    model.slot_spaces.borrow_mut().insert(slot.id, space);
    let Some(view) = model.overlay_view(space) else {
        return false;
    };
    let mut view_hits = Vec::new();
    let drawn = crate::ui::graphics::in_slot(slot.id, || {
        crate::ui::extension_view::render(
            frame,
            &view,
            content,
            crate::ui::extension_view::NavigatorFrame::default(),
            crate::ui::extension_view::FooterKeys {
                scope: uze_keys::Scope::Declared,
                hovered: model.hovered_hint(),
                host: &[],
            },
            None,
            &mut view_hits,
        )
    });
    model.overlay_area.set(drawn.content_area);
    let tag = slot.id;
    let mut targets: Vec<(Rect, WorkspaceHit)> = vec![(close_rect, WorkspaceHit::CloseOverlay)];
    targets.extend(view_hits.into_iter().map(|(rect, hit)| {
        (
            rect,
            WorkspaceHit::Extension(ExtensionHit { slot: tag, hit }),
        )
    }));
    targets.extend(hint_targets(drawn.hints.clone()));
    targets.push((card, WorkspaceHit::OverlayBody));
    hits.splice(0..0, targets);
    drawn.marquee
}

/// What a status row or a band draws in `area`, its hits tagged with its
/// slot and the room it had kept for the click that reaches it.
pub(super) fn render_docked_slot(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    slot: SlotId,
    view: &uze_extensions::view::View,
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) -> bool {
    model.slot_spaces.borrow_mut().insert(
        slot,
        uze_extensions::view::Size {
            width: area.width,
            height: area.height,
        },
    );
    let hovered = match model.hovered {
        Some(WorkspaceHit::Extension(ExtensionHit { slot: hovered, hit })) if hovered == slot => {
            Some(hit)
        }
        _ => None,
    };
    let mut view_hits = Vec::new();
    let docked = crate::ui::graphics::in_slot(slot, || {
        crate::ui::extension_view::render_docked(frame, view, area, hovered, &mut view_hits)
    });
    hits.extend(
        view_hits
            .into_iter()
            .map(|(rect, hit)| (rect, WorkspaceHit::Extension(ExtensionHit { slot, hit }))),
    );
    docked.animates
}

fn error_title(count: usize) -> String {
    match count {
        1 => "the last save has an error".to_owned(),
        _ => format!("the last save has {count} errors"),
    }
}

fn render_inspector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    model: &WorkspaceModel,
    title: &str,
    errors: &[String],
    hits: &mut Vec<(Rect, WorkspaceHit)>,
) {
    let (inner, handle) = widget::drawer::render(frame, area, model.dragging_overview_drawer);
    hits.push((handle, WorkspaceHit::ResizeOverviewDrawer));
    let inspection = model.inspection().unwrap_or_default();
    let measure = usize::from(inner.width);
    let mut lines = vec![
        Line::from(Span::styled("inspector", theme::fg(Token::TextMuted))),
        Line::from(Span::styled(
            text::elide(title, measure),
            theme::fg(Token::TextBright),
        )),
    ];
    // Lines a click answers on, and the one the edit's keys stand on.
    let mut targets: Vec<(usize, WorkspaceHit)> = Vec::new();
    let mut keys_row = None;
    let tools = model.inspector_tools();
    if !tools.is_empty() {
        heading(&mut lines, "author");
        let selected = model
            .inspector_holds_keys()
            .then(|| model.inspector.selected.min(tools.len() - 1));
        for (index, tool) in tools.iter().enumerate() {
            let hit = WorkspaceHit::InspectorTool(index);
            let style = if selected == Some(index) {
                theme::on(Token::TextBright, Token::SurfaceSelected)
            } else if model.pointer_on(hit) == widget::ChipState::Resting {
                theme::fg(Token::TextSecondary)
            } else {
                theme::fg(Token::TextBright)
            };
            targets.push((lines.len(), hit));
            lines.push(Line::from(Span::styled(
                text::elide(&model.tool_label(tool), measure),
                style,
            )));
            if let (InspectorTool::Rerun(source), Some((editing, buffer))) =
                (tool, &model.inspector.edit)
                && source == editing
            {
                let mut field = vec![Span::raw("  ")];
                field.extend(rename_spans_within(buffer, measure.saturating_sub(2)));
                lines.push(Line::from(field));
                keys_row = Some(lines.len());
                lines.push(Line::default());
            }
        }
    }
    let nodes_heading = if inspection.traced {
        "nodes"
    } else {
        "nodes · why absent is not traced yet"
    };
    group(
        &mut lines,
        measure,
        nodes_heading,
        inspection.nodes,
        "no view compiled",
    );
    group(&mut lines, measure, "data", inspection.data, "no data");
    group(
        &mut lines,
        measure,
        "ui state",
        inspection
            .ui
            .into_iter()
            .map(|(id, value)| format!("{id} = {value}"))
            .collect(),
        "nothing chosen yet",
    );
    group(
        &mut lines,
        measure,
        "recorded actions",
        inspection.recorded,
        "nothing pressed",
    );
    group(
        &mut lines,
        measure,
        "runs",
        inspection.runs,
        "nothing asked yet",
    );
    group(
        &mut lines,
        measure,
        "events",
        inspection.events,
        "none seen",
    );
    group(
        &mut lines,
        measure,
        "grants",
        inspection.grants,
        "wants nothing",
    );
    group(
        &mut lines,
        measure,
        "cost",
        inspection.cost,
        "nothing measured",
    );
    group(&mut lines, measure, "errors", errors.to_vec(), "none");
    let (first, visible) = draw_scrolled(frame, inner, lines, model);
    let row_of = |line: usize| {
        (line >= first && line < first + visible)
            .then(|| Rect::new(inner.x, inner.y + (line - first) as u16, inner.width, 1))
    };
    for (line, hit) in targets {
        if let Some(rect) = row_of(line) {
            hits.push((rect, hit));
        }
    }
    if let Some(rect) = keys_row.and_then(row_of) {
        let scopes = [
            uze_keys::Scope::Global,
            uze_keys::Scope::Workspace,
            uze_keys::Scope::Rename,
        ];
        let keys = crate::ui::widget::hint::Hints::new(&scopes)
            .named(Action::Activate, "rerun")
            .named(Action::Dismiss, "cancel")
            .hovered(model.hovered_hint())
            .render(
                frame,
                Rect {
                    x: rect.x + 2,
                    width: rect.width.saturating_sub(2),
                    ..rect
                },
            );
        hits.extend(hint_targets(keys));
    }
    hits.push((inner, WorkspaceHit::Inspector));
}

/// A group's heading, with a line of air above it.
fn heading(lines: &mut Vec<Line<'static>>, heading: &str) {
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        heading.to_owned(),
        theme::fg(Token::TextSecondary),
    )));
}

/// One of the inspector's groups: its heading, then a row a line, or what
/// it says when there are none.
fn group(
    lines: &mut Vec<Line<'static>>,
    measure: usize,
    name: &str,
    rows: Vec<String>,
    empty: &str,
) {
    heading(lines, name);
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            empty.to_owned(),
            theme::fg(Token::TextMuted),
        )));
    }
    for row in rows {
        lines.push(Line::from(Span::styled(
            text::elide(&row, measure),
            theme::fg(Token::TextPrimary),
        )));
    }
}

/// The inspector's lines from where it is scrolled, settled so its last
/// line stops at its foot, with a handle in the drawer's right margin when
/// they do not all fit — as the overview's drawer draws its text. Answers
/// the first line shown and how many fit.
fn draw_scrolled(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    model: &WorkspaceModel,
) -> (usize, usize) {
    let visible = usize::from(area.height);
    let first = usize::from(model.inspector_scroll.get()).min(lines.len().saturating_sub(visible));
    model
        .inspector_scroll
        .set(u16::try_from(first).unwrap_or(u16::MAX));
    model.inspector_rows.set(area.height);
    let track = Rect::new(
        area.right().saturating_add(1),
        area.y,
        widget::Scrollbar::width(),
        area.height,
    );
    if let Some(bar) = widget::Scrollbar::measure(track, visible, lines.len()) {
        bar.render(frame, first);
    }
    let shown: Vec<_> = lines.into_iter().skip(first).take(visible).collect();
    frame.render_widget(Paragraph::new(shown), area);
    (first, visible)
}

/// The rows a declared section takes: its header, and while it is open its
/// content and a row of air, within a third of what the column has left.
/// Nothing when the header would not fit.
pub(super) fn declared_section_height(
    section: &uze_extensions::view::Section,
    width: u16,
    remaining: u16,
) -> u16 {
    let budget = remaining / 3;
    let chrome = SECTION_CHROME;
    if budget < chrome {
        return 0;
    }
    if section.collapsed {
        return chrome;
    }
    let open = match &section.tree {
        Some(tree) => crate::ui::extension_view::tree_section_height(tree, width),
        None => SECTION_CHROME + u16::try_from(section.rows.len()).unwrap_or(u16::MAX),
    };
    (open + SECTION_AIR).min(budget)
}
