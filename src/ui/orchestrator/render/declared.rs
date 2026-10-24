//! What the host draws around a plugin's view: the error over its last
//! good draw, and the inspector beside it.

use super::*;
use crate::ui::extension_view::{SECTION_AIR, SECTION_CHROME};

/// The pane, split for the inspector when it is open beside a declared
/// view: the view keeps what the drawer leaves.
pub(super) fn declared_split(model: &WorkspaceModel, pane: Rect) -> (Rect, Option<Rect>) {
    if model.declared_open.is_none() || !model.inspecting {
        return (pane, None);
    }
    let width = widget::drawer::width(None, pane.width);
    let surface = Rect::new(pane.x, pane.y, pane.width - width, pane.height);
    let drawer = Rect::new(surface.right(), pane.y, width, pane.height);
    (surface, Some(drawer))
}

/// The open declared view's error, over the bottom of its content and
/// above its footer, and the inspector in the room `declared_split` left.
pub(super) fn render_declared_overlays(
    frame: &mut ratatui::Frame<'_>,
    surface: Rect,
    inspector: Option<Rect>,
    model: &WorkspaceModel,
) {
    let Some(slot) = model.open_declared_slot() else {
        return;
    };
    let errors = model.remembered.declared.errors_of(&slot.identity);
    if !errors.is_empty() {
        let content = Rect {
            height: surface.height.saturating_sub(1),
            ..surface
        };
        widget::banner::render(frame, content, &error_title(errors.len()), errors);
    }
    if let Some(area) = inspector {
        render_inspector(frame, area, model, &slot.surface.title, errors);
    }
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
) {
    let (inner, _handle) = widget::drawer::render(frame, area, false);
    let inspection = model.inspection().unwrap_or_default();
    let measure = usize::from(inner.width);
    let mut lines = vec![
        Line::from(Span::styled("inspector", theme::fg(Token::TextMuted))),
        Line::from(Span::styled(
            text::elide(title, measure),
            theme::fg(Token::TextBright),
        )),
    ];
    let mut group = |heading: &str, rows: Vec<String>, empty: &str| {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            heading.to_owned(),
            theme::fg(Token::TextSecondary),
        )));
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
    };
    let nodes_heading = if inspection.traced {
        "nodes"
    } else {
        "nodes · why absent is not traced yet"
    };
    group(nodes_heading, inspection.nodes, "no view compiled");
    group("data", inspection.data, "no data");
    group(
        "ui state",
        inspection
            .ui
            .into_iter()
            .map(|(id, value)| format!("{id} = {value}"))
            .collect(),
        "nothing chosen yet",
    );
    group("recorded actions", inspection.recorded, "nothing pressed");
    group("errors", errors.to_vec(), "none");
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The rows a declared section takes: its header, and while it is open its
/// content and a row of air, within a third of what the column has left.
pub(super) fn declared_section_height(
    section: &uze_extensions::view::Section,
    width: u16,
    remaining: u16,
) -> u16 {
    let budget = remaining / 3;
    if budget < SECTION_CHROME {
        return 0;
    }
    if section.collapsed {
        return SECTION_CHROME;
    }
    let open = match &section.tree {
        Some(tree) => crate::ui::extension_view::tree_section_height(tree, width),
        None => SECTION_CHROME + u16::try_from(section.rows.len()).unwrap_or(u16::MAX),
    };
    (open + SECTION_AIR).min(budget)
}
