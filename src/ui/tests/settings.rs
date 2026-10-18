//! The Settings screen: themes, glyph sets and the chime.

use ratatui::{Terminal, backend::TestBackend};

use crate::ui::hit::Hit;
use crate::ui::management::render;
use crate::ui::model::{Focus, ROUTES, Route, TuiModel};

use super::support::*;

/// Every card of the catalogue is reachable however narrow the column
/// gets, and the last of them can be brought on screen.
///
/// The catalogue is a grid: narrow the column and the cards wrap onto
/// more lines than there are rows. It used to draw from the top and stop
/// at the foot, so everything past the last full line was invisible and
/// unreachable at once — the selection walked off the bottom and nothing
/// followed it.
#[test]
fn the_settings_catalog_follows_its_selection_down_a_narrow_column() {
    let mut model = model_with_data();
    model.set_route(Route::Settings);
    model.focus = Focus::Content;
    model.settings_themes = (0..8)
        .map(|index| uze_application::application::ThemeSummary {
            id: format!("theme-{index}"),
            active: index == 0,
            path: None,
        })
        .collect();
    model.settings_glyph_sets = (0..4)
        .map(|index| uze_application::application::GlyphSetSummary {
            id: format!("set-{index}"),
            active: false,
        })
        .collect();
    model.settle_settings_selection();

    // One card per line, and more lines than rows.
    let drawn = |model: &TuiModel| {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        let mut hits = Vec::new();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), model, false, &mut hits);
            })
            .unwrap();
        let rows = buffer_rows(&terminal).join("\n");
        let cards: Vec<usize> = hits
            .iter()
            .filter_map(|(_, hit)| match hit {
                Hit::SettingsRow(index) => Some(*index),
                _ => None,
            })
            .collect();
        (cards, rows)
    };

    let last = model.settings_rows().len() - 1;
    let (first_page, rows) = drawn(&model);
    assert!(!first_page.is_empty(), "the catalogue is drawn: {rows}");
    assert!(
        !first_page.contains(&last),
        "the last card is past the first screenful: {rows}"
    );

    model.settings_selected = last;
    let (followed, rows) = drawn(&model);
    assert!(
        followed.contains(&last),
        "and the window follows the selection to it: {rows}"
    );
    assert!(
        !followed.contains(&first_page[0]),
        "leaving the first behind rather than drawing both: {rows}"
    );
}

/// The claim the Settings screen exists to make: each glyph set is drawn
/// in *its own* glyphs, not in the ones currently in force. Without that,
/// choosing a set is choosing a name and hoping — which is the guess the
/// whole change removes, since no terminal can be asked what font it has.
#[test]
fn each_glyph_set_is_previewed_in_its_own_glyphs() {
    let mut terminal = Terminal::new(TestBackend::new(92, 40)).unwrap();
    let model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_glyph_sets: uze_theme::glyph_sets()
            .iter()
            .map(|id| uze_application::application::GlyphSetSummary {
                id: (*id).to_owned(),
                active: false,
            })
            .collect(),
        ..TuiModel::default()
    };
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let rows = buffer_rows(&terminal);

    // A card names the set in its frame and draws the preview two rows
    // below: the frame, the line saying what the set asks of the machine,
    // then the marks themselves.
    let row_for = |id: &str| -> String {
        let title = rows
            .iter()
            .position(|row| row.contains('\u{256d}') && row.contains(id))
            .unwrap_or_else(|| panic!("no card for the `{id}` set in {rows:#?}"));
        rows[title + 2].clone()
    };

    // ASCII's own marks, on the ASCII row, while the default theme — the
    // one actually in force here — draws none of them.
    let ascii = row_for("ascii");
    assert!(
        ascii.contains('*'),
        "the ascii set is not in ascii: {ascii:?}"
    );
    assert!(
        !ascii.contains('✓'),
        "the ascii row borrowed the active theme's glyphs: {ascii:?}"
    );

    // The default set draws its own, on its own row.
    let default = row_for("default");
    assert!(
        default.contains('✓'),
        "the default set is not in its own glyphs: {default:?}"
    );

    // And the nerd set draws Codicons, which live in the private-use area.
    let nerd = row_for("nerd");
    assert!(
        nerd.chars().any(|c| ('\u{ea60}'..='\u{ec84}').contains(&c)),
        "the nerd row carries no Codicon: {nerd:?}"
    );
}

/// The card the keyboard is on says so on its frame; the card in force says
/// so with its fill. Painting the fill over the frame rather than inside it
/// takes the frame away from every card that is in force but not selected —
/// which is every card in force, most of the time.
#[test]
fn the_fill_that_marks_what_is_in_force_leaves_the_frame_alone() {
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_glyph_sets: uze_theme::glyph_sets()
            .iter()
            .map(|id| uze_application::application::GlyphSetSummary {
                id: (*id).to_owned(),
                active: *id == "nerd",
            })
            .collect(),
        ..TuiModel::default()
    };
    model.settle_settings_selection();
    let in_force = model
        .settings_rows()
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::ui::model::SettingsRow::GlyphSet { active: true, .. }
            )
        })
        .expect("a set in force to draw");
    assert_ne!(
        in_force, model.settings_selected,
        "the case this guards is the card in force that the keyboard is not on"
    );

    let (terminal, hits) = drawn_at(&model, 190, 30);

    let card = hits
        .iter()
        .find_map(|(rect, hit)| (*hit == Hit::SettingsRow(in_force)).then_some(*rect))
        .expect("the set in force drew no card");
    let buffer = terminal.backend().buffer();
    let fill = crate::ui::theme::color(uze_theme::Token::SurfaceSelected);
    assert_eq!(
        buffer[(card.x + 1, card.y + 1)].bg,
        fill,
        "the card in force is not filled"
    );
    assert_ne!(
        buffer[(card.x, card.y + 1)].bg,
        fill,
        "the fill reached the frame, which is where the selection has to show"
    );
}

#[test]
fn choosing_a_glyph_set_is_a_different_intent_from_choosing_a_theme() {
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![uze_application::application::ThemeSummary {
            id: "dracula".to_owned(),
            active: false,
            path: None,
        }],
        settings_glyph_sets: vec![uze_application::application::GlyphSetSummary {
            id: "nerd".to_owned(),
            active: false,
        }],
        ..TuiModel::default()
    };
    model.settle_settings_selection();

    // The list opens on the first *choice*, never on the heading above it.
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectTheme("dracula".to_owned())
    );

    // Walking down crosses the second heading without stopping on it.
    model.move_settings_selection(1);
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectGlyphSet("nerd".to_owned()),
        "the glyph set was reached as if it were a theme"
    );
}

#[test]
fn the_chime_is_chosen_on_the_settings_screen_and_marks_the_one_in_force() {
    use uze_application::Chime;
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![uze_application::application::ThemeSummary {
            id: "dracula".to_owned(),
            active: true,
            path: None,
        }],
        settings_chime: Chime::OutOfSight,
        ..TuiModel::default()
    };
    model.settle_settings_selection();

    let chimes: Vec<(Chime, bool)> = model
        .settings_rows()
        .into_iter()
        .filter_map(|row| match row {
            crate::ui::model::SettingsRow::Chime { chime, active } => Some((chime, active)),
            _ => None,
        })
        .collect();
    assert_eq!(
        chimes,
        vec![
            (Chime::Silent, false),
            (Chime::OutOfSight, true),
            (Chime::Always, false),
        ]
    );

    model.move_settings_selection(3);
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectChime(Chime::Always)
    );
}

/// The adaptive card is no theme of its own, so it says what it follows
/// and, in the drawer, which theme it draws in on each background.
#[test]
fn the_adaptive_card_says_it_follows_the_system_and_names_both_themes() {
    let mut model = model_with_data();
    model.set_route(Route::Settings);
    model.focus = Focus::Content;
    model.settings_themes = vec![uze_application::application::ThemeSummary {
        id: uze_application::ADAPTIVE.to_owned(),
        active: true,
        path: None,
    }];
    model.settings_adaptive = Some(("light".to_owned(), "dracula".to_owned()));
    model.settle_settings_selection();

    let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    let rows = buffer_rows(&terminal).join("\n");
    assert!(rows.contains("follows the system"), "{rows}");
    assert!(rows.contains("light"), "{rows}");
    assert!(rows.contains("dracula"), "{rows}");
    assert!(!rows.contains("built in"), "{rows}");
}

/// The list opens with a heading, so moving up from the first choice has
/// nowhere to go: it stays put. It used to step past the heading forever,
/// clamping back onto it at every step, and freeze the whole client.
#[test]
fn moving_up_from_the_first_settings_choice_stays_put() {
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![uze_application::application::ThemeSummary {
            id: "dracula".to_owned(),
            active: false,
            path: None,
        }],
        ..TuiModel::default()
    };
    model.settle_settings_selection();
    let (done, finished) = std::sync::mpsc::channel();
    let mover = std::thread::spawn(move || {
        model.move_settings_selection(-1);
        let _ = done.send(model.activate_settings());
    });
    let answered = finished.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(
        answered.ok(),
        Some(crate::ui::worker::Intent::SelectTheme("dracula".to_owned())),
        "moving up past the leading heading never returned"
    );
    let _ = mover.join();
}

/// Coming back to Settings builds the model afresh, so the selection has
/// to be settled again — on the theme in force, not on the first card.
#[test]
fn returning_to_settings_lands_on_the_theme_in_force() {
    let theme = |id: &str, active: bool| uze_application::application::ThemeSummary {
        id: id.to_owned(),
        active,
        path: None,
    };
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_themes: vec![
            theme("default", false),
            theme("dracula", false),
            theme("tokyo-night", true),
        ],
        ..TuiModel::default()
    };
    model.settle_settings_selection();
    assert_eq!(
        model.activate_settings(),
        crate::ui::worker::Intent::SelectTheme("tokyo-night".to_owned()),
        "the selection fell back to the first card"
    );
}

#[test]
fn opening_settings_asks_for_the_lists_it_chooses_from() {
    let mut model = TuiModel::default();
    assert_eq!(
        model.set_route(Route::Settings),
        crate::ui::worker::Intent::LoadSettings
    );
    // Every other route asks for nothing on arrival, so this one is not
    // paying for a read it does not need.
    assert_eq!(
        model.set_route(Route::Keys),
        crate::ui::worker::Intent::None
    );
}

/// Arriving is the only moment Settings asks for its lists, so every way
/// of arriving has to carry the ask. A gesture that dropped it left the
/// screen showing its two headings and nothing under them — and leaving and
/// coming back was no cure, because coming back was the gesture that dropped
/// it.
#[test]
fn every_way_of_reaching_settings_carries_the_ask() {
    let landing = Route::Settings.index();

    let mut numbered = TuiModel::default();
    assert_eq!(
        numbered.act(uze_keys::Action::SelectTab(landing as u8 + 1)),
        crate::ui::worker::Intent::LoadSettings,
        "choosing Settings by its number reached it without asking for its lists"
    );

    let mut walked = TuiModel {
        focus: Focus::Sidebar,
        ..TuiModel::default()
    };
    let mut asked = None;
    for _ in 0..ROUTES.len() {
        let intent = walked.act(uze_keys::Action::SelectNext);
        if walked.route.index() == landing {
            asked = Some(intent);
            break;
        }
    }
    assert_eq!(
        asked,
        Some(crate::ui::worker::Intent::LoadSettings),
        "walking the screen tabs reached Settings without asking for its lists"
    );

    let mut clicked = TuiModel::default();
    let (_terminal, hits) = drawn_at(&clicked, 120, 40);
    clicked.hits = hits;
    let (rect, _) = clicked
        .hits
        .iter()
        .find(|(_, hit)| *hit == crate::ui::hit::Hit::Route(Route::Settings))
        .expect("Settings is reachable from its tab")
        .clone();
    assert_eq!(
        clicked.click(rect.x + 1, rect.y),
        crate::ui::worker::Intent::LoadSettings,
        "clicking into Settings reached it without asking for its lists"
    );
}

#[test]
fn clicking_a_glyph_set_chooses_it() {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        settings_glyph_sets: vec![uze_application::application::GlyphSetSummary {
            id: "ascii".to_owned(),
            active: false,
        }],
        ..TuiModel::default()
    };
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &model, false, &mut hits);
        })
        .unwrap();
    model.hits = hits;

    let rows = model.settings_rows();
    let index = rows
        .iter()
        .position(|row| matches!(row, crate::ui::model::SettingsRow::GlyphSet { .. }))
        .expect("a set row");
    let (rect, _) = model
        .hits
        .iter()
        .find(|(_, hit)| *hit == crate::ui::hit::Hit::SettingsRow(index))
        .expect("the set row is clickable")
        .clone();

    assert_eq!(
        model.click(rect.x + 3, rect.y),
        crate::ui::worker::Intent::SelectGlyphSet("ascii".to_owned())
    );
}

/// The wheel takes the catalogue down to its end and back up to its top:
/// a window kept in whole cards still moves a step each way.
#[test]
fn the_settings_catalogue_scrolls_back_up_from_its_end() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let mut model = TuiModel {
        route: Route::Settings,
        focus: Focus::Content,
        ..model_with_data()
    };
    let wheel = |model: &mut TuiModel, kind| {
        model.apply_mouse(
            MouseEvent {
                kind,
                column: 20,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
            Rect::new(0, 0, 100, 20),
        );
        drawn_at(model, 100, 20);
    };
    drawn_at(&model, 100, 20);
    for _ in 0..60 {
        wheel(&mut model, MouseEventKind::ScrollDown);
    }
    let bottom = model.settings_scroll.get().first;
    assert!(bottom > 0, "the catalogue did not scroll down");
    wheel(&mut model, MouseEventKind::ScrollUp);
    assert_eq!(
        model.settings_scroll.get().first,
        bottom - 1,
        "a step up from the end did not move"
    );
    for _ in 0..60 {
        wheel(&mut model, MouseEventKind::ScrollUp);
    }
    assert_eq!(
        model.settings_scroll.get().first,
        0,
        "the top was not reached"
    );
}
