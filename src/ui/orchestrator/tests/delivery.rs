//! A task's work state on the strip and its delivery: the status a click
//! answers, the instruction it sends, and the delivery button.

use super::WorkspaceHit;
use super::support::*;
use super::{
    DeliveryResolution, UnspelledGates, WorkStateView, WorkspaceModel, agent_activity_frame,
    render::{LeadingSlot, render_tab_strip, task_mark},
};
use crate::ui::theme::{self, Token};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use std::time::Instant;
use uze_application::CompletionBehavior;
use uze_core::UzeHome;
use uze_terminal::{ClientRequest, PaneId};

#[test]
fn a_ready_task_names_its_row_and_marks_it() {
    let model = agent_with_task(WorkStateView::Ready, 3);
    let rows = sidebar(&model, &identities_fixture()).rows;
    let name_row = rows
        .iter()
        .find(|row| row.contains("Agent"))
        .expect("the agent names its own row: {rows:?}");
    let (ready, _) = task_mark(&WorkStateView::Ready).expect("ready is marked");
    assert!(
        name_row.contains(&ready),
        "ready carries its own mark: {name_row}"
    );
    assert!(
        rows.iter().any(|row| row.contains("agent")),
        "and what runs it reads underneath it: {rows:?}"
    );

    let (rows, hits) = tab_strip(&model);
    assert!(rows.iter().any(|row| row.contains("→ main ↑3")), "{rows:?}");
    assert!(
        hits.iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_))),
        "ready work is an action with no instruction declared: a click delivers it"
    );
}

/// Uncommitted changes are already reported by the change count, which
/// opens them on a click; a status beside it is drawn only to send the
/// project's `on_change`, and then it is an action.
#[test]
fn uncommitted_work_shows_a_status_only_when_the_project_asks_something_there() {
    let model = agent_with_task(WorkStateView::Uncommitted, 0);
    let (rows, hits) = tab_strip(&model);
    assert!(
        !rows.iter().any(|row| row.contains("uncommitted")),
        "{rows:?}"
    );
    assert!(
        !hits.iter().any(|(_, hit)| matches!(
            hit,
            WorkspaceHit::StatusAction(_) | WorkspaceHit::StatusReport(_)
        )),
        "nothing to report the count does not already"
    );

    let model = answered(model, "Commit your work.");
    let (rows, hits) = tab_strip(&model);
    assert!(
        rows.iter().any(|row| row.contains("uncommitted")),
        "{rows:?}"
    );
    assert!(
        hits.iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_))),
        "declared, it is the control that sends the instruction"
    );
}

/// The work's own model, with `instruction` declared for every state.
fn answered(mut model: WorkspaceModel, instruction: &str) -> WorkspaceModel {
    for task in model.remembered.tasks.values_mut().flatten() {
        task.action = Some(instruction.to_owned());
    }
    model
}

/// The label cell's foreground, its background, and whether it is bold.
fn status_cell(model: &WorkspaceModel, rect: Rect) -> (Color, Color, bool) {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    terminal
        .draw(|frame| {
            render_tab_strip(
                frame,
                frame.area(),
                model,
                &identities_fixture(),
                LeadingSlot::AgentTabs,
                &mut Vec::new(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let cell = &buffer[(rect.x + 1, rect.y)];
    (
        cell.fg,
        cell.bg,
        cell.modifier.contains(ratatui::style::Modifier::BOLD),
    )
}

/// What a click answers and what it does not read apart at rest, by
/// weight and hue, and neither stands on a ground of its own: told
/// apart by the shade of a fill, the two read alike in a terminal.
#[test]
fn a_status_a_click_answers_is_bold_and_lifts_and_a_report_never_reacts() {
    let mut action = answered(
        agent_with_task(WorkStateView::Published, 3),
        "Open a pull request.",
    );
    let tab = action.selected_tab().expect("a selected tab");
    let rect = hit_rect(&action, WorkspaceHit::StatusAction(tab));
    let (fg, bg, bold) = status_cell(&action, rect);
    assert_eq!(
        fg,
        theme::color(Token::StatePublished),
        "the state's full hue"
    );
    assert!(bold, "and the only bold status");
    for ground in [
        Token::SurfaceRaised,
        Token::SurfaceHover,
        Token::SurfaceRecessed,
    ] {
        assert_ne!(bg, theme::color(ground), "no ground of its own");
    }
    action.hovered = Some(WorkspaceHit::StatusAction(tab));
    assert_eq!(
        status_cell(&action, rect).0,
        theme::lifted(Token::StatePublished),
        "under the pointer it lifts, keeping its hue"
    );

    let mut report = agent_with_task(WorkStateView::Published, 3);
    let rect = hit_rect(&report, WorkspaceHit::StatusReport(tab));
    let at_rest = status_cell(&report, rect);
    assert_eq!(at_rest.0, theme::color(Token::StatePublishedMuted));
    assert!(!at_rest.2, "a report is not bold");
    report.hovered = Some(WorkspaceHit::StatusReport(tab));
    assert_eq!(
        status_cell(&report, rect),
        at_rest,
        "and nothing about it moves under the pointer"
    );
}

/// The header draws reports in the same row as its controls, and the
/// shape has to tell them apart: a report is recessed and answers no
/// pointer, where a button is raised and does.
#[test]
fn a_report_in_the_actions_row_is_not_dressed_as_a_button() {
    let mut model = agent_with_task(WorkStateView::Integrating, 3);
    model.tick = a_tick_showing_the_fullest_frame();
    let (rows, hits) = tab_strip(&model);
    let row = rows.join("\n");
    assert!(
        row.contains(&format!("→ main {}", agent_activity_frame(model.tick))),
        "the report names the delivery and turns while it runs: {row}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_))),
        "a delivery in flight answers no click"
    );

    let spinner = agent_activity_frame(model.tick);
    let column = rows[0]
        .find(&spinner)
        .map(|index| rows[0][..index].chars().count() as u16)
        .expect("the report is on the strip");
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
    terminal
        .draw(|frame| {
            render_tab_strip(
                frame,
                frame.area(),
                &model,
                &identities_fixture(),
                LeadingSlot::AgentTabs,
                &mut Vec::new(),
            );
        })
        .unwrap();
    let cell = &terminal.backend().buffer()[(column, 0)];
    assert_eq!(cell.fg, theme::color(Token::StateInFlightMuted));
    assert_ne!(
        cell.bg,
        theme::color(Token::SurfaceRecessed),
        "a report wears no ground of its own"
    );
}

/// Where the work goes is the subject, whatever the project's
/// delivery: a request is a fact of the remote, and until there is
/// one the work is headed for the target.
#[test]
fn the_chip_names_where_the_work_goes_whatever_the_delivery() {
    let strip = |completion, request| {
        let mut model = agent_with_task(WorkStateView::Ready, 3);
        for task in model.remembered.tasks.values_mut().flatten() {
            task.completion = completion;
            task.published_request = request;
        }
        let (rows, _) = tab_strip(&model);
        rows.join("\n")
    };

    for completion in [
        None,
        Some(CompletionBehavior::Merge),
        Some(CompletionBehavior::Push),
    ] {
        let drawn = strip(completion, None);
        assert!(drawn.contains("→ main ↑3"), "{completion:?}: {drawn}");
        let drawn = strip(completion, Some(41));
        assert!(drawn.contains("#41"), "{completion:?}: {drawn}");
        assert!(
            !drawn.contains("PR") && !drawn.contains("MR"),
            "a number needs no word in front of it: {drawn}"
        );
    }
}

/// `Ready` is the one state whose sidebar mark the strip may not
/// borrow. That mark is the vocabulary's "there is work to hand
/// over", and the patched set draws it as a create-a-request icon —
/// true for the completion that opens one, a lie in front of a button
/// about to fast-forward the target or to touch nothing outside the
/// branch. The strip says the commits instead, which is what a press
/// sends whatever the ending.
#[test]
fn a_ready_task_is_marked_in_the_sidebar_but_never_by_that_mark_on_the_strip() {
    let state = WorkStateView::Ready;
    let model = agent_with_task(state.clone(), 3);
    let (mark, _) = super::render::task_mark(&state).expect("ready is marked");
    let sidebar = sidebar(&model, &identities_fixture()).rows;
    assert!(
        sidebar.iter().any(|row| row.contains(mark.trim())),
        "{sidebar:#?}"
    );
    let (rows, _) = tab_strip(&model);
    let strip = rows.join("\n");
    assert!(strip.contains("→ main ↑3"), "{strip}");
    assert!(
        !strip.contains(mark.trim()),
        "and never the mark that would claim a request this press does not open: {strip}"
    );
}

/// `pr` is two actions over a task's life, and the button is how the
/// operator tells them apart: an errand while no request exists, a
/// sync onto a named one once it does.
#[test]
fn a_published_request_turns_the_delivery_button_into_a_sync() {
    let mut model = agent_with_task(WorkStateView::Ready, 4);
    for task in model.remembered.tasks.values_mut().flatten() {
        task.completion = Some(CompletionBehavior::Push);
    }
    let (before, _) = tab_strip(&model);
    let before = before.join("\n");
    assert!(before.contains("→ main ↑4"), "{before}");
    assert!(
        !before.contains(&crate::ui::theme::glyph(
            crate::ui::theme::Symbol::TaskReady
        )),
        "the button's words say what a press does; no mark in front of them: {before}"
    );

    for task in model.remembered.tasks.values_mut().flatten() {
        task.published_request = Some(11);
    }
    let (after, _) = tab_strip(&model);
    let after = after.join("\n");
    assert!(after.contains("#11 ↑4"), "{after}");
    assert!(
        !after.contains("→ main ↑4"),
        "a request that exists is named by its number, not by the target: {after}"
    );
}

/// The count on the button is what pressing it would send, and a
/// branch level with its request would send nothing. Counting commits
/// against the target instead left `6 #20` standing on a request that
/// already carried all six — a merge's question asked of a sync.
#[test]
fn a_branch_level_with_its_request_reports_the_sync_instead_of_a_count() {
    let mut model = agent_with_task(WorkStateView::Published, 6);
    for task in model.remembered.tasks.values_mut().flatten() {
        task.completion = Some(CompletionBehavior::Push);
        task.published_as = Some("fix-auth-redirect".into());
        task.published_request = Some(20);
        task.unsynced = Some(0);
    }
    let (synced, hits) = tab_strip(&model);
    let synced = synced.join("\n");
    assert!(synced.contains("#20 ↗"), "{synced}");
    assert!(
        !synced.contains("↑6"),
        "the target is still six commits away, and that is not this button's question: {synced}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_))),
        "with no instruction declared, nothing answers a click"
    );

    // Two commits later the button counts those two, not the six the
    // request has carried since the last sync.
    for task in model.remembered.tasks.values_mut().flatten() {
        task.state = WorkStateView::Ready;
        task.unsynced = Some(2);
    }
    let (behind_by_two, _) = tab_strip(&model);
    let behind_by_two = behind_by_two.join("\n");
    assert!(behind_by_two.contains("#20 ↑2"), "{behind_by_two}");
}

/// The sidebar and the header answer the same question, so they had
/// better answer it the same way. Reading only `Ready`, the row went
/// on wearing the "there is work to hand over" mark for the whole life
/// of an open request, one column away from a button that had already
/// stopped saying it.
#[test]
fn a_published_task_is_marked_as_gone_not_as_waiting_to_be_delivered() {
    let mut model = agent_with_task(WorkStateView::Published, 6);
    for task in model.remembered.tasks.values_mut().flatten() {
        task.completion = Some(CompletionBehavior::Push);
        task.published_request = Some(20);
        task.unsynced = Some(0);
    }
    let rows = sidebar(&model, &identities_fixture()).rows;
    let name_row = rows
        .iter()
        .find(|row| row.contains("Agent"))
        .expect("the agent names its own row");
    let (published, _) = task_mark(&WorkStateView::Published).expect("published is marked");
    let (ready, _) = task_mark(&WorkStateView::Ready).expect("ready is marked");
    assert!(
        name_row.contains(&published) && !name_row.contains(&ready),
        "the work is with its reviewer, not waiting on the operator: {name_row}"
    );
}

/// The one state no evaluation can ever report: `Integrating` is set
/// in memory by `landing::deliver` and overwritten by the outcome
/// before the store is saved, so a surface reading only the record
/// showed `ready` for the whole delivery — a gate may take half an
/// hour. The client that started it is the party that knows.
#[test]
fn a_delivery_in_flight_is_drawn_from_the_client_that_started_it() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    let task = model
        .remembered
        .tasks
        .values()
        .flatten()
        .next()
        .expect("the fixture has a task")
        .clone();
    assert_eq!(model.drawn_state(&task), WorkStateView::Ready);

    model.remembered.delivery_pending.insert(task.id.clone());
    assert_eq!(model.drawn_state(&task), WorkStateView::Integrating);

    let (delivering, _) = task_mark(&WorkStateView::Integrating).expect("delivering is marked");
    let rows = sidebar(&model, &identities_fixture()).rows;
    assert!(
        rows.iter().any(|row| row.contains(&delivering)),
        "the row says a delivery is running: {rows:?}"
    );

    let (strip, hits) = tab_strip(&model);
    let strip = strip.join("\n");
    assert!(
        strip.contains(&format!("→ main {}", agent_activity_frame(model.tick))),
        "{strip}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_))),
        "and nothing answers a click while it runs"
    );
}

/// A delivery that came back with nothing still releases the task it
/// was started for.
///
/// Releasing by walking the reports is only correct while there is
/// always a report, and three ordinary endings produce none: the
/// checkout was removed under the agent, the store no longer holds
/// the id, the application would not open. Each of those left the
/// task drawn as "delivering" for the rest of the session —
/// undeliverable again, and repainting every 120 ms forever, because
/// a pending delivery is one of the three things that keep the
/// spinner's clock turning.
/// A project whose gate this machine cannot run is said when its space
/// opens, once, with the gate and the spelling it lacks.
#[test]
fn a_gate_this_machine_cannot_run_is_said_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-unspelled-gate"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
    for _ in 0..2 {
        driven
            .attach
            .channels
            .unspelled_gates
            .sender
            .send(UnspelledGates {
                project: PathBuf::from("/repo"),
                gates: vec!["make check".to_owned()],
                platform: "windows",
            })
            .unwrap();
    }
    driven.pump();

    let toasts = &driven.attach.model.remembered.toasts;
    assert_eq!(toasts.len(), 1, "said once");
    assert_eq!(toasts[0].text, "gate can't run here");
    assert!(
        toasts[0].detail.contains("`make check`") && toasts[0].detail.contains("`windows`"),
        "{}",
        toasts[0].detail
    );
}

/// A target that new agents cannot be cut ahead of is said when the
/// project falls behind, not on every pass while the remote keeps moving, and
/// said again only after it caught up and fell behind once more.
#[test]
fn a_target_left_behind_is_said_once_until_it_catches_up() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-target-sync"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 1), &home);
    let behind = |commits: usize| uze_application::TargetSyncReport {
        project: PathBuf::from("/repo"),
        concern: Some(format!(
            "`main` carries commits `origin` does not and is {commits} commits behind it"
        )),
    };
    let caught_up = uze_application::TargetSyncReport {
        project: PathBuf::from("/repo"),
        concern: None,
    };
    let sender = &driven.attach.channels.target_syncs.sender;
    for report in [behind(1), behind(2), caught_up, behind(3)] {
        sender.send(report).unwrap();
    }
    driven.pump();

    let toasts = &driven.attach.model.remembered.toasts;
    assert_eq!(toasts.len(), 2, "once per time it fell behind");
    assert_eq!(toasts[0].text, "target diverged");
    assert!(
        toasts[1].detail.contains("3 commits"),
        "{}",
        toasts[1].detail
    );
}

#[test]
fn a_delivery_that_answered_nothing_still_gives_the_task_back() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-delivery-silence"));
    let mut driven = driven(agent_with_task(WorkStateView::Ready, 3), &home);
    let task = driven
        .attach
        .model
        .remembered
        .tasks
        .values()
        .flatten()
        .next()
        .expect("the fixture has a task")
        .clone();

    driven
        .attach
        .model
        .remembered
        .delivery_pending
        .insert(task.id.clone());
    assert_eq!(
        driven.attach.model.drawn_state(&task),
        WorkStateView::Integrating
    );

    driven
        .attach
        .channels
        .deliveries
        .sender
        .send(DeliveryResolution {
            cwd: PathBuf::from("/repo/.worktrees/ai"),
            reserved: Some(task.id.clone()),
            reports: Vec::new(),
        })
        .unwrap();
    driven.pump();

    assert_eq!(
        driven.attach.model.drawn_state(&task),
        task.state,
        "the task is drawn from its record again"
    );
    assert!(
        driven.attach.model.remembered.delivery_pending.is_empty(),
        "and nothing is left holding the spinner on"
    );
}

/// The strip at `width` columns, as rows and hits.
fn tab_strip_at(model: &WorkspaceModel, width: u16) -> (Vec<String>, Vec<(Rect, WorkspaceHit)>) {
    let mut terminal = Terminal::new(TestBackend::new(width, 3)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render_tab_strip(
                frame,
                frame.area(),
                model,
                &identities_fixture(),
                LeadingSlot::AgentTabs,
                &mut hits,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    (rows, hits)
}

/// Under the pointer an answered status names the instruction a click
/// sends, by its key — never a sentence cut short — where the notice
/// stands, and nothing the operator can press moves to make room.
#[test]
fn hovering_an_answered_status_names_the_instruction_it_sends() {
    let instruction = "Open a pull request for this branch against main, then end your turn.";
    let mut model = answered(agent_with_task(WorkStateView::Published, 3), instruction);
    let tab = model.selected_tab().expect("a selected tab");
    let (_, before) = tab_strip(&model);
    model.hovered = Some(WorkspaceHit::StatusAction(tab));
    let (rows, after) = tab_strip(&model);
    let strip = rows.join("\n");
    assert!(strip.contains("send on_publish"), "{strip}");
    assert!(!strip.contains("Open a pull request"), "{strip}");
    assert_eq!(before, after, "no control moves for the hint");
}

/// Ready work no instruction answers is a report, and the one thing
/// the pointer resting on it is told is the key that delivers it.
#[test]
fn hovering_ready_work_with_no_instruction_names_the_deliver_key() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    let tab = model.selected_tab().expect("a selected tab");
    model.hovered = Some(WorkspaceHit::StatusAction(tab));
    let chord = uze_keys::active()
        .chord_for(uze_keys::Action::DeliverTask, &[uze_keys::Scope::Workspace])
        .expect("delivery is bound");
    let (rows, _) = tab_strip(&model);
    assert!(
        rows.join("\n").contains(&format!("deliver {chord}")),
        "{rows:?}"
    );
}

/// Work in review reads as the request's number alone: the number says
/// a request is open, and a mark beside it would only repeat it.
#[test]
fn work_in_review_shows_its_request_number_and_no_mark() {
    let mut model = agent_with_task(WorkStateView::InReview, 3);
    for task in model.remembered.tasks.values_mut().flatten() {
        task.published_request = Some(259);
    }
    let (rows, _) = tab_strip(&model);
    let strip = rows.join("\n");
    let (mark, _) = task_mark(&WorkStateView::InReview).expect("in review is marked");
    assert!(strip.contains("#259"), "{strip}");
    assert!(!strip.contains(&format!("#259 {mark}")), "{strip}");
}

/// A report says under the pointer what its mark stands for, and only
/// in the notice's place: the status itself does not react.
#[test]
fn hovering_a_report_says_what_its_mark_means() {
    let mut model = agent_with_task(WorkStateView::InReview, 3);
    let tab = model.selected_tab().expect("a selected tab");
    let (_, before) = tab_strip(&model);
    model.hovered = Some(WorkspaceHit::StatusReport(tab));
    let (rows, after) = tab_strip(&model);
    let strip = rows.join("\n");
    assert!(strip.contains("in review · request open"), "{strip}");
    assert_eq!(before, after, "no control moves for the hint");
}

/// A strip with no room for the hint drops it whole: nothing is drawn
/// in its place, and the controls stay where they were.
#[test]
fn a_narrow_strip_drops_the_hint() {
    let instruction = "Open a pull request for this branch against main.";
    let mut model = answered(agent_with_task(WorkStateView::Published, 3), instruction);
    let tab = model.selected_tab().expect("a selected tab");
    let (_, before) = tab_strip_at(&model, 44);
    model.hovered = Some(WorkspaceHit::StatusAction(tab));
    let (rows, after) = tab_strip_at(&model, 44);
    assert!(!rows.join("\n").contains("send on_"), "{rows:?}");
    assert_eq!(before, after, "no control moves");
}

/// The bytes the client sent into `pane`, every submission of them.
fn typed_into(driven: &mut Driven<'_>, pane: PaneId) -> Vec<String> {
    driven
        .sent()
        .into_iter()
        .filter_map(|request| match request {
            ClientRequest::Input { pane: into, bytes } if into == pane => {
                Some(String::from_utf8_lossy(&bytes).into_owned())
            }
            _ => None,
        })
        .collect()
}

/// A click on a status the project answers sends its instruction into
/// the agent's pane once, ended with Enter, and delivers nothing.
#[test]
fn a_click_on_an_answered_status_sends_its_instruction_once() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-status-click"));
    let model = answered(
        agent_with_task(WorkStateView::Published, 3),
        "Open a pull request.",
    );
    let tab = model.selected_tab().expect("a selected tab");
    let pane = model.tab(tab).expect("the tab").pane.id;
    let mut driven = driven(model, &home);
    let rect = hit_rect(&driven.attach.model, WorkspaceHit::StatusAction(tab));
    driven.frame();
    driven.press(rect.x, rect.y);
    driven.press(rect.x, rect.y);

    let typed = typed_into(&mut driven, pane);
    assert_eq!(
        typed.len(),
        1,
        "a second click is refused as busy: {typed:?}"
    );
    assert!(typed[0].contains("Open a pull request."), "{typed:?}");
    assert!(typed[0].ends_with('\r'), "submitted, not staged: {typed:?}");
    assert!(
        driven.attach.model.remembered.delivery_pending.is_empty(),
        "a click on an instruction delivers nothing"
    );
    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(said.contains("sent to agent"), "{said}");
    assert!(said.contains("agent is busy"), "{said}");
}

/// Ends a delivery of the fixture's task with `outcome`, the task's
/// view in `state` carrying `action`, and answers what was typed into
/// its agent's pane.
fn delivered_with(
    label: &str,
    outcome: uze_application::DeliveryOutcome,
    state: WorkStateView,
    action: Option<&str>,
    busy: bool,
) -> (Vec<String>, String) {
    let home = UzeHome::at(uze_testkit::temp::scratch(label));
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    let tab = model.selected_tab().expect("a selected tab");
    let pane = model.tab(tab).expect("the tab").pane.id;
    if busy {
        model
            .remembered
            .agent_activity
            .entry(pane)
            .or_default()
            .working_until = Some(Instant::now() + std::time::Duration::from_secs(60));
    }
    let mut task = task_in("/repo/.worktrees/ai", "Agent", state, 3);
    task.action = action.map(str::to_owned);
    let mut driven = driven(model, &home);
    driven
        .attach
        .channels
        .deliveries
        .sender
        .send(DeliveryResolution {
            cwd: PathBuf::from("/repo/.worktrees/ai"),
            reserved: Some(task.id.clone()),
            reports: vec![uze_application::DeliveryReport {
                task,
                outcome,
                warnings: Vec::new(),
            }],
        })
        .unwrap();
    driven.pump();
    let said = format!("{:?}", driven.attach.model.toast_stack());
    (typed_into(&mut driven, pane), said)
}

/// An operation of UZE's that leaves the work in a state sends that
/// state's instruction once, and nothing when the project declares
/// none for it.
#[test]
fn a_delivery_sends_the_instruction_of_the_state_it_left() {
    use uze_application::DeliveryOutcome;
    let (typed, _) = delivered_with(
        "orchestrator-left-failed",
        DeliveryOutcome::Failed {
            command: "cargo test".into(),
            log: None,
        },
        WorkStateView::GateFailed,
        Some("Fix the checks."),
        false,
    );
    assert_eq!(typed.len(), 1, "{typed:?}");
    assert!(typed[0].contains("Fix the checks."), "{typed:?}");

    let (typed, _) = delivered_with(
        "orchestrator-left-published",
        DeliveryOutcome::Published {
            branch: "feat/x".into(),
            request: None,
        },
        WorkStateView::Published,
        Some("Open a pull request."),
        false,
    );
    assert_eq!(typed.len(), 1, "{typed:?}");

    let (typed, _) = delivered_with(
        "orchestrator-left-undeclared",
        DeliveryOutcome::Published {
            branch: "feat/x".into(),
            request: None,
        },
        WorkStateView::Published,
        None,
        false,
    );
    assert!(
        typed.is_empty(),
        "nothing declared, nothing typed: {typed:?}"
    );

    let (typed, _) = delivered_with(
        "orchestrator-left-merged",
        DeliveryOutcome::Merged,
        WorkStateView::Integrated,
        Some("Summarize what changed."),
        false,
    );
    assert!(
        typed.is_empty(),
        "a merge leaves the work delivered, which asks nothing: {typed:?}"
    );

    let (typed, _) = delivered_with(
        "orchestrator-left-refused",
        DeliveryOutcome::Refused("no delivery".into()),
        WorkStateView::Ready,
        Some("Review your diff."),
        false,
    );
    assert!(
        typed.is_empty(),
        "a refusal left the work where it was, so nothing is sent: {typed:?}"
    );
}

/// An automatic send into an agent mid-turn is held back rather than
/// queued, and says so.
#[test]
fn an_automatic_send_into_a_busy_agent_is_held_back() {
    let (typed, said) = delivered_with(
        "orchestrator-left-busy",
        uze_application::DeliveryOutcome::Failed {
            command: "cargo test".into(),
            log: None,
        },
        WorkStateView::GateFailed,
        Some("Fix the checks."),
        true,
    );
    assert!(typed.is_empty(), "{typed:?}");
    assert!(said.contains("held back"), "{said}");
}

/// A click on ready work delivers it, as the deliver key does, and
/// types nothing into the agent's pane.
#[test]
fn a_click_on_ready_work_delivers_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-ready-click"));
    let model = agent_with_task(WorkStateView::Ready, 3);
    let tab = model.selected_tab().expect("a selected tab");
    let pane = model.tab(tab).expect("the tab").pane.id;
    let mut driven = driven(model, &home);
    let rect = hit_rect(&driven.attach.model, WorkspaceHit::StatusAction(tab));
    driven.frame();
    driven.press(rect.x, rect.y);

    assert!(
        !driven.attach.model.remembered.delivery_pending.is_empty(),
        "the click started the delivery"
    );
    assert!(typed_into(&mut driven, pane).is_empty());
}

/// A click on a status nothing answers sends nothing and delivers
/// nothing.
#[test]
fn a_click_on_a_report_does_nothing() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-report-click"));
    let model = agent_with_task(WorkStateView::InReview, 3);
    let tab = model.selected_tab().expect("a selected tab");
    let pane = model.tab(tab).expect("the tab").pane.id;
    let mut driven = driven(model, &home);
    let rect = hit_rect(&driven.attach.model, WorkspaceHit::StatusReport(tab));
    driven.frame();
    driven.press(rect.x, rect.y);

    assert!(typed_into(&mut driven, pane).is_empty());
    assert!(driven.attach.model.remembered.delivery_pending.is_empty());
}

/// Text the operator typed and has not sent is theirs: an instruction
/// is refused rather than sent on top of it, and the buffer is left as
/// it was.
#[test]
fn an_instruction_is_refused_over_text_the_operator_typed() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-status-typed"));
    let mut model = answered(
        agent_with_task(WorkStateView::Published, 3),
        "Open a pull request.",
    );
    let tab = model.selected_tab().expect("a selected tab");
    let pane = model.tab(tab).expect("the tab").pane.id;
    let mut buffer = super::PromptBuffer::default();
    buffer.paste("half a thought");
    model.remembered.prompt_buffers.insert(pane, buffer);
    let mut driven = driven(model, &home);
    let rect = hit_rect(&driven.attach.model, WorkspaceHit::StatusAction(tab));
    driven.frame();
    driven.press(rect.x, rect.y);

    assert!(typed_into(&mut driven, pane).is_empty());
    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(said.contains("you have text typed"), "{said}");
    assert_eq!(
        driven.attach.model.remembered.prompt_buffers[&pane]
            .characters
            .iter()
            .collect::<String>(),
        "half a thought"
    );

    driven.attach.model.remembered.prompt_buffers.insert(
        pane,
        super::PromptBuffer {
            trusted: false,
            ..Default::default()
        },
    );
    driven.press(rect.x, rect.y);
    assert!(typed_into(&mut driven, pane).is_empty());
    let said = format!("{:?}", driven.attach.model.toast_stack());
    assert!(said.contains("can't tell what's typed"), "{said}");
}

#[test]
fn a_running_task_offers_no_delivery_and_carries_no_mark() {
    let model = agent_with_task(WorkStateView::Running, 0);
    let rows = sidebar(&model, &identities_fixture()).rows;
    let name_row = rows.iter().find(|row| row.contains("Agent")).unwrap();
    assert!(
        !name_row.contains('\u{2713}') && !name_row.contains('\u{26a0}'),
        "{name_row}"
    );
    let (rows, hits) = tab_strip(&model);
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("merge →") || row.contains("pr →")),
        "{rows:?}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_)))
    );
}

#[test]
fn a_conflicted_task_is_marked_and_reported_but_not_a_button() {
    let model = agent_with_task(
        WorkStateView::Conflicted {
            files: vec![PathBuf::from("src/lib.rs")],
        },
        2,
    );
    let rows = sidebar(&model, &identities_fixture()).rows;
    let name_row = rows.iter().find(|row| row.contains("Agent")).unwrap();
    let (conflict, _) =
        task_mark(&WorkStateView::Conflicted { files: Vec::new() }).expect("a conflict is marked");
    assert!(name_row.contains(&conflict), "{name_row}");
    let (rows, hits) = tab_strip(&model);
    assert!(
        rows.iter()
            .any(|row| row.contains(&format!("→ main {}", conflict.trim()))),
        "the strip wears the same mark the row does: {rows:?}"
    );
    assert!(
        !hits
            .iter()
            .any(|(_, hit)| matches!(hit, WorkspaceHit::StatusAction(_)))
    );
}
