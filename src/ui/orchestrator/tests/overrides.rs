//! The new agent's overrides: the field on the popup's first step, the
//! step that stands in the listing's place, and the agent's overview
//! naming what it was launched with.

use super::WorkspaceHit;
use super::support::*;
use super::{AgentOption, DirectoryStep, RecentOverridesRead, RootPicker, WorkspaceModel, render};
use crate::ui::theme::{self, Symbol, Token};
use ratatui::layout::Rect;
use std::path::PathBuf;
use uze_application::{OverrideMark, OverrideValue, RecentOverride};
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

/// The head's `overrides` segment opens the overrides view on a click,
/// exactly as its key does; the foot names that key too, its hint the
/// same gesture, and with none given the popup says nothing of them.
#[test]
fn the_overrides_segment_and_its_key_open_the_same_view() {
    let mut by_key = popup("overrides-open-key");
    let mut by_key = driven(by_key.model_take(), &by_key.home).on_a_roomy_terminal();
    let (_, rows) = popup_rows(&mut by_key);
    assert!(
        rows[0].contains("new agent") && rows[0].contains("overrides"),
        "{}",
        rows.join("\n")
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("overrides") && !row.contains("ctrl+f"))
            || rows[0].contains("overrides"),
        "{}",
        rows.join("\n")
    );
    assert!(
        rows[1..].iter().all(|row| !row.contains("no overrides")),
        "nothing said of none:\n{}",
        rows.join("\n")
    );
    let foot = rows
        .iter()
        .find(|row| row.contains("harness") && row.contains("enter"))
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
        .find(|(_, hit)| *hit == WorkspaceHit::ViewAction(Action::OpenOverrides))
        .map(|(rect, _)| *rect)
        .expect("the segment is a target");
    by_row.press(row.x + 2, row.y);
    assert!(in_step(&by_row), "a click on the segment opens the view");

    let mut by_hint = popup("overrides-open-hint");
    let mut by_hint = driven(by_hint.model_take(), &by_hint.home).on_a_roomy_terminal();
    by_hint.frame();
    let hint = by_hint
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::HintAction(Action::OpenOverrides))
        .map(|(rect, _)| *rect)
        .expect("the foot's hint is a target");
    by_hint.press(hint.x, hint.y);
    assert!(
        in_step(&by_hint),
        "a click on the foot's hint opens the step"
    );
}

/// The popup's own rows, its border included, cut out of the screen.
fn popup_rows(driven: &mut Driven<'_>) -> (Rect, Vec<String>) {
    let rows = screen(driven);
    let popup = popup_rect(driven);
    let cut = rows[usize::from(popup.y)..usize::from(popup.bottom())]
        .iter()
        .map(|row| {
            row.chars()
                .skip(usize::from(popup.x))
                .take(usize::from(popup.width))
                .collect()
        })
        .collect();
    (popup, cut)
}

/// A row that is one of the popup's hairlines, between its border's sides.
fn is_rule(row: &str) -> bool {
    let inside: Vec<char> = row.chars().skip(1).collect();
    let inside = &inside[..inside.len().saturating_sub(1)];
    !inside.is_empty() && inside.iter().all(|cell| *cell == '─')
}

/// The top border names the popup's two views, `new agent` and
/// `overrides`, where its title would be, with the harness against its
/// end; the body below is the one in front, and at its foot, with air over
/// and under it, a note counts the overrides the launch is given.
#[test]
fn the_head_chooses_the_view_and_a_note_counts_the_overrides() {
    let mut popup = popup("overrides-head");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    let (_, rows) = popup_rows(&mut driven);
    let last = rows.len() - 1;
    let all = rows.join("\n");
    assert!(
        rows[0].contains("new agent") && rows[0].contains("overrides"),
        "{all}"
    );
    assert!(
        rows[1].contains("Claude Code"),
        "the harness at the end:\n{all}"
    );
    assert!(
        !rows[1].contains("harness"),
        "the harness row carries no label:\n{all}"
    );
    assert!(
        rows[3].contains("filter directories"),
        "the line under the harness:\n{all}"
    );
    assert!(
        is_rule(&rows[2]) && is_rule(&rows[4]),
        "a rule under each:\n{all}"
    );
    assert!(
        all.contains("filter directories"),
        "path is in front:\n{all}"
    );
    assert!(is_rule(&rows[last - 2]), "the foot's rule:\n{all}");
    assert!(rows[last - 1].contains("ctrl+f overrides"), "{all}");
    for air in [last - 5, last - 4, last - 3] {
        assert!(
            rows[air].trim_matches('│').trim().is_empty(),
            "with none, the note's rows are air:\n{all}"
        );
    }

    open_by_key(&mut driven);
    type_text(&mut driven, "--model opus");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    type_text(&mut driven, "DEBUG=1");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    let (_, view) = popup_rows(&mut driven);
    let all = view.join("\n");
    assert_eq!(view.len(), rows.len(), "the same height:\n{all}");
    assert!(
        view[0].contains("new agent") && view[0].contains("overrides"),
        "{all}"
    );
    assert!(!all.contains("filter directories"), "{all}");
    assert!(
        all.contains("--model opus") && all.contains("DEBUG=1"),
        "{all}"
    );
    assert!(view[last - 1].contains("esc back"), "{all}");
    assert!(
        view[last - 4].contains("1 flag") && view[last - 4].contains("1 var"),
        "{all}"
    );

    // A click on `new agent` is Esc there: back to the listing.
    driven.frame();
    let path = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::ViewAction(Action::Dismiss))
        .map(|(rect, _)| *rect)
        .expect("the new agent segment is a target");
    driven.press(path.x + 1, path.y);
    assert!(!in_step(&driven), "a click on new agent goes back");
    let (_, rows) = popup_rows(&mut driven);
    assert!(rows.join("\n").contains("filter directories"));
    assert!(rows[last - 4].contains("1 flag") && rows[last - 4].contains("1 var"));
}

/// The segment in front is in the accent and the other held back, lit
/// under the pointer, parted by the separator. Colour alone tells them
/// apart: no ground and no weight; and pointing at one never lights the
/// foot's hint for the same key. In the light theme too.
#[test]
fn the_segments_are_told_apart_by_colour_alone() {
    for name in ["dark", "light"] {
        let palette = uze_theme::builtin(name).expect("bundled").clone();
        uze_theme::with_active(palette, || {
            let mut popup = popup(&format!("overrides-ink-{name}"));
            let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
            driven.frame();
            let segment = driven
                .attach
                .model
                .hits
                .iter()
                .find(|(_, hit)| *hit == WorkspaceHit::ViewAction(Action::OpenOverrides))
                .map(|(rect, _)| *rect)
                .expect("the segment is a target");
            assert_eq!(segment.width, "overrides".len() as u16, "its word");
            let hint = driven
                .attach
                .model
                .hits
                .iter()
                .find(|(_, hit)| *hit == WorkspaceHit::HintAction(Action::OpenOverrides))
                .map(|(rect, _)| *rect)
                .expect("the foot's hint");
            let cells = |driven: &mut Driven<'_>| {
                let buffer = frame_buffer_at(&mut driven.attach.model, driven.area);
                let line = &buffer_rows(&buffer)[usize::from(segment.y)];
                let at = |word: &str| {
                    let column = line[..line.find(word).unwrap()].chars().count() as u16;
                    buffer[(column, segment.y)].clone()
                };
                let separator = theme::glyph(Symbol::HintSeparator);
                assert!(
                    line.contains(&format!("new agent{separator}overrides")),
                    "parted as the keys are: {line}"
                );
                (
                    at("new agent"),
                    at("overrides"),
                    buffer[(hint.x, hint.y)].clone(),
                )
            };
            let (open, other, foot) = cells(&mut driven);
            assert_eq!(open.fg, theme::color(Token::Accent), "{name}");
            assert_eq!(other.fg, theme::color(Token::TextMuted), "{name}");
            assert_eq!(open.bg, other.bg, "no ground for the one in front: {name}");
            assert_eq!(open.modifier, other.modifier, "no weight either: {name}");
            driven.mouse(
                segment.x + 2,
                segment.y,
                crossterm::event::MouseEventKind::Moved,
            );
            let (_, pointed, pointed_foot) = cells(&mut driven);
            assert_eq!(
                pointed.fg,
                theme::color(Token::TextBright),
                "the segment lights: {name}"
            );
            assert_eq!(pointed.bg, other.bg, "in colour alone: {name}");
            assert_eq!(
                pointed_foot.fg, foot.fg,
                "the foot's hint stays put: {name}"
            );
        });
    }
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

    // Back on `new agent`, the note counts what the launch is given.
    driven.press_key(chord(Action::Dismiss, Scope::LaunchOverrides));
    assert!(!in_step(&driven), "esc goes back before it closes");
    let (_, rows) = popup_rows(&mut driven);
    assert!(
        rows.iter().any(|row| row.contains("1 var")),
        "{}",
        rows.join("\n")
    );

    // The close mark is the same taking away.
    open_by_key(&mut driven);
    driven.frame();
    let close = driven.hit(|hit| *hit == WorkspaceHit::OverrideClose(0));
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

/// `words` used lately, last in `project`.
fn used(words: &[&str], project: &str) -> RecentOverride {
    RecentOverride {
        words: words.iter().map(|word| (*word).to_owned()).collect(),
        project: PathBuf::from(project),
        last_used_unix: 1,
    }
}

/// The popup on its overrides view, with `entries` read as used lately on
/// its harness.
fn with_recent(name: &str, entries: Vec<RecentOverride>) -> (Popup, (String, PathBuf)) {
    let mut popup = popup(name);
    let question = popup
        .model
        .directory_step
        .as_ref()
        .unwrap()
        .recent_question()
        .unwrap();
    if let Some(step) = popup.model.directory_step.as_mut() {
        step.open_overrides();
        step.recent = Some(RecentOverridesRead {
            harness: question.0.clone(),
            root: question.1.clone(),
            entries,
        });
    }
    (popup, question)
}

fn words_of(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

/// The entries used lately are a list of choices under the line, one row
/// each with its `○` and its close mark, after the entries added that the
/// history does not hold; another project's says which. Nothing is marked,
/// so Enter still launches; the arrows mark a row, space switches it, and
/// the launch carries what is checked.
#[test]
fn recent_entries_are_choices_space_switches_the_marked_one() {
    let (mut popup, _) = with_recent(
        "overrides-recent",
        vec![
            used(&["--model", "opus"], "/nowhere/this-one"),
            used(&["DEBUG=1"], "/elsewhere/other-project"),
        ],
    );
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    type_text(&mut driven, "KEPT=1");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    let (on, off) = (
        theme::glyph(Symbol::MarkToggleOn),
        theme::glyph(Symbol::MarkToggleOff),
    );
    let close = theme::glyph(Symbol::MarkClose);
    let rows = screen(&mut driven);
    let all = rows.join("\n");
    let at = |text: &str| {
        rows.iter()
            .position(|row| row.contains(text))
            .unwrap_or_else(|| panic!("{text} is offered:\n{all}"))
    };
    assert!(at("KEPT=1") < at("--model opus") && at("--model opus") < at("DEBUG=1"));
    assert!(
        rows[at("KEPT=1")].contains(&format!("{on} KEPT=1")),
        "{all}"
    );
    assert!(
        rows[at("--model opus")].contains(&format!("{off} --model opus")),
        "{all}"
    );
    assert!(rows[at("DEBUG=1")].contains("other-project"), "{all}");
    for text in ["KEPT=1", "--model opus", "DEBUG=1"] {
        assert!(rows[at(text)].contains(&close), "{text} has its close mark");
    }
    assert_eq!(step_of(&driven).overrides.as_ref().unwrap().marked, None);

    // Space with nothing marked is typed.
    driven.press_key(chord(Action::ToggleChoice, Scope::LaunchOverrides));
    assert_eq!(
        step_of(&driven).overrides.as_ref().unwrap().line.text(),
        " "
    );
    driven.press_key(chord(Action::EraseBack, Scope::LaunchOverrides));

    driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    driven.press_key(chord(Action::ToggleChoice, Scope::LaunchOverrides));
    assert_eq!(
        entries(&driven),
        vec![words_of("KEPT=1"), words_of("--model opus")]
    );
    let rows = screen(&mut driven);
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("{on} --model opus"))),
        "{}",
        rows.join("\n")
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("2 flags") || row.contains("1 flag")),
        "the note counts what is checked:\n{}",
        rows.join("\n")
    );

    // Space again leaves it out; it stays offered, as one used before.
    driven.press_key(chord(Action::ToggleChoice, Scope::LaunchOverrides));
    assert_eq!(entries(&driven), vec![words_of("KEPT=1")]);
    assert_eq!(step_of(&driven).choices().len(), 3);

    // An entry added that the history does not hold stays in the list
    // once left out: only its close mark takes it away.
    driven.press_key(chord(Action::SelectPrevious, Scope::LaunchOverrides));
    driven.press_key(chord(Action::ToggleChoice, Scope::LaunchOverrides));
    assert!(entries(&driven).is_empty());
    assert_eq!(step_of(&driven).choices().len(), 3);

    // With a choice marked, Enter still starts the agent.
    driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    driven.press_key(chord(Action::ToggleChoice, Scope::LaunchOverrides));
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert!(
        driven.attach.model.placement_pending,
        "the agent was started"
    );
}

/// A click on a choice's row marks it and leaves it as it was; a click on
/// its mark switches it, as space on it marked does.
#[test]
fn a_click_on_a_choices_mark_is_space_and_on_its_row_marks_it() {
    let state = |click: bool| {
        let (mut popup, _) = with_recent(
            "overrides-toggle-click",
            vec![used(&["--model", "opus"], "/nowhere")],
        );
        let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
        driven.frame();
        let row = driven.hit(|hit| *hit == WorkspaceHit::OverrideRow(0));
        driven.press(row.x + row.width / 2, row.y);
        let marked = step_of(&driven).overrides.as_ref().unwrap().marked;
        assert_eq!(marked, Some(0), "the row is marked");
        assert!(
            entries(&driven).is_empty(),
            "a click on the row switches nothing"
        );
        if click {
            driven.frame();
            let mark = driven.hit(|hit| *hit == WorkspaceHit::OverrideToggle(0));
            driven.press(mark.x, mark.y);
        } else {
            driven.press_key(chord(Action::ToggleChoice, Scope::LaunchOverrides));
        }
        entries(&driven)
    };
    assert_eq!(state(true), state(false));
    assert_eq!(state(true), vec![words_of("--model opus")]);
}

/// The foot names space, and Delete by what it does to the choice marked:
/// `forget` one used before, `remove` one only added; each hint is its
/// key.
#[test]
fn the_foot_names_the_keys_of_a_marked_choice_and_each_is_its_key() {
    let foot = |driven: &mut Driven<'_>| {
        screen(driven)
            .into_iter()
            .find(|row| row.contains("enter new"))
            .expect("the step's foot is drawn")
    };
    {
        let (mut popup, _) = with_recent("overrides-choice-foot", vec![used(&["--verbose"], "/x")]);
        let mut shown = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
        assert!(!foot(&mut shown).contains("toggle"), "{}", foot(&mut shown));
        shown.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
        let said = foot(&mut shown);
        assert!(said.contains("space toggle"), "{said}");
        assert!(said.contains("forget"), "{said}");
    }

    for action in [Action::ToggleChoice, Action::EraseForward] {
        let state = |click: bool| {
            let (mut popup, _) =
                with_recent("overrides-choice-hint", vec![used(&["--verbose"], "/x")]);
            let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
            driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
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
            (step.entries().to_vec(), step.recent_asked.is_some())
        };
        assert_eq!(state(true), state(false), "{action}");
    }
}

/// The overrides view as the operator sees it, with entries used lately,
/// some carried by this launch.
#[test]
fn the_overrides_view_draws_its_choices() {
    let mut popup = popup("overrides-drawn");
    let (harness, root) = popup
        .model
        .directory_step
        .as_ref()
        .unwrap()
        .recent_question()
        .unwrap();
    let here = root.display().to_string();
    if let Some(step) = popup.model.directory_step.as_mut() {
        step.open_overrides();
        step.recent = Some(RecentOverridesRead {
            harness,
            root,
            entries: vec![
                used(&["--model", "opus"], &here),
                used(&["--permission-mode", "plan"], &here),
                used(&["--verbose"], &here),
                used(&["DEBUG=1"], "/elsewhere/api"),
                used(&["--add-dir", "../lib"], &here),
            ],
        });
    }
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    for at in [0, 1, 3] {
        driven
            .attach
            .model
            .directory_step
            .as_mut()
            .unwrap()
            .toggle_choice(at);
    }
    let (_, rows) = popup_rows(&mut driven);
    let drawn = rows.join("\n");
    let (on, off) = (
        theme::glyph(Symbol::MarkToggleOn),
        theme::glyph(Symbol::MarkToggleOff),
    );
    let close = theme::glyph(Symbol::MarkClose);
    let listed: Vec<&String> = rows
        .iter()
        .filter(|row| row.contains(&on) || row.contains(&off))
        .collect();
    assert_eq!(listed.len(), 5, "one row per entry:\n{drawn}");
    assert!(listed[0].contains(&format!("{on} --model opus")), "{drawn}");
    assert!(listed[2].contains(&format!("{off} --verbose")), "{drawn}");
    assert!(
        listed[3].contains("api") && !listed[0].contains("overrides-drawn"),
        "only another project's entry names it:\n{drawn}"
    );
    assert!(listed.iter().all(|row| row.contains(&close)), "{drawn}");
    assert!(
        drawn.contains("2 flags") && drawn.contains("1 var"),
        "{drawn}"
    );
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

/// The harness row is drawn alike on both views, so nothing shifts when
/// they switch, and on the overrides view a click on its arrows changes
/// the harness there: the entries, and the sets used lately, follow it,
/// while the arrow keys keep moving the caret.
#[test]
fn the_harness_changes_on_the_overrides_view_and_the_overrides_follow() {
    let mut popup = popup("overrides-harness-in-view");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    let (_, path) = popup_rows(&mut driven);
    open_by_key(&mut driven);
    let (_, view) = popup_rows(&mut driven);
    assert_eq!(path[1], view[1], "the harness row does not move");

    type_text(&mut driven, "DEBUG=1");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    let first = step_of(&driven).recent_question().unwrap().0;
    driven.frame();
    let next = driven.hit(|hit| *hit == WorkspaceHit::HarnessArrow(1));
    driven.press(next.x, next.y);
    assert!(in_step(&driven), "still on the overrides view");
    let (second, _) = step_of(&driven).recent_question().unwrap();
    assert_ne!(first, second, "the harness changed");
    assert!(entries(&driven).is_empty(), "the other harness has its own");
    assert_eq!(
        step_of(&driven)
            .recent_asked
            .as_ref()
            .map(|(harness, _)| harness),
        Some(&second),
        "its sets used lately are asked for"
    );

    driven.frame();
    let back = driven.hit(|hit| *hit == WorkspaceHit::HarnessArrow(-1));
    driven.press(back.x, back.y);
    assert_eq!(entries(&driven), vec![vec!["DEBUG=1".to_owned()]]);
}

/// While a free value is typed there is nothing to list, so the list's
/// room says what the flag is and that other users can read its value,
/// once, centred as an empty state is, and the detail row stays empty.
#[test]
fn a_free_value_being_typed_is_said_centred_in_the_lists_room() {
    let mut popup = popup("overrides-free-value");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "--model ");
    let (_, rows) = popup_rows(&mut driven);
    let all = rows.join("\n");
    let said: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.contains("other users can see flag values"))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(said.len(), 1, "said once:\n{all}");
    let at = said[0];
    assert!(at > 6 && at < rows.len() - 6, "centred down:\n{all}");
    let row = &rows[at];
    let inside = &row[row.find('│').unwrap() + '│'.len_utf8()..row.rfind('│').unwrap()];
    let text = inside.trim();
    let (before, after) = inside.split_once(text).unwrap();
    assert!(
        before.chars().count().abs_diff(after.chars().count()) <= 1,
        "centred across: {row}"
    );
    assert!(
        !rows[at - 1].trim_matches('│').trim().is_empty(),
        "what the flag is, over it:\n{all}"
    );
}

/// A flag added in the popup stays in the list once unchecked, left out
/// of the launch but there to check again; only its close mark takes it
/// out of the list.
#[test]
fn an_unchecked_flag_stays_listed_until_its_close_mark() {
    let mut popup = popup("overrides-unchecked-stays");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "--verbose");
    driven.press_key(chord(Action::Activate, Scope::LaunchOverrides));
    assert_eq!(entries(&driven), vec![vec!["--verbose".to_owned()]]);

    driven.frame();
    let toggle = driven.hit(|hit| *hit == WorkspaceHit::OverrideToggle(0));
    driven.press(toggle.x, toggle.y);
    assert!(entries(&driven).is_empty(), "left out of the launch");
    let (_, rows) = popup_rows(&mut driven);
    assert!(
        rows.iter().any(|row| row.contains("--verbose")),
        "still listed:\n{}",
        rows.join("\n")
    );

    driven.frame();
    let toggle = driven.hit(|hit| *hit == WorkspaceHit::OverrideToggle(0));
    driven.press(toggle.x, toggle.y);
    assert_eq!(
        entries(&driven),
        vec![vec!["--verbose".to_owned()]],
        "checked again"
    );

    driven.frame();
    let close = driven.hit(|hit| *hit == WorkspaceHit::OverrideClose(0));
    driven.press(close.x, close.y);
    assert!(entries(&driven).is_empty());
    let (_, rows) = popup_rows(&mut driven);
    assert!(
        !rows.iter().any(|row| row.contains("--verbose")),
        "gone once closed:\n{}",
        rows.join("\n")
    );
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
        (Action::Activate, ""),
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
            let step = driven.attach.model.directory_step.as_ref();
            (
                step.and_then(|step| step.overrides.as_ref())
                    .map(|over| over.line.text().to_owned()),
                step.map(|step| step.entries().to_vec()),
                driven.attach.model.placement_pending,
            )
        };
        assert_eq!(state(by_click, true), state(by_key, false), "{action}");
    }
}

/// The foot names Enter by what it does there and then: start the agent
/// on an empty line with nothing marked, take the row marked, or add the
/// line typed.
#[test]
fn the_foot_names_enter_by_what_it_does() {
    let mut popup = popup("overrides-enter-word");
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    let foot = |driven: &mut Driven<'_>| {
        screen(driven)
            .into_iter()
            .find(|row| row.contains("esc back"))
            .expect("the step's foot is drawn")
    };

    assert!(
        foot(&mut driven).contains("enter new"),
        "{}",
        foot(&mut driven)
    );
    type_text(&mut driven, "--");
    assert!(
        foot(&mut driven).contains("enter add"),
        "{}",
        foot(&mut driven)
    );
    driven.press_key(chord(Action::SelectNext, Scope::LaunchOverrides));
    assert!(
        foot(&mut driven).contains("enter take"),
        "{}",
        foot(&mut driven)
    );
}

/// Completion reads the word at the caret, wherever the caret is in the
/// line, and Tab completes that word in place, leaving the caret after it.
#[test]
fn completion_follows_the_caret_not_the_end_of_the_line() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut popup = popup("overrides-caret-completion");
    let effort = {
        let step = popup.model.directory_step.as_mut().unwrap();
        step.harness = step
            .harnesses
            .iter()
            .position(|option| option.overrides.flag("--effort").is_some())
            .expect("a harness offering --effort");
        step.catalog().flag("--effort").unwrap()
    };
    let mut driven = driven(popup.model_take(), &popup.home).on_a_roomy_terminal();
    driven.frame();
    open_by_key(&mut driven);
    type_text(&mut driven, "--debug --mod");
    driven.press_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    type_text(&mut driven, "--eff");

    assert_eq!(
        step_of(&driven).override_rows(),
        [super::OverrideRow::Flag(effort)],
        "the list completes the word at the caret"
    );
    driven.press_key(chord(Action::Expand, Scope::LaunchOverrides));

    let line = &step_of(&driven).overrides.as_ref().unwrap().line;
    assert_eq!(line.text(), "--effort --debug --mod");
    assert_eq!(line.caret, "--effort ".len());
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
            entries: Vec::new(),
        });
    }
    let (_, rows) = popup_rows(&mut driven);
    let at = rows
        .iter()
        .position(|row| row.contains("no overrides used before"))
        .unwrap_or_else(|| panic!("{}", rows.join("\n")));
    let row = &rows[at];
    let (before, after) = row.split_once("no overrides used before").unwrap();
    let (before, after) = (before.chars().count(), after.chars().count());
    assert!(before.abs_diff(after) <= 1, "centred across: {row}");
    assert!(
        at > 6 && at < rows.len() - 6,
        "centred down the list:\n{}",
        rows.join("\n")
    );
    type_text(&mut driven, "--");
    assert!(step_of(&driven).override_rows().is_empty());
}

/// At a narrow width the head keeps both segments and the harness, the
/// foot its keys whole, and a long value is cut at its end in the view.
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
    let (_, rows) = popup_rows(&mut driven);
    assert!(
        rows[0].contains("new agent") && rows[0].contains("overrides"),
        "the segments are whole:\n{}",
        rows.join("\n")
    );
    let entry = rows
        .iter()
        .find(|row| row.contains("LONG=x"))
        .unwrap_or_else(|| panic!("{}", rows.join("\n")));
    assert!(entry.contains('…'), "cut at its end: {entry}");
    assert!(
        rows.iter().any(|row| row.contains("esc back")),
        "the way back is whole:\n{}",
        rows.join("\n")
    );
    driven.press_key(chord(Action::Dismiss, Scope::LaunchOverrides));
    let (popup, rows) = popup_rows(&mut driven);
    assert_eq!(popup.width, 46, "the popup gives way to the terminal");
    assert!(rows[1].contains("Claude Code"), "{}", rows.join("\n"));
    assert!(
        rows.iter().any(|row| row.contains("1 var")),
        "{}",
        rows.join("\n")
    );
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

/// A recent entry's close mark forgets it and leaves it out of the launch,
/// as Delete does on the entry marked: either way a read goes out that
/// forgets it and reads the entries again.
#[test]
fn a_recent_entrys_close_mark_and_delete_both_forget_it() {
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
                entries: vec![used(&["GONE=1"], "/elsewhere")],
            });
        }
        if click {
            driven.frame();
            let close = driven.hit(|hit| *hit == WorkspaceHit::OverrideClose(0));
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
