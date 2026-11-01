//! A surface's text body: its lines, the gutter, the caret, icons, modes, chips and the scrollbar.

use super::*;

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
pub(super) struct Lines<'a> {
    pub(super) heading: &'a str,
    pub(super) scroll: usize,
    /// Where `lines` starts in the whole content — see
    /// [`Content::Lines::first`]. Every index this draws with is an index
    /// into the content, never into the window.
    pub(super) first: usize,
    pub(super) lines: &'a [ContentLine],
    pub(super) total: usize,
    pub(super) caret: Option<Caret>,
    /// The line under the pointer, when a click there would place a
    /// caret: its number is lit, the hover of a line that answers one.
    pub(super) pointed: Option<usize>,
    /// The pressable span under the pointer, by line and span: lifted, as
    /// a button is.
    pub(super) pressing: Option<(usize, usize)>,
    pub(super) medium: Medium,
    pub(super) selection: Option<&'a TextSelection>,
    pub(super) modes: &'a [Mode],
}

pub(super) fn render_lines(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    content_lines: Lines<'_>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> (Option<Scrollbar>, bool, Vec<TextRow>) {
    let Lines {
        heading,
        scroll,
        first,
        lines,
        total,
        caret,
        pointed,
        pressing,
        medium,
        selection,
        modes,
    } = content_lines;
    let marked = selection.and_then(|selection| selection.marked_in(heading));
    let mut drawn = Vec::new();
    // One column in, where a tab's name starts over it.
    frame.render_widget(
        Paragraph::new(TextSpan::styled(
            heading.to_owned(),
            theme::fg(Token::TextSecondary),
        )),
        Rect::new(
            area.x.saturating_add(HEADING_INSET),
            area.y,
            area.width
                .saturating_sub(modes_width(modes))
                .saturating_sub(HEADING_INSET),
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
    let gutter = gutter_width(lines);
    // Unnumbered content is prose, and prose is the case the gutter was
    // silently paying for everywhere else.
    let inset = if gutter == 0 { PROSE_INSET } else { 0 };
    let padding = match medium {
        Medium::Text if body.height > TEXT_PADDING.saturating_mul(2) => TEXT_PADDING,
        _ => 0,
    };
    let padded = Rect::new(
        body.x,
        body.y.saturating_add(padding),
        body.width,
        body.height.saturating_sub(padding.saturating_mul(2)),
    );
    let content = if padded.width > inset.saturating_mul(2) {
        Rect::new(
            padded.x.saturating_add(inset),
            padded.y,
            padded.width.saturating_sub(inset.saturating_mul(2)),
            padded.height,
        )
    } else {
        padded
    };
    let bar = Scrollbar::measure(
        Rect::new(body.right(), body.y, Scrollbar::width(), body.height),
        content.height as usize,
        total,
    );
    let text_width = text_width(content.width, gutter);
    let mut y = content.y;
    let mut at_end = true;
    for (offset, line) in lines
        .iter()
        .enumerate()
        .map(|(offset, line)| (offset + first, line))
        .skip(scroll.saturating_sub(first))
    {
        let height = line_height(line, content.width, gutter);
        if y.saturating_add(height) > content.bottom() {
            at_end = false;
            break;
        }
        let row = Rect::new(content.x, y, content.width, height);
        let light = if caret.is_some_and(|caret| caret.line == offset) {
            LineLight::Caret
        } else if pointed == Some(offset) {
            LineLight::Pointed
        } else {
            LineLight::Rest
        };
        render_line(frame, row, line, gutter, true, light);
        if medium == Medium::Text {
            let rows = text_rows(
                line,
                offset,
                row,
                gutter,
                text_width,
                Breaks::for_gutter(gutter),
            );
            if let Some(marked) = &marked {
                crate::ui::selection::invert(frame, &rows, marked);
            }
            drawn.extend(rows);
        }
        // Words that act are targets of their own, ahead of the text's
        // hit, which the first match wins over: a press on them acts and
        // never starts a selection.
        for (span, cells) in pressable_cells(line, row, gutter, text_width) {
            if pressing == Some((offset, span)) {
                lift_cells(frame, cells);
            }
            hits.push((cells, ViewHit::Press { line: offset, span }));
        }
        // The number is about the line rather than its text: a press on
        // it marks the line, where one on the text would select text. Ahead
        // of the text's own hit, which the first match wins over.
        if medium == Medium::Text && gutter > 0 && !line.number.is_empty() {
            hits.push((
                Rect::new(row.x, row.y, gutter.min(row.width), 1),
                ViewHit::LineNumber {
                    line: offset,
                    extend: false,
                },
            ));
        }
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
    // The window handed over may stop short of the content: what it left
    // out is below the last row drawn, so the end is not on screen.
    let at_end = at_end && first + lines.len() >= total;
    let bar = render_scrollbar(frame, bar, scroll, hits, ViewHit::DragContentScrollbar);
    (bar, at_end, drawn)
}

/// Where each pressable span of `line` stands on its first row, by its
/// index among the line's spans. A span that would run past the first row
/// is not offered there: the pressable words of a line are its few
/// actions, drawn where they fit.
fn pressable_cells(
    line: &ContentLine,
    row: Rect,
    gutter: u16,
    text_width: usize,
) -> Vec<(usize, Rect)> {
    use unicode_width::UnicodeWidthStr as _;
    let mut at = 0usize;
    let mut cells = Vec::new();
    for (index, span) in line.spans.iter().enumerate() {
        let width = span.text.width();
        if span.pressable && width > 0 && at + width <= text_width {
            let x = row.x.saturating_add(gutter).saturating_add(at as u16);
            cells.push((index, Rect::new(x, row.y, width as u16, 1)));
        }
        at += width;
    }
    cells
}

/// The chip under the pointer, a mode of the content or a subject of the
/// bar, lifted in its own ink, as every control is: it answers a click,
/// so it answers the pointer passing over it.
pub(super) fn lift_pointed_chip(
    frame: &mut ratatui::Frame<'_>,
    hits: &[(Rect, ViewHit)],
    pointed: Option<ViewHit>,
) {
    let Some(pointed) =
        pointed.filter(|hit| matches!(hit, ViewHit::SelectMode(_) | ViewHit::SelectSubject(_)))
    else {
        return;
    };
    if let Some((rect, _)) = hits.iter().find(|(_, hit)| *hit == pointed) {
        lift_cells(frame, *rect);
    }
}

/// Draws the cells of `area` again in their own ink lifted, the hover of
/// words that act: colour only, the weight kept.
fn lift_cells(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let buffer = frame.buffer_mut();
    let area = area.intersection(buffer.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            cell.fg = theme::lifted_ink(cell.fg);
        }
    }
}

// What an empty surface says is the widget vocabulary's, shared with the
// workspace's overview.
pub(super) use crate::ui::widget::message::render_message;

/// What a row's kind is called in the vocabulary.
///
/// Separate from drawing it so the mapping can be checked without a theme
/// in force: putting one there is process-wide, and a test that did it
/// would swap the glyphs under every neighbour drawing at the same moment.
pub(super) fn icon_symbol(icon: RowIcon) -> Option<Symbol> {
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
        RowIcon::Decision => Symbol::Decision,
        RowIcon::Viewed => Symbol::MarkViewed,
        RowIcon::NotViewed | RowIcon::ViewedThenChanged => Symbol::MarkUnviewed,
    })
}

/// The glyph a section row's mark is drawn as, with the cells it takes.
pub(super) fn row_mark(mark: RowMark) -> (String, u16) {
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
pub(super) fn row_icon(icon: RowIcon) -> Option<TextSpan<'static>> {
    // Muted on purpose: the icon classifies, the name identifies, and an
    // icon drawn at the name's weight competes with the thing the reader
    // is actually scanning for. A checklist's box is the exception: its
    // hue is what a reader working down the list reads.
    let token = match icon {
        RowIcon::Viewed => Token::StateSuccess,
        RowIcon::ViewedThenChanged => Token::StateWarning,
        _ => Token::TextDim,
    };
    Some(icon_span(icon)?.style(theme::fg(token)))
}

/// The mark and the space after it, or nothing at all where the glyph
/// set has no icon for this kind. Unstyled: a row draws its marks muted,
/// a control draws its own in the weight the member is in.
pub(super) fn icon_span(icon: RowIcon) -> Option<TextSpan<'static>> {
    let glyph = theme::glyph(icon_symbol(icon)?);
    if glyph.trim().is_empty() {
        return None;
    }
    Some(TextSpan::raw(format!("{glyph} ")))
}

/// How much of the heading row the mode control takes, so the heading
/// itself is drawn shorter rather than under it.
pub(super) fn modes_width(modes: &[Mode]) -> u16 {
    modes.iter().map(mode_width).sum()
}

/// One member's cells: its label, its padding, and the mark before it
/// where the glyph set has one to draw.
pub(super) fn mode_width(mode: &Mode) -> u16 {
    let icon = icon_cells(mode.icon);
    TextSpan::raw(&mode.label).width() as u16 + icon + 2 * MODE_PAD
}

/// How many cells a mark takes, counted from what would be drawn rather
/// than from the meaning: a glyph set with no icon for it draws nothing,
/// and a column reserved for nothing is a gap the reader has to account
/// for.
pub(super) fn icon_cells(icon: RowIcon) -> u16 {
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
pub(super) fn render_modes(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    modes: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let x = area.right().saturating_sub(modes_width(modes));
    render_chips(frame, x, area.y, modes, hits, ViewHit::SelectMode);
}

/// What the surface is about, as the workspace bar's leading slot shows
/// it: in lower case, the way every other label on that bar is set — the
/// agent, its shells, the surfaces' own buttons.
pub(super) fn render_subjects(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    subjects: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
) {
    let subjects: Vec<Mode> = subjects
        .iter()
        .map(|subject| Mode {
            label: subject.label.to_lowercase(),
            ..subject.clone()
        })
        .collect();
    render_chips(
        frame,
        area.x,
        area.y,
        &subjects,
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
pub(super) fn render_chips(
    frame: &mut ratatui::Frame<'_>,
    from: u16,
    y: u16,
    modes: &[Mode],
    hits: &mut Vec<(Rect, ViewHit)>,
    hit_of: impl Fn(usize) -> ViewHit,
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
pub(super) fn text_width(width: u16, gutter: u16) -> usize {
    usize::from(width.saturating_sub(gutter).max(1))
}

/// How wide the gutter is for these lines: nothing at all when none of
/// them is numbered.
///
/// A rendered document has no line numbers, and reserving the column
/// anyway indents the whole thing by seven cells of blank — which is
/// what the gutter looked like in preview, and what it cost was the
/// left margin of every paragraph.
///
/// Otherwise as wide as its numbers, and a marker's column only where the
/// lines carry one: a fixed seven cells left a file's `1` five blanks off
/// the edge.
pub(super) fn gutter_width(lines: &[ContentLine]) -> u16 {
    let numbered = lines
        .iter()
        .any(|line| !line.number.is_empty() || !line.gutter.trim().is_empty());
    if !numbered {
        return 0;
    }
    // As wide as the widest marker and the space after it: one column
    // for `+` or `-`, two where a comment's mark leads one.
    let widest = lines
        .iter()
        .map(|line| text::columns(&line.gutter) as u16)
        .max()
        .unwrap_or(0);
    let lead = match widest {
        0 => 1,
        widest => (widest + 1).max(MARKER_COLUMNS),
    };
    lead + number_columns(lines) + 1
}

/// The columns a gutter's line numbers are right-aligned in: the widest
/// number on show, never under [`MIN_NUMBER_COLUMNS`] so a file's first
/// thousand lines scroll without the text moving under them.
fn number_columns(lines: &[ContentLine]) -> u16 {
    let widest = lines
        .iter()
        .map(|line| line.number.chars().count())
        .max()
        .unwrap_or(0);
    u16::try_from(widest)
        .unwrap_or(u16::MAX)
        .max(MIN_NUMBER_COLUMNS)
}

/// Where a content heading starts: under a tab's name, past its lead.
pub(super) const HEADING_INSET: u16 = 1;

/// A marker and the space after it.
const MARKER_COLUMNS: u16 = 2;
const MIN_NUMBER_COLUMNS: u16 = 3;

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
pub(super) fn render_caret(
    frame: &mut ratatui::Frame<'_>,
    row: Rect,
    line: &ContentLine,
    column: usize,
    gutter: u16,
) {
    let width = text_width(row.width, gutter);
    let mut seen = 0usize;
    let mut at = None;
    let end = fold(
        line,
        width,
        Breaks::for_gutter(gutter),
        |row, cell, _, _| {
            if seen == column {
                at = Some((row, cell));
            }
            seen += 1;
        },
    );
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

/// Where each character of `line` landed in `row`, one [`TextRow`] per
/// screen row it took — found by the walk that drew them, so the pointer
/// is resolved against the rows a character is actually on. `width` is
/// how many cells the text was given before it folds, which for a
/// drawing that is cut at the edge rather than folded is all of them.
pub(super) fn text_rows(
    line: &ContentLine,
    index: usize,
    row: Rect,
    gutter: u16,
    width: usize,
    breaks: Breaks,
) -> Vec<TextRow> {
    let mut rows: Vec<TextRow> = Vec::new();
    let mut character = 0usize;
    let (last, _) = fold(line, width, breaks, |down, across, _, glyph| {
        while rows.len() <= down {
            rows.push(TextRow {
                area: Rect::new(row.x, row.y + rows.len() as u16, row.width, 1),
                line: index,
                ..TextRow::default()
            });
        }
        // Checked, because a line cut at the edge rather than folded is
        // walked to its end however far past the row it runs.
        let x = u16::try_from(across)
            .ok()
            .and_then(|across| (row.x + gutter).checked_add(across))
            .filter(|x| *x < row.right());
        if let Some(x) = x {
            rows[down].glyphs.push(Glyph {
                x,
                width: cell_width(glyph) as u16,
                index: character,
            });
        }
        character += 1;
    });
    if rows.is_empty() {
        rows.push(TextRow {
            area: Rect::new(row.x, row.y, row.width, 1),
            line: index,
            ..TextRow::default()
        });
    }
    for (down, text) in rows.iter_mut().enumerate() {
        text.beyond = match (down == last, text.glyphs.last()) {
            (false, Some(glyph)) => glyph.index,
            _ => character,
        };
    }
    rows.retain(|text| text.area.y < row.bottom());
    rows
}

/// Draws the groove for a surface, and makes the whole of it the drag
/// target.
///
/// The handle alone would be the obvious target and the wrong one:
/// clicking above or below it is how a pointer says "go there", and a
/// drag that wanders off the handle has to keep working.
pub(super) fn render_scrollbar(
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

pub(super) fn line_height(line: &ContentLine, width: u16, gutter: u16) -> u16 {
    let (row, _) = fold(
        line,
        text_width(width, gutter),
        Breaks::for_gutter(gutter),
        |_, _, _, _| {},
    );
    u16::try_from(row + 1).unwrap_or(u16::MAX)
}

/// How a line of text stands out from its neighbours, across the whole
/// row the way an editor marks one: the line being typed on reads as the
/// active one, and the one under the pointer lifts to say a click lands
/// there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LineLight {
    Rest,
    Pointed,
    Caret,
}

impl LineLight {
    fn number(self) -> Token {
        match self {
            Self::Rest => Token::TextDim,
            Self::Pointed => Token::TextSecondary,
            Self::Caret => Token::TextBright,
        }
    }

    /// The ground under the whole row. A diff's own tint wins over it:
    /// what changed is the line's meaning, the band only where it is.
    fn band(self) -> Option<Token> {
        match self {
            Self::Rest => None,
            Self::Pointed => Some(Token::SurfaceHover),
            Self::Caret => Some(Token::SurfaceSelected),
        }
    }
}

pub(super) fn render_line(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    line: &ContentLine,
    gutter: u16,
    wrapped: bool,
    light: LineLight,
) {
    let number = match line.marked {
        true => Token::TextBright,
        false => light.number(),
    };
    let (marker_style, background) = match line.tone {
        LineTone::Neutral => (theme::fg(Token::TextFaint), light.band().map(theme::color)),
        LineTone::Added => (
            theme::fg(Token::StateSuccess),
            Some(theme::color(Token::StateDiffAdded)),
        ),
        LineTone::Removed => (
            theme::fg(Token::StateDanger),
            Some(theme::color(Token::StateDiffRemoved)),
        ),
        LineTone::Card(_) => (
            theme::fg(Token::TextFaint),
            Some(theme::color(Token::SurfaceRaised)),
        ),
    };
    // A marked run is the reader's own, and it reads over the diff's wash:
    // the gold of a draft comment, the sign keeping the change's hue.
    let background = match line.marked {
        true => Some(theme::color(Token::StateDiffMarked)),
        false => background,
    };
    // A marked run is the whole line, its sign and number with it; a card
    // stands in the text's column, the numbers beside it on the content's
    // ground.
    let gutter_ground = match line.tone {
        LineTone::Card(_) if !line.marked => None,
        _ => background,
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
        let (commented, marker) = match line.gutter.strip_prefix(COMMENT_MARK) {
            Some(rest) => (true, rest),
            None => (false, line.gutter.as_str()),
        };
        let lead = match line.gutter.is_empty() {
            true => " ".to_owned(),
            false => format!("{marker} "),
        };
        let mut spans = Vec::new();
        if commented {
            spans.push(TextSpan::styled(
                COMMENT_MARK.to_string(),
                theme::fg(Token::StateWarning),
            ));
        }
        let taken = text::columns(&lead) + usize::from(commented);
        let digits = usize::from(gutter).saturating_sub(taken + 1);
        spans.push(TextSpan::styled(lead, marker_style));
        spans.push(TextSpan::styled(
            format!("{:>digits$} ", line.number),
            theme::fg(number),
        ));
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(
                Style::default()
                    .bg(gutter_ground.unwrap_or(theme::color(Token::SurfaceBackground))),
            ),
            columns[0],
        );
    }
    // A drawing is cut at the edge; prose breaks at its words and code at
    // the cell, both by the walk that places the caret and resolves the
    // pointer, so what is drawn is where everything else thinks it is.
    let lines = match wrapped {
        true => folded_rows(
            line,
            usize::from(columns[1].width),
            Breaks::for_gutter(gutter),
        )
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
    if let LineTone::Card(role) = line.tone {
        card_rule(frame, columns[1], role, background);
    }
}

/// The rule down a card's left, over the first cell of each of its rows:
/// what tells a comment from the code around it at a glance.
fn card_rule(frame: &mut ratatui::Frame<'_>, area: Rect, role: Role, ground: Option<Color>) {
    let mut style = Style::default().fg(super::prose::color(role));
    if let Some(ground) = ground {
        style = style.bg(ground);
    }
    let rule = theme::glyph(Symbol::BarMedium);
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        if buffer.area.contains((area.x, y).into()) {
            buffer[(area.x, y)].set_symbol(&rule).set_style(style);
        }
    }
}
