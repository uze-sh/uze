//! The files open in a surface's content, as a strip of tabs over it: the
//! one on show raised, the one only passed over set in italics, each with
//! its close mark — or, while it holds unsaved changes, the editor's dot
//! in that place, which turns back into the close mark under the pointer.

use super::*;
use uze_extensions::view::FileTab;

/// Draws `tabs` along the top row of `area` and answers with the rest of
/// it. The strip starts far enough along that the tab on show is on it.
pub(super) fn render_file_tabs(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    tabs: &[FileTab],
    hovered: Option<ViewHit>,
    hits: &mut Vec<(Rect, ViewHit)>,
) -> Rect {
    if tabs.is_empty() || area.height < 2 {
        return area;
    }
    let strip = Rect::new(area.x, area.y, area.width, 1);
    widget::fill(frame, strip, Token::SurfaceRecessed);
    let chips: Vec<Vec<TextSpan<'static>>> = tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| chip(index, tab, hovered))
        .collect();
    let widths: Vec<u16> = chips
        .iter()
        .map(|chip| navigator::spans_width(chip))
        .collect();
    let active = tabs.iter().position(|tab| tab.active).unwrap_or(0);
    let first = first_shown(&widths, active, strip.width);
    let mut x = strip.x;
    for (index, (chip, width)) in chips.into_iter().zip(&widths).enumerate().skip(first) {
        if x >= strip.right() {
            break;
        }
        let width = (*width).min(strip.right() - x);
        let rect = Rect::new(x, strip.y, width, 1);
        let close = Rect::new(rect.right().saturating_sub(2), strip.y, 1, 1);
        // The close mark first: the hit list is read first-come, and the
        // mark lies on the tab it closes.
        if rect.width >= 3 {
            hits.push((close, ViewHit::CloseTab(index)));
        }
        hits.push((rect, ViewHit::SelectTab(index)));
        frame.render_widget(Paragraph::new(Line::from(chip)), rect);
        x = x.saturating_add(width);
    }
    Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    )
}

/// One tab as spans: ` name detail × `, on its ground.
fn chip(index: usize, tab: &FileTab, hovered: Option<ViewHit>) -> Vec<TextSpan<'static>> {
    let ground = match (tab.active, hovered == Some(ViewHit::SelectTab(index))) {
        (true, _) => Token::SurfaceBackground,
        (false, true) => Token::SurfaceHover,
        (false, false) => Token::SurfaceRecessed,
    };
    let ink = match tab.active {
        true => Token::TextBright,
        false => Token::TextMuted,
    };
    let mut name = theme::fg(ink);
    if tab.preview {
        name = name.add_modifier(Modifier::ITALIC);
    }
    let closing = hovered == Some(ViewHit::CloseTab(index));
    let mark = match (tab.unsaved, closing) {
        (true, false) => TextSpan::styled(
            theme::glyph(Symbol::MarkUnsaved),
            theme::fg(Token::TextSecondary),
        ),
        _ => TextSpan::styled(
            theme::glyph(Symbol::MarkClose),
            theme::fg(mark::close_tone(match closing {
                true => widget::ChipState::Hovered,
                false => widget::ChipState::Resting,
            })),
        ),
    };
    let mut spans = vec![TextSpan::raw(" "), TextSpan::styled(tab.name.clone(), name)];
    if !tab.detail.is_empty() {
        spans.push(TextSpan::styled(
            format!(" {}", tab.detail),
            theme::fg(Token::TextDim),
        ));
    }
    spans.push(TextSpan::raw(" "));
    spans.push(mark);
    spans.push(TextSpan::raw(" "));
    let ground = theme::color(ground);
    for span in &mut spans {
        span.style = span.style.bg(ground);
    }
    spans
}

/// The first tab drawn, so that the one on show fits: as far left as
/// possible, moved right only as far as it has to be.
fn first_shown(widths: &[u16], active: usize, room: u16) -> usize {
    let mut first = 0;
    while first < active
        && widths[first..=active]
            .iter()
            .map(|width| u32::from(*width))
            .sum::<u32>()
            > u32::from(room)
    {
        first += 1;
    }
    first
}
