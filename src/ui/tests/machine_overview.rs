//! The Overview: what every harness on the machine carries.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use uze_application::machine_overview::{
    HarnessReport, Location, MachineOverview, Origin, OverviewResource, ResourceKind,
    SessionCounts, Status, Uses,
};

use super::support::{buffer_rows, drawn_at};
use crate::ui::hit::Hit;
use crate::ui::machine::{Column, Field, Group, Line, Pane};
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
        uses_known: true,
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
        checked: true,
        removal: None,
        uses: Default::default(),
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
fn the_menu_lists_every_kind_with_its_count_and_what_needs_looking_at() {
    let model = model();
    let drawn = rows(&model);
    let skills = drawn
        .iter()
        .find(|row| row.contains("skills"))
        .expect("skills");
    assert!(skills.contains('2'), "{skills}");
    let hooks = drawn.iter().find(|row| row.contains("hooks")).unwrap();
    assert!(
        hooks.contains("! 1"),
        "a broken hook marks its section: {hooks}"
    );
    assert!(
        drawn
            .iter()
            .any(|row| row.contains("rules") && row.contains('0')),
        "a kind with nothing still has its place: {drawn:#?}"
    );
    assert!(drawn.iter().any(|row| row.contains("every harness")));
    assert!(drawn.iter().any(|row| row.contains("30 days")));
}

#[test]
fn the_table_shows_the_section_chosen() {
    let model = model();
    let drawn = rows(&model);
    assert!(drawn.iter().any(|row| row.contains("git:commit")));
    assert!(drawn.iter().any(|row| row.contains("vercel:nextjs")));
    assert!(
        !drawn.iter().any(|row| row.contains("planner")),
        "an agent is not a skill"
    );
}

#[test]
fn every_row_says_where_it_stands() {
    let mut model = model();
    let drawn = rows(&model);
    let commit = drawn.iter().find(|row| row.contains("git:commit")).unwrap();
    assert!(
        commit.contains("ok"),
        "a checked resource reads ok: {commit}"
    );
    key(&mut model, KeyCode::Down);
    key(&mut model, KeyCode::Down);
    key(&mut model, KeyCode::Down);
    let drawn = rows(&model);
    let guard = drawn.iter().find(|row| row.contains("lint-guard")).unwrap();
    assert!(guard.contains("broken"), "{guard}");
}

#[test]
fn a_resource_nothing_can_judge_claims_no_check() {
    let mut model = model();
    let mut overview = overview();
    overview.resources[0].checked = false;
    model.remembered.machine.overview = Some(overview);
    let drawn = rows(&model);
    let commit = drawn.iter().find(|row| row.contains("git:commit")).unwrap();
    assert!(!commit.contains("ok"), "{commit}");
}

#[test]
fn the_arrows_walk_the_menu_and_step_into_its_table() {
    let mut model = model();
    assert_eq!(model.remembered.machine.pane, Pane::Menu);
    key(&mut model, KeyCode::Down);
    assert_eq!(model.machine_section(), Group::Kind(ResourceKind::Agent));
    key(&mut model, KeyCode::Right);
    assert_eq!(model.remembered.machine.pane, Pane::Table);
    assert!(matches!(model.machine_line(), Some(Line::Resource(_))));
    key(&mut model, KeyCode::Left);
    assert_eq!(model.remembered.machine.pane, Pane::Menu);
}

#[test]
fn clicking_a_section_is_walking_to_it() {
    let mut clicked = model();
    let mut pressed = model();
    click(
        &mut clicked,
        &Hit::MachineSection(Group::Kind(ResourceKind::Agent)),
    );
    key(&mut pressed, KeyCode::Down);
    assert_eq!(clicked.machine_section(), pressed.machine_section());
    assert_eq!(
        clicked.remembered.machine.pane,
        pressed.remembered.machine.pane
    );
}

#[test]
fn clicking_a_row_is_stepping_onto_it() {
    let mut clicked = model();
    let mut pressed = model();
    click(&mut clicked, &Hit::MachineLine(1));
    key(&mut pressed, KeyCode::Right);
    key(&mut pressed, KeyCode::Down);
    assert_eq!(clicked.machine_line(), pressed.machine_line());
    assert_eq!(clicked.remembered.machine.pane, Pane::Table);
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
fn a_harness_narrows_the_menu_the_table_and_the_totals() {
    let mut model = model();
    model.remembered.machine.harness = Some("codex".to_owned());
    let drawn = rows(&model);
    assert!(drawn.iter().any(|row| row.contains("Codex")));
    assert!(drawn.iter().any(|row| row.contains("git:commit")));
    assert!(
        !drawn.iter().any(|row| row.contains("vercel:nextjs")),
        "Claude's only"
    );
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
    assert_eq!(model.remembered.machine.field, Field::Columns);
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
fn uzes_own_findings_are_a_section_only_when_there_are_some() {
    let model = model();
    assert!(!model.machine_sections().contains(&Group::Uze));
}

fn on_resource(model: &mut TuiModel, kind: ResourceKind, name: &str) {
    model.remembered.machine.section = Some(Group::Kind(kind));
    model.remembered.machine.pane = Pane::Table;
    let at = model
        .machine_lines()
        .iter()
        .position(|line| {
            matches!(line, Line::Resource(index)
                if model.remembered.machine.resource(*index).is_some_and(|r| r.name == name))
        })
        .unwrap();
    model.remembered.machine.list.selected = at;
}

#[test]
fn enter_on_a_harness_plugins_resource_types_its_removal_and_runs_nothing() {
    let mut model = model();
    on_resource(&mut model, ResourceKind::Skill, "vercel:nextjs");
    assert_eq!(
        key(&mut model, KeyCode::Enter),
        Intent::TypeInShell("claude plugin uninstall vercel@official".to_owned())
    );
}

#[test]
fn enter_on_what_uze_installed_shows_it_in_the_marketplace() {
    let mut model = model();
    on_resource(&mut model, ResourceKind::Skill, "git:commit");
    key(&mut model, KeyCode::Enter);
    assert_eq!(model.route, Route::Plugins);
}

#[test]
fn the_panel_says_why_a_resource_is_broken() {
    let mut model = model();
    on_resource(&mut model, ResourceKind::Hook, "lint-guard");
    let drawn = rows(&model);
    assert!(
        drawn.iter().any(|row| row.contains("script missing")),
        "{drawn:#?}"
    );
}

#[test]
fn the_search_narrows_the_table_by_name_origin_or_status() {
    let mut model = model();
    model.remembered.machine.list.filter = "vercel".to_owned();
    assert_eq!(model.machine_lines().len(), 1);
    model.remembered.machine.section = Some(Group::Kind(ResourceKind::Hook));
    model.remembered.machine.list.filter = "broken".to_owned();
    assert_eq!(model.machine_lines().len(), 1);
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

fn names(model: &TuiModel) -> Vec<String> {
    model
        .machine_lines()
        .iter()
        .filter_map(|line| match line {
            Line::Resource(index) => model
                .remembered
                .machine
                .resource(*index)
                .map(|r| r.name.clone()),
            Line::Finding(_) => None,
        })
        .collect()
}

#[test]
fn a_heading_orders_the_table_and_a_second_click_turns_it_round() {
    let mut model = model();
    let mut overview = overview();
    overview.resources[0].per_session = Some(10);
    model.remembered.machine.overview = Some(overview);
    click(&mut model, &Hit::MachineSort(Column::Name));
    assert_eq!(names(&model), ["git:commit", "vercel:nextjs"]);
    click(&mut model, &Hit::MachineSort(Column::Name));
    assert_eq!(names(&model), ["vercel:nextjs", "git:commit"]);
    click(&mut model, &Hit::MachineSort(Column::PerSession));
    assert_eq!(
        names(&model),
        ["vercel:nextjs", "git:commit"],
        "the heaviest first"
    );
}

#[test]
fn the_headings_are_reached_by_the_keyboard_too() {
    let mut clicked = model();
    let mut pressed = model();
    click(&mut clicked, &Hit::MachineSort(Column::Name));
    for _ in 0..3 {
        key(&mut pressed, KeyCode::Tab);
    }
    assert_eq!(pressed.remembered.machine.field, Field::Columns);
    assert_eq!(
        pressed.remembered.machine.column,
        Column::Status,
        "the keys land on the column the table is ordered by"
    );
    key(&mut pressed, KeyCode::Right);
    assert_eq!(
        pressed.remembered.machine.column,
        Column::Name,
        "wrapped round"
    );
    key(&mut pressed, KeyCode::Enter);
    assert_eq!(
        pressed.remembered.machine.sort,
        clicked.remembered.machine.sort
    );
}

#[test]
fn the_column_ordered_by_carries_its_chevron() {
    let model = model();
    let drawn = rows(&model);
    let heading = drawn
        .iter()
        .find(|row| row.contains("name") && row.contains("per session"))
        .unwrap();
    let ascending = crate::ui::theme::glyph(crate::ui::theme::Symbol::SortAscending);
    assert!(
        heading.contains(&format!("status {ascending}")),
        "{heading}"
    );
}

#[test]
fn an_empty_section_says_so_in_the_middle_of_its_room_with_no_headings() {
    let mut model = model();
    model.remembered.machine.section = Some(Group::Kind(ResourceKind::Rule));
    let drawn = rows(&model);
    assert!(drawn.iter().any(|row| row.contains("NO RULES")));
    assert!(
        !drawn
            .iter()
            .any(|row| row.contains("per session") && row.contains("name")),
        "nothing to head"
    );
    let at = drawn
        .iter()
        .position(|row| row.contains("NO RULES"))
        .unwrap();
    assert!(
        at > 8,
        "centred down the room, not on its first row: row {at}"
    );
}

/// `resource` used `days_30` times in 30 days on each harness named.
fn used_on(resource: &mut OverviewResource, harnesses: &[&str], days_30: u64, estimate: bool) {
    for harness in harnesses {
        resource.uses.insert(
            (*harness).to_owned(),
            Uses {
                days_7: days_30,
                days_30,
                days_90: days_30,
                estimate,
                complete: true,
            },
        );
    }
}

#[test]
fn each_skill_says_how_often_it_was_used_in_the_period() {
    let mut model = model();
    let mut overview = overview();
    used_on(
        &mut overview.resources[0],
        &["claude-code", "codex"],
        6,
        false,
    );
    used_on(&mut overview.resources[1], &["claude-code"], 3, true);
    model.remembered.machine.overview = Some(overview);
    let drawn = rows(&model);
    let header = drawn
        .iter()
        .find(|row| row.contains("per session") && row.contains("status"))
        .unwrap();
    assert!(header.contains("uses"), "{header}");
    let commit = drawn.iter().find(|row| row.contains("git:commit")).unwrap();
    assert!(commit.contains(" 12 "), "every harness's uses: {commit}");
    let vercel = drawn
        .iter()
        .find(|row| row.contains("vercel:nextjs"))
        .unwrap();
    assert!(vercel.contains("~3"), "a partly inferred count: {vercel}");
}

#[test]
fn a_harness_narrows_the_uses_to_its_own() {
    let mut model = model();
    let mut overview = overview();
    used_on(&mut overview.resources[0], &["claude-code"], 9, false);
    used_on(&mut overview.resources[0], &["codex"], 0, false);
    model.remembered.machine.overview = Some(overview);
    model.remembered.machine.harness = Some("codex".to_owned());
    let drawn = rows(&model);
    let commit = drawn.iter().find(|row| row.contains("git:commit")).unwrap();
    assert!(
        commit.contains("unused"),
        "nobody used it on Codex: {commit}"
    );
    assert!(!commit.contains(" 9 "), "{commit}");
}

#[test]
fn a_skill_nobody_used_reads_unused_only_when_every_harness_was_read() {
    let mut model = model();
    let mut overview = overview();
    used_on(
        &mut overview.resources[0],
        &["claude-code", "codex"],
        0,
        false,
    );
    used_on(&mut overview.resources[1], &[], 0, false);
    model.remembered.machine.overview = Some(overview);
    let drawn = rows(&model);
    let commit = drawn.iter().find(|row| row.contains("git:commit")).unwrap();
    assert!(commit.contains("unused"), "{commit}");
    let vercel = drawn
        .iter()
        .find(|row| row.contains("vercel:nextjs"))
        .unwrap();
    assert!(
        !vercel.contains("unused"),
        "a harness left unread: {vercel}"
    );
    assert_eq!(
        names(&model),
        ["git:commit", "vercel:nextjs"],
        "unused comes before what is fine"
    );
}

#[test]
fn uses_nobody_could_read_are_unknown() {
    let model = model();
    let drawn = rows(&model);
    let commit = drawn.iter().find(|row| row.contains("git:commit")).unwrap();
    assert!(commit.contains(" ? "), "{commit}");
}

#[test]
fn the_uses_heading_orders_the_most_used_first() {
    let mut model = model();
    let mut overview = overview();
    used_on(&mut overview.resources[0], &["claude-code"], 1, false);
    used_on(&mut overview.resources[1], &["claude-code"], 5, false);
    model.remembered.machine.overview = Some(overview);
    click(&mut model, &Hit::MachineSort(Column::Uses));
    assert_eq!(names(&model), ["vercel:nextjs", "git:commit"]);
}

#[test]
fn the_panel_says_what_a_use_count_rests_on() {
    let mut model = model();
    let mut overview = overview();
    used_on(&mut overview.resources[0], &["claude-code"], 4, true);
    model.remembered.machine.overview = Some(overview);
    on_resource(&mut model, ResourceKind::Skill, "git:commit");
    let drawn = rows(&model);
    assert!(
        drawn
            .iter()
            .any(|row| row.contains("4 in 30 days · partly inferred · some")),
        "{drawn:#?}"
    );
}
