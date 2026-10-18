//! What reaches the terminal: the palette a pane is told about, a theme put
//! in force elsewhere, and controls that never reach the screen.

use super::support::*;
use crate::ui::theme::Token;
use uze_core::UzeHome;
use uze_terminal::ClientRequest;

/// What a pane's own program is told the terminal's colours are is the
/// theme's, not a transcription of it.
///
/// The two constants this replaced lived in another crate, copied from
/// the palette by hand — so a colour changed here left a program inside
/// a pane picking a light- or dark-adapted UI against a background
/// nobody was drawing.
#[test]
fn the_palette_a_pane_is_told_about_is_the_one_being_drawn() {
    let theme = uze_theme::active();
    let palette = crate::ui::orchestrator::active_palette();
    let triple = |token| {
        let rgb = theme.color(token);
        (rgb.0, rgb.1, rgb.2)
    };
    assert_eq!(palette.foreground, triple(Token::TextPrimary));
    assert_eq!(palette.background, triple(Token::SurfaceBackground));
    for (index, entry) in palette.ansi.iter().enumerate() {
        assert_eq!(*entry, triple(Token::ANSI[index]), "ansi.{index}");
    }
}

/// A theme put in force from elsewhere — the desktop turning light, or
/// Settings — is drawn on the next frame, and the server hears the
/// colours a pane's program will be told when it asks. Once.
#[test]
fn a_theme_put_in_force_elsewhere_is_drawn_and_told_to_the_server_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-follows-theme"));
    let (model, _first, _second) = two_agents_with_shells();
    let mut driven = driven(model, &home);
    let _ = driven.pump();
    let _ = driven.sent();
    driven.attach.model.dirty = false;

    // As if another theme had been put in force since this client drew,
    // without swapping the process-wide one under its neighbours.
    driven.attach.theme_generation = uze_theme::generation().wrapping_sub(1);
    let _ = driven.pump();
    assert!(driven.attach.model.dirty, "the new theme is drawn");
    let palettes = |sent: Vec<ClientRequest>| {
        sent.iter()
            .filter(|request| matches!(request, ClientRequest::SetPalette(_)))
            .count()
    };
    assert_eq!(palettes(driven.sent()), 1);

    let _ = driven.pump();
    assert_eq!(palettes(driven.sent()), 0, "told once, not every frame");
}

/// A plugin name carrying a right-to-left override draws as its letters:
/// the override, which ratatui keeps because it is zero-width, never
/// reaches the terminal to reorder the row.
#[test]
fn a_bidi_override_never_reaches_the_terminal() {
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 12, 1));
    buffer.set_string(0, 0, "ab\u{202e}cd", ratatui::style::Style::default());
    let mut drawn = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 12, 1));
    ratatui::widgets::Widget::render(
        ratatui::text::Line::raw("ab\u{202e}cd\u{1b}[2K"),
        drawn.area,
        &mut drawn,
    );
    assert!(
        drawn
            .content
            .iter()
            .any(|cell| cell.symbol().contains('\u{202e}')),
        "ratatui itself keeps the override, so this pass is what removes it"
    );
    crate::ui::settle_controls(&mut drawn);
    let row: String = drawn.content.iter().map(|cell| cell.symbol()).collect();
    assert!(
        !row.chars().any(uze_application::is_terminal_control),
        "{row:?}"
    );
    assert!(row.starts_with("abcd"), "{row:?}");
}
