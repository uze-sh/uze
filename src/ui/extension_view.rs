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
    widgets::{Clear, Padding, Paragraph},
};
use uze_extensions::view::{
    Caret, Choosing, Command, Content, ContentLine, Layout as ViewLayout, LineTone, MarkerSide,
    Medium, Mode, Navigator, NavigatorRow, PROSE_INSET, PanDirection, Role, RowIcon, RowMark,
    ScrollTarget, Section, Size, Span, TAB_WIDTH, TEXT_PADDING, TrailStep, View, ViewHit,
};

use crate::ui::selection::{Glyph, TextRow, TextSelection};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::{
    self, Edge, Rule, Scrollbar, Surface, TRAILING_PAD, hint, mark, row, text,
};

mod footer;
mod geometry;
mod lines;
mod navigator;
mod prose;
mod section;

pub(crate) use footer::*;
pub(crate) use geometry::*;
pub(crate) use lines::*;
use navigator::*;
pub(crate) use prose::*;
pub(crate) use section::*;

/// Narrowest/widest the navigator can be dragged, and the floor left for
/// the content column — the same shape as the host TUI's own
/// `clamp_sidebar_width`, scoped to an extension overlay.
const MIN_NAVIGATOR_WIDTH: u16 = 20;
const MAX_NAVIGATOR_WIDTH: u16 = 50;
const MIN_EXTENSION_CONTENT_WIDTH: u16 = 40;

const GUTTER_WIDTH: u16 = 7;

/// Columns of padding on each side of a mode segment's label. The padding
/// is part of the button — it is filled, and clicked, like the label is.
const MODE_PAD: u16 = 1;
/// The gap between the hints on a footer's left and the caption on its
/// right, so the two never meet at the width where both are widest.
const FOOTER_GAP: u16 = 2;

/// What one frame of an extension surface left behind for the next event
/// to read: where its list settled, the two scrollbars it drew, and where
/// its text landed.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rendered {
    pub(crate) navigator_scroll: NavigatorScroll,
    /// Whether a row's name was drawn sliding, which is what keeps the
    /// host's clock turning for the next frame of it.
    pub(crate) marquee: bool,
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
    /// Whether the content's last line is on screen, so the wheel knows
    /// to stop. Only the drawing can say: lines wrap, and how many rows
    /// the last ones took is settled here and nowhere else.
    pub(crate) content_at_end: bool,
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
    /// Every row of text drawn, with the character in each of its cells:
    /// what a press is resolved against to start a selection, and a drag
    /// to extend one. Empty for a [`Medium::Drawing`], which is pointed
    /// at, never marked.
    pub(crate) text_rows: Vec<TextRow>,
    /// The heading of the content drawn, which names what a selection
    /// started on it was made in.
    pub(crate) heading: String,
}

/// Where the host holds the navigator this frame: the width it was
/// dragged to, where its list is scrolled, and whether its edge is being
/// dragged right now.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NavigatorFrame {
    pub(crate) width: Option<u16>,
    pub(crate) scroll: NavigatorScroll,
    pub(crate) resizing: bool,
    /// The row under the pointer and the clock its name slides by when it
    /// is too long for the column.
    pub(crate) sliding: Option<(ViewHit, usize)>,
}

pub(crate) fn render(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    area: Rect,
    held: NavigatorFrame,
    scope: uze_keys::Scope,
    selection: Option<&TextSelection>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rendered {
    let mut rendered = render_surface(frame, view, area, held, scope, selection, hits);
    // A question is drawn over the content by the caller, with a scrim
    // and nothing else to point at: what lies under it is not text a
    // press can start marking until it is answered.
    if view.confirm.is_some() {
        rendered.text_rows.clear();
    }
    rendered
}

fn render_surface(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    area: Rect,
    held: NavigatorFrame,
    scope: uze_keys::Scope,
    selection: Option<&TextSelection>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rendered {
    // The pane's own ground, and nothing drawn around it: the surface
    // stands where a shell would, and is put away the way one is left —
    // its button in the strip, `Esc`, or another tab — so it carries no
    // frame, no name and no close mark of its own.
    frame.render_widget(Clear, area);
    widget::fill(frame, area, Token::SurfaceBackground);
    if view.layout == ViewLayout::Board {
        let mut rendered = render_board(frame, view, area, scope, selection, hits);
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
    // What the surface is about is the workspace bar's to show (see
    // [`render_navigation`]); inside, only the ways of drawing what is on
    // show remain, at the end of the content's own heading row.
    let column_modes = &view.modes[..];

    let mut rendered = Rendered {
        navigator_scroll: held.scroll,
        ..Rendered::default()
    };
    if let Some(navigator) = view.navigator.as_ref() {
        let (settled, bar, slid) =
            render_navigator(frame, navigator_area, navigator, &view.subjects, held, hits);
        rendered.navigator_scroll = settled;
        rendered.navigator_bar = bar;
        rendered.marquee = slid;
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
            medium,
        } => {
            rendered.content_gutter = gutter_width(lines);
            rendered.heading = heading.clone();
            (
                rendered.content_bar,
                rendered.content_at_end,
                rendered.text_rows,
            ) = render_lines(
                frame,
                content_area,
                Lines {
                    heading,
                    scroll: *scroll,
                    first: *first,
                    lines,
                    total: *total,
                    caret: *caret,
                    medium: *medium,
                    selection,
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
        .find(|(_, hit)| {
            *hit == ViewHit::SelectItem(menu.row) || *hit == ViewHit::ToggleGroup(menu.row)
        })
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
        // A yes-or-no is asked only before what cannot be undone; a
        // question that takes text is asking for a name.
        tone: match confirm.field {
            Some(_) => widget::dialog::Tone::Neutral,
            None => widget::dialog::Tone::Danger,
        },
        title: &confirm.title,
        subject: Some(Line::from(confirm.subject.clone())),
        body: vec![confirm.body.clone()],
        confirm: Some(&confirm.confirm),
        focus: Some(match confirm.on_confirm {
            true => 1,
            false => widget::dialog::CANCEL,
        }),
        field: confirm
            .field
            .as_deref()
            .map(|text| widget::field::Field::new(text, "")),
    };
    let answers = widget::dialog::render(
        frame,
        area,
        &dialog,
        &[uze_keys::Scope::Global, uze_keys::Scope::Workspace, scope],
        ViewHit::Answer(false),
        ViewHit::Answer(true),
    );
    hits.splice(0..0, answers.buttons);
}

/// A [`ViewLayout::Board`]: the list as a row of tabs, and under it the
/// drawing, given every cell that is left and cut at the edge.
fn render_board(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    area: Rect,
    scope: uze_keys::Scope,
    selection: Option<&TextSelection>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rendered {
    let (menu, board, footer) = board_rows(area);
    let mut rendered = Rendered::default();
    // Where the board is and how to walk it are the workspace bar's (see
    // [`render_navigation`]); its own first row keeps only the ways of
    // drawing it.
    render_modes(frame, menu, &view.modes, hits);
    match &view.content {
        Content::Message { text, hint, role } => {
            render_message(frame, board, text, hint.as_deref(), color(*role));
        }
        Content::Lines {
            heading,
            scroll,
            first,
            lines,
            total,
            medium,
            ..
        } => {
            let gutter = gutter_width(lines);
            rendered.content_gutter = gutter;
            rendered.heading = heading.clone();
            let marked = selection.and_then(|selection| selection.marked_in(heading));
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
                // Cut at the edge rather than folded: the whole line is
                // one row, however far past the edge it runs.
                if *medium == Medium::Text {
                    let rows = text_rows(line, offset, rect, gutter, usize::MAX, Breaks::Cells);
                    if let Some(marked) = &marked {
                        crate::ui::selection::invert(frame, &rows, marked);
                    }
                    rendered.text_rows.extend(rows);
                }
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
            rendered.content_at_end = usize::from(*scroll) + usize::from(board.height) >= *total;
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
    rendered
}

/// What the surface is about and how to move through it, drawn in the
/// workspace bar's leading slot rather than inside the surface: the
/// subjects, and on a board the selector and the descent it walks.
///
/// Called after the surface is drawn, because a selector's list opens
/// over it; the caller puts these hits ahead of the surface's for the same
/// reason, so a click on that list never reaches the surface under it.
pub(crate) fn render_navigation(
    frame: &mut ratatui::Frame<'_>,
    view: &View,
    slot: Rect,
    surface: Rect,
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    render_subjects(frame, slot, &view.subjects, hits);
    if view.layout == ViewLayout::Board
        && let Some(navigator) = view.navigator.as_ref()
    {
        let after_subjects = match view.subjects.is_empty() {
            true => slot.x,
            false => slot.x.saturating_add(modes_width(&view.subjects) + 2),
        };
        let (_, board, _) = board_rows(surface);
        render_menu(
            frame,
            Rect::new(
                after_subjects,
                slot.y,
                slot.right().saturating_sub(after_subjects),
                1,
            ),
            board,
            navigator,
            &view.trail,
            hits,
        );
    }
}

#[cfg(test)]
mod tests;
