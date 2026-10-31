//! The Overview: what every harness on the machine carries.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use uze_application::machine_overview::{
    HarnessReport, Location, MachineOverview, Origin, OverviewResource, ResourceKind,
    SessionCounts, Status,
};

use super::support::{buffer_rows, drawn_at};
use crate::ui::hit::Hit;
use crate::ui::machine::{Field, Group, Line};
use crate::ui::model::{Focus, Route, TuiModel};
use crate::ui::worker::Intent;

fn harness(id: &str, name: &str, sessions: u64) -> HarnessReport {
    HarnessReport {
        id: id.to_owned(),
        name: name.to_owned(),
        inventory_known: true,
        sessions: Some(SessionCounts {
            days_7: sessions / 4,
            days_30: sessions,
            days_90: sessions * 2,
        }),
    }
}

fn resource(
    kind: ResourceKind,
    name: &str,
    origin: Origin,
    harnesses: &[&str],
) -> OverviewResource {
    OverviewResource {
        id: format!("{name}:{kind:?}"),
        kind,
        name: name.to_owned(),
        origin,
        locations: harnesses
            .iter()
            .map(|harness| Location {
                harness: (*harness).to_owned(),
                path: PathBuf::from(format!("/home/me/.{harness}/{name}/SKILL.md")),
            })
            .collect(),
        description: Some(format!("What {name} does.")),
        per_session: Some(120),
        when_used: Some(2400),
        status: Status::Ok,
        removal: None,
    }
}

fn overview() -> MachineOverview {
    let mut broken = resource(ResourceKind::Hook, "lint-guard", Origin::You, &["codex"]);
    broken.status = Status::Broken {
        reason: "script missing · /home/me/bin/lint-guard.sh".to_owned(),
    };
    broken.per_session = None;
    broken.when_used = None;
    let mut vercel = resource(
        ResourceKind::Skill,
        "vercel:nextjs",
        Origin::Plugin {
            name: "vercel".into(),
        },
        &["claude-code"],
    );
    vercel.removal = Some("claude plugin uninstall vercel@official".to_owned());
    MachineOverview {
        harnesses: vec![
            harness("claude-code", "Claude Code", 40),
            harness("codex", "Codex", 5),
        ],
        resources: vec![
            resource(
                ResourceKind::Skill,
                "git:commit",
                Origin::Uze {
                    package: "git@uze".into(),
                },
                &["claude-code", "codex"],
            ),
            vercel,
            resource(ResourceKind::Agent, "planner", Origin::You, &["codex"]),
            broken,
        ],
    }
}

fn model() -> TuiModel {
    let mut model = TuiModel {
        route: Route::Overview,
        focus: Focus::Content,
        ..TuiModel::default()
    };
    model.remembered.machine.overview = Some(overview());
    model
}

fn key(model: &mut TuiModel, code: KeyCode) -> Intent {
    model.apply_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn rows(model: &TuiModel) -> Vec<String> {
    buffer_rows(&drawn_at(model, 180, 36).0)
}

fn click(model: &mut TuiModel, target: &Hit) -> Intent {
    let (_, hits) = drawn_at(model, 180, 36);
    let (rect, _) = *hits
        .iter()
        .find(|(_, hit)| hit == target)
        .unwrap_or_else(|| panic!("{target:?} is drawn as a target"));
    model.hits = hits;
    model.apply_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 180, 36),
    )
}

#[test]
fn it_opens_folded_with_each_kinds_totals() {
    let model = model();
    let drawn = rows(&model);
    let skills = drawn
        .iter()
        .find(|row| row.contains("skills  2"))
        .expect("the skills row");
    assert!(skills.contains("2"), "{skills}");
    assert!(
        skills.contains("~240"),
        "the kind's tokens per session: {skills}"
    );
    let hooks = drawn.iter().find(|row| row.contains("hooks  1")).unwrap();
    assert!(hooks.contains("1 broken"), "{hooks}");
    assert!(
        !drawn.iter().any(|row| row.contains("git:commit")),
        "folded: no resource row is drawn"
    );
    assert!(drawn.iter().any(|row| row.contains("every harness")));
    assert!(drawn.iter().any(|row| row.contains("30 days")));
}

#[test]
fn the_arrows_unfold_step_in_and_fold_again() {
    let mut model = model();
    assert_eq!(
        model.machine_line(),
        Some(Line::Group(Group::Kind(ResourceKind::Skill)))
    );
    key(&mut model, KeyCode::Right);
    assert!(rows(&model).iter().any(|row| row.contains("git:commit")));
    key(&mut model, KeyCode::Right);
    assert!(
        matches!(model.machine_line(), Some(Line::Resource(_))),
        "stepped in"
    );
    key(&mut model, KeyCode::Left);
    assert_eq!(
        model.machine_line(),
        Some(Line::Group(Group::Kind(ResourceKind::Skill))),
        "back to its group"
    );
    key(&mut model, KeyCode::Left);
    assert!(!rows(&model).iter().any(|row| row.contains("git:commit")));
}

#[test]
fn clicking_a_group_is_pressing_enter_on_it() {
    let mut clicked = model();
    let mut pressed = model();
    click(&mut clicked, &Hit::MachineLine(1));
    key(&mut pressed, KeyCode::Down);
    key(&mut pressed, KeyCode::Enter);
    assert_eq!(clicked.machine_lines(), pressed.machine_lines());
    assert!(
        clicked
            .machine_lines()
            .iter()
            .any(|line| matches!(line, Line::Resource(_))),
        "the first click unfolds"
    );
}

#[test]
fn clicking_a_fields_arrow_is_turning_it_with_the_keyboard() {
    let mut clicked = model();
    let mut pressed = model();
    click(&mut clicked, &Hit::MachineField(Field::Harness, true));
    key(&mut pressed, KeyCode::Tab);
    key(&mut pressed, KeyCode::Right);
    assert_eq!(
        clicked.remembered.machine.harness.as_deref(),
        Some("claude-code")
    );
    assert_eq!(
        clicked.remembered.machine.harness,
        pressed.remembered.machine.harness
    );
}

#[test]
fn a_harness_narrows_every_row_and_total() {
    let mut model = model();
    model.remembered.machine.harness = Some("codex".to_owned());
    let drawn = rows(&model);
    assert!(drawn.iter().any(|row| row.contains("Codex")));
    let skills = drawn.iter().find(|row| row.contains("skills  1")).unwrap();
    assert!(skills.contains("~120"), "only Codex's skill: {skills}");
    assert!(
        drawn.iter().any(|row| row.contains("5 sessions")),
        "{drawn:#?}"
    );
}

#[test]
fn the_period_sets_what_sessions_are_counted_over() {
    let mut model = model();
    key(&mut model, KeyCode::Tab);
    key(&mut model, KeyCode::Tab);
    key(&mut model, KeyCode::Right);
    let drawn = rows(&model);
    assert!(drawn.iter().any(|row| row.contains("90 days")));
    assert!(
        drawn.iter().any(|row| row.contains("90 sessions")),
        "{drawn:#?}"
    );
}

#[test]
fn tab_walks_the_fields_and_reaches_the_screens() {
    let mut model = model();
    key(&mut model, KeyCode::Tab);
    assert_eq!(model.remembered.machine.field, Field::Harness);
    key(&mut model, KeyCode::Tab);
    assert_eq!(model.remembered.machine.field, Field::Period);
    key(&mut model, KeyCode::Tab);
    assert_eq!(model.focus, Focus::Sidebar, "on to the screens' strip");
}

#[test]
fn ten_harnesses_take_the_room_of_four() {
    let mut model = model();
    let four = rows(&model);
    let mut many = overview();
    many.harnesses = (0..10)
        .map(|n| harness(&format!("h{n}"), &format!("Harness {n}"), 1))
        .collect();
    model.remembered.machine.overview = Some(many);
    let ten = rows(&model);
    let heading = |drawn: &[String]| {
        drawn
            .iter()
            .find(|row| row.contains("name") && row.contains("per session"))
            .cloned()
            .unwrap()
    };
    assert_eq!(heading(&four), heading(&ten));
}

#[test]
fn uzes_own_findings_head_the_tree_only_when_there_are_some() {
    let model = model();
    assert!(
        !model.machine_lines().contains(&Line::Group(Group::Uze)),
        "no findings, no group"
    );
}

#[test]
fn enter_on_a_harness_plugins_resource_types_its_removal_and_runs_nothing() {
    let mut model = model();
    model
        .remembered
        .machine
        .set_open(Group::Kind(ResourceKind::Skill), true);
    let at = model
        .machine_lines()
        .iter()
        .position(|line| {
            matches!(line, Line::Resource(index)
                if model.remembered.machine.resource(*index).is_some_and(|r| r.name == "vercel:nextjs"))
        })
        .unwrap();
    model.remembered.machine.list.selected = at;
    assert_eq!(
        key(&mut model, KeyCode::Enter),
        Intent::TypeInShell("claude plugin uninstall vercel@official".to_owned())
    );
}

#[test]
fn enter_on_what_uze_installed_shows_it_in_the_marketplace() {
    let mut model = model();
    model
        .remembered
        .machine
        .set_open(Group::Kind(ResourceKind::Skill), true);
    key(&mut model, KeyCode::Down);
    assert!(matches!(model.machine_line(), Some(Line::Resource(_))));
    key(&mut model, KeyCode::Enter);
    assert_eq!(model.route, Route::Plugins);
}

#[test]
fn the_panel_says_why_a_resource_is_broken() {
    let mut model = model();
    model
        .remembered
        .machine
        .set_open(Group::Kind(ResourceKind::Hook), true);
    let at = model
        .machine_lines()
        .iter()
        .position(|line| {
            matches!(line, Line::Resource(_)) && {
                let Line::Resource(index) = line else {
                    unreachable!()
                };
                model.remembered.machine.resource(*index).unwrap().name == "lint-guard"
            }
        })
        .unwrap();
    model.remembered.machine.list.selected = at;
    let drawn = rows(&model);
    assert!(
        drawn.iter().any(|row| row.contains("script missing")),
        "{drawn:#?}"
    );
}

#[test]
fn the_filter_finds_by_name_origin_or_status() {
    let mut model = model();
    model.remembered.machine.list.filter = "broken".to_owned();
    let lines = model.machine_lines();
    assert_eq!(lines.len(), 2, "the hooks group, open, and its broken hook");
    model.remembered.machine.list.filter = "vercel".to_owned();
    assert!(
        rows(&model).iter().any(|row| row.contains("vercel:nextjs")),
        "a search opens what it finds"
    );
}

#[test]
fn another_screen_in_front_collects_nothing_and_stops_what_runs() {
    let mut model = model();
    model.route = Route::Plugins;
    assert_eq!(
        model.machine_intent(std::time::Instant::now()),
        Intent::None
    );
    model.route = Route::Overview;
    let start = std::time::Instant::now();
    assert_eq!(model.machine_intent(start), Intent::None, "not settled yet");
    let Intent::CollectMachineOverview { cancel, .. } =
        model.machine_intent(start + crate::ui::machine::SETTLE)
    else {
        panic!("a settled Overview collects");
    };
    model.route = Route::Harnesses;
    model.machine_intent(start + crate::ui::machine::SETTLE * 2);
    assert!(
        cancel.0.load(std::sync::atomic::Ordering::Relaxed),
        "leaving the Overview stops the collection"
    );
}

#[test]
fn the_first_frame_draws_what_was_collected_without_asking_anything() {
    let model = model();
    // Drawing reads only the model: every harness could be hung and this
    // frame would still be drawn from what the last collection left.
    let drawn = rows(&model);
    assert!(drawn.iter().any(|row| row.contains("skills")));
    assert!(model.remembered.machine.collecting.is_none());
}
