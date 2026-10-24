//! A declared extension's [`Content::Tree`]: the catalog's nodes, laid out
//! and drawn by the host.
//!
//! Layout is two walks over the same rules: [`height`] says how many rows a
//! node takes at a width, and [`Painter::node`] draws it there. Rows are
//! counted from the top of the tree and only the ones inside the window are
//! drawn, so scrolling and clipping are one question asked of every row.
//!
//! Nothing here is kept between frames. What moves on its own, an
//! [`NodeKind::Activity`] and an [`NodeKind::Elapsed`], is drawn from the
//! [`Clock`] the frame is handed, which is how both stay current without
//! the extension being asked for a new tree.

use std::time::{SystemTime, UNIX_EPOCH};

use uze_extensions::view::{Align, Node, NodeAct, NodeKind, Segment, Tab};

use super::*;
use crate::ui::widget::{Button, row::RowState};

/// How long an activity mark shows each of its frames: the CLI spinner's
/// beat, so the two read as one motion.
const ACTIVITY_FRAME_MS: u64 = 120;

/// The air each side of a badge's label, part of its ground.
const BADGE_PAD: u16 = 1;

/// Between one column of a table and the next.
const TABLE_GAP: u16 = 2;

/// The width a bar takes when nothing sized it: so many cells a segment,
/// and never less than the minimum.
const PROGRESS_CELLS_PER_SEGMENT: u16 = 4;
const PROGRESS_MIN_WIDTH: u16 = 12;

/// The columns an elapsed time is laid out in, whatever it says now:
/// `59m59s` is the widest it gets before it turns to hours, and a row that
/// changed width every minute would shove everything beside it.
const ELAPSED_WIDTH: u16 = 6;

/// The host's clock for one frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Clock {
    /// Milliseconds of the Unix epoch.
    pub(crate) now_ms: u64,
}

impl Clock {
    pub(crate) fn now() -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        Self { now_ms }
    }

    fn tick(self) -> usize {
        usize::try_from(self.now_ms / ACTIVITY_FRAME_MS).unwrap_or(usize::MAX)
    }
}

/// What the host knows while drawing a tree, and the tree may not hold.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TreeFrame {
    pub(crate) clock: Clock,
    /// The hit under the pointer, which lights the control it belongs to.
    pub(crate) hovered: Option<ViewHit>,
    /// Draw only whole nodes, ending in a mark saying how many more there
    /// are, rather than cutting the last one at the edge: what a section
    /// does, since nothing scrolls it into sight.
    pub(crate) fold: bool,
}

impl TreeFrame {
    pub(crate) fn now(hovered: Option<ViewHit>) -> Self {
        Self {
            clock: Clock::now(),
            hovered,
            fold: false,
        }
    }
}

/// What one frame of a tree left behind for the host.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TreeDrawn {
    /// Whether anything on it moves with the clock, which is what keeps
    /// the host's clock turning for the next frame.
    pub(crate) animates: bool,
    pub(crate) bar: Option<Scrollbar>,
    /// Whether the tree's last row is on screen.
    pub(crate) at_end: bool,
}

/// Draws `root` into `area` from row `scroll`, registering a hit for every
/// node that answers a gesture: each tab, toggle, button and list row.
pub(super) fn render_tree(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    scroll: usize,
    root: &Node,
    held: TreeFrame,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> TreeDrawn {
    let total = height(root, area.width);
    let room = usize::from(area.height);
    let scroll = scroll.min(total.saturating_sub(room));
    let overflows = total.saturating_sub(scroll) > room;
    // The last row is the fold's own while anything is left out.
    let rows = match held.fold && overflows {
        true => room.saturating_sub(1),
        false => room,
    };
    let mut painter = Painter {
        frame,
        area,
        scroll,
        rows,
        held,
        hits,
        folded: None,
    };
    painter.node(root, area.x, area.width, 0);
    if held.fold && overflows {
        let (at, more) = painter.folded.unwrap_or((scroll + rows, 0));
        painter.more(at.min(scroll + rows), more);
    }
    TreeDrawn {
        animates: tree_animates(root),
        bar: None,
        at_end: !overflows,
    }
}

/// A tree as a view's content: its heading over it, the way a view's lines
/// have one, and the standard scrollbar on the edge beside it when it is
/// taller than the room.
pub(super) fn render_tree_content(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    tree: (&str, usize, &Node),
    modes: &[Mode],
    held: TreeFrame,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> TreeDrawn {
    let (heading, scroll, root) = tree;
    let mut body = area;
    if !heading.is_empty() || !modes.is_empty() {
        frame.render_widget(
            Paragraph::new(TextSpan::styled(
                heading.to_owned(),
                theme::fg(Token::TextSecondary),
            )),
            Rect::new(
                area.x,
                area.y,
                area.width.saturating_sub(modes_width(modes)),
                area.height.min(1),
            ),
        );
        render_modes(frame, area, modes, hits);
        body = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        );
    }
    let padding = match body.height > TEXT_PADDING.saturating_mul(2) {
        true => TEXT_PADDING,
        false => 0,
    };
    let inset = match body.width > PROSE_INSET.saturating_mul(2) {
        true => PROSE_INSET,
        false => 0,
    };
    let content = Rect::new(
        body.x.saturating_add(inset),
        body.y.saturating_add(padding),
        body.width.saturating_sub(inset.saturating_mul(2)),
        body.height.saturating_sub(padding.saturating_mul(2)),
    );
    let total = height(root, content.width);
    let scroll = scroll.min(total.saturating_sub(usize::from(content.height)));
    let mut drawn = render_tree(frame, content, scroll, root, held, hits);
    let bar = Scrollbar::measure(
        Rect::new(body.right(), body.y, Scrollbar::width(), body.height),
        usize::from(content.height),
        total,
    );
    drawn.bar = render_scrollbar(frame, bar, scroll, hits, ViewHit::DragContentScrollbar);
    drawn
}

/// The rows `root` takes at `width`, for a host reserving a section's
/// room before it draws it.
pub(crate) fn tree_height(root: &Node, width: u16) -> u16 {
    u16::try_from(height(root, width)).unwrap_or(u16::MAX)
}

/// Whether anything in `root` moves with the clock.
pub(crate) fn tree_animates(root: &Node) -> bool {
    match root.kind {
        NodeKind::Activity { busy: true } => true,
        NodeKind::Elapsed {
            since_ms: Some(_), ..
        } => true,
        _ => root.children.iter().any(tree_animates),
    }
}

/// The strip naming whose a declared view is, across the top of `area`,
/// answering with the room left under it: the view is drawn there, so no
/// part of it can cover the strip.
pub(crate) fn render_attribution(frame: &mut ratatui::Frame<'_>, area: Rect, said: &str) -> Rect {
    if area.height == 0 {
        return area;
    }
    let strip = Rect::new(area.x, area.y, area.width, 1);
    widget::fill(frame, strip, Token::SurfaceRaisedSubtle);
    let room = usize::from(area.width.saturating_sub(TRAILING_PAD.saturating_mul(2)));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            TextSpan::raw(" ".repeat(usize::from(TRAILING_PAD))),
            TextSpan::styled(text::elide(said, room), theme::fg(Token::TextMuted)),
        ])),
        strip,
    );
    Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    )
}

/// What the attribution strip says: the extension, and the plugin and
/// marketplace it came from.
pub(crate) fn attribution(extension: &str, plugin: &str, marketplace: &str) -> String {
    format!(
        "{extension}{}{plugin}@{marketplace}",
        theme::glyph(Symbol::HintSeparator)
    )
}

/// The time since `since_ms` at `now_ms`, in the fewest characters that
/// still say it: `12s`, `4m12s`, `1h04m`, `3d02h`.
pub(super) fn elapsed(since_ms: u64, now_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(since_ms) / 1000;
    let (minutes, hours, days) = (seconds / 60, seconds / 3600, seconds / 86_400);
    if minutes == 0 {
        format!("{seconds}s")
    } else if hours == 0 {
        format!("{minutes}m{:02}s", seconds % 60)
    } else if days == 0 {
        format!("{hours}h{:02}m", minutes % 60)
    } else {
        format!("{days}d{:02}h", hours % 24)
    }
}

fn cells_of(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

fn sum(widths: impl IntoIterator<Item = u16>) -> u16 {
    widths.into_iter().fold(0, u16::saturating_add)
}

/// The cells a node asks for when nothing tells it otherwise.
fn natural(node: &Node) -> u16 {
    let own = match &node.kind {
        NodeKind::Stack { .. } => node.children.iter().map(natural).max().unwrap_or(0),
        NodeKind::Row { gap, .. } => sum(node.children.iter().map(natural))
            .saturating_add(gap.saturating_mul(cells_of(node.children.len().saturating_sub(1)))),
        NodeKind::Text { spans, .. } => {
            sum(spans.iter().map(|span| cells_of(text::columns(&span.text))))
        }
        NodeKind::Badge { text: label, .. } => {
            cells_of(text::columns(label)).saturating_add(BADGE_PAD * 2)
        }
        NodeKind::Marks(roles) => {
            cells_of(roles.len()).saturating_mul(theme::width(Symbol::MarkFacet))
        }
        NodeKind::Pips { total, .. } => total.saturating_mul(pip_width()),
        NodeKind::Progress { segments } => cells_of(segments.len())
            .saturating_mul(PROGRESS_CELLS_PER_SEGMENT)
            .max(PROGRESS_MIN_WIDTH),
        NodeKind::List { empty, .. } => match node.children.is_empty() {
            true => cells_of(empty.as_deref().map_or(0, text::columns)),
            false => node
                .children
                .iter()
                .map(natural)
                .max()
                .unwrap_or(0)
                .saturating_add(TRAILING_PAD * 2),
        },
        NodeKind::Table { columns, empty } => match node.children.is_empty() {
            true => cells_of(empty.as_deref().map_or(0, text::columns)),
            false => {
                let widths = column_widths(columns, &node.children);
                let gaps = TABLE_GAP.saturating_mul(cells_of(widths.len().saturating_sub(1)));
                sum(widths).saturating_add(gaps)
            }
        },
        NodeKind::Tabs { tabs } => modes_width(&modes_of(tabs)),
        NodeKind::Toggle { label, .. } => toggle_mark_width()
            .saturating_add(1)
            .saturating_add(cells_of(text::columns(label))),
        NodeKind::Button { label } => button(label, false).width(),
        NodeKind::Activity { .. } => {
            theme::width(Symbol::ActivityBusy).max(theme::width(Symbol::ActivityIdle))
        }
        NodeKind::Elapsed { since_ms, .. } => match since_ms {
            Some(_) => ELAPSED_WIDTH,
            None => 0,
        },
        NodeKind::Log { lines } => cells_of(
            lines
                .iter()
                .map(|line| text::columns(&line.text))
                .max()
                .unwrap_or(0),
        ),
        NodeKind::Notice { text: said, .. } => cells_of(text::columns(said)),
    };
    own.max(node.min_width)
}

/// The rows a node takes at `width`.
fn height(node: &Node, width: u16) -> usize {
    match &node.kind {
        NodeKind::Stack { gap } => {
            let rows: usize = node.children.iter().map(|child| height(child, width)).sum();
            rows + usize::from(*gap) * node.children.len().saturating_sub(1)
        }
        NodeKind::Row { gap, align } => {
            let (_, widths) = row_widths(&node.children, *gap, *align, width);
            // A child squeezed to no cells draws nothing, so it takes no
            // rows: wrapped at zero width, a text would count a row a
            // character and fold a whole section away.
            node.children
                .iter()
                .zip(widths)
                .filter(|(_, width)| *width > 0)
                .map(|(child, width)| height(child, width))
                .max()
                .unwrap_or(0)
        }
        NodeKind::Text { spans, truncate } => match truncate {
            true => 1,
            false => wrapped(spans, width).len().max(1),
        },
        NodeKind::Elapsed { since_ms, .. } => usize::from(since_ms.is_some()),
        NodeKind::List { empty, .. } => match node.children.is_empty() {
            true => usize::from(empty.is_some()),
            false => {
                let inner = list_inner(width);
                node.children
                    .iter()
                    .map(|child| height(child, inner).max(1))
                    .sum()
            }
        },
        NodeKind::Table { columns, empty } => match node.children.is_empty() {
            true => usize::from(empty.is_some()),
            false => {
                let widths = fitted_columns(columns, &node.children, width);
                // The headings and the blank row under them, then the rows.
                2 + node
                    .children
                    .iter()
                    .map(|row| table_row_height(row, &widths))
                    .sum::<usize>()
            }
        },
        NodeKind::Log { lines } => lines.len(),
        NodeKind::Notice { text: said, role } => wrapped(&[Span::new(said.clone(), *role)], width)
            .len()
            .max(1),
        NodeKind::Badge { .. }
        | NodeKind::Marks(_)
        | NodeKind::Pips { .. }
        | NodeKind::Progress { .. }
        | NodeKind::Tabs { .. }
        | NodeKind::Toggle { .. }
        | NodeKind::Button { .. }
        | NodeKind::Activity { .. } => 1,
    }
}

/// Where a row's children start and how wide each is drawn.
///
/// Every child gets what it asks for; the growing ones share what is left;
/// a row with none that grow is placed by its alignment. A row that cannot
/// give everyone what they ask takes it first from text that elides, then
/// from text that wraps, widest first and never below its longest word, and
/// only then from whoever comes last: a paragraph folding onto a second row
/// reads, where a word pushed to the edge was cut to a sliver or lost.
fn row_widths(children: &[Node], gap: u16, align: Align, width: u16) -> (u16, Vec<u16>) {
    if children.is_empty() {
        return (0, Vec::new());
    }
    let gaps = gap.saturating_mul(cells_of(children.len() - 1));
    let room = width.saturating_sub(gaps);
    let mut widths: Vec<u16> = children
        .iter()
        .map(|child| match child.grow {
            true => child.min_width,
            false => natural(child),
        })
        .collect();
    let asked = sum(widths.iter().copied());
    if asked > room {
        let mut over = asked - room;
        for (child, width) in children.iter().zip(widths.iter_mut()) {
            if matches!(child.kind, NodeKind::Text { truncate: true, .. }) {
                let given = over.min(width.saturating_sub(child.min_width));
                *width -= given;
                over -= given;
            }
        }
        while over > 0 {
            let widest = children
                .iter()
                .zip(widths.iter())
                .enumerate()
                .filter(|(_, (child, width))| {
                    matches!(
                        child.kind,
                        NodeKind::Text {
                            truncate: false,
                            ..
                        }
                    ) && **width > wrap_floor(child)
                })
                .max_by_key(|(_, (_, width))| **width)
                .map(|(index, _)| index);
            let Some(index) = widest else { break };
            widths[index] -= 1;
            over -= 1;
        }
        let mut left = room;
        for width in &mut widths {
            *width = (*width).min(left);
            left -= *width;
        }
        return (0, widths);
    }
    let spare = room - asked;
    let growing = cells_of(children.iter().filter(|child| child.grow).count());
    if growing == 0 {
        let offset = match align {
            Align::Start => 0,
            Align::Center => spare / 2,
            Align::End => spare,
        };
        return (offset, widths);
    }
    let (share, mut extra) = (spare / growing, spare % growing);
    for (child, width) in children.iter().zip(widths.iter_mut()) {
        if child.grow {
            *width += share + u16::from(extra > 0);
            extra = extra.saturating_sub(1);
        }
    }
    (0, widths)
}

/// The narrowest a wrapping text is drawn in a crowded row: its longest
/// word, or its whole self when that is shorter, and never under its
/// `min-width`.
fn wrap_floor(node: &Node) -> u16 {
    let NodeKind::Text { spans, .. } = &node.kind else {
        return natural(node);
    };
    let longest = spans
        .iter()
        .flat_map(|span| span.text.split_whitespace())
        .map(|word| cells_of(text::columns(word)))
        .max()
        .unwrap_or(0);
    longest.max(node.min_width).min(natural(node))
}

/// The cells of a table's row: a row node's children, or any other node as
/// its one cell.
fn cells(row: &Node) -> &[Node] {
    match row.kind {
        NodeKind::Row { .. } => &row.children,
        _ => std::slice::from_ref(row),
    }
}

/// Each column as wide as its heading or its widest cell.
fn column_widths(columns: &[String], rows: &[Node]) -> Vec<u16> {
    let count = rows
        .iter()
        .map(|row| cells(row).len())
        .max()
        .unwrap_or(0)
        .max(columns.len());
    (0..count)
        .map(|index| {
            let heading = cells_of(
                columns
                    .get(index)
                    .map_or(0, |heading| text::columns(heading)),
            );
            rows.iter()
                .filter_map(|row| cells(row).get(index))
                .map(natural)
                .max()
                .unwrap_or(0)
                .max(heading)
        })
        .collect()
}

/// [`column_widths`], given up a cell at a time from the widest column
/// until the table fits `width`.
fn fitted_columns(columns: &[String], rows: &[Node], width: u16) -> Vec<u16> {
    let mut widths = column_widths(columns, rows);
    let gaps = TABLE_GAP.saturating_mul(cells_of(widths.len().saturating_sub(1)));
    let room = width.saturating_sub(gaps);
    let mut total = sum(widths.iter().copied());
    while total > room {
        let Some(widest) = widths
            .iter_mut()
            .filter(|width| **width > 1)
            .max_by_key(|width| **width)
        else {
            break;
        };
        *widest -= 1;
        total -= 1;
    }
    widths
}

fn table_row_height(row: &Node, widths: &[u16]) -> usize {
    cells(row)
        .iter()
        .zip(widths)
        .map(|(cell, width)| height(cell, *width))
        .max()
        .unwrap_or(0)
        .max(1)
}

/// A list row's room inside the inset its ground keeps either side.
fn list_inner(width: u16) -> u16 {
    match width > TRAILING_PAD * 2 {
        true => width - TRAILING_PAD * 2,
        false => width,
    }
}

fn wrapped(spans: &[Span], width: u16) -> Vec<Line<'static>> {
    let line = ContentLine {
        gutter: String::new(),
        number: String::new(),
        tone: LineTone::Neutral,
        spans: spans.to_vec(),
    };
    prose_rows(&line, usize::from(width))
}

/// `spans` cut to `width` cells, ending in the house ellipsis when they do
/// not fit, in the hue of the span it cut.
fn elided(spans: &[Span], width: u16) -> Line<'static> {
    let width = usize::from(width);
    let drawn: Vec<TextSpan<'static>> = spans.iter().map(styled).collect();
    if drawn.iter().map(TextSpan::width).sum::<usize>() <= width {
        return Line::from(drawn);
    }
    let Some(room) = width.checked_sub(usize::from(theme::width(Symbol::Ellipsis))) else {
        return Line::default();
    };
    let mut kept = Vec::new();
    let mut used = 0;
    let mut cut_style = Style::default();
    'spans: for span in drawn {
        cut_style = span.style;
        let mut piece = String::new();
        for character in span.content.chars() {
            let cells = text::cell_width(character);
            if used + cells > room {
                kept.push(TextSpan::styled(piece, span.style));
                break 'spans;
            }
            piece.push(character);
            used += cells;
        }
        kept.push(TextSpan::styled(piece, span.style));
    }
    kept.push(TextSpan::styled(theme::glyph(Symbol::Ellipsis), cut_style));
    Line::from(kept)
}

fn modes_of(tabs: &[Tab]) -> Vec<Mode> {
    tabs.iter()
        .map(|tab| Mode {
            label: tab.label.clone(),
            active: tab.active,
            icon: RowIcon::None,
        })
        .collect()
}

fn pip_width() -> u16 {
    theme::width(Symbol::PipFilled).max(theme::width(Symbol::PipEmpty))
}

fn toggle_mark_width() -> u16 {
    theme::width(Symbol::MarkToggleOn).max(theme::width(Symbol::MarkToggleOff))
}

/// The house button, in the neutral hue: what a declared button does is
/// recorded, never run, so none of them builds or destroys anything.
fn button(label: &str, hovered: bool) -> Button {
    Button::new(label, Token::TextSecondary).strong(hovered)
}

/// A segmented bar `width` cells wide: each segment as wide as its weight
/// is of their sum, a cell apart where there is room for it, and filled to
/// its share to the finest step the glyph set draws.
fn progress_line(segments: &[Segment], width: u16) -> Line<'static> {
    let width = usize::from(width);
    if segments.is_empty() || width == 0 {
        return Line::default();
    }
    // A gap between segments only while every one keeps two cells of its
    // own: below that the gaps would be most of the bar.
    let gaps = match width >= segments.len() * 3 - 1 {
        true => segments.len() - 1,
        false => 0,
    };
    let steps = theme::frames(Symbol::ProgressFill);
    let full = steps.last().cloned().unwrap_or_default();
    let track = theme::glyph(Symbol::ProgressTrack);
    let faint = theme::fg(Token::TextFaint);
    let mut spans = Vec::new();
    for (index, (segment, cells)) in segments
        .iter()
        .zip(segment_cells(segments, width - gaps))
        .enumerate()
    {
        if index > 0 && gaps > 0 {
            spans.push(TextSpan::raw(" "));
        }
        let hue = theme::fg(token(segment.role));
        let eighths = (u64::from(segment.filled.min(1000)) * cells as u64 * 8 + 500) / 1000;
        let eighths = usize::try_from(eighths).unwrap_or(usize::MAX);
        let (whole, part) = (eighths / 8, eighths % 8);
        spans.push(TextSpan::styled(full.repeat(whole), hue));
        let mut drawn = whole;
        // The step nearest the part of the cell that is filled, out of as
        // many as the set draws; none at all leaves the track showing.
        let step = (part * steps.len() + 4) / 8;
        if part > 0 && drawn < cells {
            match step.checked_sub(1).and_then(|index| steps.get(index)) {
                Some(glyph) => spans.push(TextSpan::styled(glyph.clone(), hue)),
                None => spans.push(TextSpan::styled(track.clone(), faint)),
            }
            drawn += 1;
        }
        spans.push(TextSpan::styled(
            track.repeat(cells.saturating_sub(drawn)),
            faint,
        ));
    }
    Line::from(spans)
}

/// `width` cells shared by weight, the largest remainders rounding up, and
/// a cell for every weighted segment while the widest can spare one.
fn segment_cells(segments: &[Segment], width: usize) -> Vec<usize> {
    let weights: Vec<u64> = match segments.iter().all(|segment| segment.weight == 0) {
        true => vec![1; segments.len()],
        false => segments
            .iter()
            .map(|segment| u64::from(segment.weight))
            .collect(),
    };
    let total: u64 = weights.iter().sum();
    let cells64 = width as u64;
    let mut cells: Vec<usize> = weights
        .iter()
        .map(|weight| usize::try_from(weight * cells64 / total).unwrap_or(0))
        .collect();
    let mut order: Vec<usize> = (0..segments.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(weights[*index] * cells64 % total));
    let left = width.saturating_sub(cells.iter().sum());
    for index in order.iter().cycle().take(left) {
        cells[*index] += 1;
    }
    for index in 0..cells.len() {
        if weights[index] == 0 || cells[index] > 0 {
            continue;
        }
        if let Some(widest) = (0..cells.len()).max_by_key(|other| cells[*other])
            && cells[widest] > 1
        {
            cells[widest] -= 1;
            cells[index] = 1;
        }
    }
    cells
}

/// Draws one frame of a tree, a row at a time, into the window it owns.
struct Painter<'p, 'f> {
    frame: &'p mut ratatui::Frame<'f>,
    area: Rect,
    /// Rows of the tree above the window.
    scroll: usize,
    /// Rows of the window the tree may draw in.
    rows: usize,
    held: TreeFrame,
    hits: &'p mut Vec<(Rect, ViewHit)>,
    /// Where a folded tree stopped and how many nodes it left out there.
    folded: Option<(usize, usize)>,
}

impl Painter<'_, '_> {
    fn screen_row(&self, y: usize) -> Option<u16> {
        let below = y.checked_sub(self.scroll)?;
        (below < self.rows).then(|| self.area.y.saturating_add(cells_of(below)))
    }

    /// `x`..`x + width`, kept inside the area.
    fn columns(&self, x: u16, width: u16) -> Option<(u16, u16)> {
        let start = x.max(self.area.x);
        let end = x.saturating_add(width).min(self.area.right());
        (end > start).then_some((start, end - start))
    }

    fn fits(&self, y: usize, rows: usize) -> bool {
        y + rows <= self.scroll + self.rows
    }

    fn line(&mut self, y: usize, x: u16, width: u16, line: Line<'static>) {
        let (Some(row), Some((x, width))) = (self.screen_row(y), self.columns(x, width)) else {
            return;
        };
        self.frame
            .render_widget(Paragraph::new(line), Rect::new(x, row, width, 1));
    }

    /// The window's part of `rows` rows from `y`, `width` cells from `x`.
    fn visible(&self, y: usize, rows: usize, x: u16, width: u16) -> Option<Rect> {
        let (x, width) = self.columns(x, width)?;
        let top = y.max(self.scroll);
        let bottom = (y + rows).min(self.scroll + self.rows);
        let first = self.screen_row(top)?;
        let height = cells_of(bottom.checked_sub(top)?);
        (height > 0).then(|| Rect::new(x, first, width, height))
    }

    fn hit(&mut self, y: usize, rows: usize, x: u16, width: u16, hit: ViewHit) {
        if let Some(rect) = self.visible(y, rows, x, width) {
            self.hits.push((rect, hit));
        }
    }

    fn hovered(&self, hit: ViewHit) -> bool {
        self.held.hovered == Some(hit)
    }

    /// Ends a fold at row `y`, `left` nodes short.
    fn fold_at(&mut self, y: usize, left: usize) -> bool {
        self.folded.get_or_insert((y, left));
        false
    }

    /// The row saying how much a folded tree left out, in the row it kept
    /// for that past the content's window.
    fn more(&mut self, y: usize, count: usize) {
        let mark = theme::glyph(Symbol::MarkMore);
        let said = match count {
            0 => mark,
            count => format!("{mark} {count} more"),
        };
        self.rows += 1;
        self.line(
            y,
            self.area.x,
            self.area.width,
            Line::from(TextSpan::styled(said, theme::fg(Token::TextMuted))),
        );
        self.rows -= 1;
    }

    /// Draws `node` at row `y`, `width` cells from `x`. Answers `false`
    /// once a fold has ended the tree, so nothing after it is drawn.
    fn node(&mut self, node: &Node, x: u16, width: u16, y: usize) -> bool {
        if y >= self.scroll + self.rows && !self.held.fold {
            return true;
        }
        match &node.kind {
            NodeKind::Stack { gap } => return self.stack(&node.children, *gap, x, width, y),
            NodeKind::Row { gap, align } => {
                let (offset, widths) = row_widths(&node.children, *gap, *align, width);
                let mut at = x.saturating_add(offset);
                for (child, width) in node.children.iter().zip(widths) {
                    if !self.node(child, at, width, y) {
                        return false;
                    }
                    at = at.saturating_add(width).saturating_add(*gap);
                }
            }
            NodeKind::Text { spans, truncate } => match truncate {
                true => self.line(y, x, width, elided(spans, width)),
                false => {
                    for (row, line) in wrapped(spans, width).into_iter().enumerate() {
                        self.line(y + row, x, width, line);
                    }
                }
            },
            NodeKind::Badge { text: label, role } => {
                let pad = " ".repeat(usize::from(BADGE_PAD));
                let style = Style::default()
                    .fg(color(*role))
                    .bg(theme::color(Token::SurfaceRaisedSubtle));
                let line = Line::from(TextSpan::styled(format!("{pad}{label}{pad}"), style));
                self.line(y, x, width, line);
            }
            NodeKind::Marks(roles) => {
                let mark = theme::glyph(Symbol::MarkFacet);
                let spans: Vec<TextSpan<'static>> = roles
                    .iter()
                    .map(|role| TextSpan::styled(mark.clone(), Style::default().fg(color(*role))))
                    .collect();
                self.line(y, x, width, Line::from(spans));
            }
            NodeKind::Pips { done, total } => {
                let done = (*done).min(*total);
                let line = Line::from(vec![
                    TextSpan::styled(
                        theme::glyph(Symbol::PipFilled).repeat(usize::from(done)),
                        theme::fg(Token::TextSecondary),
                    ),
                    TextSpan::styled(
                        theme::glyph(Symbol::PipEmpty).repeat(usize::from(total - done)),
                        theme::fg(Token::TextFaint),
                    ),
                ]);
                self.line(y, x, width, line);
            }
            NodeKind::Progress { segments } => {
                self.line(y, x, width, progress_line(segments, width));
            }
            NodeKind::List { selected, empty } => {
                return self.list(node, *selected, empty.as_deref(), (x, width), y);
            }
            NodeKind::Table { columns, empty } => {
                return self.table(node, columns, empty.as_deref(), (x, width), y);
            }
            NodeKind::Tabs { tabs } => self.tabs(node.id, tabs, x, width, y),
            NodeKind::Toggle { label, on } => self.toggle(node, label, *on, (x, width), y),
            NodeKind::Button { label } => {
                let hit = ViewHit::Node {
                    node: node.id,
                    act: NodeAct::Press,
                };
                let drawn = button(label, self.hovered(hit));
                if let Some(rect) = self.visible(y, 1, x, drawn.width().min(width)) {
                    drawn.render(self.frame, rect);
                    self.hits.push((rect, hit));
                }
            }
            NodeKind::Activity { busy } => {
                let (glyph, ink) = match busy {
                    true => (
                        theme::frame(Symbol::ActivityBusy, self.held.clock.tick()),
                        Token::StateInFlight,
                    ),
                    false => (theme::glyph(Symbol::ActivityIdle), Token::TextFaint),
                };
                self.line(
                    y,
                    x,
                    width,
                    Line::from(TextSpan::styled(glyph, theme::fg(ink))),
                );
            }
            NodeKind::Elapsed { since_ms, role } => {
                if let Some(since) = since_ms {
                    let said = elapsed(*since, self.held.clock.now_ms);
                    let line =
                        Line::from(TextSpan::styled(said, Style::default().fg(color(*role))));
                    self.line(y, x, width, line);
                }
            }
            NodeKind::Log { lines } => {
                for (row, line) in lines.iter().enumerate() {
                    if self.held.fold && !self.fits(y + row, 1) {
                        return self.fold_at(y + row, lines.len() - row);
                    }
                    self.line(y + row, x, width, elided(std::slice::from_ref(line), width));
                }
            }
            NodeKind::Notice { text: said, role } => {
                let rows = wrapped(&[Span::new(said.clone(), *role)], width);
                for (row, line) in rows.into_iter().enumerate() {
                    self.line(y + row, x, width, line);
                }
            }
        }
        true
    }

    /// Children one above another; folding, the first that does not fit
    /// whole ends the tree.
    fn stack(&mut self, children: &[Node], gap: u16, x: u16, width: u16, y: usize) -> bool {
        let mut at = y;
        for (index, child) in children.iter().enumerate() {
            let rows = height(child, width);
            if self.held.fold && !self.fits(at, rows) {
                return self.fold_at(at, children.len() - index);
            }
            if !self.node(child, x, width, at) {
                return false;
            }
            at += rows + usize::from(gap);
        }
        true
    }

    /// The mark and the label, the whole of it a click target, the label
    /// brightening under the pointer.
    fn toggle(&mut self, node: &Node, label: &str, on: bool, (x, width): (u16, u16), y: usize) {
        let hit = ViewHit::Node {
            node: node.id,
            act: NodeAct::Toggle,
        };
        let (symbol, mark) = match on {
            true => (Symbol::MarkToggleOn, Token::Accent),
            false => (Symbol::MarkToggleOff, Token::TextMuted),
        };
        let ink = match (self.hovered(hit), on) {
            (true, _) => Token::TextBright,
            (false, true) => Token::TextPrimary,
            (false, false) => Token::TextSecondary,
        };
        let line = Line::from(vec![
            TextSpan::styled(format!("{} ", theme::glyph(symbol)), theme::fg(mark)),
            TextSpan::styled(label.to_owned(), theme::fg(ink)),
        ]);
        let taken = natural(node).min(width);
        self.line(y, x, taken, line);
        self.hit(y, 1, x, taken, hit);
    }

    /// What a list or a table says with nothing in it, in the quiet tone
    /// every empty state takes.
    fn empty(&mut self, said: Option<&str>, x: u16, width: u16, y: usize) {
        if let Some(said) = said {
            let line = Line::from(TextSpan::styled(
                text::elide(said, usize::from(width)),
                theme::fg(Token::TextMuted),
            ));
            self.line(y, x, width, line);
        }
    }

    /// The rows of a list, each on the ground a row of every other list
    /// takes when it is chosen or pointed at, and each a click target.
    fn list(
        &mut self,
        node: &Node,
        selected: Option<usize>,
        empty: Option<&str>,
        (x, width): (u16, u16),
        y: usize,
    ) -> bool {
        if node.children.is_empty() {
            self.empty(empty, x, width, y);
            return true;
        }
        let inner = list_inner(width);
        let inset = (width - inner) / 2;
        let mut at = y;
        for (index, child) in node.children.iter().enumerate() {
            let rows = height(child, inner).max(1);
            if self.held.fold && !self.fits(at, rows) {
                return self.fold_at(at, node.children.len() - index);
            }
            let hit = ViewHit::Node {
                node: node.id,
                act: NodeAct::Row(index),
            };
            let state = RowState::of(selected == Some(index), self.hovered(hit));
            if let (Some(ground), Some(rect)) = (state.ground(), self.visible(at, rows, x, width)) {
                widget::fill(self.frame, rect, ground);
            }
            if !self.node(child, x.saturating_add(inset), inner, at) {
                return false;
            }
            self.hit(at, rows, x, width, hit);
            at += rows;
        }
        true
    }

    /// Headings over a blank row, then the rows, every cell in the column
    /// its heading stands over.
    fn table(
        &mut self,
        node: &Node,
        columns: &[String],
        empty: Option<&str>,
        (x, width): (u16, u16),
        y: usize,
    ) -> bool {
        if node.children.is_empty() {
            self.empty(empty, x, width, y);
            return true;
        }
        let widths = fitted_columns(columns, &node.children, width);
        let starts: Vec<u16> = widths
            .iter()
            .scan(x, |at, width| {
                let start = *at;
                *at = at.saturating_add(*width).saturating_add(TABLE_GAP);
                Some(start)
            })
            .collect();
        for ((heading, start), column) in columns.iter().zip(&starts).zip(&widths) {
            let line = Line::from(TextSpan::styled(
                text::elide(heading, usize::from(*column)),
                theme::fg(Token::TextMuted),
            ));
            self.line(y, *start, *column, line);
        }
        let mut at = y + 2;
        for (index, row) in node.children.iter().enumerate() {
            let rows = table_row_height(row, &widths);
            if self.held.fold && !self.fits(at, rows) {
                return self.fold_at(at, node.children.len() - index);
            }
            for ((cell, start), column) in cells(row).iter().zip(&starts).zip(&widths) {
                if !self.node(cell, *start, *column, at) {
                    return false;
                }
            }
            at += rows;
        }
        true
    }

    /// The tabs as the segmented control a view's modes are drawn as, as
    /// many as fit whole; one at rest brightens under the pointer.
    fn tabs(&mut self, id: usize, tabs: &[Tab], x: u16, width: u16, y: usize) {
        let (Some(row), Some((x, room))) = (self.screen_row(y), self.columns(x, width)) else {
            return;
        };
        let modes = modes_of(tabs);
        let mut used = 0u16;
        let fitting = modes
            .iter()
            .take_while(|mode| {
                used = used.saturating_add(mode_width(mode));
                used <= room
            })
            .count();
        let first = self.hits.len();
        render_chips(self.frame, x, row, &modes[..fitting], self.hits, |index| {
            ViewHit::Node {
                node: id,
                act: NodeAct::Tab(index),
            }
        });
        let hovered = self.held.hovered;
        for (rect, hit) in &self.hits[first..] {
            if let ViewHit::Node {
                act: NodeAct::Tab(index),
                ..
            } = hit
                && hovered == Some(*hit)
                && !modes[*index].active
            {
                self.frame
                    .buffer_mut()
                    .set_style(*rect, theme::fg(Token::TextBright));
            }
        }
    }
}

#[cfg(test)]
mod tests;
