//! The new agent's overrides: the field on the popup's first step, the
//! step that stands in the listing's place, and the agent's overview
//! naming what it was launched with.

use super::WorkspaceHit;
use super::support::*;
use super::{AgentOption, DirectoryStep, RecentOverridesRead, RootPicker, WorkspaceModel, render};
use crate::ui::theme::{self, Symbol, Token};
use ratatui::layout::Rect;
use std::path::PathBuf;
use uze_application::{OverrideMark, OverrideValue, RecentOverrides};
use uze_core::UzeHome;
use uze_keys::{Action, Scope};
use uze_testkit::temp::TempDir;

/// The harness set up whose catalog offers a flag that weakens safety and
/// one with a fixed set of values: the step's whole vocabulary.
fn telling_harness(options: &[AgentOption]) -> usize {
    options
        .iter()
        .position(|option| {
            let flags: Vec<_> = option.overrides.flags().collect();
            flags
                .iter()
                .any(|flag| matches!(flag.mark, OverrideMark::Weakens(_)))
                && flags
                    .iter()
                    .any(|flag| matches!(flag.value, OverrideValue::OneOf(_)))
        })
        .expect("a harness whose catalog weakens and offers values")
}

/// The popup open over a directory of its own, on the harness whose
/// catalog says the most, driven on a roomy terminal.
struct Popup {
    home: UzeHome,
    _root: TempDir,
    model: WorkspaceModel,
}

fn popup(name: &str) -> Popup {
    let home = UzeHome::at(uze_testkit::temp::scratch(name));
    set_up_every_harness(&home);
    let root = TempDir::new(&format!("{name}-root"));
    std::fs::create_dir_all(root.join("engine")).unwrap();
    let mut model = agent_session_in("/repo");
    let picker = RootPicker::opened_in(&root.path().display().to_string(), None);
    let rows = render::new_agent_popup_rows(picker.match_count());
    let space = model.session.as_ref().unwrap().selected_space().id;
    let options = super::agent_options(&home);
    let harness = telling_harness(&options);
    let mut step = DirectoryStep::new(
        picker,
        space,
        Rect::new(20, 1, 3, 1),
        rows,
        options,
        harness,
    );
    step.picker.select(0);
    model.directory_step = Some(step);
    Popup {
        home,
        _root: root,
        model,
    }
}

fn chord(action: Action, scope: Scope) -> crossterm::event::KeyEvent {
    key_event(
        uze_keys::active()
            .chord_for(action, &[scope])
            .unwrap_or_else(|| panic!("{action} has a key in {}", scope.name())),
    )
}

fn type_text(driven: &mut Driven<'_>, text: &str) {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    for character in text.chars() {
        let modifiers = if character.is_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        driven.press_key(KeyEvent::new(KeyCode::Char(character), modifiers));
    }
}

fn step_of<'a>(driven: &'a Driven<'_>) -> &'a DirectoryStep {
    driven
        .attach
        .model
        .directory_step
        .as_ref()
        .expect("the popup is open")
}

fn in_step(driven: &Driven<'_>) -> bool {
    step_of(driven).overrides.is_some()
}

fn entries(driven: &Driven<'_>) -> Vec<Vec<String>> {
    step_of(driven).entries().to_vec()
}

fn screen(driven: &mut Driven<'_>) -> Vec<String> {
    driven.frame();
    frame_rows_at(&mut driven.attach.model, driven.area)
}

fn popup_rect(driven: &Driven<'_>) -> Rect {
    driven.hit(|hit| *hit == WorkspaceHit::NewAgentBody)
}

fn open_by_key(driven: &mut Driven<'_>) {
    driven.press_key(chord(Action::OpenOverrides, Scope::RootPicker));
}

/// The first step's field says `none` and opens the step on a click
/// anywhere on its row, exactly as its key does; the foot names that key
/// too, and its hint is the same gesture.
#[test]
fn the_overrides_row_and_its_key_open_the_same_step() {
    let mut by_key = popup("overrides-open-key");
    let mut by_key = driven(by_key.model_take(), &by_key.home).on_a_roomy_terminal();
    let rows = screen(&mut by_key);
    let field = rows
        .iter()
        .find(|row| row.contains(" overrides "))
        .unwrap_or_else(|| panic!("the field is drawn:\n{}", rows.join("\n")));
    assert!(field.contains("none"), "{field}");
    assert!(field.contains(&theme::glyph(Symbol::Submenu)), "{field}");
    let foot = rows
        .iter()
        .find(|row| row.contains("harness") && row.contains("close"))
        .expect("the foot");
    assert!(foot.contains("overrides"), "the foot names the key: {foot}");
    open_by_key(&mut by_key);
    assert!(in_step(&by_key));

    let mut by_row = popup("overrides-open-row");
    let mut by_row = driven(by_row.model_take(), &by_row.home).on_a_roomy_terminal();
    by_row.frame();
    let row = by_row
        .attach
        .model
        .hits
        .iter()
        .filter(|(_, hit)| *hit == WorkspaceHit::HintAction(Action::OpenOverrides))
        .map(|(rect, _)| *rect)
        .max_by_key(|rect| rect.width)
        .expect("the row is a target");
    by_row.press(row.x + 2, row.y);
    assert!(in_step(&by_row), "a click on the row opens the step");

    let mut by_hint = popup("overrides-open-hint");
    let mut by_hint = driven(by_hint.model_take(), &by_hint.home).on_a_roomy_terminal();
    by_hint.frame();
    let hint = by_hint
        .attach
        .model
        .hits
        .iter()
        .filter(|(_, hit)| *hit == WorkspaceHit::HintAction(Action::OpenOverrides))
        .map(|(rect, _)| *rect)
        .min_by_key(|rect| rect.width)
        .expect("the foot's hint is a target");
    by_hint.press(hint.x, hint.y);
    assert!(
        in_step(&by_hint),
        "a click on the foot's hint opens the step"
    );
}

impl Popup {
    fn model_take(&mut self) -> WorkspaceModel {
        std::mem::replace(&mut self.model, agent_session_in("/repo"))
    }
}

/// The popup is as tall on the overrides step as on the first, and stays
/// so while the line is typed into and entries are added.
#[test]
fn the_popup_keeps_its_height_across_the_steps() {
    let mut popup = popup("overrides-height");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    let first = popup_rect(&driven);
    open_by_key(&mut driven);
    driven.frame();
    assert_eq!(popup_rect(&driven), first, "switching resized the popup");
    type_text(&mut driven, "A=1 B=2 C=3 D=4 E=5 F=6 G=7 H=8 I=9 J=10");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(entries(&driven).len(), 10);
    driven.frame();
    assert_eq!(popup_rect(&driven), first, "entries resized the popup");
    driven.press_key(chord(Action::Dismiss, Scope::LaunchOverrides));
    driven.frame();
    assert_eq!(popup_rect(&driven), first);
}

/// Enter adds what the line holds and empties it; the first step's field
/// then names the entries; Backspace on an empty line takes the last one
/// back, as the entry's close mark does.
#[test]
fn entries_are_added_named_and_taken_away() {
    let mut popup = popup("overrides-entries");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "DEBUG=1 TRACE=yes");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(
        entries(&driven),
        vec![vec!["DEBUG=1".to_owned()], vec!["TRACE=yes".to_owned()]]
    );
    assert!(
        step_of(&driven)
            .overrides
            .as_ref()
            .unwrap()
            .line
            .text()
            .is_empty()
    );

    driven.press_key(chord(Action::EraseBack, Scope::LaunchOverrides));
    assert_eq!(entries(&driven), vec![vec!["DEBUG=1".to_owned()]]);

    // Back on the first step, the field names what the launch is given.
    driven.press_key(chord(Action::Dismiss, Scope::LaunchOverrides));
    assert!(!in_step(&driven), "esc goes back before it closes");
    let rows = screen(&mut driven);
    let field = rows.iter().find(|row| row.contains(" overrides ")).unwrap();
    assert!(field.contains("DEBUG=1"), "{field}");

    // The close mark is the same taking away.
    open_by_key(&mut driven);
    driven.frame();
    let close = driven.hit(|hit| *hit == WorkspaceHit::OverrideEntryClose(0));
    driven.press(close.x, close.y);
    assert!(entries(&driven).is_empty(), "the close mark took it away");
}

/// A variable UZE owns is not added: it stays in the line in the danger
/// hue, and the detail line says why.
#[test]
fn a_refused_token_stays_in_the_line_and_says_why() {
    let mut popup = popup("overrides-refused");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "DEBUG=1 UZE_HOME=/tmp/x");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(entries(&driven), vec![vec!["DEBUG=1".to_owned()]]);
    let over = step_of(&driven).overrides.as_ref().unwrap();
    assert_eq!(over.line.text(), "UZE_HOME=/tmp/x");
    driven.frame();
    let buffer = frame_buffer_at(&mut driven.attach.model, driven.area);
    let rows = buffer_rows(&buffer);
    let (y, line) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("UZE_HOME=/tmp/x"))
        .expect("the refused token is drawn");
    let x = line.find("UZE_HOME").unwrap();
    let x = line[..x].chars().count() as u16;
    assert_eq!(
        buffer[(x, y as u16)].fg,
        theme::color(Token::StateDanger),
        "in the danger hue"
    );
    assert!(
        rows.iter().any(|row| row.contains("managed by uze")),
        "the detail says why:\n{}",
        rows.join("\n")
    );
}

/// A flag that weakens safety is accepted, and marked on its entry with
/// the attention mark in the warning hue.
#[test]
fn a_weakening_flag_is_marked() {
    let mut popup = popup("overrides-weakens");
    let weakening = {
        let step = popup.model.directory_step.as_ref().unwrap();
        step.catalog()
            .flags()
            .find(|flag| {
                matches!(flag.mark, OverrideMark::Weakens(_)) && flag.value == OverrideValue::Switch
            })
            .expect("a switch that weakens")
            .name
    };
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, weakening);
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(entries(&driven), vec![vec![weakening.to_owned()]]);
    driven.frame();
    let buffer = frame_buffer_at(&mut driven.attach.model, driven.area);
    let rows = buffer_rows(&buffer);
    let attention = theme::glyph(Symbol::MarkAttention);
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains(weakening) && row.contains(&attention))
        .unwrap_or_else(|| panic!("the entry is marked:\n{}", rows.join("\n")));
    let x = row[..row.find(&attention).unwrap()].chars().count() as u16;
    assert_eq!(buffer[(x, y as u16)].fg, theme::color(Token::StateWarning));
    assert!(
        rows.iter().any(|row| row.contains("gives up")),
        "the detail names what is lost:\n{}",
        rows.join("\n")
    );
}

/// A configuration key uze sets on some launches is marked on its entry,
/// and the detail line says, in words, what the operator's value takes
/// the place of.
#[test]
fn a_key_uze_may_set_says_what_it_takes_the_place_of() {
    const LINE: &str = "-c notify=[]";
    let mut popup = popup("overrides-conditional-key");
    {
        let step = popup.model.directory_step.as_mut().unwrap();
        step.harness = step
            .harnesses
            .iter()
            .position(|option| {
                option.overrides.check(LINE).is_ok_and(|checked| {
                    matches!(
                        checked.as_slice(),
                        [entry] if matches!(entry.verdict, uze_application::OverrideVerdict::Degrades(_))
                    )
                })
            })
            .expect("a harness whose configuration flag has a key uze may set");
    }
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, LINE);
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(
        entries(&driven),
        vec![vec!["-c".to_owned(), "notify=[]".to_owned()]]
    );

    let rows = screen(&mut driven);

    assert!(
        rows.iter()
            .any(|row| row.contains("-c turns off uze's own value for this key")),
        "{}",
        rows.join("\n")
    );
}

/// The head of a flag lists its completions with nothing marked; Tab
/// completes it, and after a space the list offers the values it takes.
#[test]
fn tab_completes_a_flag_and_its_values_follow() {
    let mut popup = popup("overrides-complete");
    let flag = {
        let step = popup.model.directory_step.as_ref().unwrap();
        step.catalog()
            .flags()
            .find(|flag| matches!(flag.value, OverrideValue::OneOf(_)))
            .unwrap()
    };
    let OverrideValue::OneOf(values) = flag.value else {
        unreachable!()
    };
    let head = &flag.name[..flag.name.len() - 2];
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, head);
    assert_eq!(step_of(&driven).overrides.as_ref().unwrap().marked, None);
    // The first completion is what Tab takes with nothing marked; mark
    // this flag so the test does not depend on the catalog's order.
    let rows = step_of(&driven).override_rows();
    let at = rows
        .iter()
        .position(|row| *row == super::OverrideRow::Flag(flag))
        .expect("the flag is offered");
    for _ in 0..=at {
        driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    }
    driven.press_key(chord(Action::Expand, Scope::LaunchOverrides));
    let line = step_of(&driven)
        .overrides
        .as_ref()
        .unwrap()
        .line
        .text()
        .to_owned();
    assert_eq!(line, format!("{} ", flag.name));
    let offered = step_of(&driven).override_rows();
    assert_eq!(
        offered,
        values
            .iter()
            .map(|value| super::OverrideRow::Value(value))
            .collect::<Vec<_>>()
    );
    let rows = screen(&mut driven);
    assert!(
        rows.iter().any(|row| row.contains(values[0])),
        "the values are listed:\n{}",
        rows.join("\n")
    );
}

/// The sets used lately head the list with nothing marked, so Enter still
/// launches; another project's says which, a set with a secret held back
/// says so, and taking one replaces the entries.
#[test]
fn recent_sets_are_offered_and_replace_the_entries() {
    let mut popup = popup("overrides-recent");
    let (harness, root) = popup
        .model
        .directory_step
        .as_ref()
        .unwrap()
        .recent_question()
        .unwrap();
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    if let Some(step) = driven.attach.model.directory_step.as_mut() {
        step.recent = Some(RecentOverridesRead {
            harness,
            root,
            sets: vec![
                RecentOverrides {
                    tokens: vec!["HERE=1".to_owned()],
                    withheld: 0,
                    project: PathBuf::from("/nowhere/this-one"),
                    last_used_unix: 2,
                },
                RecentOverrides {
                    tokens: vec!["THERE=1".to_owned()],
                    withheld: 1,
                    project: PathBuf::from("/elsewhere/other-project"),
                    last_used_unix: 1,
                },
            ],
        });
    }
    type_text(&mut driven, "KEPT=1");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    let rows = screen(&mut driven);
    let there = rows
        .iter()
        .find(|row| row.contains("THERE=1"))
        .unwrap_or_else(|| panic!("the set is offered:\n{}", rows.join("\n")));
    assert!(there.contains("other-project"), "{there}");
    assert!(there.contains("1 variable not kept"), "{there}");
    assert_eq!(step_of(&driven).overrides.as_ref().unwrap().marked, None);

    driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(entries(&driven), vec![vec!["THERE=1".to_owned()]]);
}

/// Each harness keeps its own entries in the popup: moving the harness
/// shows its own, never another's.
#[test]
fn each_harness_keeps_its_own_entries() {
    let mut popup = popup("overrides-per-harness");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "DEBUG=1");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    driven.press_key(chord(Action::Dismiss, Scope::LaunchOverrides));
    driven.press_key(chord(Action::FocusNext, Scope::RootPicker));
    assert!(entries(&driven).is_empty(), "another harness has none");
    driven.press_key(chord(Action::FocusPrevious, Scope::RootPicker));
    assert_eq!(entries(&driven), vec![vec!["DEBUG=1".to_owned()]]);
}

/// Enter on an empty line with nothing marked starts the agent, with the
/// entries the step holds.
#[test]
fn enter_on_an_empty_line_launches() {
    let mut popup = popup("overrides-launch");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "DEBUG=1");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert!(
        driven.attach.model.directory_step.is_none(),
        "the popup closed"
    );
    assert!(
        driven.attach.model.placement_pending,
        "the agent was started"
    );
}

/// Every hint on the step's foot, and its heading, is the key it names.
#[test]
fn clicking_an_overrides_hint_is_pressing_its_key() {
    for (action, ready) in [
        (Action::Dismiss, ""),
        (Action::Activate, "DEBUG=1"),
        (Action::Expand, "--"),
    ] {
        let (by_click, by_key) = (popup("overrides-hint-click"), popup("overrides-hint-key"));
        let state = |mut popup: Popup, click: bool| {
            let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
            driven.frame();
            open_by_key(&mut driven);
            type_text(&mut driven, ready);
            if click {
                driven.frame();
                let target = driven
                    .attach
                    .model
                    .hits
                    .iter()
                    .rev()
                    .find(|(_, hit)| *hit == WorkspaceHit::HintAction(action))
                    .map(|(rect, _)| *rect)
                    .unwrap_or_else(|| panic!("{action} is a target"));
                driven.press(target.x, target.y);
            } else {
                driven.press_key(chord(action, Scope::LaunchOverrides));
            }
            let step = step_of(&driven);
            (
                step.overrides
                    .as_ref()
                    .map(|over| over.line.text().to_owned()),
                step.entries().to_vec(),
            )
        };
        assert_eq!(state(by_click, true), state(by_key, false), "{action}");
    }
}

/// Nothing used before and nothing typed: the list says so in one line.
/// A harness with no catalog completes nothing.
#[test]
fn an_empty_history_and_an_empty_catalog_say_little() {
    let mut popup = popup("overrides-empty");
    if let Some(step) = popup.model.directory_step.as_mut() {
        for option in &mut step.harnesses {
            option.overrides = Default::default();
        }
    }
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    let (harness, root) = step_of(&driven).recent_question().unwrap();
    if let Some(step) = driven.attach.model.directory_step.as_mut() {
        step.recent = Some(RecentOverridesRead {
            harness,
            root,
            sets: Vec::new(),
        });
    }
    let rows = screen(&mut driven);
    assert!(
        rows.iter()
            .any(|row| row.contains("no overrides used before")),
        "{}",
        rows.join("\n")
    );
    type_text(&mut driven, "--");
    assert!(step_of(&driven).override_rows().is_empty());
}

/// A long value is cut at its end on the field, the arrow kept; at a
/// narrow width the step still draws its heading and its foot whole.
#[test]
fn long_values_are_cut_at_their_end_and_narrow_widths_hold() {
    let mut popup = popup("overrides-narrow");
    let mut driven = driven(popup.model_take(), &popup.home);
    driven.area = Rect::new(0, 0, 46, 30);
    let rows = screen(&mut driven);
    // A hint that does not fit is left out whole, never cut mid-word.
    for row in &rows {
        if let Some(at) = row.find("ctrl+f") {
            assert!(row[at..].starts_with("ctrl+f overrides"), "{row}");
        }
    }
    open_by_key(&mut driven);
    let long = format!("LONG={}", "x".repeat(80));
    type_text(&mut driven, &long);
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    let rows = screen(&mut driven);
    let heading = format!("{} overrides", theme::glyph(Symbol::StepPrevious));
    assert!(
        rows.iter().any(|row| row.contains(&heading)),
        "the heading is whole:\n{}",
        rows.join("\n")
    );
    assert!(
        rows.iter().any(|row| row.contains("esc back")),
        "the way back is whole:\n{}",
        rows.join("\n")
    );
    driven.press_key(chord(Action::Dismiss, Scope::LaunchOverrides));
    let rows = screen(&mut driven);
    let field = rows.iter().find(|row| row.contains(" overrides ")).unwrap();
    assert!(field.contains("LONG=x"), "{field}");
    assert!(field.contains('…'), "cut at its end: {field}");
    assert!(field.contains(&theme::glyph(Symbol::Submenu)), "{field}");
}

/// In the light theme the refused token still wears that theme's danger
/// hue.
#[test]
fn the_light_theme_colours_the_step() {
    let light = uze_theme::builtin("light").expect("bundled").clone();
    uze_theme::with_active(light, || {
        let mut popup = popup("overrides-light");
        let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
        driven.frame();
        open_by_key(&mut driven);
        type_text(&mut driven, "PATH=/x");
        driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
        driven.frame();
        let buffer = frame_buffer_at(&mut driven.attach.model, driven.area);
        let rows = buffer_rows(&buffer);
        let (y, row) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.contains("PATH=/x"))
            .expect("the line is drawn");
        let x = row[..row.find("PATH").unwrap()].chars().count() as u16;
        assert_eq!(buffer[(x, y as u16)].fg, theme::color(Token::StateDanger));
    });
}

/// A set's close mark forgets it, as Delete does on the set marked: either
/// way a read goes out that forgets it and reads the sets again.
#[test]
fn a_sets_close_mark_and_delete_both_forget_it() {
    for click in [true, false] {
        let mut popup = popup("overrides-forget");
        let question = popup
            .model
            .directory_step
            .as_ref()
            .unwrap()
            .recent_question()
            .unwrap();
        let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
        driven.frame();
        open_by_key(&mut driven);
        if let Some(step) = driven.attach.model.directory_step.as_mut() {
            step.recent = Some(RecentOverridesRead {
                harness: question.0.clone(),
                root: question.1.clone(),
                sets: vec![RecentOverrides {
                    tokens: vec!["GONE=1".to_owned()],
                    withheld: 0,
                    project: PathBuf::from("/elsewhere"),
                    last_used_unix: 1,
                }],
            });
        }
        if click {
            driven.frame();
            let close = driven.hit(|hit| *hit == WorkspaceHit::ForgetOverrides(0));
            driven.press(close.x, close.y);
        } else {
            driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
            driven.press_key(chord(Action::EraseForward, Scope::LaunchOverrides));
        }
        assert_eq!(
            step_of(&driven).recent_asked,
            Some(question),
            "forgetting asked again (click: {click})"
        );
    }
}
