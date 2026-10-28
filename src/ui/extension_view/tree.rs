//! A declared extension's [`Content::Tree`]: the catalog's nodes, laid out
//! and drawn by the host.
//!
//! Layout is two walks over the same rules: [`height`] says how many rows a
//! node takes at a width, and [`Painter::node`] draws it there. Rows are
//! counted from the top of the tree and only the ones inside the window are
//! drawn, so scrolling and clipping are one question asked of every row.
//!
//! Nothing here is kept between frames. What moves on its own, an
//! [`NodeKind::Activity`], an [`NodeKind::Elapsed`] and a
//! [`NodeKind::Sprite`], is drawn from the [`Clock`] the frame is handed,
//! which is how each stays current without the extension being asked for
//! a new tree.

mod display;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::widgets::{Block, BorderType, Borders};
use uze_contract::catalog::Size as Length;
use uze_extensions::view::{
    Align, Border, BorderStyle, BoxLayout, CellGlyph, CellRegion, Ground, Justify, Node, NodeAct,
    NodeKind, Picture, Segment, Tab, sprite_frame, sprite_moves,
};

use super::*;
use crate::ui::widget::{Button, Edge, Field as Input, Rule, row::RowState};

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
    /// The same instant on the clock the host stamps a sprite's state
    /// with.
    pub(crate) instant: Instant,
}

impl Clock {
    pub(crate) fn now() -> Self {
        let instant = Instant::now();
        if let Some(now_ms) = STOPPED_AT.with(std::cell::Cell::get) {
            return Self { now_ms, instant };
        }
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        Self { now_ms, instant }
    }

    /// How far into its state a sprite stamped `since` is: from the
    /// stamp, or from the epoch, which plays a loop in step with the
    /// clock, for one never stamped.
    fn into_state(self, since: Option<Instant>) -> Duration {
        match since {
            Some(since) => self.instant.saturating_duration_since(since),
            None => Duration::from_millis(self.now_ms),
        }
    }

    fn tick(self) -> usize {
        usize::try_from(self.now_ms / ACTIVITY_FRAME_MS).unwrap_or(usize::MAX)
    }
}

thread_local! {
    static STOPPED_AT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Runs `draw` with the clock every tree is drawn from stopped at
/// `now_ms`, on this thread alone: a frame drawn through the whole client
/// at an instant a test chooses, and an extension's test drawn at its
/// `now`, so what a clock moves never drifts in a snapshot.
pub(crate) fn stopped_at<T>(now_ms: u64, draw: impl FnOnce() -> T) -> T {
    let before = STOPPED_AT.with(|stopped| stopped.replace(Some(now_ms)));
    let drawn = draw();
    STOPPED_AT.with(|stopped| stopped.set(before));
    drawn
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
        gauge_labels: None,
        ink: None,
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
/// The cells a tree asks for when nothing tells it otherwise: what an item
/// of the bar takes of it.
pub(crate) fn tree_width(root: &Node) -> u16 {
    natural(root)
}

pub(crate) fn tree_height(root: &Node, width: u16) -> u16 {
    u16::try_from(height(root, width)).unwrap_or(u16::MAX)
}

/// Whether anything in `root` moves with the clock.
pub(crate) fn tree_animates(root: &Node) -> bool {
    match &root.kind {
        NodeKind::Activity { busy: true } => true,
        NodeKind::Elapsed {
            since_ms: Some(_), ..
        } => true,
        NodeKind::Sprite {
            frames,
            fps,
            looped,
            since,
            ..
        } => sprite_moves(frames.len(), *fps, *looped, Clock::now().into_state(*since)),
        _ => root.children.iter().any(tree_animates),
    }
}

/// The columns a picture takes `rows` rows tall, two of its pixels to a
/// cell's height so its proportions hold.
fn picture_columns(picture: &Picture, rows: u16) -> u16 {
    if picture.height == 0 {
        return 1;
    }
    let columns = picture.width * usize::from(rows) * 2 / picture.height;
    cells_of(columns.max(1))
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
/// marketplace it came from, as the identity spells itself. Never the
/// name the extension gives itself, which any plugin may choose.
pub(crate) fn attribution(identity: &uze_contract::Identity) -> String {
    identity.to_string()
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
        NodeKind::Box(layout) => box_natural(node, layout),
        NodeKind::Icon { glyph, .. } => theme::icon(glyph).1,
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
        NodeKind::Progress { segments, .. } => cells_of(segments.len())
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
        NodeKind::Select { label, options, .. } => select_width(label, options),
        NodeKind::Field {
            label,
            value,
            placeholder,
            ..
        } => lead_width(label)
            .saturating_add(cells_of(
                text::columns(value).max(text::columns(placeholder)),
            ))
            .saturating_add(1)
            .max(FIELD_MIN_WIDTH),
        NodeKind::Form { submit, .. } => node
            .children
            .iter()
            .map(natural)
            .max()
            .unwrap_or(0)
            .max(button(submit, false).width()),
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
        NodeKind::Rule { label, .. } => display::rule_width(label.as_deref()),
        NodeKind::Details => {
            let (keys, values) = entry_columns(&node.children);
            keys.saturating_add(2).saturating_add(values)
        }
        NodeKind::Entry { key, value, .. } => cells_of(text::columns(key))
            .saturating_add(2)
            .saturating_add(cells_of(text::columns(value))),
        NodeKind::Markdown { text: source } => display::block_width(source).min(MARKDOWN_MEASURE),
        NodeKind::Code { text: source, .. } => display::block_width(source),
        NodeKind::Gauge { label, .. } => display::gauge_width(label),
        NodeKind::Chart { levels, .. } => cells_of(levels.len()),
        NodeKind::Stat {
            value,
            label,
            delta,
            ..
        } => display::stat_width(value, label, delta.as_deref()),
        NodeKind::Kbd { key } => cells_of(text::columns(key)),
        NodeKind::Grid { .. } => node.children.iter().map(natural).max().unwrap_or(0),
        NodeKind::Cells { rows, .. } => cells_of(rows.iter().map(Vec::len).max().unwrap_or(0)),
        NodeKind::Image {
            picture,
            alt,
            refused,
            height,
        } => match picture {
            Some(picture) => picture_columns(picture, *height),
            None => cells_of(text::columns(refused.as_deref().unwrap_or(alt)).max(1)),
        },
        NodeKind::Sprite {
            frames,
            height,
            refused,
            ..
        } => match frames.first() {
            Some(frame) => picture_columns(frame, *height),
            None => cells_of(refused.as_deref().map_or(1, text::columns)),
        },
    };
    own.max(node.min_width)
}

/// The rows a node takes at `width`.
fn height(node: &Node, width: u16) -> usize {
    match &node.kind {
        NodeKind::Box(layout) => box_height(node, layout, own_width(layout, width)),
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
        NodeKind::Details => node.children.len(),
        NodeKind::Markdown { text: source } => display::markdown(source, width).len().max(1),
        NodeKind::Code {
            text: source,
            language,
        } => display::code(source, language, width).len().max(1),
        NodeKind::Chart { height: rows, .. } => usize::from(*rows),
        NodeKind::Cells { rows, .. } => rows.len(),
        NodeKind::Image { height: rows, .. } | NodeKind::Sprite { height: rows, .. } => {
            usize::from(*rows)
        }
        NodeKind::Stat { .. } => 2,
        // The line typed into, and the rule under it that says it is one.
        NodeKind::Field { .. } => 2,
        // Its fields one above another, then its button.
        NodeKind::Form { .. } => {
            node.children
                .iter()
                .map(|child| height(child, width))
                .sum::<usize>()
                + 1
        }
        NodeKind::Grid { gap } => {
            let (columns, tile) = grid_columns(node, *gap, width);
            let rows: Vec<usize> = node
                .children
                .chunks(columns)
                .map(|row| {
                    row.iter()
                        .map(|child| height(child, tile))
                        .max()
                        .unwrap_or(0)
                })
                .collect();
            rows.iter().sum::<usize>() + usize::from(*gap > 0) * rows.len().saturating_sub(1)
        }
        NodeKind::Icon { glyph, .. } => usize::from(theme::icon(glyph).1 > 0),
        NodeKind::Rule { .. }
        | NodeKind::Entry { .. }
        | NodeKind::Gauge { .. }
        | NodeKind::Kbd { .. }
        | NodeKind::Badge { .. }
        | NodeKind::Marks(_)
        | NodeKind::Pips { .. }
        | NodeKind::Progress { .. }
        | NodeKind::Tabs { .. }
        | NodeKind::Toggle { .. }
        | NodeKind::Button { .. }
        | NodeKind::Select { .. }
        | NodeKind::Activity { .. } => 1,
    }
}

/// The narrowest a field is drawn, so an empty one still reads as a
/// place to type.
const FIELD_MIN_WIDTH: u16 = 16;

/// A select's or a field's label and the column after it, when it has one.
fn lead_width(label: &str) -> u16 {
    match label.is_empty() {
        true => 0,
        false => cells_of(text::columns(label)).saturating_add(1),
    }
}

/// `label ‹ choice ›`, as wide as its widest choice so stepping never
/// moves what follows it.
fn select_width(label: &str, options: &[String]) -> u16 {
    let widest = options.iter().map(|option| text::columns(option)).max();
    lead_width(label)
        .saturating_add(step_width(Symbol::StepPrevious))
        .saturating_add(cells_of(widest.unwrap_or(0)))
        .saturating_add(step_width(Symbol::StepNext))
}

/// An arrow of a select and the column between it and the choice.
fn step_width(symbol: Symbol) -> u16 {
    theme::width(symbol).saturating_add(1)
}

/// The widest a `Markdown` asks to be: a reading measure, past which its
/// paragraphs would be lines too long to follow.
const MARKDOWN_MEASURE: u16 = 80;
/// The cells a box's border takes on each side.
fn border_cells(layout: &BoxLayout) -> u16 {
    u16::from(layout.border.is_some())
}

/// The columns and rows a box keeps between its edge and its children.
fn box_chrome(layout: &BoxLayout) -> (u16, u16) {
    let border = border_cells(layout);
    (
        layout.pad_x.saturating_add(border).saturating_mul(2),
        layout.pad_y.saturating_add(border).saturating_mul(2),
    )
}

/// A box's own width in `room`: its `width` of it, or all of it.
fn own_width(layout: &BoxLayout, room: u16) -> u16 {
    layout.width.map_or(room, |size| size.of(room))
}

/// The width a box gives a child of its inner `room`: a box's own size of
/// it, else what the child asks, never more than the room.
fn child_width(child: &Node, room: u16) -> u16 {
    match &child.kind {
        NodeKind::Box(layout) if layout.width.is_some() => own_width(layout, room),
        _ => natural(child).min(room),
    }
}

/// The rows a child takes at `width`, a box already sized by its parent.
fn sized_height(child: &Node, width: u16) -> usize {
    match &child.kind {
        NodeKind::Box(layout) => box_height(child, layout, width),
        _ => height(child, width),
    }
}

fn box_natural(node: &Node, layout: &BoxLayout) -> u16 {
    if let Some(Length::Cells(cells)) = layout.width {
        return cells;
    }
    let gaps = layout
        .gap
        .saturating_mul(cells_of(node.children.len().saturating_sub(1)));
    let inner = match layout.row {
        true => sum(node.children.iter().map(natural)).saturating_add(gaps),
        false => node.children.iter().map(natural).max().unwrap_or(0),
    };
    inner.saturating_add(box_chrome(layout).0)
}

/// A row box's children in lines of `room` cells, each child with the
/// width it is drawn at: a line breaks before a child that does not fit
/// when the box wraps, and the growing ones of a line share what is left.
fn box_lines(node: &Node, layout: &BoxLayout, room: u16) -> Vec<Vec<(usize, u16)>> {
    let mut lines: Vec<Vec<(usize, u16)>> = vec![Vec::new()];
    let mut used = 0u16;
    for (index, child) in node.children.iter().enumerate() {
        let width = child_width(child, room);
        let line = lines.last_mut().expect("there is always a line");
        let needed = match line.is_empty() {
            true => width,
            false => layout.gap.saturating_add(width),
        };
        if layout.wrap && !line.is_empty() && used.saturating_add(needed) > room {
            lines.push(vec![(index, width)]);
            used = width;
        } else {
            line.push((index, width.min(room.saturating_sub(used.min(room)))));
            used = used.saturating_add(needed);
        }
    }
    for line in &mut lines {
        let taken = sum(line.iter().map(|(_, width)| *width)).saturating_add(
            layout
                .gap
                .saturating_mul(cells_of(line.len().saturating_sub(1))),
        );
        let growing: Vec<usize> = (0..line.len())
            .filter(|at| node.children[line[*at].0].grow)
            .collect();
        let left = room.saturating_sub(taken);
        let count = cells_of(growing.len());
        if let (Some(each), Some(rest)) = (left.checked_div(count), left.checked_rem(count)) {
            for (nth, at) in growing.iter().enumerate() {
                let share = each + u16::from(cells_of(nth) < rest);
                line[*at].1 = line[*at].1.saturating_add(share);
            }
        }
    }
    lines
}

/// The rows a box takes when it is `width` cells wide.
fn box_height(node: &Node, layout: &BoxLayout, width: u16) -> usize {
    if let Some(rows) = layout.height {
        return usize::from(rows);
    }
    let (chrome_x, chrome_y) = box_chrome(layout);
    let room = width.saturating_sub(chrome_x);
    let gap = usize::from(layout.gap);
    let content = match layout.row {
        false => {
            let rows: usize = node
                .children
                .iter()
                .map(|child| sized_height(child, child_width_in_column(child, room)))
                .sum();
            rows + gap * node.children.len().saturating_sub(1)
        }
        true => {
            let lines = box_lines(node, layout, room);
            let rows: usize = lines
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|(index, width)| sized_height(&node.children[*index], *width))
                        .max()
                        .unwrap_or(0)
                })
                .sum();
            rows + gap * lines.len().saturating_sub(1)
        }
    };
    content + usize::from(chrome_y)
}

/// The width a column box gives a child: all of its room, but a box its
/// own size of it.
fn child_width_in_column(child: &Node, room: u16) -> u16 {
    match &child.kind {
        NodeKind::Box(layout) => own_width(layout, room),
        _ => room,
    }
}

/// Where the children of a line start, and the gap after each, for the
/// box's `justify` over the `left` cells nothing took.
fn justified(justify: Justify, left: u16, count: usize, gap: u16) -> (u16, u16) {
    match justify {
        Justify::Start => (0, gap),
        Justify::Center => (left / 2, gap),
        Justify::End => (left, gap),
        Justify::Between if count > 1 => (0, gap.saturating_add(left / cells_of(count - 1))),
        Justify::Between => (0, gap),
    }
}

/// The ground's colour, and the ink that reads on it.
fn ground_colours(ground: &Ground) -> (Color, Color) {
    let hue = token(ground.role);
    let paint = match ground.soft {
        true => theme::softened(hue, Token::SurfaceBackground),
        false => theme::color(hue),
    };
    (paint, theme::ink_on(paint))
}

/// A `Grid`'s tile when it names no `min-width`.
const GRID_TILE: u16 = 24;

/// A `Grid`'s columns at `width`, and each tile's width: as many as fit
/// its `min-width` with `gap` between them, never fewer than one.
fn grid_columns(grid: &Node, gap: u16, width: u16) -> (usize, u16) {
    let tile = match grid.min_width {
        0 => GRID_TILE,
        asked => asked,
    };
    let columns = (u32::from(width) + u32::from(gap)) / (u32::from(tile) + u32::from(gap));
    let columns = u16::try_from(columns.max(1))
        .unwrap_or(1)
        .min(cells_of(grid.children.len().max(1)));
    let gaps = gap.saturating_mul(columns - 1);
    (usize::from(columns), width.saturating_sub(gaps) / columns)
}

/// The key and value columns of a `Details`' entries.
fn entry_columns(entries: &[Node]) -> (u16, u16) {
    entries
        .iter()
        .fold((0, 0), |(keys, values), entry| match &entry.kind {
            NodeKind::Entry { key, value, .. } => (
                keys.max(cells_of(text::columns(key))),
                values.max(cells_of(text::columns(value))),
            ),
            _ => (keys, values),
        })
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

/// A row of a `Cells` grid as spans, one for each run of a role.
fn glyph_runs(glyphs: &[CellGlyph]) -> Vec<TextSpan<'static>> {
    let mut spans: Vec<TextSpan<'static>> = Vec::new();
    let mut run = String::new();
    let mut role = None;
    for glyph in glyphs {
        if role.is_some_and(|role| role != glyph.role) {
            let ink = Style::default().fg(color(role.unwrap_or_default()));
            spans.push(TextSpan::styled(std::mem::take(&mut run), ink));
        }
        role = Some(glyph.role);
        run.push(glyph.ch);
    }
    if let Some(role) = role {
        spans.push(TextSpan::styled(run, Style::default().fg(color(role))));
    }
    spans
}

/// The house button, in the neutral hue: what a declared button does is
/// recorded, never run, so none of them builds or destroys anything.
fn button(label: &str, hovered: bool) -> Button {
    Button::new(label, Token::TextSecondary).strong(hovered)
}

/// A segmented bar `width` cells wide: each segment as wide as its weight
/// is of their sum, a cell apart where there is room for it, and filled to
/// its share to the finest step the glyph set draws.
/// A progress bar as a thin line: each segment's filled cells in its role,
/// the rest in a quiet one, to the whole cell. Read as a measure even full,
/// where a bar of blocks reads as a slab.
fn thin_progress_line(segments: &[Segment], width: u16) -> Line<'static> {
    let width = usize::from(width);
    if segments.is_empty() || width == 0 {
        return Line::default();
    }
    let glyph = theme::glyph(Symbol::ProgressLine);
    let track = theme::fg(Token::TextFaint);
    let mut spans = Vec::new();
    for (segment, cells) in segments.iter().zip(segment_cells(segments, width)) {
        let filled =
            usize::try_from((u64::from(segment.filled.min(1000)) * cells as u64 + 500) / 1000)
                .unwrap_or(cells)
                .min(cells);
        spans.push(TextSpan::styled(
            glyph.repeat(filled),
            theme::fg(token(segment.role)),
        ));
        spans.push(TextSpan::styled(glyph.repeat(cells - filled), track));
    }
    Line::from(spans)
}

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
    /// The label column the gauges of the stack being drawn share, so
    /// their bars start together.
    gauge_labels: Option<u16>,
    /// The ink every line takes while it is drawn on a box's ground, so a
    /// role never sits unreadable on another's hue.
    ink: Option<Color>,
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

    fn line(&mut self, y: usize, x: u16, width: u16, mut line: Line<'static>) {
        let (Some(row), Some((x, width))) = (self.screen_row(y), self.columns(x, width)) else {
            return;
        };
        if let Some(ink) = self.ink {
            for span in &mut line.spans {
                span.style = span.style.fg(ink);
            }
        }
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
            NodeKind::Box(layout) => {
                return self.boxed(node, layout, x, own_width(layout, width), y);
            }
            NodeKind::Icon { glyph, role } => {
                let (drawn, _) = theme::icon(glyph);
                let line = Line::from(TextSpan::styled(drawn, Style::default().fg(color(*role))));
                self.line(y, x, width, line);
            }
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
            NodeKind::Progress { segments, line } => {
                let drawn = match line {
                    true => thin_progress_line(segments, width),
                    false => progress_line(segments, width),
                };
                self.line(y, x, width, drawn);
            }
            NodeKind::List { selected, empty } => {
                return self.list(node, *selected, empty.as_deref(), (x, width), y);
            }
            NodeKind::Table { columns, empty } => {
                return self.table(node, columns, empty.as_deref(), (x, width), y);
            }
            NodeKind::Tabs { tabs } => self.tabs(node.id, tabs, x, width, y),
            NodeKind::Select {
                label,
                options,
                chosen,
                focused,
            } => self.select(
                node.id,
                label,
                options.get(*chosen),
                *focused,
                (x, width),
                y,
            ),
            NodeKind::Field {
                label,
                value,
                placeholder,
                focused,
                editing,
            } => self.field(
                node.id,
                (label, value, placeholder),
                (*focused, *editing),
                (x, width),
                y,
            ),
            NodeKind::Form { submit, focused } => {
                return self.form(node, submit, *focused, (x, width), y);
            }
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
            NodeKind::Rule { label, role } => {
                self.line(y, x, width, display::rule(label.as_deref(), *role, width));
            }
            NodeKind::Details => {
                let keys: Vec<&str> = node
                    .children
                    .iter()
                    .filter_map(|entry| match &entry.kind {
                        NodeKind::Entry { key, .. } => Some(key.as_str()),
                        _ => None,
                    })
                    .collect();
                let column = display::key_column(&keys, width);
                for (row, entry) in node.children.iter().enumerate() {
                    if let NodeKind::Entry { key, value, role } = &entry.kind {
                        if self.held.fold && !self.fits(y + row, 1) {
                            return self.fold_at(y + row, node.children.len() - row);
                        }
                        self.line(
                            y + row,
                            x,
                            width,
                            display::entry(key, value, *role, column, width),
                        );
                    }
                }
            }
            NodeKind::Entry { key, value, role } => {
                let column = display::key_column(&[key], width);
                self.line(
                    y,
                    x,
                    width,
                    display::entry(key, value, *role, column, width),
                );
            }
            NodeKind::Markdown { text: source } => {
                return self.rows_of(display::markdown(source, width), x, width, y);
            }
            NodeKind::Code {
                text: source,
                language,
            } => return self.rows_of(display::code(source, language, width), x, width, y),
            NodeKind::Gauge {
                label,
                filled,
                role,
            } => {
                let column = self.gauge_labels.unwrap_or(cells_of(text::columns(label)));
                self.line(
                    y,
                    x,
                    width,
                    display::gauge(label, column, *filled, *role, width),
                );
            }
            NodeKind::Chart {
                kind,
                levels,
                height: rows,
                role,
            } => {
                return self.rows_of(
                    display::chart(*kind, levels, *rows, *role, width),
                    x,
                    width,
                    y,
                );
            }
            NodeKind::Stat {
                value,
                label,
                delta,
                role,
            } => {
                return self.rows_of(
                    display::stat(value, label, delta.as_deref(), *role, width),
                    x,
                    width,
                    y,
                );
            }
            NodeKind::Kbd { key } => self.line(y, x, width, display::kbd(key, width)),
            NodeKind::Grid { gap } => return self.grid(node, *gap, x, width, y),
            NodeKind::Cells { rows, regions } => {
                return self.cells(node.id, rows, regions, (x, width), y);
            }
            NodeKind::Image {
                picture,
                alt,
                refused,
                height,
            } => match picture {
                Some(picture) => self.picture(picture, (x, width), y, *height, false),
                None => self.standing_in(refused.as_deref(), alt, (x, width), y),
            },
            NodeKind::Sprite {
                frames,
                fps,
                looped,
                since,
                height,
                refused,
            } => {
                let elapsed = self.held.clock.into_state(*since);
                match frames.get(sprite_frame(frames.len(), *fps, *looped, elapsed)) {
                    Some(frame) => self.picture(frame, (x, width), y, *height, true),
                    None => self.standing_in(refused.as_deref(), "", (x, width), y),
                }
            }
        }
        true
    }

    /// A picture in the `rows` rows from `y`, fitted to the node's width.
    /// Only while all of it is in the window are its pixels placed: a
    /// picture cut at the window's edge is drawn in half blocks alone.
    fn picture(
        &mut self,
        picture: &Picture,
        (x, width): (u16, u16),
        y: usize,
        rows: u16,
        crisp: bool,
    ) {
        let width = picture_columns(picture, rows).min(width);
        let Some(rect) = self.visible(y, usize::from(rows), x, width) else {
            return;
        };
        let cell = (rect.height == rows)
            .then(crate::ui::graphics::cell_pixels)
            .flatten();
        super::picture::render_fitted(self.frame, rect, picture, cell, crisp);
    }

    /// What stands in for a picture there is none of: why it was refused,
    /// in the warning's hue, or its `alt`, quiet, while it is read.
    fn standing_in(&mut self, refused: Option<&str>, alt: &str, (x, width): (u16, u16), y: usize) {
        let (said, style) = match refused {
            Some(why) => (why, Style::default().fg(color(Role::Warning))),
            None => (alt, theme::fg(Token::TextMuted)),
        };
        let line = Line::from(TextSpan::styled(
            text::elide(said, usize::from(width)),
            style,
        ));
        self.line(y, x, width, line);
    }

    /// The grid a row at a time, each run of one role a span, then a
    /// target over every region of it, brightening under the pointer.
    fn cells(
        &mut self,
        id: usize,
        rows: &[Vec<CellGlyph>],
        regions: &[CellRegion],
        (x, width): (u16, u16),
        y: usize,
    ) -> bool {
        for (row, glyphs) in rows.iter().enumerate() {
            if self.held.fold && !self.fits(y + row, 1) {
                return self.fold_at(y + row, rows.len() - row);
            }
            self.line(y + row, x, width, Line::from(glyph_runs(glyphs)));
        }
        for (index, region) in regions.iter().enumerate() {
            let top = usize::from(region.y);
            let room = width.saturating_sub(region.x).min(region.width);
            if top >= rows.len() || room == 0 {
                continue;
            }
            let tall = usize::from(region.height).min(rows.len() - top);
            let hit = ViewHit::Node {
                node: id,
                act: NodeAct::Region(index),
            };
            let Some(rect) = self.visible(y + top, tall, x.saturating_add(region.x), room) else {
                continue;
            };
            if self.hovered(hit) {
                widget::lift(self.frame, rect);
            }
            self.hits.push((rect, hit));
        }
        true
    }

    /// Lines one under another from row `y`; folding, the first that does
    /// not fit ends the tree.
    fn rows_of(&mut self, lines: Vec<Line<'static>>, x: u16, width: u16, y: usize) -> bool {
        let count = lines.len();
        for (row, line) in lines.into_iter().enumerate() {
            if self.held.fold && !self.fits(y + row, 1) {
                return self.fold_at(y + row, count - row);
            }
            self.line(y + row, x, width, line);
        }
        true
    }

    /// Tiles row by row, each row as tall as its tallest tile.
    /// A box `width` cells wide: its ground and border first, then its
    /// children in its inner area, a line at a time. Folding, a box that
    /// does not fit whole ends the tree, as a stacked child does.
    fn boxed(&mut self, node: &Node, layout: &BoxLayout, x: u16, width: u16, y: usize) -> bool {
        let rows = box_height(node, layout, width);
        if self.held.fold && !self.fits(y, rows) {
            return self.fold_at(y, 1);
        }
        let ink = layout.ground.map(|ground| {
            let (paint, ink) = ground_colours(&ground);
            if let Some(rect) = self.visible(y, rows, x, width) {
                self.frame
                    .render_widget(Block::new().style(Style::default().bg(paint)), rect);
            }
            ink
        });
        if let Some(border) = layout.border {
            self.border(border, (x, width), y, rows);
        }
        let inherited = self.ink;
        let outer = std::mem::replace(&mut self.ink, ink.or(inherited));
        let edge = border_cells(layout);
        let inner_x = x.saturating_add(edge).saturating_add(layout.pad_x);
        let room = width.saturating_sub(box_chrome(layout).0);
        let mut at = y + usize::from(edge.saturating_add(layout.pad_y));
        let gap = usize::from(layout.gap);
        let mut drawn = true;
        if layout.row {
            for line in box_lines(node, layout, room) {
                let taken = sum(line.iter().map(|(_, width)| *width));
                let gaps = layout
                    .gap
                    .saturating_mul(cells_of(line.len().saturating_sub(1)));
                let left = room.saturating_sub(taken.saturating_add(gaps));
                let (start, between) = justified(layout.justify, left, line.len(), layout.gap);
                let mut left_x = inner_x.saturating_add(start);
                let mut tallest = 0;
                for (index, child_width) in &line {
                    let child = &node.children[*index];
                    tallest = tallest.max(sized_height(child, *child_width));
                    if !self.sized(child, left_x, *child_width, at) {
                        drawn = false;
                        break;
                    }
                    left_x = left_x.saturating_add(*child_width).saturating_add(between);
                }
                if !drawn {
                    break;
                }
                at += tallest + gap;
            }
        } else {
            for child in &node.children {
                let child_width = child_width_in_column(child, room);
                if !self.sized(child, inner_x, child_width, at) {
                    drawn = false;
                    break;
                }
                at += sized_height(child, child_width) + gap;
            }
        }
        self.ink = outer;
        drawn
    }

    /// A child of a box at the width the box gave it: a box is not sized
    /// again by its own `width`.
    fn sized(&mut self, child: &Node, x: u16, width: u16, y: usize) -> bool {
        match &child.kind {
            NodeKind::Box(layout) => self.boxed(child, layout, x, width, y),
            _ => self.node(child, x, width, y),
        }
    }

    /// A box's border, in its role: its top and bottom edges only where
    /// they are in the window, so a box scrolled half away keeps its sides.
    fn border(&mut self, border: Border, (x, width): (u16, u16), y: usize, rows: usize) {
        let Some(rect) = self.visible(y, rows, x, width) else {
            return;
        };
        let mut sides = Borders::LEFT | Borders::RIGHT;
        if self.screen_row(y).is_some() {
            sides |= Borders::TOP;
        }
        if self.screen_row(y + rows.saturating_sub(1)).is_some() {
            sides |= Borders::BOTTOM;
        }
        let kind = match border.style {
            BorderStyle::Line => BorderType::Plain,
            BorderStyle::Round => BorderType::Rounded,
            BorderStyle::Heavy => BorderType::Thick,
        };
        let ink = self.ink.unwrap_or_else(|| color(border.role));
        self.frame.render_widget(
            Block::new()
                .borders(sides)
                .border_type(kind)
                .border_style(Style::default().fg(ink)),
            rect,
        );
    }

    fn grid(&mut self, node: &Node, gap: u16, x: u16, width: u16, y: usize) -> bool {
        let (columns, tile) = grid_columns(node, gap, width);
        let mut at = y;
        for (index, row) in node.children.chunks(columns).enumerate() {
            let rows = row
                .iter()
                .map(|child| height(child, tile))
                .max()
                .unwrap_or(0);
            if self.held.fold && !self.fits(at, rows) {
                return self.fold_at(at, node.children.len() - index * columns);
            }
            let mut left = x;
            for child in row {
                if !self.node(child, left, tile, at) {
                    return false;
                }
                left = left.saturating_add(tile).saturating_add(gap);
            }
            at += rows + usize::from(gap > 0);
        }
        true
    }

    /// Children one above another; folding, the first that does not fit
    /// whole ends the tree.
    fn stack(&mut self, children: &[Node], gap: u16, x: u16, width: u16, y: usize) -> bool {
        let labels = children
            .iter()
            .filter_map(|child| match &child.kind {
                NodeKind::Gauge { label, .. } => Some(cells_of(text::columns(label))),
                _ => None,
            })
            .max();
        let outer = std::mem::replace(&mut self.gauge_labels, labels);
        let drawn = self.stacked(children, gap, x, width, y);
        self.gauge_labels = outer;
        drawn
    }

    fn stacked(&mut self, children: &[Node], gap: u16, x: u16, width: u16, y: usize) -> bool {
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

    /// `label ‹ choice ›`: the label quiet, the choice strong, accent while
    /// it has focus; a press on `‹` steps back and anywhere on the choice or
    /// `›` steps on, as Enter does.
    fn select(
        &mut self,
        id: usize,
        label: &str,
        chosen: Option<&String>,
        focused: bool,
        (x, width): (u16, u16),
        y: usize,
    ) {
        let back = ViewHit::Node {
            node: id,
            act: NodeAct::Step { forward: false },
        };
        let on = ViewHit::Node {
            node: id,
            act: NodeAct::Step { forward: true },
        };
        let arrow = |hovered: bool| match hovered {
            true => Token::Accent,
            false => Token::TextFaint,
        };
        let chosen = chosen.map_or("", String::as_str);
        let ink = match focused || self.hovered(on) {
            true => Token::Accent,
            false => Token::TextPrimary,
        };
        let mut spans = Vec::new();
        if !label.is_empty() {
            spans.push(TextSpan::styled(
                format!("{label} "),
                theme::fg(Token::TextDim),
            ));
        }
        spans.push(TextSpan::styled(
            format!("{} ", theme::glyph(Symbol::StepPrevious)),
            theme::fg(arrow(self.hovered(back))),
        ));
        spans.push(TextSpan::styled(chosen.to_owned(), theme::fg_bold(ink)));
        spans.push(TextSpan::styled(
            format!(" {}", theme::glyph(Symbol::StepNext)),
            theme::fg(arrow(self.hovered(on))),
        ));
        self.line(y, x, width, Line::from(spans));
        let lead = lead_width(label);
        let back_width = step_width(Symbol::StepPrevious);
        let on_width = cells_of(text::columns(chosen)).saturating_add(step_width(Symbol::StepNext));
        let room = |from: u16| width.saturating_sub(from);
        self.hit(
            y,
            1,
            x.saturating_add(lead),
            back_width.min(room(lead)),
            back,
        );
        let on_from = lead.saturating_add(back_width);
        self.hit(
            y,
            1,
            x.saturating_add(on_from),
            on_width.min(room(on_from)),
            on,
        );
    }

    /// A field: its label and what is typed (or its placeholder), with the
    /// caret while it is typed into, over the rule every input wears, which
    /// lights while it has focus. The whole of it takes typing on a press.
    fn field(
        &mut self,
        id: usize,
        (label, value, placeholder): (&str, &str, &str),
        (focused, editing): (bool, bool),
        (x, width): (u16, u16),
        y: usize,
    ) {
        let hit = ViewHit::Node {
            node: id,
            act: NodeAct::Edit,
        };
        let mut spans = Vec::new();
        if !label.is_empty() {
            let ink = match self.hovered(hit) {
                true => Token::TextBright,
                false => Token::TextDim,
            };
            spans.push(TextSpan::styled(format!("{label} "), theme::fg(ink)));
        }
        spans.extend(Input::new(value, placeholder).focused(editing).spans());
        self.line(y, x, width, Line::from(spans));
        let tone = match (focused || editing, self.hovered(hit)) {
            (true, _) => Token::Accent,
            (false, true) => Token::TextSecondary,
            (false, false) => Token::BorderDefault,
        };
        if let (Some(row), Some((x, width))) = (self.screen_row(y + 1), self.columns(x, width)) {
            Rule::new(Edge::Bottom)
                .tone(tone)
                .render(self.frame, Rect::new(x, row, width, 1));
        }
        self.hit(y, 2, x, width, hit);
    }

    /// A form's fields one above another, then its button, accent while it
    /// has focus, sending the form when pressed as Enter does.
    fn form(
        &mut self,
        node: &Node,
        submit: &str,
        focused: bool,
        (x, width): (u16, u16),
        y: usize,
    ) -> bool {
        if !self.stacked(&node.children, 0, x, width, y) {
            return false;
        }
        let at = y + node
            .children
            .iter()
            .map(|child| height(child, width))
            .sum::<usize>();
        if self.held.fold && !self.fits(at, 1) {
            return self.fold_at(at, 1);
        }
        let hit = ViewHit::Node {
            node: node.id,
            act: NodeAct::Press,
        };
        let hue = match focused {
            true => Token::Accent,
            false => Token::TextSecondary,
        };
        let drawn = Button::new(submit, hue).strong(self.hovered(hit));
        if let Some(rect) = self.visible(at, 1, x, drawn.width().min(width)) {
            drawn.render(self.frame, rect);
            self.hits.push((rect, hit));
        }
        true
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

#[cfg(test)]
mod media_tests;
