//! Tests for the management client, one file per screen or concern.

// Gated again so the architecture suite, which reads `#[cfg(test)] mod`
// declarations, knows these files are test code.
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod harnesses;
#[cfg(test)]
mod keys;
#[cfg(test)]
mod layout;
#[cfg(test)]
mod marketplaces;
#[cfg(test)]
mod navigation;
#[cfg(test)]
mod overlays;
#[cfg(test)]
mod overview;
#[cfg(test)]
mod plugins;
#[cfg(test)]
mod profiles;
#[cfg(test)]
mod search;
#[cfg(test)]
mod session;
#[cfg(test)]
mod settings;
#[cfg(test)]
mod support;

use std::path::PathBuf;

use ratatui::layout::Rect;

use uze_application::application::{DoctorReport, MaintenanceReport};

use super::view::health::{Severity, actionable_alerts};
use crate::ui::theme::{self, Token};
use crate::ui::widget::text;

#[test]
fn overview_alerts_classify_conflicts_as_high_and_missing_as_low() {
    use uze_application::application::{ManagedStateSummary, PackageManagedState};
    let doctor = DoctorReport {
        uze_home: PathBuf::from("/home"),
        store: uze_application::application::StoreHealth::Ready,
        plugins: Vec::new(),
        harnesses: Vec::new(),
        attachments: vec![
            PackageManagedState {
                hooks: Vec::new(),
                plugin: "acme".to_owned(),
                state: ManagedStateSummary {
                    matched: 0,
                    missing: 1,
                    drifted: 0,
                    conflicts: 1,
                    blocked: 0,
                    ledger_error: None,
                },
            },
            PackageManagedState {
                hooks: Vec::new(),
                plugin: "example".to_owned(),
                state: ManagedStateSummary {
                    matched: 0,
                    missing: 1,
                    drifted: 0,
                    conflicts: 0,
                    blocked: 0,
                    ledger_error: None,
                },
            },
        ],
        deliveries: Vec::new(),
        ledger_error: None,
        provisioning_state_error: None,
        leftovers: Default::default(),
        maintenance: MaintenanceReport::default(),
        git_found: true,
        shell_refusal: None,
        ssh_missing: false,
        machine_concerns: Vec::new(),
    };
    let alerts = actionable_alerts(Some(&doctor));
    assert_eq!(alerts[0].severity, Severity::High);
    assert!(alerts.iter().any(|alert| alert.severity == Severity::Low));
}

/// A hint names a key and what it does, and it asks the keymap for both.
/// The strings this replaced were typed by hand — which is how the help
/// overlay came to omit nine of the keys it was supposed to document.
#[test]
fn a_hint_line_reads_its_keys_off_the_keymap() {
    use ratatui::text::Line;
    use uze_keys::{Action, Scope};

    let scopes = [Scope::Global, Scope::Management, Scope::Plugins];
    let line: Line<'static> = crate::ui::widget::hint::Hints::new(&scopes)
        .key(Action::UpdatePlugin)
        .key(Action::Refresh)
        .key(Action::OpenActionIndex)
        .line();
    let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let keymap = uze_keys::active();
    for action in [Action::UpdatePlugin, Action::Refresh] {
        let chord = keymap.chord_for(action, &scopes).expect("bound here");
        assert!(
            content.contains(&chord.to_string()),
            "the hint names {action} without its key: {content}"
        );
        assert!(
            content.contains(&action.label().to_lowercase()),
            "{content}"
        );
    }
    assert_eq!(line.spans[0].style, theme::fg_bold(Token::Accent));
    assert_eq!(line.spans[1].style, theme::fg(Token::TextMuted));

    // An action with no key here is skipped rather than printed keyless:
    // a hint is a list of shortcuts, and what has none is offered where a
    // pointer can reach it.
    let unbound: Line<'static> = crate::ui::widget::hint::Hints::new(&scopes)
        .key(Action::NewSpace)
        .line();
    assert!(unbound.spans.is_empty());
}

/// A name or caption in wide characters — a CJK branch, an emoji — takes
/// two cells a character. Counted as one, the row ran past its column by
/// the difference and the frame cut the pad or the caption off.
#[test]
fn a_row_of_wide_characters_fits_the_columns_it_was_given() {
    use ratatui::{style::Style, text::Span};

    use crate::ui::widget::row;

    let width = |spans: &[Span<'_>]| spans.iter().map(Span::width).sum::<usize>();

    let mut caption = vec![Span::raw("▾ Git")];
    row::push_trailing(
        &mut caption,
        20,
        "功能分支".to_owned(),
        theme::color(Token::TextMuted),
    );
    assert_eq!(width(&caption), 20);

    let title = row::title_row("提交说明", "esc", 30);
    assert_eq!(title.width(), 30);

    let name = "エージェントの作業ブランチ";
    for tick in 0..200 {
        let window = row::marquee(&[(name, Style::default())], 9, tick)
            .expect("a name wider than the room slides");
        assert_eq!(width(&window), 9, "at tick {tick}");
    }
}

/// A caption pinned to the right edge is elided to what is left of the
/// row. It used to be appended whole and cut by the frame, which is how a
/// long branch name on the Git section header lost both its ending and any
/// sign that it had one.
#[test]
fn a_trailing_caption_is_elided_to_the_room_the_row_has_left() {
    use ratatui::text::Span;

    let mut spans = vec![Span::raw("▾ Git")];
    crate::ui::widget::row::push_trailing(
        &mut spans,
        20,
        "agent/a-very-long-branch-name".to_owned(),
        theme::color(Token::TextMuted),
    );
    let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(
        row.ends_with("… "),
        "the caption says it was shortened: {row}"
    );
    assert!(
        Span::raw(&row).width() <= 20,
        "and the row still fits the column: {row}"
    );

    let mut spans = vec![Span::raw("▾ Git")];
    crate::ui::widget::row::push_trailing(
        &mut spans,
        20,
        "main".to_owned(),
        theme::color(Token::TextMuted),
    );
    let row: String = spans.iter().map(|span| span.content.as_ref()).collect();
    assert!(
        row.contains("main"),
        "a caption that fits is left alone: {row}"
    );
    assert!(!row.contains('…'), "{row}");
}

#[test]
fn clip_line_truncates_long_status_with_ellipsis() {
    use ratatui::text::Line;

    let mut line =
        Line::from("Installed plugin root: /home/user/.codex/plugins/cache/very/long/path");
    text::clip(&mut line, 20);
    let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(content, "Installed plugin ro…");
    assert_eq!(ratatui::text::Span::raw(&content).width(), 20);

    let mut line = Line::from("Installed uze");
    text::clip(&mut line, 20);
    let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(content, "Installed uze");
}

/// A row eliding text stays inside the width it was given, whatever the
/// active theme's elision marker costs.
///
/// The ASCII theme spends three cells on `...` where the default spends one
/// on `…`, and the old code reserved a hard-coded `1` — so this is the
/// property that says a theme can replace a symbol without shearing every
/// row that draws it.
#[test]
fn eliding_reserves_the_active_themes_own_marker_width() {
    let marker = theme::glyph(theme::Symbol::Ellipsis);
    let marker_width = usize::from(theme::width(theme::Symbol::Ellipsis));
    for width in (marker_width + 1)..12usize {
        let elided = text::elide("a subject line long enough to be cut", width);
        let cells = elided.chars().count() - marker.chars().count() + marker_width;
        assert!(
            cells <= width,
            "elided to {elided:?} ({cells} cells) for a width of {width}"
        );
        assert!(
            elided.ends_with(&marker),
            "an elided row has to say it was cut: {elided:?}"
        );
    }
}

// The sidebar draws the badge into the label column, beside a
// right-aligned count, and the workspace tab measures the alias against
// the width it has left — so a label that changed length or cell count on
// its way to the screen would take a column with it. Cell count holds
// because every small capital is East Asian width *neutral*: a terminal
// gives each one cell whether or not its font has the glyph.
#[test]
fn small_caps_preserves_a_labels_length_and_its_cells() {
    use ratatui::text::Span;
    for label in ["Beta", "claude", "codex", "antigravity", "Run uze setup"] {
        let drawn = crate::ui::widget::text::small_caps(label);
        assert_eq!(
            drawn.chars().count(),
            label.chars().count(),
            "{label:?} changed length as {drawn:?}"
        );
        assert_eq!(
            Span::raw(drawn.clone()).width(),
            Span::raw(label).width(),
            "{label:?} changed cell count as {drawn:?}"
        );
        assert_eq!(
            drawn.split(' ').count(),
            label.split(' ').count(),
            "{label:?} lost a word boundary as {drawn:?}"
        );
    }
}

// Mixed case has to arrive as one even run — a full-height initial next to
// small capitals is the thing this exists to avoid. `q` and `x`, which
// Unicode has no small capital for, come out lowercase rather than
// vanishing or standing up as the one full-height letter in the run.
#[test]
fn small_caps_levels_mixed_case_and_keeps_what_it_cannot_fold() {
    assert_eq!(crate::ui::widget::text::small_caps("Beta"), "ʙᴇᴛᴀ");
    assert_eq!(
        crate::ui::widget::text::small_caps("PATH shadowed"),
        "ᴘᴀᴛʜ ꜱʜᴀᴅᴏᴡᴇᴅ"
    );
    assert_eq!(crate::ui::widget::text::small_caps("Query X2"), "qᴜᴇʀʏ x2");
}

/// A card grid longer than its area scrolls to the selected card instead
/// of drawing what fits and leaving the selection off the bottom.
#[test]
fn a_card_grid_brings_the_selected_card_into_view() {
    let scroll = std::cell::Cell::new(Default::default());
    let area = Rect::new(0, 0, 80, 16);
    let grid = crate::ui::view::catalog::cards(area, 10, 9, &scroll);
    assert!(
        grid.cards.iter().any(|(position, _)| *position == 9),
        "the selected card is not drawn: {:?}",
        grid.cards
    );
    assert!(grid.first > 0 && grid.rows == 5);
}
