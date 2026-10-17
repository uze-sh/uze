//! A collapsible section drawn inside one of the host's own columns.

use super::*;

/// The rows of a section that are not its content: its header alone.
/// Whoever reserves a section's rows adds it.
pub(crate) const SECTION_CHROME: u16 = 1;

/// The blank row an open section keeps under its last row while another
/// section's header sits below it. Reserved by the host, never drawn: the
/// renderer runs out of rows before it reaches it.
pub(crate) const SECTION_AIR: u16 = 1;

/// What the host knows about a section while drawing it, and the section
/// itself may not hold: every field is the host's own state, because the
/// gestures belong to the host, not to whoever the section came from.
#[derive(Clone, Copy, Default)]
pub(crate) struct SectionState<'a> {
    /// Whether the header is being pulled right now.
    pub(crate) dragging: bool,
    /// The hit under the pointer and the clock a caption of it too long
    /// for its room slides by — the header's caption, or a row's name —
    /// instead of stopping at an "…".
    pub(crate) sliding: Option<(ViewHit, usize)>,
    /// The rows lifted to the bright text the row receiving keystrokes
    /// wears: the one under the pointer, since a row that opens something
    /// on a click says so before it is clicked, and the one whatever it
    /// opened is showing right now. The text alone, not a ground — a band
    /// under a row in this column reads as the selection.
    pub(crate) lit_rows: &'a [usize],
}

/// Draws one extension [`Section`] into the rows it is given, and reports
/// what a click on each row would mean. Answers whether anything actually
/// slid, which is what tells the host's own clock it has a reason to keep
/// turning.
///
/// The counterpart to [`render`] for a section rather than a full frame,
/// and the reason it is here rather than in either sidebar: an extension
/// section is an extension surface, and both of them resolve colour,
/// eliding and hit rectangles in this one module.
pub(crate) fn render_section(
    frame: &mut ratatui::Frame<'_>,
    section: &Section,
    rows: &mut crate::ui::Rows,
    state: SectionState<'_>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> bool {
    let SectionState {
        dragging,
        sliding,
        lit_rows,
    } = state;
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
    // The header is the handle the foot is pulled by, and the title goes
    // to the accent while it is pulled, the way the sidebar's own border
    // does: the one sign the drag took before the rows move.
    if dragging {
        title_style = title_style.fg(theme::color(Token::Accent));
    }
    // One column in, the pad the caption keeps at the other end: on the
    // band an open section wears, a chevron flush against the edge under a
    // caption held off it reads as a row that slipped. Its rows keep the
    // same column, so the heading still stands over them.
    let lead = " ".repeat(usize::from(TRAILING_PAD));
    let mut spans = vec![
        TextSpan::raw(lead.clone()),
        TextSpan::styled(format!("{fold} "), theme::fg(Token::TextSecondary)),
        TextSpan::styled(section.title.clone(), title_style),
    ];
    let tick_on = |hit: ViewHit| {
        sliding
            .filter(|(under, _)| *under == hit)
            .map(|(_, tick)| tick)
    };
    let mut slid = match tick_on(ViewHit::ToggleSection) {
        Some(tick) => row::push_trailing_marquee(
            &mut spans,
            header_rect.width,
            section.caption.text.clone(),
            color(section.caption.role),
            tick,
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
    // The subtler of the raised grounds: the band only has to set the
    // heading off, and the brighter one outweighed the rows it heads.
    if !section.collapsed {
        row::pad_to(
            &mut spans,
            header_rect.width,
            theme::color(Token::SurfaceRaisedSubtle),
        );
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), header_rect);
    hits.push((header_rect, ViewHit::ToggleSection));
    if section.collapsed {
        return slid;
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
        let trailing_width = text::columns(&row.trailing.text) as u16;
        // The name gives way before the trailing value, and one column is
        // reserved for the gap `push_trailing` always leaves between them.
        let name_width = rect
            .width
            .saturating_sub(TRAILING_PAD + marker_width + 1 + trailing_width + TRAILING_PAD);
        let name_style = if lit_rows.contains(&index) {
            theme::fg(Token::TextBright)
        } else {
            Style::default().fg(color(row.name.role))
        };
        let mut spans = vec![
            TextSpan::raw(lead.clone()),
            TextSpan::styled(
                format!("{mark} "),
                Style::default().fg(color(row.mark_role)),
            ),
        ];
        let room = usize::from(name_width);
        match tick_on(ViewHit::SelectItem(index))
            .and_then(|tick| row::marquee(&[(&row.name.text, name_style)], room, tick))
        {
            Some(window) => {
                slid = true;
                spans.extend(window);
            }
            None => spans.push(TextSpan::styled(
                text::elide(&row.name.text, room),
                name_style,
            )),
        }
        row::push_trailing(
            &mut spans,
            rect.width,
            row.trailing.text.clone(),
            color(row.trailing.role),
        );
        frame.render_widget(Paragraph::new(Line::from(spans)), rect);
        hits.push((rect, ViewHit::SelectItem(index)));
    }
    slid
}
