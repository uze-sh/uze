//! Drawing an extension's [`View`].
//!
//! Everything geometric about an extension overlay lives here: the split
//! between navigator and content, how a row scrolls into sight, how wide a
//! wrapped diff line ends up, and which rectangle a click belongs to. An
//! extension answers with content and never sees a coordinate, so this is
//! the only side that can be wrong about layout — which is the point, since
//! it used to be two sides deriving the same rectangles independently.
//!
//! Colour resolution lives here too: [`Role`] is the extension's
//! vocabulary, and mapping it onto the palette below is what keeps an
//! overlay looking like the rest of the TUI without an extension holding a
//! copy of the colour table.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span as TextSpan},
    widgets::{Clear, Padding, Paragraph, Wrap},
};
use uze_extensions::view::{
    Caret, Choosing, Command, Content, ContentLine, Layout as ViewLayout, LineTone, MarkerSide,
    Mode, Navigator, NavigatorRow, PanDirection, Role, RowIcon, RowMark, ScrollTarget, Section,
    Size, Span, TAB_WIDTH, TrailStep, View, ViewHit,
};

use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{
    self, Edge, Rule, Scrollbar, Surface, TRAILING_PAD, hint, mark, row, text,
};

/// Narrowest/widest the navigator can be dragged, and the floor left for
/// the content column — the same shape as the host TUI's own
/// `clamp_sidebar_width`, scoped to an extension overlay.
const MIN_NAVIGATOR_WIDTH: u16 = 20;
const MAX_NAVIGATOR_WIDTH: u16 = 50;
const MIN_EXTENSION_CONTENT_WIDTH: u16 = 40;

const GUTTER_WIDTH: u16 = 7;

/// Margin on each side of unnumbered content.
///
/// A numbered line already starts a gutter's width in, and ends well
/// short of the edge because code is short; that is where every other
/// mode's breathing room comes from. A rendered document has neither — it
/// has no gutter, and its paragraphs wrap to the full width — so without
/// this it runs into both borders. Two columns, the same as the
/// management screens' own content inset, so the two surfaces indent
/// their text by the same amount.
const PROSE_INSET: u16 = 2;

/// Columns of padding on each side of a mode segment's label. The padding
/// is part of the button — it is filled, and clicked, like the label is.
const MODE_PAD: u16 = 1;
/// The gap between the hints on a footer's left and the caption on its
/// right, so the two never meet at the width where both are widest.
const FOOTER_GAP: u16 = 2;

/// The extension's palette, resolved. An extension names meaning; the host
/// names colour, exactly once, here.
fn color(role: Role) -> Color {
    theme::color(token(role))
}

/// The token a role means. Kept apart from [`color`] because a role is
/// asked for two different things — the ink it draws in, and the hue it
/// lends a ground — and both have to answer from one mapping or a
/// selection stops matching what it is marking.
fn token(role: Role) -> Token {
    match role {
        Role::Default => Token::TextBright,
        Role::Muted => Token::TextMuted,
        Role::Secondary => Token::TextSecondary,
        Role::Bright => Token::TextBright,
        Role::Inactive => Token::TextInactive,
        Role::Accent => Token::Accent,
        Role::Dim => Token::TextDim,
        Role::Faint => Token::TextFaint,
        Role::Info => Token::StateInfo,
        Role::Success => Token::StateSuccess,
        Role::Warning => Token::StateWarning,
        Role::Danger => Token::StateDanger,
    }
}

/// Rendered Markdown as lines a wrapped `Paragraph` can draw, styled the
/// way the code surface styles its preview.
pub(crate) fn prose(lines: &[ContentLine]) -> Vec<Line<'static>> {
    lines
        .iter()
        .map(|line| Line::from(line.spans.iter().map(styled).collect::<Vec<_>>()))
        .collect()
}

fn styled(span: &Span) -> TextSpan<'static> {
    let mut style = Style::default().fg(span
        .color
        .map(|rgb| theme::content(rgb.0, rgb.1, rgb.2))
        .unwrap_or_else(|| color(span.role)));
    // A ground the span named: its own hue, let into the surface far
    // enough to mark the area and not far enough to compete with what is
    // written on it — the same strength a selected row wears.
    if let Some(ground) = span.ground {
        style = style.bg(theme::tinted(token(ground), Token::SurfaceBackground));
    }
    // Emphasis is stated both ways round, never left to whatever the
    // span is drawn inside. A ratatui title carries a style its spans
    // are patched over, so a span that only *omits* bold comes out bold
    // on a frame's edge and plain everywhere else — the same span
    // meaning two things depending on where it landed.
    style = match span.bold {
        true => style.add_modifier(Modifier::BOLD),
        false => style.remove_modifier(Modifier::BOLD),
    };
    style = match span.italic {
        true => style.add_modifier(Modifier::ITALIC),
        false => style.remove_modifier(Modifier::ITALIC),
    };
    TextSpan::styled(without_tabs(&span.text), style)
}

/// `text` with each tab spelled as the [`TAB_WIDTH`] spaces it occupies.
///
/// ratatui drops control characters as it draws, so a tab left in is an
/// indentation that silently is not there — and a caret counted past it
/// sits a column short of the character it names.
fn without_tabs(text: &str) -> String {
    match text.contains('\t') {
        true => text.replace('\t', &" ".repeat(TAB_WIDTH)),
        false => text.to_owned(),
    }
}

/// How many cells `character` takes: a tab its [`TAB_WIDTH`], anything
/// else what Unicode says, and never nothing — a zero-width character the
/// caret can stand on still needs a cell to be seen standing there.
fn cell_width(character: char) -> usize {
    match character {
        '\t' => TAB_WIDTH,
        _ => unicode_width::UnicodeWidthChar::width(character)
            .unwrap_or(0)
            .max(1),
    }
}

/// Folds `line` at `width` cells, calling `visit` with the row and cell
/// each character lands on, and answers where the next one would.
///
/// One walk for the three things that must agree about it — which row a
/// character is drawn on, how many rows the line takes, and where the
/// caret stands — because the paragraph wrap this replaced broke at
/// words while the other two counted cells, and a caret on a long line
/// drifted a word further off with every row.
fn fold(
    line: &ContentLine,
    width: usize,
    mut visit: impl FnMut(usize, usize, &Span, char),
) -> (usize, usize) {
    let width = width.max(1);
    let (mut row, mut cell) = (0usize, 0usize);
    for span in &line.spans {
        for character in span.text.chars() {
            let taken = cell_width(character);
            if cell > 0 && cell + taken > width {
                row += 1;
                cell = 0;
            }
            visit(row, cell, span, character);
            cell += taken;
        }
    }
    (row, cell)
}

/// `line`'s spans, styled and folded into the rows [`fold`] puts them on.
fn folded_rows(line: &ContentLine, width: usize) -> Vec<Vec<TextSpan<'static>>> {
    let mut rows: Vec<Vec<TextSpan<'static>>> = vec![Vec::new()];
    let mut last: Option<*const Span> = None;
    fold(line, width, |row, _, span, character| {
        if rows.len() <= row {
            rows.push(Vec::new());
            last = None;
        }
        let spans = rows.last_mut().expect("a row was pushed above");
        let text = match character {
            '\t' => " ".repeat(TAB_WIDTH),
            _ => character.to_string(),
        };
        match spans.last_mut() {
            Some(piece) if last == Some(std::ptr::from_ref(span)) => {
                piece.content.to_mut().push_str(&text);
            }
            _ => {
                spans.push(TextSpan::styled(text, styled(span).style));
                last = Some(std::ptr::from_ref(span));
            }
        }
    });
    rows
}

pub(crate) fn clamp_navigator_width(width: u16, total_width: u16) -> u16 {
    let max = total_width
        .saturating_sub(MIN_EXTENSION_CONTENT_WIDTH)
        .clamp(MIN_NAVIGATOR_WIDTH, MAX_NAVIGATOR_WIDTH);
    width.clamp(MIN_NAVIGATOR_WIDTH, max)
}

/// The navigator/content split, derived from the outer overlay area so
/// drawing and hit-testing always share the exact same geometry. This
/// splits horizontally first, so the navigator column spans the entire
/// inner height and its right-hand divider reaches edge to edge; only the
/// content side is split again to carve out a footer that belongs to that
/// column alone rather than reading as a global app bar.
/// The surface's own nav row: where the control that says which half you
/// are in is drawn, in every layout.
///
/// A fixed column of the frame rather than the head of whichever column
/// happens to be beside it — the halves are not the list's, they are the
/// surface's, and a control that moved a cell when it was used was one
/// the eye had to find again after every press. It is also why the row
/// is the list's and the content's alike: neither owns it.
pub(crate) fn nav_row(frame_area: Rect) -> Rect {
    Rect::new(frame_area.x, frame_area.y, frame_area.width, 1)
}

/// Everything below the nav row — or from the frame's edge, for a surface
/// that has no halves to offer.
fn below_nav(rect: Rect, nav_rows: u16) -> Rect {
    Rect::new(
        rect.x,
        rect.y.saturating_add(nav_rows),
        rect.width,
        rect.height.saturating_sub(nav_rows),
    )
}

pub(crate) fn content_columns(
    frame_area: Rect,
    navigator_width_override: Option<u16>,
) -> (Rect, Rect, Rect) {
    // No frame and no margin: the surface stands where the pane is, and
    // the pane's own edges — the sidebar's divider, the strip's rule —
    // already say where it begins, the way they do for a shell.
    let inner = frame_area;
    let navigator_width = navigator_width_override
        .map(|width| clamp_navigator_width(width, inner.width))
        .unwrap_or_else(|| (inner.width / 4).clamp(MIN_NAVIGATOR_WIDTH, MAX_NAVIGATOR_WIDTH));
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(navigator_width), Constraint::Min(10)])
        .split(inner);
    let content_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(FOOTER_ROWS)])
        .split(columns[1]);
    (columns[0], content_rows[0], content_rows[1])
}

/// [`content_columns`] for a view with no navigator: no column for it,
/// and the content and the keys across the whole width.
fn whole_width(frame_area: Rect) -> (Rect, Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(FOOTER_ROWS)])
        .split(frame_area);
    (
        Rect {
            width: 0,
            ..frame_area
        },
        rows[0],
        rows[1],
    )
}

/// A board's rows, top to bottom: its menu, the board itself and the
/// footer. The board gets everything the other two do not need — no
/// navigator column, no blank row under the title, no reading margin, and
/// one row of menu rather than one per level of it.
///
/// The menu sits straight under the title because there is nothing up
/// there to crowd: the frame's top edge carries the surface's name and
/// nothing else, and where it is open is written along the foot. A row
/// of chips under one word is a row of chips, not a continuation.
pub(crate) fn board_rows(frame_area: Rect) -> (Rect, Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(FOOTER_ROWS),
        ])
        .split(frame_area);
    (rows[0], rows[1], rows[2])
}

/// How many cells a board has to show its drawing in — exact, unlike
/// [`content_space`]: the extension cuts the screen it hands over to this.
pub(crate) fn board_space(frame_area: Rect) -> Size {
    let (_, board, _) = board_rows(frame_area);
    Size {
        width: board.width,
        height: board.height,
    }
}

/// How much room the content column has, for an extension deciding how
/// much to produce.
/// The room the code surface has. Which of the two it is depends on what
/// the surface is showing, and only the surface knows: the map is a
/// picture of the whole checkout and takes the frame, while everything
/// else is read in the column beside the tree.
pub(crate) fn code_space(
    frame_area: Rect,
    navigator_width_override: Option<u16>,
    code: Option<&uze_extensions::code::CodeView>,
) -> Size {
    match code.map(uze_extensions::code::CodeView::showing) {
        // The board's menu row *is* the nav row — the same cells, which
        // is what makes the control stay put across the switch — so the
        // board loses nothing to it.
        Some(uze_extensions::code::ContentMode::Map) => board_space(frame_area),
        // A row less: this surface always has halves to offer, so it
        // always has the row that offers them.
        _ => {
            let space = content_space(frame_area, navigator_width_override);
            Size {
                height: space.height.saturating_sub(NAV_ROWS),
                ..space
            }
        }
    }
}

/// How many rows the nav takes from a surface that has one. One, and it
/// is a constant so the room an extension is told it has and the room it
/// is drawn in cannot disagree.
const NAV_ROWS: u16 = 1;

/// The keys that act here, on one row: with no frame beneath it there is
/// no edge the row needs keeping off.
const FOOTER_ROWS: u16 = 1;

pub(crate) fn content_space(frame_area: Rect, navigator_width_override: Option<u16>) -> Size {
    let (_, content, _) = content_columns(frame_area, navigator_width_override);
    Size {
        width: content.width,
        height: content.height,
    }
}

/// Which half of the overlay the pointer is over — the host's answer,
/// because the host owns the layout.
pub(crate) fn scroll_target(
    frame_area: Rect,
    navigator_width_override: Option<u16>,
    column: u16,
    row: u16,
) -> Option<ScrollTarget> {
    let (navigator, content, _) = content_columns(frame_area, navigator_width_override);
    let pointer = ratatui::layout::Position::new(column, row);
    if navigator.contains(pointer) {
        Some(ScrollTarget::Navigator)
    } else if content.contains(pointer) {
        Some(ScrollTarget::Content)
    } else {
        None
    }
}

/// Draws the overlay across the entire frame — every other row this frame
/// would otherwise have drawn is skipped by the caller rather than drawn
/// and covered.
/// Where the navigator's list is scrolled to, which the host keeps
/// because only the host knows how many rows fit (see
/// [`Navigator::anchor`]). Handed into a frame and handed back settled:
/// clamped to the rows that exist, and moved just far enough to show an
/// anchor the extension has changed since the last frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NavigatorScroll {
    /// The first row shown.
    pub(crate) first: usize,
    /// The anchor the list was last scrolled to reveal, so the same
    /// anchor asked for again does not pull the list back to it.
    pub(crate) revealed: Option<usize>,
}

impl NavigatorScroll {
    pub(crate) fn scrolled(self, direction: uze_extensions::view::ScrollDirection) -> Self {
        use uze_extensions::view::ScrollDirection;
        Self {
            first: match direction {
                ScrollDirection::Up => self.first.saturating_sub(1),
                ScrollDirection::Down => self.first.saturating_add(1),
            },
            ..self
        }
    }

    /// The scroll a list of `rows` rows in `visible` lines settles at.
    fn settled(self, anchor: Option<usize>, rows: usize, visible: usize) -> Self {
        let mut first = self.first.min(rows.saturating_sub(visible));
        if anchor != self.revealed
            && let Some(anchor) = anchor
        {
            if anchor < first {
                first = anchor;
            } else if anchor >= first + visible {
                first = (anchor + 1).saturating_sub(visible);
            }
        }
        Self {
            first,
            revealed: anchor,
        }
    }
}

/// What one frame of an extension surface left behind for the next event
/// to read: where its list settled, and the two scrollbars it drew.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rendered {
    pub(crate) navigator_scroll: NavigatorScroll,
    pub(crate) navigator_bar: Option<Scrollbar>,
    pub(crate) content_bar: Option<Scrollbar>,
    /// How many columns of the content row the gutter took, so a click
    /// on it can be turned into a position in the text.
    ///
    /// Reported rather than assumed: it is nothing at all for content
    /// that is not numbered, and a click resolved as though it were
    /// seven landed seven cells to the left of the pointer — which on
    /// the map is a tile or two over, and near an edge is nothing at
    /// all.
    pub(crate) content_gutter: u16,
    /// The room the extension was given to lay this frame out in, so a
    /// click resolves against the geometry that produced what is on
    /// screen.
    ///
    /// Recorded for the same reason as `content_gutter`, and against a
    /// worse failure. The click path used to recompute it from the
    /// *pane's* size — the rect a tab's PTY is sized by, which is the
    /// frame less the sidebar and the tab strip — while this drew in the
    /// whole frame. A surface that lays itself out in the room it is
    /// given, as the map and the architect's diagrams do, was then laid
    /// out twice in two different spaces: everything to the right of the
    /// pane's width and below its height belonged to no tile at all, and
    /// everything else belonged to the wrong one.
    pub(crate) content_space: uze_extensions::view::Size,
}

/// Where the host holds the navigator this frame: the width it was
/// dragged to, where its list is scrolled, and whether its edge is being
/// dragged right now.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NavigatorFrame {
    pub(crate) width: Option<u16>,
    pub(crate) scroll: NavigatorScroll,
    pub(crate) resizing: bool,
}

pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    area: Rect,
    held: NavigatorFrame,
    scope: uze_keys::Scope,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rendered {
    // The pane's own ground, and nothing drawn around it: the surface
    // stands where a shell would, and is put away the way one is left —
    // its button in the strip, `Esc`, or another tab — so it carries no
    // frame, no name and no close mark of its own.
    frame.render_widget(Clear, area);
    widget::fill(frame, area, Token::SurfaceBackground);
    if view.layout == ViewLayout::Board {
        let mut rendered = render_board(frame, view, area, scope, hits);
        rendered.content_space = board_space(area);
        return rendered;
    }
    // A view with no list — nothing found, or nothing to find — is one
    // column: a navigator's column left empty reads as a list that failed
    // to draw, and pushes the message and the keys off to one side.
    let (navigator_area, content_area, footer) = match view.navigator {
        Some(_) => content_columns(area, held.width),
        None => whole_width(area),
    };
    // The nav is the surface's, not the list's: it spans the frame, and
    // both columns start below it.
    let nav_rows = match view.subjects.is_empty() {
        true => 0,
        false => NAV_ROWS,
    };
    let nav = nav_row(area);
    render_subjects(frame, nav, &view.subjects, hits);
    // Where there is a nav row, the ways of drawing what it selected ride
    // it: they are what this half can be *asked*, and a control of its
    // own a row below reads as a second header rather than as the other
    // end of the first.
    let (nav_modes, column_modes) = match nav_rows {
        0 => (&[][..], &view.modes[..]),
        _ => (&view.modes[..], &[][..]),
    };
    render_modes(frame, nav, nav_modes, hits);
    let navigator_area = below_nav(navigator_area, nav_rows);
    let content_area = below_nav(content_area, nav_rows);

    let mut rendered = Rendered {
        navigator_scroll: held.scroll,
        ..Rendered::default()
    };
    if let Some(navigator) = view.navigator.as_ref() {
        let (settled, bar) = render_navigator(
            frame,
            navigator_area,
            navigator,
            &view.subjects,
            held.scroll,
            held.resizing,
            hits,
        );
        rendered.navigator_scroll = settled;
        rendered.navigator_bar = bar;
    }
    // One target for the whole edge, because the edge is one line doing
    // two jobs: the split moves sideways, the list scrolls down. Which a
    // press meant is the first movement's to say — see
    // [`ViewHit::GrabNavigatorEdge`].
    if view.navigator.is_some() {
        hits.push((
            Rect::new(
                navigator_area.right().saturating_sub(1),
                navigator_area.y,
                1,
                navigator_area.height,
            ),
            ViewHit::GrabNavigatorEdge,
        ));
    }
    match &view.content {
        Content::Message { text, hint, role } => {
            render_message(frame, content_area, text, hint.as_deref(), color(*role))
        }
        Content::Lines {
            heading,
            scroll,
            first,
            lines,
            total,
            caret,
        } => {
            rendered.content_gutter = gutter_width(lines);
            rendered.content_bar = render_lines(
                frame,
                content_area,
                Lines {
                    heading,
                    scroll: *scroll,
                    first: *first,
                    lines,
                    total: *total,
                    caret: *caret,
                    modes: column_modes,
                },
                hits,
            );
        }
    }
    render_footer_row(
        frame,
        footer,
        &view.footer,
        scope,
        view.notice.as_ref().map(styled),
    );
    rendered.content_space = uze_extensions::view::Size {
        width: content_area.width,
        height: content_area.height,
    };
    rendered
}

/// A [`uze_extensions::view::RowMenu`], drawn after the view so it lies over whatever it
/// overlaps: where the pointer asked for it (`at`), or under the row it
/// was opened on when the keyboard did — and not at all while that row is
/// scrolled out of sight, since a menu pointing at nothing is a menu
/// about nothing.
///
/// Its entries go to the front of `hits`, because the first rect holding
/// a point is the one a click lands on, and under the menu there are rows
/// the click was not meant for.
pub(crate) fn render_row_menu(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    area: Rect,
    at: Option<Rect>,
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let Some(menu) = view
        .navigator
        .as_ref()
        .and_then(|navigator| navigator.menu.as_ref())
    else {
        return;
    };
    let Some(row) = hits
        .iter()
        .find(|(_, hit)| *hit == ViewHit::SelectItem(menu.row))
        .map(|(rect, _)| *rect)
    else {
        return;
    };
    // Under the name rather than at the row's edge, where the accent bar
    // and the indent are: the menu belongs to the file, not to the column.
    let anchor = at.unwrap_or(Rect::new(row.x.saturating_add(2), row.y, 1, 1));
    let entries: Vec<&str> = menu.entries.iter().map(String::as_str).collect();
    let rows = widget::menu::render(frame, area, anchor, &entries, menu.highlighted);
    for (index, rect) in rows.into_iter().enumerate().rev() {
        hits.insert(0, (rect, ViewHit::MenuEntry(index)));
    }
}

/// A [`uze_extensions::view::Confirm`], as the same dialog every other
/// question in the product is asked in, centred in `area`, with the keys
/// that answer it where the surface is open: `scope`. The caller puts the
/// scrim behind it and its answers ahead of everything it covers.
pub(crate) fn render_confirm(
    frame: &mut ratatui::Frame<'_>,
    confirm: &uze_extensions::view::Confirm,
    area: Rect,
    scope: uze_keys::Scope,
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let dialog = widget::dialog::Dialog {
        // Asked only before what cannot be undone.
        tone: widget::dialog::Tone::Danger,
        title: &confirm.title,
        subject: Some(Line::from(confirm.subject.clone())),
        body: vec![confirm.body.clone()],
        confirm: Some(&confirm.confirm),
        focus: Some(match confirm.on_confirm {
            true => 1,
            false => widget::dialog::CANCEL,
        }),
    };
    let keys = widget::dialog::Keys {
        scopes: &[uze_keys::Scope::Global, uze_keys::Scope::Workspace, scope],
        yes: uze_keys::Action::ConfirmDelete,
        no: uze_keys::Action::Dismiss,
    };
    let answers = widget::dialog::render(
        frame,
        area,
        &dialog,
        &keys,
        ViewHit::Answer(false),
        ViewHit::Answer(true),
    );
    hits.splice(0..0, answers);
}

/// A [`ViewLayout::Board`]: the list as a row of tabs, and under it the
/// drawing, given every cell that is left and cut at the edge.
fn render_board(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    area: Rect,
    scope: uze_keys::Scope,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rendered {
    let (menu, board, footer) = board_rows(area);
    let mut rendered = Rendered::default();
    // The board's menu row and the column layout's nav row are the same
    // cells, which is the whole of why the control does not move when
    // the map takes the frame. A board draws one or the other on it: the
    // halves where it has them, the descent where it does not — nothing
    // has both, and a board that did would have to say which side each
    // belongs on.
    render_subjects(frame, menu, &view.subjects, hits);
    render_modes(frame, menu, &view.modes, hits);
    match &view.content {
        Content::Message { text, hint, role } => {
            render_message(frame, board, text, hint.as_deref(), color(*role));
        }
        Content::Lines {
            scroll,
            first,
            lines,
            total,
            ..
        } => {
            let gutter = gutter_width(lines);
            rendered.content_gutter = gutter;
            for (row, (offset, line)) in lines
                .iter()
                .enumerate()
                .map(|(offset, line)| (offset + first, line))
                .skip(usize::from(*scroll).saturating_sub(*first))
                .take(usize::from(board.height))
                .enumerate()
            {
                let rect = Rect::new(board.x, board.y + row as u16, board.width, 1);
                render_line(frame, rect, line, gutter, false);
                // The whole row, gutter included: where in it the pointer
                // landed is settled once, by `caret_cell_at`, against the
                // gutter this frame actually drew.
                hits.push((
                    rect,
                    ViewHit::PlaceCaret {
                        line: offset,
                        cell: 0,
                    },
                ));
            }
            let bar = Scrollbar::measure(
                Rect::new(board.right(), board.y, Scrollbar::width(), board.height),
                usize::from(board.height),
                *total,
            );
            rendered.content_bar = render_scrollbar(
                frame,
                bar,
                usize::from(*scroll),
                hits,
                ViewHit::DragContentScrollbar,
            );
        }
    }
    // A notice is about what was just done and the caption is there all
    // along, so while there is one it has the caption's place.
    let trailing = match (&view.notice, &view.content) {
        (Some(notice), _) => Some(styled(notice)),
        (None, Content::Lines { heading, .. }) if !heading.is_empty() => Some(TextSpan::styled(
            heading.clone(),
            theme::fg(Token::TextMuted),
        )),
        _ => None,
    };
    render_footer_row(frame, footer, &view.footer, scope, trailing);
    // Last, because its list of groups opens over the board — and its
    // hits first, because a click on that list must not reach the row of
    // board lying under it.
    if let Some(navigator) = view.navigator.as_ref() {
        let room = menu.width.saturating_sub(modes_width(&view.modes) + 2);
        let mut menu_hits = Vec::new();
        render_menu(
            frame,
            Rect::new(menu.x, menu.y, room, 1),
            board,
            navigator,
            &view.trail,
            &mut menu_hits,
        );
        hits.splice(0..0, menu_hits);
    }
    rendered
}

/// A board's menu, on one row: two selectors, the group on show and the
/// item on show in it, each opening a list over the board.
///
/// One level is visible at a time and one word of each, which is what
/// keeps the row readable however many items a group holds — a row of
/// every item is a row that has to be cut, and the cut lands on whatever
/// the viewer was looking for. Once something has been entered the way
/// in takes the place of the second selector, whose job its last step
/// then does.
fn render_menu(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    board: Rect,
    navigator: &Navigator,
    trail: &[TrailStep],
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let groups = groups_of(navigator);
    let Some(active) = active_group(navigator).and_then(|id| groups.iter().find(|g| g.id == id))
    else {
        return;
    };
    let items = items_of(navigator, active.id);
    let on_show = items
        .iter()
        .find(|item| item.selected)
        .or(items.first())
        .map_or("", |item| item.name);

    // A selector opens only where there is something to choose. With one
    // group, or one item in it, the list would offer what is already on
    // show — so the mark that says it opens is left off and the press
    // that would open it is never offered.
    let opens_groups = groups.len() > 1;
    let group_selector = render_selector(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        active.name,
        // How many the area holds, unless the trail beside it is already
        // showing them one by one.
        Some(active.items).filter(|&items| items > 1 && trail.is_empty()),
        opens_groups,
    );
    if opens_groups {
        hits.push((group_selector, ViewHit::ChooseGroup));
    }

    let divider = "  /  ";
    let mut x = group_selector.right();
    if x.saturating_add(divider.len() as u16) < area.right() {
        frame.render_widget(
            Paragraph::new(TextSpan::styled(divider, theme::fg(Token::TextDim))),
            Rect::new(x, area.y, divider.len() as u16, 1),
        );
        x += divider.len() as u16;
    }
    let rest = Rect::new(x, area.y, area.right().saturating_sub(x), 1);
    // Which of the two the second half is, is the view's answer: a
    // descent is walked, and where there is none there is a list to pick
    // from. One control, never both — an area is one thing or the other.
    let opens_items = trail.is_empty() && items.len() > 1;
    let item_selector = match trail.is_empty() {
        true => render_selector(frame, rest, on_show, None, opens_items),
        false => render_trail(frame, rest, trail, hits),
    };
    if opens_items {
        hits.push((item_selector, ViewHit::ChooseItem));
    }

    match navigator.choosing {
        Some(Choosing::Group(highlighted)) => {
            let rows: Vec<ChoiceRow<'_>> = groups
                .iter()
                .map(|group| ChoiceRow {
                    hit: ViewHit::ToggleGroup(group.id),
                    name: group.name,
                    trailing: group.items.to_string(),
                    highlighted: group.id == highlighted,
                })
                .collect();
            render_choice_list(
                frame,
                group_selector,
                board,
                &rows,
                ViewHit::ChooseGroup,
                hits,
            );
        }
        Some(Choosing::Item(highlighted)) => {
            let rows: Vec<ChoiceRow<'_>> = items
                .iter()
                .map(|item| ChoiceRow {
                    hit: ViewHit::SelectItem(item.id),
                    name: item.name,
                    trailing: String::new(),
                    highlighted: item.id == highlighted,
                })
                .collect();
            render_choice_list(
                frame,
                item_selector,
                board,
                &rows,
                ViewHit::ChooseItem,
                hits,
            );
        }
        None => {}
    }
}

/// One selector: what is chosen, how many there are to choose from when
/// that is worth a number, and — when it opens onto anything other than
/// itself — the mark that says so. Without that mark it is not a control
/// but a label: where the viewer is.
fn render_selector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    name: &str,
    count: Option<usize>,
    opens: bool,
) -> Rect {
    let raised = theme::color(Token::SurfaceRaised);
    let mut spans = vec![TextSpan::styled(
        format!(" {name} "),
        theme::fg_bold(Token::TextBright).bg(raised),
    )];
    if let Some(count) = count {
        spans.push(TextSpan::styled(
            format!("{count} "),
            theme::fg(Token::TextDim).bg(raised),
        ));
    }
    if opens {
        spans.push(TextSpan::styled(
            format!("{} ", theme::glyph(Symbol::ChevronExpanded)),
            theme::fg(Token::TextMuted).bg(raised),
        ));
    }
    let rect = Rect::new(area.x, area.y, spans_width(&spans).min(area.width), 1);
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
    rect
}

struct MenuGroup<'a> {
    id: usize,
    name: &'a str,
    items: usize,
}

struct MenuItem<'a> {
    id: usize,
    name: &'a str,
    selected: bool,
}

fn groups_of(navigator: &Navigator) -> Vec<MenuGroup<'_>> {
    let mut groups: Vec<MenuGroup<'_>> = Vec::new();
    for row in &navigator.rows {
        match row {
            // A board's menu has one level of heading, so a band is one.
            NavigatorRow::Group { id, name, .. } | NavigatorRow::Band { id, name, .. } => {
                groups.push(MenuGroup {
                    id: *id,
                    name,
                    items: 0,
                });
            }
            NavigatorRow::Item { .. } => {
                if let Some(group) = groups.last_mut() {
                    group.items += 1;
                }
            }
            NavigatorRow::Gap => {}
        }
    }
    groups
}

fn items_of(navigator: &Navigator, group: usize) -> Vec<MenuItem<'_>> {
    let mut items = Vec::new();
    let mut current = None;
    for row in &navigator.rows {
        match row {
            NavigatorRow::Group { id, .. } | NavigatorRow::Band { id, .. } => current = Some(*id),
            NavigatorRow::Item {
                id, name, selected, ..
            } if current == Some(group) => items.push(MenuItem {
                id: *id,
                name,
                selected: *selected,
            }),
            NavigatorRow::Item { .. } | NavigatorRow::Gap => {}
        }
    }
    items
}

fn spans_width(spans: &[TextSpan<'_>]) -> u16 {
    spans.iter().map(TextSpan::width).sum::<usize>() as u16
}

/// The group the selection is in — the one whose items are on show.
fn active_group(navigator: &Navigator) -> Option<usize> {
    let mut group = None;
    for row in &navigator.rows {
        match row {
            NavigatorRow::Group { id, .. } | NavigatorRow::Band { id, .. } => group = Some(*id),
            NavigatorRow::Item { selected: true, .. } => return group,
            NavigatorRow::Item { .. } | NavigatorRow::Gap => {}
        }
    }
    // Nothing selected is still somewhere: the first group.
    navigator.rows.iter().find_map(|row| match row {
        NavigatorRow::Group { id, .. } | NavigatorRow::Band { id, .. } => Some(*id),
        NavigatorRow::Item { .. } | NavigatorRow::Gap => None,
    })
}

/// A descent, drawn as the row it is walked on: a step per level, the
/// one being looked at drawn as the chip the item selector would be, and
/// every other step a place to go — back, for the ones already walked,
/// and on, for the ones this descent reaches but nobody has entered.
///
/// Cut around the current step rather than from one end, because that is
/// the one step that must never be the one cut off, and it need not be
/// the last: a model's levels are all there from the start, and the
/// viewer may be standing in the middle of them. Answers where the
/// current step was drawn.
fn render_trail(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    trail: &[TrailStep],
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rect {
    let chevron = format!(" {} ", theme::glyph(Symbol::ChevronRight));
    let chevron_width = TextSpan::raw(chevron.as_str()).width() as u16;
    let more = theme::glyph(Symbol::Ellipsis);
    let more_width = TextSpan::raw(more.as_str()).width() as u16 + chevron_width;
    let width_of = |step: &TrailStep| TextSpan::raw(step.name.as_str()).width() as u16 + 2;
    let Some(current) = trail.iter().position(|step| step.current) else {
        return Rect::new(area.x, area.y, 0, 1);
    };

    // Outward from the current step, taking what fits — the way back
    // first, because a step already walked is the one more likely wanted.
    let (mut first, mut last) = (current, current);
    let mut used = width_of(&trail[current]) + 2;
    loop {
        let room = |used: u16, edge: bool| {
            area.width
                .saturating_sub(used + if edge { more_width } else { 0 })
        };
        let back = first
            .checked_sub(1)
            .filter(|&step| width_of(&trail[step]) + chevron_width <= room(used, step > 0));
        if let Some(step) = back {
            used += width_of(&trail[step]) + chevron_width;
            first = step;
        }
        let on = Some(last + 1)
            .filter(|&step| step < trail.len())
            .filter(|&step| {
                width_of(&trail[step]) + chevron_width <= room(used, step + 1 < trail.len())
            });
        if let Some(step) = on {
            used += width_of(&trail[step]) + chevron_width;
            last = step;
        }
        if back.is_none() && on.is_none() {
            break;
        }
    }

    let quiet = theme::fg(Token::TextDim);
    let mut x = area.x;
    let draw = |frame: &mut ratatui::Frame<'_>, text: String, style: Style, x: &mut u16| {
        let width =
            (TextSpan::raw(text.as_str()).width() as u16).min(area.right().saturating_sub(*x));
        let rect = Rect::new(*x, area.y, width, 1);
        frame.render_widget(Paragraph::new(TextSpan::styled(text, style)), rect);
        *x += width;
        rect
    };
    if first > 0 {
        draw(frame, more.clone(), quiet, &mut x);
        draw(frame, chevron.clone(), quiet, &mut x);
    }
    let mut here = Rect::new(x, area.y, 0, 1);
    for (step, name) in trail.iter().enumerate().take(last + 1).skip(first) {
        if step == current {
            here = render_selector(
                frame,
                Rect::new(x, area.y, area.right().saturating_sub(x), 1),
                &name.name,
                None,
                false,
            );
            x = here.right();
        } else {
            // A step behind is the way back and a step ahead is a level
            // not yet reached: told apart by weight, since both are
            // pressed the same way.
            let ink = match step < current {
                true => theme::fg(Token::TextSecondary),
                false => quiet,
            };
            let rect = draw(frame, format!(" {} ", name.name), ink, &mut x);
            hits.push((rect, ViewHit::SelectTrail(step)));
        }
        if step < last {
            draw(frame, chevron.clone(), quiet, &mut x);
        }
    }
    if last + 1 < trail.len() {
        draw(frame, chevron, quiet, &mut x);
        draw(frame, more, quiet, &mut x);
    }
    here
}

struct ChoiceRow<'a> {
    hit: ViewHit,
    name: &'a str,
    trailing: String,
    highlighted: bool,
}

/// An open list, hung from the selector it belongs to and drawn over the
/// board: a row for each choice, the highlighted one filled. A list
/// longer than the board is tall shows the rows around the highlighted
/// one, which is the one row that must never be the one cut off.
///
/// Dressed as the workspace's other dropdowns are — the surface it sits
/// on rather than one raised above it, a fill on the highlighted row and
/// nothing on the rest. The highlight follows the pointer, so hovering a
/// row *is* highlighting it and there is no second, quieter state to
/// draw.
fn render_choice_list(
    frame: &mut ratatui::Frame<'_>,
    selector: Rect,
    board: Rect,
    rows: &[ChoiceRow<'_>],
    own_selector: ViewHit,
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let widest = rows
        .iter()
        .map(|row| TextSpan::raw(row.name).width() + row.trailing.len())
        .max()
        .unwrap_or(0) as u16;
    let width = widest
        .saturating_add(6)
        .max(selector.width)
        .min(board.width);
    let visible = (rows.len() as u16)
        .min(board.height.saturating_sub(2))
        .max(1);
    let area = Rect::new(
        selector.x.min(board.right().saturating_sub(width)),
        selector.bottom(),
        width,
        visible + 2,
    );
    let highlighted = rows.iter().position(|row| row.highlighted).unwrap_or(0);
    let first = highlighted
        .saturating_sub(usize::from(visible) / 2)
        .min(rows.len().saturating_sub(usize::from(visible)));

    frame.render_widget(Clear, area);
    let mut frame_surface = Surface::card();
    // A list that was cut says so, and where in it the highlight is —
    // otherwise its last row reads as the last there is.
    if rows.len() > usize::from(visible) {
        frame_surface = frame_surface.hint(Line::from(TextSpan::styled(
            format!(" {}/{} ", highlighted + 1, rows.len()),
            theme::fg(Token::TextDim),
        )));
    }
    frame_surface.render(frame, area);
    for (line, row) in rows.iter().skip(first).take(visible.into()).enumerate() {
        let rect = Rect::new(
            area.x + 1,
            area.y + 1 + line as u16,
            area.width.saturating_sub(2),
            1,
        );
        // A filled bar for the highlighted row, not just bold text: the
        // same narrowly-scoped exception the agent picker makes, for the
        // same reason — a menu the pointer and the arrows share needs the
        // affordance.
        let fill = match row.highlighted {
            true => theme::color(Token::Accent),
            false => theme::color(Token::SurfaceBackground),
        };
        let ink = match row.highlighted {
            true => theme::fg_bold(Token::SurfaceBackground),
            false => theme::fg(Token::TextInactive),
        };
        let gap = usize::from(rect.width)
            .saturating_sub(TextSpan::raw(row.name).width() + row.trailing.len() + 2);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                TextSpan::styled(format!(" {}", row.name), ink.bg(fill)),
                TextSpan::styled(" ".repeat(gap), Style::default().bg(fill)),
                TextSpan::styled(
                    format!("{} ", row.trailing),
                    theme::fg(Token::TextDim).bg(fill),
                ),
            ])),
            rect,
        );
        hits.push((rect, row.hit));
    }
    // The list's own frame is still the list: a click on it is not a
    // click on the board beneath.
    hits.push((area, own_selector));
}

fn render_navigator(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    navigator: &Navigator,
    subjects: &[Mode],
    scroll: NavigatorScroll,
    resizing: bool,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> (NavigatorScroll, Option<Scrollbar>) {
    // Padding on the divider's side only: a column indented from the
    // pane's own edge as well leaves its rows further in than the nav
    // above them, and there is nothing on that side for them to clear.
    let inner = Rule::draggable(Edge::Right, resizing)
        .ground(Token::SurfaceBackground)
        .padding(Padding::new(0, 1, 0, 0))
        .render(frame, area);

    // A heading only where nothing above it already names the list: the
    // nav row says which half you are in, and a word repeating it is a
    // word that has to be read to learn nothing.
    let mut heading = match subjects.is_empty() {
        true => vec![TextSpan::styled(
            navigator.heading.clone(),
            Style::default()
                .fg(theme::color(Token::TextSecondary))
                .add_modifier(Modifier::BOLD),
        )],
        false => Vec::new(),
    };
    // The panel's own right padding is the gap a trailing caption keeps
    // off the divider, so the row is measured as if it were the pad.
    row::push_trailing(
        &mut heading,
        inner.width + TRAILING_PAD,
        navigator.badge.clone(),
        theme::color(Token::TextMuted),
    );
    frame.render_widget(
        Paragraph::new(Line::from(heading)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let rows = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    );
    // The groove comes out of the list's own width, so a row is never
    // drawn under the handle that would sit on top of it.
    // On the divider itself, not beside it. Beside it was two lines, and
    // adjacent was still two lines; drawn *on* it, the divider is the
    // line and the handle is the stretch of it that says where you are —
    // which is the only thing a scrollbar was ever adding.
    //
    // Sharing the column is what makes the two gestures separable rather
    // than ambiguous: the handle is grabbed to scroll, and the rest of
    // the line is grabbed to move the divider. What it costs is clicking
    // the empty groove to jump, which is the lesser of the two.
    let bar = Scrollbar::measure(
        Rect::new(
            area.right().saturating_sub(Scrollbar::width()),
            rows.y,
            Scrollbar::width(),
            rows.height,
        ),
        rows.height as usize,
        navigator.rows.len(),
    );
    let list = rows;
    let visible = list.height as usize;
    let settled = scroll.settled(navigator.anchor, navigator.rows.len(), visible);
    for (offset, row) in navigator
        .rows
        .iter()
        .skip(settled.first)
        .take(visible)
        .enumerate()
    {
        let rect = Rect::new(list.x, list.y + offset as u16, list.width, 1);
        match row {
            NavigatorRow::Gap => {}
            // Where the eye lands to find its part of the list, so it is
            // told apart from the tree by form rather than by hue: capitals
            // in bold, with the count quieter beside them. Plain ASCII on
            // purpose — small capitals and subscript digits are glyphs a
            // terminal's font may not carry, and a heading drawn as boxes
            // divides nothing.
            NavigatorRow::Band {
                id,
                name,
                count,
                collapsed,
            } => {
                let fold = mark::disclosure(!*collapsed);
                let spans = vec![
                    TextSpan::raw(" "),
                    TextSpan::styled(format!("{fold} "), theme::fg(Token::TextMuted)),
                    TextSpan::styled(
                        name.to_uppercase(),
                        theme::fg(Token::TextSecondary).add_modifier(Modifier::BOLD),
                    ),
                    TextSpan::raw(" "),
                    TextSpan::styled(count.to_string(), theme::fg(Token::TextMuted)),
                ];
                frame.render_widget(Paragraph::new(Line::from(spans)), rect);
                hits.push((rect, ViewHit::ToggleGroup(*id)));
            }
            NavigatorRow::Group {
                id,
                name,
                depth,
                collapsed,
                icon,
            } => {
                let fold = mark::disclosure(!*collapsed);
                let mut spans = vec![
                    TextSpan::raw(" "),
                    TextSpan::raw(tree_indent(*depth, rect.width)),
                    TextSpan::styled(format!("{fold} "), theme::fg(Token::TextMuted)),
                ];
                spans.extend(row_icon(*icon));
                // A compact row's name is a path, whose end is where the
                // row leads; a plain directory's is a name, read from the
                // start.
                let room = label_room(&spans, rect.width);
                let name = match name.contains('/') {
                    true => text::elide_head(name, room),
                    false => text::elide(name, room),
                };
                spans.push(TextSpan::styled(name, theme::fg(Token::TextSecondary)));
                frame.render_widget(Paragraph::new(Line::from(spans)), rect);
                hits.push((rect, ViewHit::ToggleGroup(*id)));
            }
            NavigatorRow::Item {
                id,
                name,
                depth,
                marker,
                marker_side,
                detail,
                selected,
                icon,
            } => {
                let label_style = match (*selected, navigator.focused) {
                    (true, true) => Style::default()
                        .fg(theme::color(Token::TextBright))
                        .add_modifier(Modifier::BOLD),
                    (true, false) => theme::fg(Token::TextBright),
                    (false, _) => theme::fg(Token::TextInactive),
                };
                // The selected row is marked the way every other list in
                // the product marks its selection — the accent bar and the
                // selected surface the status listing uses — rather than a
                // neutral lift that the diff beside it easily outshone.
                let mut spans = vec![
                    TextSpan::styled(
                        if *selected {
                            theme::glyph(Symbol::TreeColumnDivider)
                        } else {
                            " ".to_owned()
                        },
                        theme::fg(Token::Accent),
                    ),
                    TextSpan::raw(tree_indent(*depth, rect.width)),
                ];
                match marker_side {
                    MarkerSide::Leading => {
                        spans.push(leading_marker(marker));
                        spans.extend(row_icon(*icon));
                        let room = label_room(&spans, rect.width);
                        spans.push(TextSpan::styled(
                            text::elide_file_name(name, room),
                            label_style,
                        ));
                    }
                    MarkerSide::Trailing => {
                        spans.extend(row_icon(*icon));
                        push_flat_label(&mut spans, rect.width, name, detail, label_style, marker);
                    }
                }
                if *selected {
                    row::pad_to(&mut spans, rect.width, theme::color(Token::SurfaceSelected));
                }
                frame.render_widget(Paragraph::new(Line::from(spans)), rect);
                hits.push((rect, ViewHit::SelectItem(*id)));
            }
        }
    }
    if let Some(bar) = bar {
        bar.render_on_draggable(frame, settled.first, resizing);
    }
    (settled, bar)
}

/// A tree row's marker: in the column a group's fold mark stands in, and
/// holding that column's width even when it has nothing to say — so a
/// file with no status still lines its icon and name up with the folders
/// beside it.
fn leading_marker(marker: &Span) -> TextSpan<'static> {
    let fold_width = TextSpan::raw(theme::glyph(Symbol::ChevronCollapsed)).width();
    let marker_width = TextSpan::raw(marker.text.as_str()).width();
    styled(&Span {
        text: format!(
            "{}{} ",
            marker.text,
            " ".repeat(fold_width.saturating_sub(marker_width))
        ),
        ..marker.clone()
    })
}

/// A flat row's name, its `detail` quieter after it, and the marker pinned
/// to the right edge. The name gives way last: it is what the row is read
/// for, and the detail only tells apart two rows that share one.
/// Levels past which a tree indents by one column rather than two.
const FULL_INDENT_LEVELS: usize = 4;

/// Columns a tree row keeps for its name however deep it sits.
const LABEL_FLOOR: usize = 12;

/// The blank a row at `depth` starts with.
///
/// Two columns a level near the top, where the shape of the tree is read,
/// and one past [`FULL_INDENT_LEVELS`], where it is only counted; and never
/// so much that fewer than [`LABEL_FLOOR`] columns are left for the name.
/// Indentation that pushes the name out of the column shows where a file
/// is at the cost of which file it is.
fn tree_indent(depth: usize, width: u16) -> String {
    let natural = 2 * depth.min(FULL_INDENT_LEVELS) + depth.saturating_sub(FULL_INDENT_LEVELS);
    let ceiling = usize::from(width).saturating_sub(LABEL_FLOOR + usize::from(TRAILING_PAD) + 4);
    " ".repeat(natural.min(ceiling))
}

/// The columns left for a row's name after what `spans` already holds,
/// keeping the trailing pad off the divider.
fn label_room(spans: &[TextSpan<'_>], width: u16) -> usize {
    let used: usize = spans.iter().map(TextSpan::width).sum();
    usize::from(width)
        .saturating_sub(used + usize::from(TRAILING_PAD))
        .max(1)
}

fn push_flat_label(
    spans: &mut Vec<TextSpan<'static>>,
    width: u16,
    name: &str,
    detail: &str,
    label_style: Style,
    marker: &Span,
) {
    let leading: usize = spans.iter().map(TextSpan::width).sum();
    let marker_width = TextSpan::raw(marker.text.as_str()).width();
    let room = usize::from(width)
        .saturating_sub(leading + marker_width + usize::from(TRAILING_PAD) + 1)
        .max(1);
    let name = text::elide(name, room);
    let left = room.saturating_sub(TextSpan::raw(name.as_str()).width());
    spans.push(TextSpan::styled(name, label_style));
    if !detail.is_empty() && left > 1 {
        spans.push(TextSpan::styled(
            format!(" {}", text::elide(detail, left - 1)),
            theme::fg(Token::TextMuted),
        ));
    }
    let hue = styled(marker)
        .style
        .fg
        .unwrap_or_else(|| color(marker.role));
    row::push_trailing(spans, width, marker.text.clone(), hue);
}

/// The content column: a heading, then as many lines as fit.
///
/// Also the one place a click inside the content means anything — every
/// drawn line pushes a [`ViewHit::SelectLine`] for the rows it occupies,
/// so an extension that puts a caret somewhere can be told where the
/// pointer wanted it without ever seeing a coordinate.
/// The [`Content::Lines`] a frame is drawing, borrowed together — they
/// arrive as one thing from the extension and are laid out as one thing
/// here, so they travel as one rather than as five arguments in an order
/// somebody has to keep.
struct Lines<'a> {
    heading: &'a str,
    scroll: u16,
    /// Where `lines` starts in the whole content — see
    /// [`Content::Lines::first`]. Every index this draws with is an index
    /// into the content, never into the window.
    first: usize,
    lines: &'a [ContentLine],
    total: usize,
    caret: Option<Caret>,
    modes: &'a [Mode],
}

fn render_lines(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    content_lines: Lines<'_>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Option<Scrollbar> {
    let Lines {
        heading,
        scroll,
        first,
        lines,
        total,
        caret,
        modes,
    } = content_lines;
    frame.render_widget(
        Paragraph::new(TextSpan::styled(
            heading.to_owned(),
            theme::fg(Token::TextSecondary),
        )),
        Rect::new(
            area.x,
            area.y,
            area.width.saturating_sub(modes_width(modes)),
            1,
        ),
    );
    render_modes(frame, area, modes, hits);
    let body = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        area.height.saturating_sub(1),
    );
    // The overlay's own right padding, for the same reason the
    // navigator's groove sits in its panel's: flush against the edge
    // rather than a column short of it, and the content keeps its full
    // width because that column was never the content's.
    let bar = Scrollbar::measure(
        Rect::new(body.right(), body.y, Scrollbar::width(), body.height),
        body.height as usize,
        total,
    );
    let gutter = gutter_width(lines);
    // Unnumbered content is prose, and prose is the case the gutter was
    // silently paying for everywhere else.
    let inset = if gutter == 0 { PROSE_INSET } else { 0 };
    let content = if body.width > inset.saturating_mul(2) {
        Rect::new(
            body.x.saturating_add(inset),
            body.y,
            body.width.saturating_sub(inset.saturating_mul(2)),
            body.height,
        )
    } else {
        body
    };
    let text_width = text_width(content.width, gutter);
    let mut y = content.y;
    for (offset, line) in lines
        .iter()
        .enumerate()
        .map(|(offset, line)| (offset + first, line))
        .skip((scroll as usize).saturating_sub(first))
    {
        let height = line_height(line, content.width, gutter);
        if y.saturating_add(height) > content.bottom() {
            break;
        }
        let row = Rect::new(content.x, y, content.width, height);
        render_line(frame, row, line, gutter, true);
        // One hit per *visual* row, not per line: a wrapped line covers
        // several, and which one the pointer is on is half of where in
        // the text it landed. The cell offset here is the row's own
        // start; the caller adds the horizontal distance, which is the
        // only part of the answer that needs the pointer.
        for wrapped in 0..height {
            hits.push((
                Rect::new(row.x, row.y + wrapped, row.width, 1),
                ViewHit::PlaceCaret {
                    line: offset,
                    cell: wrapped as usize * text_width,
                },
            ));
        }
        if let Some(caret) = caret.filter(|caret| caret.line == offset) {
            render_caret(frame, row, line, caret.column, gutter);
        }
        y = y.saturating_add(height);
    }
    render_scrollbar(
        frame,
        bar,
        scroll as usize,
        hits,
        ViewHit::DragContentScrollbar,
    )
}

/// An empty surface, or one that failed: what is the matter, and — when
/// there is something to do about it — what to do.
///
/// Set a third of the way down rather than pinned to the top edge. A line
/// of text against the top-left corner reads as a document that got cut
/// off, which is the one thing an empty surface must not look like; the
/// same words with space above and below read as the state they are.
fn render_message(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    text: &str,
    hint: Option<&str>,
    colour: Color,
) {
    if area.height == 0 {
        return;
    }
    let lines = message_lines(text, hint, area.width, colour);
    // Centred on the block's real height, wrapped lines included, so a
    // long hint does not push the message off the middle it was aimed at.
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .min(area.height);
    let top = area.y + (area.height - height) / 2;
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        Rect::new(area.x, top, area.width, height),
    );
}

/// The widest a message's lines run: a sentence across a whole wide pane
/// is read as a strip, not as a sentence.
const MESSAGE_MEASURE: u16 = 48;

/// A message as the lines it is drawn in. A message that carries a hint is
/// a heading over it, so it reads as one in capitals; one on its own — an
/// error, "reading…" — is a sentence and keeps its case.
fn message_lines(text: &str, hint: Option<&str>, width: u16, colour: Color) -> Vec<Line<'static>> {
    let measure = usize::from(width.clamp(1, MESSAGE_MEASURE));
    let title = if hint.is_some() {
        text.to_uppercase()
    } else {
        text.to_owned()
    };
    let mut lines: Vec<Line<'static>> = text::fold(&title, measure)
        .into_iter()
        .map(|line| {
            Line::from(TextSpan::styled(
                line,
                Style::default().fg(colour).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    if let Some(hint) = hint {
        lines.push(Line::from(""));
        lines.extend(
            hint_rows(hint, measure)
                .into_iter()
                .map(|line| Line::from(TextSpan::styled(line, theme::fg(Token::TextMuted)))),
        );
    }
    lines
}

/// A hint's rows: each of its lines on a row of its own, kept as written
/// where it fits — so a list the extension lined up in columns stays lined
/// up — and folded between words where it does not.
fn hint_rows(hint: &str, measure: usize) -> Vec<String> {
    hint.lines()
        .flat_map(|line| {
            if text::columns(line) <= measure {
                vec![line.to_owned()]
            } else {
                text::fold(line, measure)
            }
        })
        .collect()
}

/// What a row's kind is called in the vocabulary.
///
/// Separate from drawing it so the mapping can be checked without a theme
/// in force: putting one there is process-wide, and a test that did it
/// would swap the glyphs under every neighbour drawing at the same moment.
fn icon_symbol(icon: RowIcon) -> Option<Symbol> {
    Some(match icon {
        RowIcon::None => return None,
        RowIcon::Directory => Symbol::FileDirectory,
        RowIcon::DirectoryOpen => Symbol::FileDirectoryOpen,
        RowIcon::File => Symbol::FileDefault,
        RowIcon::Code => Symbol::FileCode,
        RowIcon::Markup => Symbol::FileMarkup,
        RowIcon::Config => Symbol::FileConfig,
        RowIcon::Lock => Symbol::FileLock,
        RowIcon::Data => Symbol::FileData,
        RowIcon::Image => Symbol::FileImage,
        RowIcon::Archive => Symbol::FileArchive,
        RowIcon::Git => Symbol::FileGit,
        RowIcon::Legal => Symbol::FileLegal,
        RowIcon::Map => Symbol::Map,
        RowIcon::Changes => Symbol::Changes,
        RowIcon::InFlight => Symbol::InFlight,
        RowIcon::Contract => Symbol::Contract,
        RowIcon::Finished => Symbol::Finished,
    })
}

/// The glyph a section row's mark is drawn as, with the cells it takes.
fn row_mark(mark: RowMark) -> (String, u16) {
    let symbol = match mark {
        RowMark::Head => Symbol::CommitHead,
        RowMark::Commit => Symbol::Commit,
        RowMark::Step { .. } => Symbol::MarkDone,
    };
    let width = theme::width(symbol);
    match mark {
        RowMark::Step { done: false } => (" ".repeat(width as usize), width),
        _ => (theme::glyph(symbol), width),
    }
}

/// The mark a navigator row carries before its name.
///
/// The one place a [`RowIcon`] becomes a glyph: the extension said what
/// the row *is*, and the vocabulary says what that looks like under the
/// active theme. A set that draws none of them — every built-in one but
/// `nerd`, since plain Unicode has no folder mark that is not an emoji —
/// resolves to nothing, and nothing is what gets drawn, without a column
/// held open for it.
fn row_icon(icon: RowIcon) -> Option<TextSpan<'static>> {
    // Muted on purpose: the icon classifies, the name identifies, and an
    // icon drawn at the name's weight competes with the thing the reader
    // is actually scanning for.
    Some(icon_span(icon)?.style(theme::fg(Token::TextDim)))
}

/// The mark and the space after it, or nothing at all where the glyph
/// set has no icon for this kind. Unstyled: a row draws its marks muted,
/// a control draws its own in the weight the member is in.
fn icon_span(icon: RowIcon) -> Option<TextSpan<'static>> {
    let glyph = theme::glyph(icon_symbol(icon)?);
    if glyph.trim().is_empty() {
        return None;
    }
    Some(TextSpan::raw(format!("{glyph} ")))
}

/// How much of the heading row the mode control takes, so the heading
/// itself is drawn shorter rather than under it.
fn modes_width(modes: &[Mode]) -> u16 {
    modes.iter().map(mode_width).sum()
}

/// One member's cells: its label, its padding, and the mark before it
/// where the glyph set has one to draw.
fn mode_width(mode: &Mode) -> u16 {
    let icon = icon_cells(mode.icon);
    TextSpan::raw(&mode.label).width() as u16 + icon + 2 * MODE_PAD
}

/// How many cells a mark takes, counted from what would be drawn rather
/// than from the meaning: a glyph set with no icon for it draws nothing,
/// and a column reserved for nothing is a gap the reader has to account
/// for.
fn icon_cells(icon: RowIcon) -> u16 {
    match icon_span(icon) {
        Some(span) => span.width() as u16,
        None => 0,
    }
}

/// The ways the content can be shown, offered as a segmented control at
/// the end of its heading row.
///
/// A control rather than a hint, and drawn where the thing it changes is:
/// the same choice a key makes has to be one a pointer can make, or the
/// mode belongs to whoever read the keymap.
fn render_modes(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    modes: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let x = area.right().saturating_sub(modes_width(modes));
    render_chips(frame, x, area.y, modes, hits, ViewHit::SelectMode);
}

/// The same control at the other end of the row: what the surface is
/// about, over the half that does the finding.
fn render_subjects(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    subjects: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    render_chips(
        frame,
        area.x,
        area.y,
        subjects,
        hits,
        ViewHit::SelectSubject,
    );
}

/// One segmented control, from `x` rightwards.
///
/// The members meet on their own padding and the boundary *is* where the
/// fill changes — the same construction the tab strip's button groups
/// use, for the same reason: a divider inside a group is a column
/// belonging to no member.
fn render_chips(
    frame: &mut ratatui::Frame<'_>,
    from: u16,
    y: u16,
    modes: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
    hit_of: fn(usize) -> ViewHit,
) {
    let mut x = from;
    for (index, mode) in modes.iter().enumerate() {
        let width = mode_width(mode);
        let rect = Rect::new(x, y, width, 1);
        // The same lift a chip rests on, and the same one the board's
        // own selector wears — never the accent tint. That tint means
        // "this is the one picked out of several", which is what a list
        // says; a member of this control is *where you are*, and the two
        // surfaces of one extension saying that two different ways is
        // how a vocabulary stops being one.
        let (fill, ink) = match mode.active {
            true => (Token::SurfaceRaised, Token::TextBright),
            false => (Token::SurfaceBackground, Token::TextMuted),
        };
        let mut style = Style::default()
            .fg(theme::color(ink))
            .bg(theme::color(fill));
        if mode.active {
            style = style.add_modifier(Modifier::BOLD);
        }
        let pad = " ".repeat(MODE_PAD as usize);
        let mut spans = vec![TextSpan::styled(pad.clone(), style)];
        if let Some(mark) = icon_span(mode.icon) {
            spans.push(mark.style(style));
        }
        spans.push(TextSpan::styled(format!("{}{pad}", mode.label), style));
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        hits.push((rect, hit_of(index)));
        x = x.saturating_add(width);
    }
}

/// Finishes a [`ViewHit::PlaceCaret`] the last frame produced, using where
/// the pointer actually is.
///
/// The render knew which line a row belonged to and where that row began;
/// only the click knows how far along it landed. Splitting it this way is
/// what keeps the hit table one entry per drawn row instead of one per
/// cell — and the arithmetic stays here, with the layout that produced
/// `row`, rather than in the event loop.
pub(crate) fn caret_cell_at(row: Rect, cell: usize, column: u16, gutter: u16) -> usize {
    cell + usize::from(column.saturating_sub(row.x.saturating_add(gutter)))
}

/// How many cells a line's text has, once the gutter has taken its share.
fn text_width(width: u16, gutter: u16) -> usize {
    usize::from(width.saturating_sub(gutter).max(1))
}

/// How wide the gutter is for these lines: nothing at all when none of
/// them is numbered.
///
/// A rendered document has no line numbers, and reserving the column
/// anyway indents the whole thing by seven cells of blank — which is
/// what the gutter looked like in preview, and what it cost was the
/// left margin of every paragraph.
fn gutter_width(lines: &[ContentLine]) -> u16 {
    let numbered = lines
        .iter()
        .any(|line| !line.number.is_empty() || !line.gutter.trim().is_empty());
    if numbered { GUTTER_WIDTH } else { 0 }
}

/// The caret, drawn by inverting the cell it sits on rather than by
/// drawing a mark into it.
///
/// A glyph rendered at the caret's position *replaces* the character
/// underneath, so the letter being edited is the one letter the person
/// cannot see — the caret eats exactly what the caret is pointing at.
/// Setting the cell's colours leaves the character where it is and makes
/// it the block cursor, which is what a terminal's own cursor does.
///
/// Drawn against the terminal's own cursor rather than with it: the
/// workspace client hides that for the whole session (a pane's PTY draws
/// its own), and turning it back on for one overlay would leave it
/// blinking over a pane the moment the overlay closes.
fn render_caret(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    line: &ContentLine,
    column: usize,
    gutter: u16,
) {
    let width = text_width(row.width, gutter);
    let mut seen = 0usize;
    let mut at = None;
    let end = fold(line, width, |row, cell, _, _| {
        if seen == column {
            at = Some((row, cell));
        }
        seen += 1;
    });
    // A caret past the last character sits one cell beyond it, which is
    // where the next one will be typed — on the next row when this one is
    // full.
    let (down, across) = at.unwrap_or(match end {
        (row, cell) if cell >= width => (row + 1, 0),
        (row, cell) => (row, cell + column.saturating_sub(seen)),
    });
    let x = row.x + gutter + across as u16;
    let y = row.y + down as u16;
    if y >= row.bottom() || x >= row.right() {
        return;
    }
    let cell = &mut frame.buffer_mut()[(x, y)];
    cell.set_bg(theme::color(Token::Accent));
    cell.set_fg(theme::color(Token::SurfaceBackground));
}

/// Draws the groove for a surface, and makes the whole of it the drag
/// target.
///
/// The handle alone would be the obvious target and the wrong one:
/// clicking above or below it is how a pointer says "go there", and a
/// drag that wanders off the handle has to keep working.
fn render_scrollbar(
    frame: &mut ratatui::Frame<'_>,
    bar: Option<Scrollbar>,
    first: usize,
    hits: &mut Vec<(Rect, ViewHit)>,
    hit: ViewHit,
) -> Option<Scrollbar> {
    let bar = bar?;
    bar.render(frame, first);
    hits.push((bar.track, hit));
    Some(bar)
}

fn line_height(line: &ContentLine, width: u16, gutter: u16) -> u16 {
    let (row, _) = fold(line, text_width(width, gutter), |_, _, _, _| {});
    u16::try_from(row + 1).unwrap_or(u16::MAX)
}

fn render_line(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    line: &ContentLine,
    gutter: u16,
    wrapped: bool,
) {
    let (marker_style, background) = match line.tone {
        LineTone::Neutral => (theme::fg(Token::TextFaint), None),
        LineTone::Added => (
            theme::fg(Token::StateSuccess),
            Some(theme::color(Token::StateDiffAdded)),
        ),
        LineTone::Removed => (
            theme::fg(Token::StateDanger),
            Some(theme::color(Token::StateDiffRemoved)),
        ),
    };
    let mut content_spans: Vec<TextSpan<'static>> = line.spans.iter().map(styled).collect();
    if let Some(background) = background {
        for span in &mut content_spans {
            span.style = span.style.bg(background);
        }
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(gutter), Constraint::Min(1)])
        .split(area);
    if gutter > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                TextSpan::styled(format!("{} ", line.gutter), marker_style),
                TextSpan::styled(format!("{:>4} ", line.number), theme::fg(Token::TextDim)),
            ]))
            .style(
                Style::default().bg(background.unwrap_or(theme::color(Token::SurfaceBackground))),
            ),
            columns[0],
        );
    }
    // A drawing is cut at the edge and prose breaks at its words. Code
    // is folded at the cell, by the same walk that places the caret on
    // it, since a caret is only ever drawn on a numbered line.
    if wrapped && gutter == 0 {
        let text = Paragraph::new(Line::from(content_spans))
            .style(
                Style::default().bg(background.unwrap_or(theme::color(Token::SurfaceBackground))),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(text, columns[1]);
        return;
    }
    let lines = match wrapped {
        true => folded_rows(line, usize::from(columns[1].width))
            .into_iter()
            .map(|mut row| {
                if let Some(background) = background {
                    for span in &mut row {
                        span.style = span.style.bg(background);
                    }
                }
                Line::from(row)
            })
            .collect(),
        false => vec![Line::from(content_spans)],
    };
    let text = Paragraph::new(lines)
        .style(Style::default().bg(background.unwrap_or(theme::color(Token::SurfaceBackground))));
    frame.render_widget(text, columns[1]);
}

/// A hairline top border plus the hint text directly under it — the same
/// shape `management::render_footer` uses.
/// A footer row: the keys on the left, and what the surface has to say
/// right-aligned at the other end.
///
/// The hints are given what is left of the row rather than the whole
/// width: drawn over each other, a narrow board reads `g ren3 boxes · 2
/// edges`, which is two truths written into one run of cells and no truth
/// at all.
fn render_footer_row(
    frame: &mut ratatui::Frame<'_>,
    footer: Rect,
    commands: &[Command],
    scope: uze_keys::Scope,
    trailing: Option<TextSpan<'static>>,
) {
    let Some(trailing) = trailing else {
        render_footer(frame, footer, commands, scope);
        return;
    };
    let width = (trailing.width() as u16).min(footer.width / 2);
    let hints = Rect {
        width: footer.width.saturating_sub(width + FOOTER_GAP),
        ..footer
    };
    render_footer(frame, hints, commands, scope);
    let mut line = Line::from(trailing);
    text::clip(&mut line, usize::from(width));
    frame.render_widget(
        Paragraph::new(line).alignment(ratatui::layout::Alignment::Right),
        Rect::new(footer.right() - width, footer.y, width, 1),
    );
}

fn render_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    commands: &[Command],
    scope: uze_keys::Scope,
) {
    // The overlay is what is open, so its own scope is what a key would
    // resolve against — the same stack `Attach::scopes` builds.
    let scopes = [uze_keys::Scope::Global, uze_keys::Scope::Workspace, scope];
    let actions: Vec<uze_keys::Action> = commands.iter().copied().filter_map(action_of).collect();
    frame.render_widget(
        Paragraph::new(hint::within(area.width, &scopes, &actions)),
        area,
    );
}

/// What each extension command means in the product's own vocabulary, read
/// both ways: a key the workspace resolves is handed down as the command
/// beside its action, and a footer names a command by the key its action
/// is bound to. Kept here, beside the render that needs it, rather than in
/// the extension, which knows nothing of either. Where two actions reach
/// one command, the first row is the one a footer names.
const COMMAND_ACTIONS: [(Command, uze_keys::Action); 39] = [
    (Command::Close, uze_keys::Action::Dismiss),
    (Command::FocusNext, uze_keys::Action::FocusNext),
    (Command::FocusNext, uze_keys::Action::FocusPrevious),
    (Command::SelectNext, uze_keys::Action::SelectNext),
    (Command::SelectPrevious, uze_keys::Action::SelectPrevious),
    (Command::Collapse, uze_keys::Action::Collapse),
    (Command::Expand, uze_keys::Action::Expand),
    (Command::Activate, uze_keys::Action::Activate),
    (Command::OpenMenu, uze_keys::Action::OpenMenu),
    (Command::ScrollPageUp, uze_keys::Action::ScrollPageUp),
    (Command::ScrollPageDown, uze_keys::Action::ScrollPageDown),
    (Command::Edit, uze_keys::Action::EditFile),
    (Command::TogglePreview, uze_keys::Action::TogglePreview),
    (Command::Save, uze_keys::Action::SaveFile),
    (Command::Delete, uze_keys::Action::DeleteFile),
    (Command::ConfirmDelete, uze_keys::Action::ConfirmDelete),
    (Command::CaretLeft, uze_keys::Action::CaretLeft),
    (Command::CaretRight, uze_keys::Action::CaretRight),
    (Command::CaretLineStart, uze_keys::Action::CaretLineStart),
    (Command::CaretLineEnd, uze_keys::Action::CaretLineEnd),
    (Command::Newline, uze_keys::Action::InsertNewline),
    (Command::Indent, uze_keys::Action::InsertIndent),
    (Command::EraseBack, uze_keys::Action::EraseBack),
    (Command::EraseForward, uze_keys::Action::EraseForward),
    (Command::Pan(PanDirection::Left), uze_keys::Action::PanLeft),
    (
        Command::Pan(PanDirection::Right),
        uze_keys::Action::PanRight,
    ),
    (Command::Pan(PanDirection::Up), uze_keys::Action::PanUp),
    (Command::Pan(PanDirection::Down), uze_keys::Action::PanDown),
    (Command::NextView, uze_keys::Action::NextDiagram),
    (Command::PreviousView, uze_keys::Action::PreviousDiagram),
    (Command::NextMode, uze_keys::Action::NextRendering),
    (Command::ChooseGroup, uze_keys::Action::ChooseArea),
    (Command::ChooseItem, uze_keys::Action::ChooseArtifact),
    (
        Command::SelectToward(PanDirection::Left),
        uze_keys::Action::SelectBoxLeft,
    ),
    (
        Command::SelectToward(PanDirection::Right),
        uze_keys::Action::SelectBoxRight,
    ),
    (
        Command::SelectToward(PanDirection::Up),
        uze_keys::Action::SelectBoxUp,
    ),
    (
        Command::SelectToward(PanDirection::Down),
        uze_keys::Action::SelectBoxDown,
    ),
    (Command::Back, uze_keys::Action::LevelUp),
    (Command::ToggleMap, uze_keys::Action::ToggleMap),
];

/// The action a command is named by. `None` for typing, which has no
/// single key to name.
fn action_of(command: Command) -> Option<uze_keys::Action> {
    COMMAND_ACTIONS
        .iter()
        .find(|(candidate, _)| *candidate == command)
        .map(|(_, action)| *action)
}

/// The command a resolved action hands down to an extension's surface,
/// when it means one.
pub(crate) fn command_for(action: uze_keys::Action) -> Option<Command> {
    COMMAND_ACTIONS
        .iter()
        .find(|(_, candidate)| *candidate == action)
        .map(|(command, _)| *command)
}

/// Draws one extension [`Section`] into the rows it is given, and reports
/// what a click on each row would mean.
///
/// The counterpart to [`render`] for a section rather than a full frame,
/// and the reason it is here rather than in either sidebar: an extension
/// section is an extension surface, and both of them resolve colour,
/// eliding and hit rectangles in this one module. `dragging` is the host's
/// own state — whether the divider under the header is being pulled right
/// now — because the gesture belongs to the host, not to whoever the
/// section came from.
pub(crate) fn render_section(
    frame: &mut ratatui::Frame<'_>,
    section: &Section,
    rows: &mut crate::ui::Rows,
    dragging: bool,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> bool {
    render_section_with(frame, section, rows, dragging, None, None, hits)
}

/// The same, with `marquee` the clock a caption too long for its room
/// slides by — `Some` while the pointer is on this header, `None`
/// otherwise. Answers whether a caption actually slid, which is what
/// tells the host's own clock it has a reason to keep turning.
///
/// `hovered_row` is the row under the pointer, its name lifted to the
/// bright text the row receiving keystrokes wears: a row that opens
/// something on a click says so before it is clicked. The text alone, not
/// a ground — a band under a row in this column reads as the selection.
pub(crate) fn render_section_with(
    frame: &mut ratatui::Frame<'_>,
    section: &Section,
    rows: &mut crate::ui::Rows,
    dragging: bool,
    marquee: Option<usize>,
    hovered_row: Option<usize>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> bool {
    let Some(header_rect) = rows.next(1) else {
        return false;
    };
    let fold = theme::glyph(if section.collapsed {
        Symbol::ChevronCollapsed
    } else {
        Symbol::ChevronExpanded
    });
    // Bold only while open, over its filled row: a heading over the content
    // beneath it. Folded there is nothing under it to head, and bold titles
    // stacked at the foot of the column shouted over the tree above.
    let mut title_style = Style::default().fg(theme::color(Token::TextSecondary));
    if !section.collapsed {
        title_style = title_style.add_modifier(Modifier::BOLD);
    }
    let mut spans = vec![
        TextSpan::styled(format!("{fold} "), theme::fg(Token::TextSecondary)),
        TextSpan::styled(section.title.clone(), title_style),
    ];
    let sliding = match marquee {
        Some(tick) => row::push_trailing_marquee(
            &mut spans,
            header_rect.width,
            section.caption.text.clone(),
            color(section.caption.role),
            // A column every other tick: one per tick reads as a flicker
            // at the clock the spinners turn on.
            tick / 2,
        ),
        None => {
            row::push_trailing(
                &mut spans,
                header_rect.width,
                section.caption.text.clone(),
                color(section.caption.role),
            );
            false
        }
    };
    // Filled only while there is something under it. A band across the
    // column says "this is a heading over content"; on a folded section
    // there is no content, and the band reads as a control of its own —
    // two of them stacked at the foot of the sidebar read as a toolbar.
    if !section.collapsed {
        row::pad_to(
            &mut spans,
            header_rect.width,
            theme::color(Token::SurfaceRaised),
        );
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), header_rect);
    hits.push((header_rect, ViewHit::ToggleSection));
    if section.collapsed {
        return sliding;
    }
    // The divider between the header and its rows doubles as the drag
    // handle, the way the sidebar's own border does — lit in the accent
    // while it is being dragged, same as that border.
    if section.resizable
        && let Some(handle_rect) = rows.next(1)
    {
        let hue = if dragging {
            theme::color(Token::Accent)
        } else {
            theme::color(Token::BorderFaint)
        };
        frame.render_widget(
            Paragraph::new(TextSpan::styled(
                theme::glyph(Symbol::TreeDivider).repeat(handle_rect.width as usize),
                Style::default().fg(hue),
            )),
            handle_rect,
        );
        hits.push((handle_rect, ViewHit::ResizeSection));
    }
    // Scrolled by whole rows, never past the page that ends on the last
    // one — so the section is always full when its content is.
    let visible = usize::from(rows.remaining());
    let first = section
        .scroll
        .min(section.rows.len().saturating_sub(visible));
    for (index, row) in section.rows.iter().enumerate().skip(first) {
        let Some(rect) = rows.next(1) else {
            break;
        };
        let (mark, mark_width) = row_mark(row.mark);
        let marker_width = mark_width + 1;
        let trailing_width = row.trailing.text.chars().count() as u16;
        // The name gives way before the trailing value, and one column is
        // reserved for the gap `push_trailing` always leaves between them.
        let name_width = rect
            .width
            .saturating_sub(marker_width + 1 + trailing_width + TRAILING_PAD);
        let mut spans = vec![
            TextSpan::styled(
                format!("{mark} "),
                Style::default().fg(color(row.mark_role)),
            ),
            TextSpan::styled(
                text::elide(&row.name.text, name_width as usize),
                Style::default().fg(if hovered_row == Some(index) {
                    theme::color(Token::TextBright)
                } else {
                    color(row.name.role)
                }),
            ),
        ];
        row::push_trailing(
            &mut spans,
            rect.width,
            row.trailing.text.clone(),
            color(row.trailing.role),
        );
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        hits.push((rect, ViewHit::SelectItem(index)));
    }
    sliding
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use uze_extensions::view::{ContentLine, LineTone, Rgb, RowMenu};

    #[test]
    fn a_choice_list_on_a_board_too_narrow_for_it_draws_without_panicking() {
        let mut terminal = Terminal::new(TestBackend::new(8, 8)).unwrap();
        let rows = [ChoiceRow {
            hit: ViewHit::GrabNavigatorEdge,
            name: "overview",
            trailing: String::new(),
            highlighted: true,
        }];
        for width in [0, 1, 2] {
            let mut hits = Vec::new();
            terminal
                .draw(|frame| {
                    render_choice_list(
                        frame,
                        Rect::new(0, 0, width, 1),
                        Rect::new(0, 0, width, 6),
                        &rows,
                        ViewHit::GrabNavigatorEdge,
                        &mut hits,
                    );
                })
                .unwrap();
        }
    }

    /// A caption that fits is drawn where it always was; one that does
    /// not passes through the room it has, a column at a time, and comes
    /// round again — and says so, which is what keeps the clock turning
    /// for it.
    ///
    /// A branch name is the caption with no natural length, and it is
    /// read from both ends: the end is exactly the half an "…" eats.
    #[test]
    fn a_caption_too_long_for_its_room_passes_through_it() {
        let header = |caption: &str, tick: Option<usize>| {
            let section = Section {
                title: "timeline".to_owned(),
                caption: Span::new(caption, Role::Muted),
                collapsed: true,
                resizable: false,
                scroll: 0,
                rows: Vec::new(),
            };
            let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
            let mut hits = Vec::new();
            let mut slid = false;
            terminal
                .draw(|frame| {
                    let mut rows = crate::ui::Rows::over(frame.area());
                    slid = render_section_with(
                        frame, &section, &mut rows, false, tick, None, &mut hits,
                    );
                })
                .unwrap();
            let row: String = (0..40)
                .map(|column| terminal.backend().buffer()[(column, 0)].symbol())
                .collect();
            (slid, row)
        };

        let short = "main";
        let (slid, row) = header(short, Some(0));
        assert!(!slid, "a caption that fits does not move: {row:?}");
        assert!(row.contains(short), "{row:?}");

        // Longer than the room the title leaves it.
        let long = "refactor/a-branch-nobody-shortened";
        let (slid, at_rest) = header(long, None);
        assert!(!slid, "and nothing moves unasked: {at_rest:?}");

        let (slid, first) = header(long, Some(0));
        assert!(slid, "asked, it slides: {first:?}");
        // Two ticks a column, so the clock the spinners turn on does not
        // read as a flicker here.
        let (_, same) = header(long, Some(1));
        assert_eq!(first, same, "a column every other tick: {first:?}");
        let (_, moved) = header(long, Some(2));
        assert_ne!(first, moved, "and then it has moved: {moved:?}");

        // It comes round rather than jumping back: one full cycle of the
        // run lands on what it started from.
        let cycle = long.chars().count() + 3;
        let (_, round) = header(long, Some(cycle * 2));
        assert_eq!(first, round, "one cycle returns it: {round:?}");
    }

    /// The row under the pointer has its name lifted to the bright text,
    /// on the ground it already had, and no other row does.
    #[test]
    fn the_row_under_the_pointer_lifts_its_name() {
        let commit = |name: &str| uze_extensions::view::SectionRow {
            mark: uze_extensions::view::RowMark::Commit,
            mark_role: Role::Muted,
            name: Span::new(name, Role::Dim),
            trailing: Span::new("8m", Role::Muted),
        };
        let section = Section {
            title: "timeline".to_owned(),
            caption: Span::new("main", Role::Muted),
            collapsed: false,
            resizable: false,
            scroll: 0,
            rows: vec![commit("fix(ui): one"), commit("feat(cli): two")],
        };
        let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
        terminal
            .draw(|frame| {
                let mut rows = crate::ui::Rows::over(frame.area());
                render_section_with(
                    frame,
                    &section,
                    &mut rows,
                    false,
                    None,
                    Some(1),
                    &mut Vec::new(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let bright = theme::color(Token::TextBright);
        // The header, then one row per commit; the name starts past the
        // mark and its gap.
        assert_ne!(buffer[(2, 1)].fg, bright, "the resting row's name");
        assert_eq!(buffer[(2, 2)].fg, bright, "the hovered row's name");
        assert_eq!(
            buffer[(2, 2)].bg,
            buffer[(2, 1)].bg,
            "on the same ground as its neighbour"
        );
    }

    /// Which drawn row a board's menu lands on: the first, with no frame
    /// above it.
    const MENU_ROW: usize = 0;

    fn sample() -> View {
        View {
            title: vec![Span::new("demo", Role::Bright)],
            caption: Vec::new(),
            navigator: Some(Navigator {
                heading: "CHANGES".to_owned(),
                badge: "2".to_owned(),
                focused: true,
                anchor: Some(1),
                choosing: None,
                menu: None,
                rows: vec![
                    NavigatorRow::Group {
                        id: 0,
                        name: "src/".to_owned(),
                        depth: 0,
                        collapsed: false,
                        icon: RowIcon::Directory,
                    },
                    NavigatorRow::Item {
                        id: 7,
                        name: "ui.rs".to_owned(),
                        depth: 1,
                        marker: Span::new("M", Role::Warning),
                        marker_side: MarkerSide::Leading,
                        detail: String::new(),
                        selected: true,
                        icon: RowIcon::Code,
                    },
                ],
            }),
            content: Content::Lines {
                first: 0,
                caret: None,
                total: 1,
                heading: "DIFF · src/ui.rs".to_owned(),
                scroll: 0,
                lines: vec![ContentLine {
                    gutter: "+".to_owned(),
                    number: "12".to_owned(),
                    tone: LineTone::Added,
                    spans: vec![Span {
                        text: "let x = 1;".to_owned(),
                        role: Role::Default,
                        color: Some(Rgb(1, 2, 3)),
                        ground: None,
                        bold: false,
                        italic: false,
                    }],
                }],
            },
            footer: vec![Command::Close],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Sidebar,
            trail: Vec::new(),
        }
    }

    /// A band is told apart from the tree by form — its name in capitals
    /// with its count beside it, in glyphs every font has — and the air
    /// before the next one is drawn blank and answers nothing.
    #[test]
    fn a_band_heading_is_set_apart_and_the_gap_before_it_is_air() {
        let mut view = sample();
        let navigator = view.navigator.as_mut().expect("the sample has a navigator");
        navigator.anchor = None;
        navigator.rows = vec![
            NavigatorRow::Band {
                id: 0,
                name: "in progress".to_owned(),
                count: 2,
                collapsed: false,
            },
            NavigatorRow::Item {
                id: 1,
                name: "a-change".to_owned(),
                depth: 1,
                marker: Span::new("1/2", Role::Muted),
                marker_side: MarkerSide::Trailing,
                detail: String::new(),
                selected: false,
                icon: RowIcon::None,
            },
            NavigatorRow::Gap,
            NavigatorRow::Band {
                id: 3,
                name: "ready to archive".to_owned(),
                count: 1,
                collapsed: true,
            },
        ];
        let (rows, hits) = draw(&view);

        let band = rows
            .iter()
            .position(|row| row.contains("IN PROGRESS 2"))
            .expect("the band's name is drawn in capitals, with its count");
        let gap = band + 2;
        assert!(
            rows[gap].chars().take(20).all(|glyph| glyph == ' '),
            "the gap is blank: {:?}",
            rows[gap]
        );
        assert!(rows[gap + 1].contains("READY TO ARCHIVE 1"));

        let answering: Vec<ViewHit> = hits
            .iter()
            .filter(|(rect, _)| usize::from(rect.y) == gap && rect.x < 20)
            .map(|(_, hit)| *hit)
            .collect();
        assert!(
            answering.is_empty(),
            "the gap answers nothing: {answering:?}"
        );
        assert!(
            hits.iter()
                .any(|(rect, hit)| usize::from(rect.y) == band && *hit == ViewHit::ToggleGroup(0)),
            "a band folds from its heading"
        );
    }

    fn draw(view: &View) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
        let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                    },
                    uze_keys::Scope::Code,
                    &mut hits,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect()
            })
            .collect();
        (rows, hits)
    }

    /// With no list there is no list's column: the message is centred on
    /// the whole width, the keys start at its left edge, and there is no
    /// edge to drag.
    #[test]
    fn a_view_with_no_navigator_takes_the_whole_width() {
        let view = View {
            navigator: None,
            content: Content::Message {
                text: "Nothing here".to_owned(),
                hint: None,
                role: Role::Muted,
            },
            ..sample()
        };
        let (rows, hits) = draw(&view);

        let message = rows
            .iter()
            .find(|row: &&String| row.contains("Nothing here"))
            .expect("the message is drawn");
        let left = message.find("Nothing here").unwrap();
        let right = message.len() - left - "Nothing here".len();
        assert!(
            left.abs_diff(right) <= 1,
            "centred on 90 columns: {message:?}"
        );
        assert!(
            rows.last().unwrap().starts_with("esc"),
            "the keys start at the frame's edge: {:?}",
            rows.last()
        );
        assert!(
            !hits
                .iter()
                .any(|(_, hit)| matches!(hit, ViewHit::GrabNavigatorEdge)),
            "no column, no edge"
        );
    }

    /// A hint's own lines are kept, and kept as written where they fit:
    /// a list lined up in columns stays lined up.
    #[test]
    fn a_hint_keeps_its_lines_and_their_spacing() {
        let rows = hint_rows(
            "Pick one:\n\nOpenSpec   openspec/\nSpec Kit   .specify/",
            48,
        );
        assert_eq!(
            rows,
            [
                "Pick one:",
                "",
                "OpenSpec   openspec/",
                "Spec Kit   .specify/"
            ]
        );
        assert_eq!(
            hint_rows("one two three", 7),
            ["one two", "three"],
            "a line wider than the measure still folds"
        );
    }

    /// A refused gesture says so on the footer's row, in the ink its role
    /// names, beside the keys rather than over them: otherwise a key that
    /// was refused is a key that seemed to do nothing.
    #[test]
    fn a_notice_is_said_at_the_end_of_the_footer() {
        let view = View {
            notice: Some(Span::new("a.txt has unsaved changes", Role::Warning)),
            ..sample()
        };
        let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                    },
                    uze_keys::Scope::Code,
                    &mut Vec::new(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let footer = buffer.area.height - 1;
        let row: String = (0..buffer.area.width)
            .map(|column| buffer[(column, footer)].symbol())
            .collect();
        assert!(
            row.trim_end().ends_with("a.txt has unsaved changes"),
            "{row:?}"
        );
        let at = row.find("a.txt").expect("the notice is on the row") as u16;
        assert_eq!(buffer[(at, footer)].fg, theme::color(Token::StateWarning));
        let (without, _) = draw(&sample());
        let hints = without[usize::from(footer)].trim_end();
        assert!(
            row.starts_with(hints),
            "the keys stay where they were: {row:?} against {hints:?}"
        );
    }

    /// The control that says where you are wears the same neutral lift
    /// the board's own selector does — never the accent tint, which is
    /// what a *list* marks its selection with. One extension, two
    /// surfaces, one vocabulary.
    #[test]
    fn the_control_wears_the_lift_and_the_list_wears_the_tint() {
        let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
        terminal
            .draw(|frame| {
                render_chips(
                    frame,
                    0,
                    0,
                    &[
                        Mode {
                            label: "Files".to_owned(),
                            active: true,
                            icon: RowIcon::None,
                        },
                        Mode {
                            label: "Map".to_owned(),
                            active: false,
                            icon: RowIcon::None,
                        },
                    ],
                    &mut Vec::new(),
                    ViewHit::SelectSubject,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert_eq!(
            buffer[(1, 0)].bg,
            theme::color(Token::SurfaceRaised),
            "the member you are on is lifted, not tinted"
        );
        assert_ne!(
            theme::color(Token::SurfaceRaised),
            theme::color(Token::SurfaceSelected),
            "and the two are genuinely different grounds"
        );
        assert_eq!(
            buffer[(9, 0)].bg,
            theme::color(Token::SurfaceBackground),
            "a member you are not on has no ground at all"
        );
    }

    /// A flat row reads name, then where it sits, quieter, then its status
    /// at the right edge — and in a narrow column the place gives way
    /// before the name does.
    #[test]
    fn a_flat_row_pins_its_marker_right_and_gives_up_the_detail_first() {
        let marker = Span::new("M", Role::Warning);
        let drawn = |width: u16| {
            let mut spans = vec![TextSpan::raw(" ")];
            push_flat_label(
                &mut spans,
                width,
                "ui.rs",
                "src/components/network",
                Style::default(),
                &marker,
            );
            spans
        };

        let wide = drawn(40);
        let text: String = wide.iter().map(|span| span.content.as_ref()).collect();
        assert!(
            text.starts_with(" ui.rs src/components/network"),
            "{text:?}"
        );
        assert!(text.ends_with(&format!("M{}", " ".repeat(TRAILING_PAD.into()))));
        assert_eq!(spans_width(&wide), 40, "the marker is pinned to the edge");
        let detail = wide
            .iter()
            .find(|span| span.content.contains("src/"))
            .expect("the place is drawn");
        assert_eq!(detail.style.fg, Some(theme::color(Token::TextMuted)));

        let narrow = drawn(12);
        let text: String = narrow.iter().map(|span| span.content.as_ref()).collect();
        assert!(text.contains("ui.rs") && !text.contains("src"), "{text:?}");
        assert_eq!(spans_width(&narrow), 12);
    }

    /// The header is one row with a question at each end: which half you
    /// are in, and how the half you are in is drawn.
    ///
    /// The row is the surface's own — it starts at the pane's edge and
    /// spans both columns — so switching halves, which
    /// switches the layout under it, leaves every control on it exactly
    /// where it was.
    #[test]
    fn the_nav_row_is_the_frames_and_does_not_move_with_the_layout() {
        let sidebar = View {
            title: vec![Span::new("code", Role::Muted)],
            caption: Vec::new(),
            navigator: Some(Navigator {
                heading: "FILES".to_owned(),
                badge: "7".to_owned(),
                focused: true,
                rows: vec![NavigatorRow::Item {
                    id: 0,
                    name: "main.rs".to_owned(),
                    depth: 0,
                    marker: Span::new("", Role::Muted),
                    marker_side: MarkerSide::Leading,
                    detail: String::new(),
                    selected: true,
                    icon: RowIcon::None,
                }],
                anchor: None,
                choosing: None,
                menu: None,
            }),
            content: Content::Lines {
                heading: "main.rs".to_owned(),
                scroll: 0,
                first: 0,
                total: 1,
                lines: vec![ContentLine {
                    gutter: " ".to_owned(),
                    number: "1".to_owned(),
                    tone: LineTone::Neutral,
                    spans: vec![Span::new("fn main() {}", Role::Default)],
                }],
                caret: None,
            },
            footer: Vec::new(),
            notice: None,
            confirm: None,
            modes: vec![
                Mode {
                    label: "Preview".to_owned(),
                    active: true,
                    icon: RowIcon::None,
                },
                Mode {
                    label: "Source".to_owned(),
                    active: false,
                    icon: RowIcon::None,
                },
            ],
            subjects: vec![
                Mode {
                    label: "Files".to_owned(),
                    active: true,
                    icon: RowIcon::None,
                },
                Mode {
                    label: "Map".to_owned(),
                    active: false,
                    icon: RowIcon::None,
                },
                Mode {
                    label: "Changes".to_owned(),
                    active: false,
                    icon: RowIcon::None,
                },
            ],
            layout: ViewLayout::Sidebar,
            trail: Vec::new(),
        };
        let (rows, hits) = draw_sized(&sidebar, 80, 12);
        assert!(
            rows[0].contains("Files") && rows[0].contains("Changes"),
            "the halves are the surface's first row: {:?}",
            rows[0]
        );
        assert!(
            rows[0].contains("Preview") && rows[0].contains("Source"),
            "and the ways of drawing that half ride the same row: {:?}",
            rows[0]
        );
        assert!(
            rows[0].find("Files") < rows[0].find("Preview"),
            "one question at each end: {:?}",
            rows[0]
        );
        assert!(
            rows[1].contains("main.rs") && !rows[1].contains("Preview"),
            "the row below is the columns' own, and carries no control: {:?}",
            rows[1]
        );
        let nav_at = |hits: &[(Rect, ViewHit)]| {
            hits.iter()
                .find(|(_, hit)| matches!(hit, ViewHit::SelectSubject(0)))
                .map(|(rect, _)| (rect.x, rect.y))
                .expect("the halves can be pointed at")
        };
        let sidebar_at = nav_at(&hits);
        assert_eq!(sidebar_at, (0, 0), "flush with the pane's corner");

        // The map takes the frame, which is the switch that used to move
        // the control a column sideways.
        let board = View {
            navigator: None,
            layout: ViewLayout::Board,
            modes: vec![Mode {
                label: "Map".to_owned(),
                active: true,
                icon: RowIcon::None,
            }],
            ..sidebar
        };
        let (rows, hits) = draw_sized(&board, 80, 12);
        assert!(rows[0].contains("Files"), "the same row: {:?}", rows[0]);
        assert_eq!(nav_at(&hits), sidebar_at, "at the same cell");
    }

    /// A row's menu is drawn under that row, over whatever it covers, and
    /// its entries are the first thing a click there lands on.
    #[test]
    fn a_row_menu_lies_under_its_row_and_takes_the_click() {
        let row = |id: usize, name: &str| NavigatorRow::Item {
            id,
            name: name.to_owned(),
            depth: 0,
            marker: Span::new("M", Role::Warning),
            marker_side: MarkerSide::Trailing,
            detail: String::new(),
            selected: id == 1,
            icon: RowIcon::None,
        };
        let view = View {
            title: vec![Span::new("code", Role::Muted)],
            caption: Vec::new(),
            navigator: Some(Navigator {
                heading: "CHANGES".to_owned(),
                badge: "3".to_owned(),
                focused: true,
                rows: vec![row(0, "a.rs"), row(1, "b.rs"), row(2, "c.rs")],
                anchor: None,
                choosing: None,
                menu: Some(RowMenu {
                    row: 1,
                    entries: vec!["Open file".to_owned(), "Copy path".to_owned()],
                    highlighted: 0,
                }),
            }),
            content: Content::Message {
                text: String::new(),
                hint: None,
                role: Role::Muted,
            },
            footer: Vec::new(),
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Sidebar,
            trail: Vec::new(),
        };
        let (rows, hits) = draw_with_menu(&view, None);

        let at = |wanted: ViewHit| {
            hits.iter()
                .find(|(_, hit)| *hit == wanted)
                .map(|(rect, _)| *rect)
                .expect("drawn")
        };
        let (opened_on, first) = (at(ViewHit::SelectItem(1)), at(ViewHit::MenuEntry(0)));
        assert!(first.y > opened_on.y, "under the row it was opened on");
        assert!(rows[first.y as usize].contains("Open file"), "{rows:?}");
        let over = hits
            .iter()
            .find(|(rect, _)| rect.contains(ratatui::layout::Position::new(first.x, first.y)))
            .map(|(_, hit)| *hit);
        assert_eq!(
            over,
            Some(ViewHit::MenuEntry(0)),
            "the entry, not the row under it"
        );

        // Asked for by the pointer, it opens where the pointer is.
        let pointer = Rect::new(30, 4, 1, 1);
        let (_, hits) = draw_with_menu(&view, Some(pointer));
        let first = hits
            .iter()
            .find(|(_, hit)| *hit == ViewHit::MenuEntry(0))
            .map(|(rect, _)| *rect)
            .expect("drawn");
        assert!(
            first.y > pointer.y && first.x.abs_diff(pointer.x) <= 2,
            "at the pointer: {first:?}"
        );
    }

    /// A surface's question is the product's dialog: its title, subject,
    /// what agreeing does, and the two answers as buttons.
    #[test]
    fn a_surfaces_question_is_drawn_as_the_products_dialog() {
        let view = View {
            title: Vec::new(),
            caption: Vec::new(),
            navigator: None,
            content: Content::Message {
                text: String::new(),
                hint: None,
                role: Role::Muted,
            },
            footer: Vec::new(),
            notice: None,
            confirm: Some(uze_extensions::view::Confirm {
                title: "Discard changes".to_owned(),
                subject: "src/ui.rs".to_owned(),
                body: "Puts the file back.".to_owned(),
                confirm: "Discard".to_owned(),
                on_confirm: false,
            }),
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Sidebar,
            trail: Vec::new(),
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let mut answers = Vec::new();
        terminal
            .draw(|frame| {
                render_confirm(
                    frame,
                    view.confirm.as_ref().expect("asked"),
                    frame.area(),
                    uze_keys::Scope::Code,
                    &mut answers,
                )
            })
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for words in [
            "Discard changes",
            "src/ui.rs",
            "Puts the file back.",
            "Cancel",
            "Discard",
        ] {
            assert!(screen.contains(words), "{words:?} is on the dialog");
        }
        assert_eq!(
            answers.iter().map(|(_, hit)| *hit).collect::<Vec<_>>(),
            [ViewHit::Answer(false), ViewHit::Answer(true)],
            "the way out before the affirmative"
        );
    }

    fn draw_with_menu(view: &View, at: Option<Rect>) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                    },
                    uze_keys::Scope::Code,
                    &mut hits,
                );
                render_row_menu(frame, view, frame.area(), at, &mut hits);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect()
            })
            .collect();
        (rows, hits)
    }

    /// A board's footer carries two things — the keys on its left and
    /// what is drawn on its right — and they are written into one row.
    /// Narrow enough and they used to meet in the middle, which reads as
    /// neither: `g ren3 boxes · 2 edges`.
    #[test]
    fn a_boards_hints_stop_before_its_caption() {
        let view = View {
            title: vec![Span::new("architect", Role::Muted)],
            caption: Vec::new(),
            navigator: None,
            content: Content::Lines {
                first: 0,
                caret: None,
                total: 1,
                heading: "3 boxes · 2 edges · containers.mmd".to_owned(),
                scroll: 0,
                lines: Vec::new(),
            },
            footer: vec![
                Command::Close,
                Command::ChooseItem,
                Command::NextView,
                Command::NextMode,
            ],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Board,
            trail: Vec::new(),
        };

        for width in [70, 90, 118] {
            let (rows, _) = draw_sized(&view, width, 12);
            let footer = rows
                .iter()
                .find(|row| row.contains("3 boxes"))
                .cloned()
                .unwrap_or_else(|| panic!("the caption is drawn at {width} columns: {rows:?}"));
            // Whatever room is left, the two never touch: the hints end,
            // then blank cells, then the caption.
            let caption = footer
                .find("3 boxes")
                .expect("the caption starts where it starts");
            assert!(
                footer[..caption].ends_with("  "),
                "hints ran into the caption at {width} columns: {footer:?}"
            );
            // And what is kept is whole: a list cut at the edge ends in
            // half a word, which reads as a key nobody can press.
            let hints = footer[..caption].trim_end();
            assert!(
                hints.is_empty() || KEPT_WHOLE.iter().any(|label| hints.ends_with(label)),
                "a hint was cut mid-word at {width} columns: {footer:?}"
            );
        }
    }

    /// Every label this view's own footer can end on. Ending on anything
    /// else is a word the edge took half of.
    const KEPT_WHOLE: [&str; 4] = ["close", "artifacts", "artifact", "rendering"];

    fn draw_sized(view: &View, width: u16, height: u16) -> (Vec<String>, Vec<(Rect, ViewHit)>) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                    },
                    uze_keys::Scope::Architect,
                    &mut hits,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect()
            })
            .collect();
        (rows, hits)
    }

    /// A click anywhere on a board's drawing resolves to the cell that was
    /// drawn there, in the space the frame reports having drawn in.
    ///
    /// Both halves matter, and the second is the one that was wrong. A
    /// surface that places things in the room it is given — the map's
    /// tiles, the architect's diagrams — answers a click by laying itself
    /// out again, so it has to be told the same room twice. The click path
    /// used to recompute that from the pane's size instead of the frame's,
    /// which is smaller by the sidebar and the tab strip: the right and
    /// bottom bands of the drawing belonged to no tile at all, and
    /// everything else belonged to the wrong one.
    #[test]
    fn a_board_click_resolves_in_the_space_the_frame_drew_in() {
        for (width, height) in [(90u16, 24u16), (120, 36), (70, 20)] {
            let area = Rect::new(0, 0, width, height);
            let (_, board_rect, _) = board_rows(area);
            let space = board_space(area);
            let lines: Vec<ContentLine> = (0..space.height)
                .map(|_| ContentLine {
                    gutter: String::new(),
                    number: String::new(),
                    tone: LineTone::Neutral,
                    spans: vec![Span::new("x".repeat(space.width as usize), Role::Default)],
                })
                .collect();
            let view = View {
                title: vec![Span::new("Map", Role::Bright)],
                caption: Vec::new(),
                navigator: None,
                content: Content::Lines {
                    first: 0,
                    heading: String::new(),
                    scroll: 0,
                    total: lines.len(),
                    lines,
                    caret: None,
                },
                footer: vec![Command::Close],
                notice: None,
                confirm: None,
                modes: Vec::new(),
                subjects: Vec::new(),
                layout: ViewLayout::Board,
                trail: Vec::new(),
            };

            let mut hits = Vec::new();
            let mut reported = uze_extensions::view::Size::default();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    reported = render(
                        frame,
                        &view,
                        frame.area(),
                        NavigatorFrame {
                            width: Some(24),
                            scroll: NavigatorScroll::default(),
                            resizing: false,
                        },
                        uze_keys::Scope::Architect,
                        &mut hits,
                    )
                    .content_space;
                })
                .unwrap();

            assert_eq!(
                reported, space,
                "{width}x{height}: the frame must report the board it drew in"
            );

            for row in board_rect.y..board_rect.bottom() {
                for column in board_rect.x..board_rect.right() {
                    let (rect, hit) = hits
                        .iter()
                        .find(|(rect, hit)| {
                            matches!(hit, ViewHit::PlaceCaret { .. })
                                && rect.x <= column
                                && column < rect.right()
                                && rect.y <= row
                                && row < rect.bottom()
                        })
                        .expect("every drawn cell of a board is clickable");
                    let ViewHit::PlaceCaret { line, cell } = hit else {
                        unreachable!("filtered to PlaceCaret");
                    };
                    let resolved = caret_cell_at(*rect, *cell, column, 0);
                    assert_eq!(
                        (*line, resolved),
                        (
                            usize::from(row - board_rect.y),
                            usize::from(column - board_rect.x)
                        ),
                        "{width}x{height}: cell ({column}, {row})"
                    );
                    assert!(
                        *line < usize::from(reported.height)
                            && resolved < usize::from(reported.width),
                        "{width}x{height}: ({column}, {row}) resolved outside the \
                         space the surface was laid out in"
                    );
                }
            }
        }
    }

    /// A board menu of `groups` areas, each holding `items` artifacts,
    /// offered as `trail` — empty for a list to pick from, or a step per
    /// level with the one being looked at marked.
    fn board(groups: usize, items: usize, trail: &[(&str, bool)]) -> View {
        let mut rows = Vec::new();
        for group in 0..groups {
            rows.push(NavigatorRow::Group {
                id: group * items,
                name: format!("Area {group}"),
                depth: 0,
                collapsed: false,
                icon: RowIcon::None,
            });
            rows.extend((0..items).map(|item| NavigatorRow::Item {
                id: group * items + item,
                name: format!("Artifact {group}{item}"),
                depth: 1,
                marker: Span::default(),
                marker_side: MarkerSide::Leading,
                detail: String::new(),
                selected: group == 0 && item == 0,
                icon: RowIcon::None,
            }));
        }
        View {
            title: vec![Span::new("Board", Role::Bright)],
            caption: Vec::new(),
            navigator: Some(Navigator {
                heading: String::new(),
                badge: String::new(),
                focused: false,
                anchor: None,
                choosing: None,
                menu: None,
                rows,
            }),
            content: Content::Lines {
                first: 0,
                heading: String::new(),
                scroll: 0,
                lines: Vec::new(),
                total: 0,
                caret: None,
            },
            footer: vec![Command::Close],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Board,
            trail: trail
                .iter()
                .map(|&(name, current)| TrailStep::new(name, current))
                .collect(),
        }
    }

    /// A selector that opens onto nothing but what is already on show is
    /// not a control: no mark that says it opens, and no press that does.
    #[test]
    fn a_selector_with_one_choice_neither_opens_nor_says_it_does() {
        let chevron = theme::glyph(Symbol::ChevronExpanded);
        let (drawn, hits) = draw_sized(&board(1, 1, &[]), 80, 20);
        let menu = drawn[MENU_ROW].clone();
        assert!(!menu.contains(&chevron), "no mark on either: {menu}");
        assert!(!menu.contains(" 1 "), "nor a count of one: {menu}");
        assert!(
            !hits
                .iter()
                .any(|(_, hit)| matches!(hit, ViewHit::ChooseGroup | ViewHit::ChooseItem)),
            "and neither can be pressed"
        );

        let (drawn, hits) = draw_sized(&board(2, 3, &[]), 80, 20);
        assert_eq!(
            drawn[MENU_ROW].matches(chevron.as_str()).count(),
            2,
            "both open where there is a choice: {}",
            drawn[MENU_ROW]
        );
        assert!(
            hits.iter().any(|(_, h)| *h == ViewHit::ChooseGroup)
                && hits.iter().any(|(_, h)| *h == ViewHit::ChooseItem)
        );
    }

    /// The second half of the menu is one control or the other, never
    /// both: an area that descends is walked, and one that does not is
    /// picked from. Which it is, is the view's answer, not the host's.
    #[test]
    fn an_area_that_descends_is_walked_and_one_that_does_not_is_picked_from() {
        let chevron = theme::glyph(Symbol::ChevronExpanded);
        let (drawn, hits) = draw_sized(&board(2, 3, &[]), 100, 20);
        assert_eq!(
            drawn[MENU_ROW].matches(chevron.as_str()).count(),
            2,
            "a set is two lists: {}",
            drawn[MENU_ROW]
        );
        assert!(hits.iter().any(|(_, h)| *h == ViewHit::ChooseItem));

        // Standing on the middle level of three: the one behind is the
        // way back, the one ahead is a level this descent reaches.
        let ladder = board(
            2,
            3,
            &[
                ("Context", false),
                ("Containers", true),
                ("Components", false),
            ],
        );
        let (drawn, hits) = draw_sized(&ladder, 100, 20);
        let menu = drawn[MENU_ROW].clone();
        assert!(
            menu.contains("Context") && menu.contains("Containers") && menu.contains("Components"),
            "every level is on show at once: {menu}"
        );
        assert_eq!(
            menu.matches(chevron.as_str()).count(),
            1,
            "and only the area is still a list: {menu}"
        );
        assert!(
            !hits.iter().any(|(_, h)| *h == ViewHit::ChooseItem),
            "a descent is walked, not opened"
        );
        assert!(
            hits.iter().any(|(_, h)| *h == ViewHit::SelectTrail(0))
                && hits.iter().any(|(_, h)| *h == ViewHit::SelectTrail(2)),
            "both directions are a place to go: {hits:?}"
        );
        assert!(
            !hits.iter().any(|(_, h)| *h == ViewHit::SelectTrail(1)),
            "except where the viewer already is"
        );
    }

    /// A descent too long for the row is cut around the step the viewer
    /// is on — the one step that must never be the one cut off.
    #[test]
    fn a_descent_wider_than_the_row_keeps_the_step_it_is_standing_on() {
        let steps: Vec<(String, bool)> = (0..12)
            .map(|level| (format!("Level number {level}"), level == 7))
            .collect();
        let borrowed: Vec<(&str, bool)> = steps
            .iter()
            .map(|(name, current)| (name.as_str(), *current))
            .collect();
        let (drawn, _) = draw_sized(&board(2, 3, &borrowed), 70, 20);
        let menu = drawn[MENU_ROW].clone();
        assert!(menu.contains("Level number 7"), "{menu}");
        assert!(
            menu.contains(&theme::glyph(Symbol::Ellipsis)),
            "cut: {menu}"
        );
    }

    /// A group with more items than the board has rows: the list shows the
    /// rows around the highlighted one and says where in the list that is,
    /// so its last row is not mistaken for the last there is.
    #[test]
    fn a_list_longer_than_the_board_keeps_the_highlight_in_view_and_says_where_it_is() {
        let mut rows = vec![NavigatorRow::Group {
            id: 0,
            name: "Flowchart".to_owned(),
            depth: 0,
            collapsed: false,
            icon: RowIcon::None,
        }];
        rows.extend((0..30).map(|id| NavigatorRow::Item {
            id,
            name: format!("Flow {id:02}"),
            depth: 1,
            marker: Span::default(),
            marker_side: MarkerSide::Leading,
            detail: String::new(),
            selected: id == 0,
            icon: RowIcon::None,
        }));
        let view = View {
            title: vec![Span::new("Board", Role::Bright)],
            caption: Vec::new(),
            navigator: Some(Navigator {
                heading: String::new(),
                badge: String::new(),
                focused: false,
                anchor: None,
                choosing: Some(Choosing::Item(20)),
                menu: None,
                rows,
            }),
            content: Content::Lines {
                first: 0,
                heading: String::new(),
                scroll: 0,
                lines: Vec::new(),
                total: 0,
                caret: None,
            },
            footer: vec![Command::Close],
            notice: None,
            confirm: None,
            modes: Vec::new(),
            subjects: Vec::new(),
            layout: ViewLayout::Board,
            trail: Vec::new(),
        };
        let (drawn, hits) = draw_sized(&view, 80, 20);
        let text = drawn.join("\n");
        assert!(text.contains("Flowchart 30"), "{text}");
        assert!(
            text.contains("Flow 20") && !text.contains("Flow 02"),
            "{text}"
        );
        assert!(text.contains("21/30"), "{text}");
        let first_hit = hits
            .iter()
            .find(|(_, hit)| matches!(hit, ViewHit::SelectItem(_)))
            .map(|(_, hit)| *hit);
        assert_ne!(
            first_hit,
            Some(ViewHit::SelectItem(0)),
            "the rows cut off are not clickable"
        );
    }

    /// A board is drawn as the extension cut it: the screen it was told
    /// about is the screen it gets, so no row is folded and the last
    /// column is still the drawing's. Then the click: the row and column
    /// the host resolves must land in the box drawn there.
    #[test]
    fn a_board_is_drawn_as_it_was_cut_and_a_click_lands_in_the_box_under_it() {
        use uze_extensions::architect;
        let (width, height) = (150, 45);
        let space = board_space(Rect::new(0, 0, width, height));
        let mut state = architect::ArchitectView::opening("~/project".to_owned());
        let artifacts: Vec<architect::Artifact> = [
            (
                "containers.mmd",
                include_str!("../../docs/architecture/containers.mmd"),
            ),
            (
                "system-context.mmd",
                include_str!("../../docs/architecture/system-context.mmd"),
            ),
            (
                "install-sequence.mmd",
                include_str!("../../docs/architecture/install-sequence.mmd"),
            ),
            (
                "crate-layering.mmd",
                include_str!("../../docs/architecture/crate-layering.mmd"),
            ),
            (
                "install-pipeline.mmd",
                include_str!("../../docs/architecture/install-pipeline.mmd"),
            ),
        ]
        .map(|(origin, source)| architect::Artifact::read(origin, source))
        .into();
        state.absorb(architect::ArtifactsAnswer {
            branch: "main".to_owned(),
            artifacts: architect::Artifacts::Found {
                artifacts,
                project: std::path::PathBuf::from("/project"),
            },
        });
        // The catalog opens on the outermost view; the box this clicks is a
        // container, one level in.
        architect::handle_mouse(&mut state, Some(ViewHit::SelectItem(1)), space);
        let (rows, hits) = draw_sized(&architect::view(&state, space), width, height);
        if std::env::var_os("UZE_SHOW_BOARD").is_some() {
            println!("{}", rows.join("\n"));
        }
        assert!(
            rows[MENU_ROW].contains("C4")
                && !rows[MENU_ROW].contains("C4 2")
                && rows[MENU_ROW].contains("System context")
                && rows[MENU_ROW].contains("Containers"),
            "one row: the area on show, then its levels, the one on show among them: {}",
            rows[MENU_ROW]
        );
        assert!(
            !rows[MENU_ROW].contains("Sequence") && !rows[MENU_ROW].contains("Install"),
            "and nothing of any other area: {}",
            rows[MENU_ROW]
        );
        let footer = rows[usize::from(height) - 1].as_str();
        assert!(
            footer.contains("o artifacts")
                && footer.contains("tab next artifact")
                && footer.contains("containers.mmd"),
            "the footer names the board's own keys: {footer}"
        );

        let title_row = rows
            .iter()
            .position(|row| row.contains("Workspace TUI"))
            .expect("the box is drawn") as u16;
        let drawn = &rows[usize::from(title_row)];
        let title_column = drawn[..drawn.find("Workspace TUI").unwrap()]
            .chars()
            .count() as u16;
        let (rect, hit) = hits
            .iter()
            .find(|(rect, hit)| {
                matches!(hit, ViewHit::PlaceCaret { .. })
                    && rect.y == title_row
                    && rect.x <= title_column
                    && title_column < rect.x + rect.width
            })
            .expect("the row is a click target");
        let ViewHit::PlaceCaret { line, cell } = *hit else {
            unreachable!()
        };
        architect::handle_mouse(
            &mut state,
            Some(ViewHit::PlaceCaret {
                line,
                cell: cell + usize::from(title_column - rect.x),
            }),
            space,
        );
        let Content::Lines { heading, .. } = architect::view(&state, space).content else {
            panic!("a diagram is lines");
        };
        assert!(heading.starts_with("Workspace TUI"), "{heading}");
    }

    /// The whole point of the contract: an extension names a row, the host
    /// decides where it went, so the host is the only side that can answer
    /// a click.
    #[test]
    fn a_click_target_comes_from_what_the_host_drew() {
        let (rows, hits) = draw(&sample());
        // Not just any row mentioning the file: the content heading names
        // it too.
        let item_row = rows
            .iter()
            .position(|row: &String| row.contains("ui.rs") && !row.contains("DIFF"))
            .expect("the item is drawn") as u16;
        let hit = hits
            .iter()
            .find(|(_, hit)| matches!(hit, ViewHit::SelectItem(7)))
            .expect("the item is clickable by the id the extension gave it");
        assert_eq!(
            hit.0.y, item_row,
            "the hit must sit on the row the host actually drew"
        );
        assert!(
            hits.iter()
                .any(|(_, hit)| *hit == ViewHit::GrabNavigatorEdge),
            "the edge is one target for both of its jobs"
        );
    }

    /// An extension says what a row *is*; the vocabulary says what that
    /// looks like. So the icon appears only where the active set draws
    /// one, and takes no column where it does not — which is every
    /// built-in set but `nerd`, because plain Unicode has no folder mark
    /// that is not an emoji.
    ///
    /// Asserted at the seam rather than on a frame drawn under a swapped
    /// theme: the active theme is process-wide, and a test that put `nerd`
    /// in force to photograph a row would change the glyphs — and with the
    /// declared widths, the column positions — under every other test
    /// drawing at that moment. Which set declares which glyph is
    /// `uze-theme`'s own question, and it answers it in
    /// `every_nerd_glyph_is_an_icon_a_patched_font_supplies`.
    #[test]
    fn a_rows_icon_comes_from_the_set_and_takes_no_column_when_there_is_none() {
        // Every kind names a symbol, so a kind added later cannot quietly
        // draw nothing by falling through.
        for icon in [
            RowIcon::Directory,
            RowIcon::DirectoryOpen,
            RowIcon::File,
            RowIcon::Code,
            RowIcon::Markup,
            RowIcon::Config,
            RowIcon::Lock,
            RowIcon::Data,
            RowIcon::Image,
            RowIcon::Archive,
            RowIcon::Git,
            RowIcon::Legal,
        ] {
            let symbol = icon_symbol(icon).unwrap_or_else(|| panic!("{icon:?} names no symbol"));
            // And the set that has icons draws every one of them, in one
            // cell — resolved here rather than put in force.
            let mut layers = vec![uze_theme::default_file()];
            layers.extend(uze_theme::glyph_set_file("nerd"));
            let nerd = uze_theme::resolve_stack(
                &uze_theme::Identity::from_file("nerd", uze_theme::default_file()),
                &layers,
            )
            .expect("the bundled nerd set resolves")
            .theme;
            let drawn = nerd.symbol(symbol);
            assert!(
                !drawn.glyph().trim().is_empty(),
                "the nerd set draws nothing for {icon:?}"
            );
            assert_eq!(drawn.width(), 1, "{icon:?} is not one cell wide");

            // The default draws none of them, and the row holds no column
            // open for what is not there.
            assert!(
                uze_theme::default_theme().glyph(symbol).trim().is_empty(),
                "the default set grew a {icon:?} glyph — it has no column for one"
            );
        }
        assert!(row_icon(RowIcon::None).is_none());
        assert!(
            row_icon(RowIcon::Code).is_none(),
            "a blank glyph must take no column at all"
        );
    }

    /// A rendered document is the one content with no gutter, and the
    /// gutter is where every other mode's left margin quietly came from.
    /// Without a margin of its own, a wrapped paragraph runs into both
    /// borders.
    #[test]
    fn unnumbered_content_is_inset_where_numbered_content_leans_on_its_gutter() {
        let prose = |text: &str| ContentLine {
            gutter: String::new(),
            number: String::new(),
            tone: LineTone::Neutral,
            spans: vec![Span {
                text: text.to_owned(),
                role: Role::Default,
                color: None,
                ground: None,
                bold: false,
                italic: false,
            }],
        };

        let mut view = sample();
        let Content::Lines { lines, heading, .. } = &mut view.content else {
            unreachable!("the sample is Lines")
        };
        *lines = vec![prose("PROSE")];
        *heading = "HEADING".to_owned();
        let (rows, _) = draw(&view);

        // Measured against the heading rather than the frame, because the
        // heading is drawn at the content area's own left edge — so the
        // difference is the margin and nothing else. In *columns*: the
        // frame's own rules are multi-byte, so a byte offset is not where
        // the terminal put anything.
        let column_of = |needle: &str| -> usize {
            rows.iter()
                .find_map(|row: &String| row.find(needle).map(|byte| row[..byte].chars().count()))
                .unwrap_or_else(|| panic!("`{needle}` is drawn"))
        };
        assert_eq!(
            column_of("PROSE") - column_of("HEADING"),
            usize::from(PROSE_INSET),
            "prose must be inset from the edge its own heading sits on"
        );

        // And the numbered case is untouched — its gutter is the margin.
        let (numbered, _) = draw(&sample());
        let row = numbered
            .iter()
            .find(|row: &&String| row.contains("let x = 1;"))
            .expect("the code line is drawn");
        assert!(
            row.contains("12"),
            "the gutter still carries the number: {row:?}"
        );
    }

    /// A scrollbar is drawn only when there is something to scroll, and
    /// the column it takes comes out of the content rather than sitting
    /// on top of it.
    #[test]
    fn a_scrollbar_appears_only_when_the_content_outgrows_the_frame() {
        let mut view = sample();
        let Content::Lines { lines, total, .. } = &mut view.content else {
            unreachable!("the sample shows lines");
        };
        *total = lines.len();

        let (_rows, hits) = draw(&view);
        assert!(
            !hits
                .iter()
                .any(|(_, hit)| *hit == ViewHit::DragContentScrollbar),
            "one line in a tall frame has nowhere to scroll to"
        );

        let Content::Lines { total, .. } = &mut view.content else {
            unreachable!("the sample shows lines");
        };
        *total = 500;
        let (_rows, hits) = draw(&view);
        let track = hits
            .iter()
            .find(|(_, hit)| *hit == ViewHit::DragContentScrollbar)
            .expect("five hundred lines in a short frame is a scrollbar")
            .0;
        assert_eq!(track.width, crate::ui::widget::Scrollbar::width());
        assert!(
            track.height > 1,
            "the whole groove is the target, not just the handle"
        );
        // The complaint this answers: a groove a column short of the edge
        // and a divider beside it read as two controls arguing.
        let (_navigator, content, _footer) = content_columns(Rect::new(0, 0, 90, 14), Some(24));
        assert_eq!(
            track.x,
            content.right(),
            "the groove hugs the edge rather than leaving a gap beside it"
        );
    }

    /// A mode the keyboard can reach has to be one a pointer can reach,
    /// drawn where the thing it changes is.
    #[test]
    fn the_modes_a_surface_offers_are_a_control_on_its_heading_row() {
        let mut view = sample();
        view.modes = vec![
            Mode {
                label: "Preview".to_owned(),
                active: false,
                icon: RowIcon::None,
            },
            Mode {
                label: "Source".to_owned(),
                active: true,
                icon: RowIcon::None,
            },
        ];
        let (rows, hits) = draw(&view);

        let segments: Vec<(Rect, usize)> = hits
            .iter()
            .filter_map(|(rect, hit)| match hit {
                ViewHit::SelectMode(index) => Some((*rect, *index)),
                _ => None,
            })
            .collect();
        assert_eq!(
            segments.len(),
            2,
            "both are offered, not just the other one"
        );
        assert_eq!(segments[0].1, 0);
        assert!(
            segments[0].0.x < segments[1].0.x,
            "in the order the extension gave them"
        );

        let heading_row = rows
            .iter()
            .position(|row: &String| row.contains("DIFF"))
            .expect("the heading is drawn") as u16;
        assert_eq!(
            segments[0].0.y, heading_row,
            "beside the heading of what they change, not in the footer"
        );
        assert!(
            rows[heading_row as usize].contains("Preview")
                && rows[heading_row as usize].contains("Source"),
            "and both labels are legible: {}",
            rows[heading_row as usize]
        );
    }

    /// A surface with one way of showing itself offers no choice, and the
    /// heading gets the whole row back.
    #[test]
    fn a_surface_with_one_mode_draws_no_control() {
        let (_rows, hits) = draw(&sample());
        assert!(
            !hits
                .iter()
                .any(|(_, hit)| matches!(hit, ViewHit::SelectMode(_)))
        );
    }

    /// A rendered document has no line numbers, so it gets its left
    /// margin back rather than being indented by a column reserved for
    /// nothing.
    #[test]
    fn unnumbered_lines_are_not_indented_by_an_empty_gutter() {
        let numbered = [ContentLine {
            gutter: "+".to_owned(),
            number: "12".to_owned(),
            tone: LineTone::Added,
            spans: vec![Span::new("code", Role::Default)],
        }];
        let prose = [ContentLine {
            gutter: " ".to_owned(),
            number: String::new(),
            tone: LineTone::Neutral,
            spans: vec![Span::new("a paragraph", Role::Default)],
        }];

        assert_eq!(gutter_width(&numbered), GUTTER_WIDTH);
        assert_eq!(gutter_width(&prose), 0);
    }

    /// The caret marks the character it is on; it never replaces it.
    ///
    /// Drawing a mark into the cell is the obvious implementation and the
    /// wrong one: the letter being edited becomes the one letter the
    /// person cannot see. This is the test that says so.
    #[test]
    fn the_caret_marks_the_character_it_sits_on_without_hiding_it() {
        let mut view = sample();
        let Content::Lines { caret, lines, .. } = &mut view.content else {
            unreachable!("the sample shows lines");
        };
        *caret = Some(Caret { line: 0, column: 4 });
        let text = lines[0].spans[0].text.clone();
        let under_caret = text.chars().nth(4).expect("a character to sit on");

        let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                    },
                    uze_keys::Scope::Code,
                    &mut hits,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        let hit = hits
            .iter()
            .find(|(_, hit)| matches!(hit, ViewHit::PlaceCaret { line: 0, .. }))
            .expect("a drawn content row can be clicked to place the caret");
        let (x, y) = (hit.0.x + GUTTER_WIDTH + 4, hit.0.y);

        assert_eq!(
            buffer[(x, y)].symbol(),
            under_caret.to_string(),
            "the character under the caret is still on screen"
        );
        assert_eq!(
            buffer[(x, y)].bg,
            theme::color(Token::Accent),
            "and it is marked by inverting its cell"
        );
    }

    fn code_line(text: &str) -> ContentLine {
        ContentLine {
            gutter: " ".to_owned(),
            number: "1".to_owned(),
            tone: LineTone::Neutral,
            spans: vec![Span::new(text, Role::Default)],
        }
    }

    /// Draws `line` with the caret at `column` into a text column
    /// `width` cells wide, and answers what landed where.
    fn drawn_with_caret(line: &ContentLine, width: u16, column: usize) -> ratatui::buffer::Buffer {
        let area = Rect::new(0, 0, GUTTER_WIDTH + width, 4);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_line(frame, area, line, GUTTER_WIDTH, true);
                render_caret(frame, area, line, column, GUTTER_WIDTH);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// ratatui drops a tab as a control character, which drew every
    /// tab-indented file flush left and put the caret a tab short.
    #[test]
    fn a_tab_is_drawn_as_the_indentation_it_is() {
        let buffer = drawn_with_caret(&code_line("\tx"), 10, 1);
        let x = GUTTER_WIDTH + TAB_WIDTH as u16;
        assert_eq!(buffer[(x, 0)].symbol(), "x");
        assert_eq!(buffer[(x, 0)].bg, theme::color(Token::Accent));
    }

    /// Folded code breaks at the cell, not the word, and the caret is
    /// placed by the same walk: under word wrap the caret on a long
    /// line drifted off the character it named.
    #[test]
    fn the_caret_on_a_folded_line_sits_on_its_own_character() {
        let line = code_line("abcd efghijkl");
        assert_eq!(line_height(&line, GUTTER_WIDTH + 6, GUTTER_WIDTH), 3);
        let buffer = drawn_with_caret(&line, 6, 9);
        let (x, y) = (GUTTER_WIDTH + 3, 1);
        assert_eq!(buffer[(x, y)].symbol(), "i");
        assert_eq!(buffer[(x, y)].bg, theme::color(Token::Accent));
    }

    /// A click resolves to a text position through two halves that each
    /// know only their own side: the host counts cells from the row it
    /// drew, and the extension turns cells into characters.
    #[test]
    fn a_click_resolves_to_the_cell_it_landed_on() {
        let view = sample();
        let (_rows, hits) = draw(&view);
        let (rect, hit) = hits
            .iter()
            .find(|(_, hit)| matches!(hit, ViewHit::PlaceCaret { line: 0, .. }))
            .expect("the first content line is clickable");
        let ViewHit::PlaceCaret { cell, .. } = hit else {
            unreachable!("matched above");
        };

        assert_eq!(
            caret_cell_at(*rect, *cell, rect.x + GUTTER_WIDTH + 6, GUTTER_WIDTH),
            6,
            "six cells past the start of the text is six cells into the line"
        );
        assert_eq!(
            caret_cell_at(*rect, *cell, rect.x, GUTTER_WIDTH),
            0,
            "a click on the gutter belongs to the start of the line, not past it"
        );
        // Content that is not numbered has no gutter, and a click on it
        // resolved as though it had one landed seven cells to the left of
        // the pointer — on the map, a tile or two over.
        assert_eq!(
            caret_cell_at(*rect, *cell, rect.x + 6, 0),
            6,
            "with no gutter, the text starts where the row does"
        );
    }

    /// Chrome resolves through the palette; content keeps the colour it
    /// brought. An extension that could paint its own chrome is an
    /// extension that drifts from the design system.
    #[test]
    fn chrome_uses_the_hosts_palette_and_content_keeps_its_own() {
        // Reads the active theme, so it takes its turn with the test that
        // swaps it.
        let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    &sample(),
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll: NavigatorScroll::default(),
                        resizing: false,
                    },
                    uze_keys::Scope::Code,
                    &mut Vec::new(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // By cell, never by byte offset: the border glyphs are multi-byte,
        // so a byte index into the joined row is not a column.
        let cell_at = |needle: &str| {
            (0..buffer.area.height).find_map(|row| {
                let cells: Vec<String> = (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol().to_owned())
                    .collect();
                let wanted: Vec<String> = needle
                    .chars()
                    .map(|character| character.to_string())
                    .collect();
                cells
                    .windows(wanted.len())
                    .position(|window| window == wanted.as_slice())
                    .map(|column| buffer[(column as u16, row)].clone())
            })
        };
        assert_eq!(
            cell_at("let x = 1;").unwrap().fg,
            Color::Rgb(1, 2, 3),
            "syntax colour is the extension's own data"
        );
        assert_eq!(
            cell_at("CHANGES").unwrap().fg,
            theme::color(Token::TextSecondary),
            "a heading is chrome, so it resolves through the palette"
        );
        assert_eq!(
            cell_at("M ui.rs").unwrap().bg,
            theme::color(Token::SurfaceSelected),
            "the selected row carries the surface every other list marks its selection with"
        );
        assert_eq!(
            cell_at("M ").unwrap().fg,
            theme::color(Token::StateWarning),
            "Role::Warning"
        );
    }

    /// Wrapping is the host's, so the row a long line occupies is too —
    /// this used to be asserted inside the extension, which could only
    /// guess at the column width.
    #[test]
    fn a_line_too_long_for_the_column_occupies_more_than_one_row() {
        let line = ContentLine {
            gutter: " ".to_owned(),
            number: "1".to_owned(),
            tone: LineTone::Neutral,
            spans: vec![Span::new("abcdefgh", Role::Default)],
        };
        assert_eq!(line_height(&line, GUTTER_WIDTH + 4, GUTTER_WIDTH), 2);
        assert_eq!(line_height(&line, GUTTER_WIDTH + 8, GUTTER_WIDTH), 1);
    }

    /// A group folds from its own row, and says so with the same mark the
    /// sidebar's sections use.
    #[test]
    fn a_group_row_is_a_fold_target() {
        let mut view = sample();
        if let Some(navigator) = view.navigator.as_mut()
            && let NavigatorRow::Group { collapsed, .. } = &mut navigator.rows[0]
        {
            *collapsed = true;
        }
        let (rows, hits) = draw(&view);
        let group_row = rows
            .iter()
            .position(|row: &String| row.contains("src/") && !row.contains("DIFF"))
            .expect("the group is drawn") as u16;
        let hit = hits
            .iter()
            .find(|(_, hit)| matches!(hit, ViewHit::ToggleGroup(0)))
            .expect("the group folds by the id the extension gave it");
        assert_eq!(hit.0.y, group_row);
        assert!(
            rows[group_row as usize].contains(&theme::glyph(Symbol::ChevronCollapsed)),
            "{:?}",
            rows[group_row as usize]
        );
    }

    fn tall_navigator(anchor: Option<usize>) -> View {
        View {
            navigator: Some(Navigator {
                anchor,
                rows: (0..40)
                    .map(|index| NavigatorRow::Item {
                        icon: RowIcon::None,
                        id: index,
                        name: format!("file-{index}.rs"),
                        depth: 0,
                        marker: Span::new("M", Role::Warning),
                        marker_side: MarkerSide::Leading,
                        detail: String::new(),
                        selected: Some(index) == anchor,
                    })
                    .collect(),
                ..sample().navigator.unwrap()
            }),
            ..sample()
        }
    }

    fn drawn_with(view: &View, scroll: NavigatorScroll) -> (Vec<String>, NavigatorScroll) {
        let mut terminal = Terminal::new(TestBackend::new(90, 14)).unwrap();
        let mut settled = NavigatorScroll::default();
        terminal
            .draw(|frame| {
                settled = render(
                    frame,
                    view,
                    frame.area(),
                    NavigatorFrame {
                        width: Some(24),
                        scroll,
                        resizing: false,
                    },
                    uze_keys::Scope::Code,
                    &mut Vec::new(),
                )
                .navigator_scroll;
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol())
                    .collect()
            })
            .collect();
        (rows, settled)
    }

    /// The list scrolls where the wheel put it, and comes back to the
    /// selection only when the selection moves — a wheel looking at rows
    /// far from it is not pulled back every frame.
    #[test]
    fn the_list_follows_the_anchor_only_when_it_changes() {
        let view = tall_navigator(Some(30));

        let (rows, settled) = drawn_with(&view, NavigatorScroll::default());
        assert!(
            rows.iter().any(|row| row.contains("file-30.rs")),
            "a new anchor is brought on screen: {rows:?}"
        );
        assert!(settled.first > 0);
        assert_eq!(settled.revealed, Some(30));

        let scrolled_away = NavigatorScroll {
            first: 0,
            ..settled
        };
        let (rows, settled) = drawn_with(&view, scrolled_away);
        assert!(
            rows.iter().any(|row| row.contains("file-0.rs")),
            "the same anchor does not pull the list back: {rows:?}"
        );
        assert_eq!(settled.first, 0);

        let (_, settled) = drawn_with(
            &view,
            NavigatorScroll {
                first: 500,
                ..settled
            },
        );
        assert!(
            settled.first < 40,
            "held to the rows that exist, so the wheel back is not a long way: {settled:?}"
        );
    }

    #[test]
    fn a_view_without_a_navigator_leaves_the_column_empty() {
        let view = View {
            navigator: None,
            content: Content::Message {
                text: "not a git repository".to_owned(),
                hint: None,
                role: Role::Danger,
            },
            ..sample()
        };
        let (rows, _) = draw(&view);
        assert!(rows.iter().any(|row| row.contains("not a git repository")));
        assert!(
            !rows.iter().any(|row| row.contains("CHANGES")),
            "nothing to navigate means no list, not an empty one: {rows:?}"
        );
    }

    fn draw_message(text: &str, hint: Option<&str>, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                render_message(
                    frame,
                    frame.area(),
                    text,
                    hint,
                    theme::color(Token::TextMuted),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|row| {
                (0..buffer.area.width)
                    .map(|column| buffer[(column, row)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect()
    }

    /// An empty state sits in the middle of its pane, however many lines
    /// its hint wraps onto.
    #[test]
    fn a_message_is_centred_on_its_own_height() {
        let rows = draw_message(
            "No file selected",
            Some("Pick one on the left to read it, or press e to edit it in place."),
            100,
            21,
        );
        let drawn: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.trim().is_empty())
            .map(|(index, _)| index)
            .collect();
        let above = drawn[0];
        let below = rows.len() - 1 - drawn[drawn.len() - 1];
        assert!(
            above.abs_diff(below) <= 1,
            "{above} above, {below} below: {rows:#?}"
        );
    }

    #[test]
    fn a_hint_wraps_to_a_reading_measure_under_a_capitalised_heading() {
        let rows = draw_message(
            "No file selected",
            Some("Pick one on the left to read it, or press e to edit it in place."),
            120,
            12,
        );
        assert!(
            rows.iter().any(|row| row.contains("NO FILE SELECTED")),
            "{rows:#?}"
        );
        let hint: Vec<&String> = rows
            .iter()
            .filter(|row| {
                let row = row.trim();
                !row.is_empty() && row != "NO FILE SELECTED"
            })
            .collect();
        assert!(hint.len() >= 2, "the hint breaks into lines: {rows:#?}");
        assert!(
            hint.iter()
                .all(|row| row.trim().chars().count() <= usize::from(MESSAGE_MEASURE)),
            "{hint:#?}"
        );
    }

    #[test]
    fn a_message_without_a_hint_keeps_its_case() {
        let rows = draw_message("not a git repository", None, 60, 5);
        assert!(rows.iter().any(|row| row.contains("not a git repository")));
    }

    /// A file with nothing to report sits beside folders at its depth, and
    /// its name starts where theirs do — its empty marker keeps the column
    /// the folders' fold mark takes.
    #[test]
    fn an_item_without_a_marker_lines_up_with_the_groups_beside_it() {
        let view = View {
            navigator: Some(Navigator {
                heading: "FILES".to_owned(),
                badge: String::new(),
                focused: true,
                anchor: None,
                choosing: None,
                menu: None,
                rows: vec![
                    NavigatorRow::Group {
                        id: 0,
                        name: "src".to_owned(),
                        depth: 0,
                        collapsed: true,
                        icon: RowIcon::Directory,
                    },
                    NavigatorRow::Item {
                        id: 1,
                        name: "Cargo.toml".to_owned(),
                        depth: 0,
                        marker: Span::new(String::new(), Role::Muted),
                        marker_side: MarkerSide::Leading,
                        detail: String::new(),
                        selected: false,
                        icon: RowIcon::Config,
                    },
                    NavigatorRow::Item {
                        id: 2,
                        name: "main.rs".to_owned(),
                        depth: 0,
                        marker: Span::new("M", Role::Warning),
                        marker_side: MarkerSide::Leading,
                        detail: String::new(),
                        selected: false,
                        icon: RowIcon::Code,
                    },
                ],
            }),
            ..sample()
        };
        let (rows, _) = draw(&view);
        let column = |name: &str| {
            rows.iter()
                .find_map(|row| {
                    // The navigator column only: the content beside it
                    // names files too.
                    let cells: Vec<char> = row.chars().take(24).collect();
                    let first = name.chars().next().unwrap();
                    (0..cells.len()).find(|&start| {
                        cells[start] == first
                            && cells[start..]
                                .iter()
                                .take(name.chars().count())
                                .copied()
                                .eq(name.chars())
                    })
                })
                .unwrap_or_else(|| panic!("{name} is drawn: {rows:#?}"))
        };
        assert_eq!(column("Cargo.toml"), column("src"), "{rows:#?}");
        assert_eq!(column("main.rs"), column("src"), "{rows:#?}");
    }
}
