//! The Harnesses screen: the catalog, its cards and setting a harness up.

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{Focus, Overlay, Route, TuiModel};

use super::support::*;

/// The legend is the card's own vocabulary written out, so it names what
/// a card can wear and nothing else. It used to list three marks, one of
/// them for a state no card draws any more; the unmarked state is
/// described instead of listed, because a glyph beside it would name a
/// mark that is never on screen.
#[test]
fn the_harness_legend_names_the_words_a_card_carries() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.overlay = Overlay::HarnessHelp;
    let (terminal, _hits) = drawn_at(&model, 120, 40);
    let legend = buffer_rows(&terminal).join("\n");

    assert!(
        legend.contains("Enabled"),
        "the word a set-up card carries: {legend}"
    );
    assert!(
        legend.contains("Not configured"),
        "and the word the rest carry: {legend}"
    );
    for gone in ["Not installed", "Run uze setup"] {
        assert!(
            !legend.contains(gone),
            "{gone:?} is not a state a card has: {legend}"
        );
    }
}

/// The header counts what is on the machine, not how many harnesses UZE
/// knows about. It counted the cards, so a machine carrying three of the
/// four UZE supports was told "4 installed" — and removing one changed
/// nothing, which is how the number was caught.
#[test]
fn the_catalog_counts_the_harnesses_that_are_actually_installed() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.focus = Focus::Content;
    let header = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(150, 40)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        buffer_rows(&terminal)
            .into_iter()
            .find(|row| row.contains("installed"))
            .expect("the screen header counts them")
    };

    assert!(
        header(&model).contains("2 installed"),
        "both fixtures are on the machine: {}",
        header(&model)
    );

    model
        .remembered
        .doctor
        .as_mut()
        .expect("the fixture has a report")
        .harnesses[1]
        .detection
        .present = false;

    assert!(
        header(&model).contains("1 installed"),
        "and one that is gone is not installed: {}",
        header(&model)
    );
}

/// Setting a harness up is offered for every card in the catalog, and it
/// is the same gesture wherever it starts from: UZE provisions through the
/// vendor's own official route, which installs what is not on the machine
/// and updates what is. The drawer used to offer the action only where it
/// had already been done, and nothing at all on the one card with
/// something to do.
#[test]
fn every_harness_in_the_catalog_can_be_set_up() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.focus = Focus::Content;
    // The fixture's second harness is detected and unconfigured.
    model.remembered.harness_screen.selected = 1;
    let drawn = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(150, 40)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        let offered = hits
            .iter()
            .any(|(_, hit)| *hit == Hit::OfferedAction(uze_keys::Action::SetupHarness));
        (offered, buffer_rows(&terminal).join("\n"))
    };

    let (offered, rows) = drawn(&model);
    assert!(offered, "a harness that is here can be set up: {rows}");

    model
        .remembered
        .doctor
        .as_mut()
        .expect("the fixture has a report")
        .harnesses[1]
        .detection
        .present = false;

    let (offered, rows) = drawn(&model);
    assert!(
        offered,
        "and so can one that is not — setting it up is what installs it: {rows}"
    );
    // Read down the drawer's own column and closed up, because the note
    // wraps: it is a sentence in a panel whose width is dragged, and a
    // row-by-row search for it finds it only at the widths it happens to
    // fit on one line.
    let drawer: String = rows
        .lines()
        .filter_map(|row| row.rsplit('\u{2502}').next())
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        drawer.contains("setting it up installs it"),
        "which is what the drawer says it will do: {rows}"
    );
}

/// A harness card says its state at its foot, beside its id, the way an
/// extension card does: "Enabled" for the one UZE set up, "Not configured"
/// for the rest. The title carries the name alone, so a narrowed card has
/// nothing to clip against it.
#[test]
fn a_harness_card_says_its_state_at_its_foot() {
    let mut model = model_with_data();
    model.set_route(Route::Harnesses);
    model.focus = Focus::Content;
    // By each card's own hit rect, so this reads a card's rows rather than
    // the first mention of a name anywhere on the screen.
    let cards = |width: u16| {
        let (terminal, hits) = drawn_at(&model, width, 40);
        let rows = buffer_rows(&terminal);
        let row_of = |rect: Rect, offset: u16| -> String {
            rows[(rect.y + offset) as usize]
                .chars()
                .skip(rect.x as usize)
                .take(rect.width as usize)
                .collect()
        };
        let cards: Vec<(String, String)> = hits
            .iter()
            .filter_map(|(rect, hit)| matches!(hit, Hit::HarnessRow(_)).then_some(*rect))
            .map(|rect| (row_of(rect, 1), row_of(rect, 5)))
            .collect();
        assert!(!cards.is_empty(), "no harness card was drawn: {rows:?}");
        (cards, rows)
    };
    let card = |cards: &[(String, String)], name: &str| {
        cards
            .iter()
            .find(|(title, _)| title.contains(name))
            .unwrap_or_else(|| panic!("{name} has no card: {cards:?}"))
            .clone()
    };

    let (wide, rows) = cards(200);
    let (title, foot) = card(&wide, "Claude Code");
    assert_eq!(title.trim(), "Claude Code", "the title is the name alone");
    assert!(
        foot.contains("Enabled"),
        "the one UZE set up says so: {foot:?}"
    );
    // Pinned to the card's right edge, past its two columns of inset, so
    // every card in a row says it in the same column.
    assert!(
        foot.trim_end_matches(' ').ends_with("Enabled") && foot.ends_with("Enabled  "),
        "the state sits at the right edge: {foot:?}"
    );
    let (title, foot) = card(&wide, "Codex");
    assert_eq!(title.trim(), "Codex");
    assert!(
        foot.contains("Not configured"),
        "and the one it did not says that: {foot:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("PATH")),
        "least of all a sentence about this machine's PATH: {rows:?}"
    );

    let (narrow, _) = cards(120);
    let (title, foot) = card(&narrow, "Claude Code");
    assert_eq!(title.trim(), "Claude Code");
    assert!(
        foot.contains("Enabled"),
        "a narrowed card keeps it: {foot:?}"
    );
}
