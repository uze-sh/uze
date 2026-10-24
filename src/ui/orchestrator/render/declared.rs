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
    remaining: u16,
) -> u16 {
    let budget = remaining / 3;
    if budget < SECTION_CHROME {
        return 0;
    }
    if section.collapsed {
        return SECTION_CHROME;
    }
    let body = match &section.tree {
        Some(tree) => tree_rows(tree),
        None => u16::try_from(section.rows.len()).unwrap_or(u16::MAX),
    };
    (SECTION_CHROME + body + SECTION_AIR).min(budget)
}

/// How many rows a tree of nodes asks for, laid out as the renderer lays
/// it: a stack is its children and their gaps, a row its tallest child, a
/// list or a table a row per child (and its headings).
pub(super) fn tree_rows(node: &uze_extensions::view::Node) -> u16 {
    use uze_extensions::view::NodeKind;
    let children = || node.children.iter().map(tree_rows);
    match &node.kind {
        NodeKind::Stack { gap } => {
            let count = u16::try_from(node.children.len()).unwrap_or(u16::MAX);
            children()
                .fold(0u16, u16::saturating_add)
                .saturating_add(gap.saturating_mul(count.saturating_sub(1)))
        }
        NodeKind::Row { .. } => children().max().unwrap_or(1),
        NodeKind::List { .. } => children().fold(0u16, u16::saturating_add).max(1),
        NodeKind::Table { .. } => children().fold(0u16, u16::saturating_add).saturating_add(1),
        NodeKind::Log { lines } => u16::try_from(lines.len()).unwrap_or(u16::MAX).max(1),
        NodeKind::Tabs { .. } => children().fold(1u16, u16::saturating_add),
        _ => 1,
    }
}
