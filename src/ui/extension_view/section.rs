//! A collapsible section drawn inside one of the host's own columns.

use super::*;

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
    render_section_with(frame, section, rows, dragging, None, &[], hits)
}

/// The same, with `marquee` the clock a caption too long for its room
/// slides by — `Some` while the pointer is on this header, `None`
/// otherwise. Answers whether a caption actually slid, which is what
/// tells the host's own clock it has a reason to keep turning.
///
/// `lit_rows` are the rows lifted to the bright text the row
/// receiving keystrokes wears: the one under the pointer, since a row that
/// opens something on a click says so before it is clicked, and the one
/// whatever it opened is showing right now. The text alone, not a ground —
/// a band under a row in this column reads as the selection.
pub(crate) fn render_section_with(
    frame: &mut ratatui::Frame<'_>,
    section: &Section,
    rows: &mut crate::ui::Rows,
    dragging: bool,
    marquee: Option<usize>,
    lit_rows: &[usize],
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
            TextSpan::styled(text::elide(&row.name.text, name_width as usize), name_style),
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
