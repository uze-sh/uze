//! Recent: what it lists and in what order, what a row says selected and
//! not, the header that leads into it, and what each key does to a row.

use super::workspace_tests::{driven, key_event, model_of, preserved, session};
use super::*;
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};
use std::time::SystemTime;
use uze_application::{CheckoutOwner, CheckoutView, CheckoutsView, PreservedWork};

const PROJECT: &str = "/work/project";
const OTHER: &str = "/work/other";

fn checkout_of(task: &PreservedWork) -> CheckoutView {
    CheckoutView {
        path: task.checkout.clone().expect("the task kept a checkout"),
        owner: CheckoutOwner::Agent {
            holder: Some(task.label.clone()),
            live: false,
        },
        task: Some(task.id.clone()),
        branch: Some(task.branch.clone()),
        dirty: false,
        ahead: 3,
        last_changed: None,
        bytes: 5 * 1024 * 1024 * 1024,
    }
}

fn subagent_checkout(task: Option<&str>) -> CheckoutView {
    CheckoutView {
        path: Path::new(PROJECT).join(".worktrees/child"),
        owner: CheckoutOwner::Subagent {
            parent: "parser".to_owned(),
            parent_id: "p1".to_owned(),
            topic: Some("lexer".to_owned()),
            joinable: true,
        },
        task: task.map(str::to_owned),
        branch: Some("agent/child".to_owned()),
        dirty: false,
        ahead: 1,
        last_changed: None,
        bytes: 1024,
    }
}

fn changed(task: PreservedWork, seconds_ago: u64) -> PreservedWork {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    PreservedWork {
        created_at_unix: now - seconds_ago,
        last_active_unix: now - seconds_ago,
        ..task
    }
}

/// A model with a space open on the project, `kept` the machine's kept
/// work and `checkouts` the project's last read.
fn showing(kept: Vec<PreservedWork>, checkouts: Vec<CheckoutView>) -> WorkspaceModel {
    let mut model = model_of(session(PROJECT));
    model.remembered.preserved_work = kept;
    model.remembered.checkouts_asked = 1;
    model.recent.reads.insert(
        PathBuf::from(PROJECT),
        ProjectRead {
            asked: 1,
            answer: Some(Some(CheckoutsView {
                primary: PathBuf::from(PROJECT),
                checkouts,
            })),
        },
    );
    model.recent.open = true;
    model
}

fn sidebar_buffer(model: &WorkspaceModel, height: u16) -> ratatui::buffer::Buffer {
    sidebar_buffer_at(model, 44, height)
}

fn sidebar_buffer_at(model: &WorkspaceModel, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                model,
                &[],
                &mut Vec::new(),
                &mut render::FrameMetrics::default(),
            )
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn drawn(model: &WorkspaceModel) -> Vec<String> {
    drawn_at(model, 44)
}

fn drawn_at(model: &WorkspaceModel, width: u16) -> Vec<String> {
    let buffer = sidebar_buffer_at(model, width, 30);
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn row_of(lines: &[String], text: &str) -> usize {
    lines
        .iter()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("{text:?} is drawn:\n{}", lines.join("\n")))
}

fn press(driven: &mut super::workspace_tests::Driven<'_>, action: Action) {
    let chord = uze_keys::active()
        .chord_for(
            action,
            &[uze_keys::Scope::Workspace, uze_keys::Scope::Recent],
        )
        .unwrap_or_else(|| panic!("{action} is bound"));
    driven.press_key(key_event(chord));
}

fn reasons(model: &WorkspaceModel) -> Vec<(String, Reason)> {
    recent_rows(model)
        .into_iter()
        .map(|row| (row.name, row.reason))
        .collect()
}

#[test]
fn every_state_lands_in_its_block_with_its_reason_and_its_move() {
    let cases = [
        (
            WorkStateView::Conflicted {
                files: vec![PathBuf::from("a"), PathBuf::from("b")],
            },
            Reason::Conflict { files: 2 },
            Some("resume"),
        ),
        (
            WorkStateView::GateFailed,
            Reason::ChecksFailed,
            Some("resume"),
        ),
        (
            WorkStateView::Uncommitted,
            Reason::Uncommitted,
            Some("resume"),
        ),
        (WorkStateView::Ready, Reason::Ready, Some("resume")),
        (
            WorkStateView::Integrating,
            Reason::Delivering,
            Some("resume"),
        ),
        (
            WorkStateView::Published,
            Reason::WithReviewer,
            Some("resume"),
        ),
        (WorkStateView::Shelved, Reason::Shelved, Some("resume")),
        (WorkStateView::Running, Reason::NoChanges, Some("resume")),
        // Ended with nothing to deliver, listed for its conversation.
        (WorkStateView::Closed, Reason::NoChanges, Some("resume")),
    ];
    for (state, reason, primary) in cases {
        let model = showing(vec![preserved(PROJECT, "t1", "work", state)], Vec::new());
        let rows = recent_rows(&model);
        assert_eq!(rows[0].reason, reason);
        assert_eq!(rows[0].primary(), primary, "{reason:?}");
        assert_eq!(
            rows[0].delivers(),
            reason == Reason::Ready,
            "only ready work offers delivery: {reason:?}"
        );
        assert_eq!(rows[0].taking_away(), Some("discard"));
    }
}

/// Uncommitted changes the checkout holds are what the row waits on,
/// whatever state the record last carried.
#[test]
fn a_checkout_holding_changes_reads_as_uncommitted() {
    let task = preserved(PROJECT, "t1", "parked", WorkStateView::Shelved);
    let mut checkout = checkout_of(&task);
    checkout.dirty = true;
    let model = showing(vec![task], vec![checkout]);
    assert_eq!(recent_rows(&model)[0].reason, Reason::Uncommitted);
}

#[test]
fn what_waits_on_you_comes_first_and_each_block_by_recency() {
    let model = showing(
        vec![
            changed(
                preserved(PROJECT, "a", "old shelf", WorkStateView::Shelved),
                7200,
            ),
            changed(
                preserved(PROJECT, "b", "old ready", WorkStateView::Ready),
                9000,
            ),
            changed(
                preserved(OTHER, "c", "new review", WorkStateView::Published),
                60,
            ),
            changed(
                preserved(OTHER, "d", "new ready", WorkStateView::Ready),
                120,
            ),
        ],
        Vec::new(),
    );
    let names: Vec<String> = recent_rows(&model)
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(names, ["new ready", "old ready", "new review", "old shelf"]);
}

/// A subagent's task and its checkout are one row, and joining is what
/// moves it on; a subagent checkout no task names is still listed.
#[test]
fn a_subagent_ready_to_join_is_one_row_whose_move_is_joining() {
    let child = preserved(PROJECT, "child", "lexer", WorkStateView::Shelved);
    let named = showing(vec![child], vec![subagent_checkout(Some("child"))]);
    let rows = recent_rows(&named);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].reason, Reason::JoinReady);
    assert_eq!(rows[0].primary(), Some("join"));

    let unnamed = showing(Vec::new(), vec![subagent_checkout(None)]);
    assert_eq!(
        reasons(&unnamed),
        vec![("lexer".to_owned(), Reason::JoinReady)]
    );
}

/// Only an agent is a row: a free slot, or a checkout somebody made by
/// hand, is not listed.
#[test]
fn a_checkout_no_agent_holds_is_not_listed() {
    let free = CheckoutView {
        path: Path::new(PROJECT).join(".worktrees/free"),
        owner: CheckoutOwner::Agent {
            holder: None,
            live: false,
        },
        task: None,
        branch: None,
        dirty: false,
        ahead: 0,
        last_changed: None,
        bytes: 1024,
    };
    let by_hand = CheckoutView {
        owner: CheckoutOwner::Operator,
        path: Path::new(PROJECT).join("elsewhere"),
        ..free.clone()
    };
    assert!(recent_rows(&showing(Vec::new(), vec![free, by_hand])).is_empty());
}

#[test]
fn work_whose_project_is_gone_can_only_be_forgotten() {
    let mut gone = preserved(OTHER, "t1", "vanished", WorkStateView::Ready);
    gone.project_present = false;
    let model = showing(vec![gone], Vec::new());
    let row = &recent_rows(&model)[0];
    assert_eq!(row.reason, Reason::ProjectGone);
    assert!(!row.reason.waits_on_you());
    assert_eq!(row.primary(), None);
    assert!(!row.delivers());
    assert_eq!(row.taking_away(), Some("forget"));
}

#[test]
fn an_unselected_row_is_one_line_and_work_waiting_on_you_says_why() {
    let mut model = showing(
        vec![
            changed(
                preserved(PROJECT, "a", "fixing", WorkStateView::GateFailed),
                60,
            ),
            changed(
                preserved(PROJECT, "b", "parked", WorkStateView::Shelved),
                120,
            ),
            changed(
                preserved(PROJECT, "c", "selected", WorkStateView::Shelved),
                180,
            ),
        ],
        Vec::new(),
    );
    model.recent.selected = Some("c".to_owned());
    let lines = drawn(&model);

    let parked = row_of(&lines, "parked");
    assert!(
        !lines[parked + 1].contains("shelved"),
        "a row that waits on nobody says only its name:\n{}",
        lines.join("\n")
    );
    let fixing = row_of(&lines, "fixing");
    assert!(lines[fixing + 1].contains("checks failed"));
    assert!(lines[row_of(&lines, "needs you")].contains('1'));
    assert!(lines[row_of(&lines, "others")].contains('2'));
}

#[test]
fn the_selected_row_says_its_reason_its_project_and_its_checkout() {
    let task = preserved(
        PROJECT,
        "t1",
        "journeys",
        WorkStateView::Conflicted {
            files: vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")],
        },
    );
    let checkout = checkout_of(&task);
    let mut model = showing(vec![task], vec![checkout]);
    model.recent.selected = Some("t1".to_owned());
    let lines = drawn(&model);

    let at = row_of(&lines, "journeys");
    assert!(
        lines[at + 1].contains("conflict in 3 files"),
        "{}",
        lines.join("\n")
    );
    assert!(lines[at + 1].contains("project"), "the project's name");
    assert!(
        lines[at + 2].contains("agent/t1 · 3 ahead · 5.0 GB"),
        "{}",
        lines.join("\n")
    );
}

#[test]
fn a_project_with_no_space_open_says_resuming_opens_one() {
    let mut model = showing(
        vec![preserved(OTHER, "t1", "elsewhere", WorkStateView::Shelved)],
        Vec::new(),
    );
    model.recent.selected = Some("t1".to_owned());
    let lines = drawn(&model);
    assert!(lines[row_of(&lines, "elsewhere") + 1].contains("other · new space"));
}

#[test]
fn the_name_of_work_whose_project_is_gone_is_struck_through() {
    let mut gone = preserved(OTHER, "t1", "vanished", WorkStateView::Shelved);
    gone.project_present = false;
    let model = showing(vec![gone], Vec::new());
    let buffer = sidebar_buffer(&model, 30);
    let lines = drawn(&model);
    let row = row_of(&lines, "vanished") as u16;
    let column = lines[row as usize].find("vanished").unwrap() as u16;
    assert!(
        buffer[(column, row)]
            .modifier
            .contains(Modifier::CROSSED_OUT)
    );
}

/// The way in and out is the header's name, which answers the pointer the
/// way the header's other controls do, and carries nothing beside it.
#[test]
fn the_headers_name_opens_recent_and_brightens_under_the_pointer() {
    let mut model = showing(
        vec![preserved(PROJECT, "a", "one", WorkStateView::Ready)],
        Vec::new(),
    );
    model.recent.open = false;
    let (buffer, hits) = drawn_with_hits(&model, 20);
    let name = hits
        .iter()
        .find_map(|(rect, hit)| (*hit == WorkspaceHit::ToggleRecent).then_some(*rect))
        .expect("the name is a control");
    let drawn: String = (name.x..name.right())
        .map(|column| buffer[(column, name.y)].symbol().to_owned())
        .collect();
    assert_eq!(drawn, "work");
    assert_eq!(buffer[(name.x, name.y)].fg, theme::color(Token::TextMuted));
    let header: String = (0..buffer.area.width)
        .map(|column| buffer[(column, name.y)].symbol().to_owned())
        .collect();
    assert!(
        !header.contains(&theme::glyph(Symbol::MarkStanding)),
        "no count beside it, though an agent waits: {header:?}"
    );

    model.hovered = Some(WorkspaceHit::ToggleRecent);
    let (buffer, _) = drawn_with_hits(&model, 20);
    assert_eq!(
        buffer[(name.x, name.y)].fg,
        theme::color(Token::TextPrimary),
        "brighter under the pointer"
    );
}

#[test]
fn the_toggle_opens_recent_reads_its_projects_and_keeps_the_selection() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-open"));
    let mut model = model_of(session(PROJECT));
    model.remembered.preserved_work = vec![
        preserved(PROJECT, "a", "one", WorkStateView::Ready),
        preserved(OTHER, "b", "two", WorkStateView::Ready),
    ];
    let mut driven = driven(model, &home);

    press(&mut driven, Action::ToggleRecent);
    let recent = &driven.attach.model.recent;
    assert!(recent.open);
    assert!(recent.reads.contains_key(Path::new(PROJECT)));
    assert!(recent.reads.contains_key(Path::new(OTHER)));

    press(&mut driven, Action::SelectNext);
    let selected = driven.attach.model.recent.selected.clone();
    assert!(selected.is_some());
    press(&mut driven, Action::Dismiss);
    assert!(!driven.attach.model.recent.open, "escape leaves it");
    press(&mut driven, Action::ToggleRecent);
    assert_eq!(driven.attach.model.recent.selected, selected);
    press(&mut driven, Action::ToggleRecent);
    assert!(!driven.attach.model.recent.open, "the same key closes it");
}

#[test]
fn the_manage_mark_opens_manage_in_one_click_while_recent_is_open() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-manage"));
    let mut driven = driven(
        showing(
            vec![preserved(PROJECT, "a", "one", WorkStateView::Ready)],
            Vec::new(),
        ),
        &home,
    );
    driven.frame();
    let more = driven.hit(|hit| *hit == WorkspaceHit::OpenManage);

    driven.press(more.x, more.y);
    assert!(driven.attach.model.recent.open);
    assert!(driven.attach.model.manage.is_some());
}

/// Recent is opened and closed by its own control: a click in the pane
/// reaches the pane and hands it the keyboard, recent stays drawn, and a
/// click back in its column gives the keyboard back to it.
#[test]
fn a_click_elsewhere_keeps_recent_open_and_hands_the_keys_back() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-elsewhere"));
    let mut driven = driven(
        showing(
            vec![preserved(PROJECT, "a", "one", WorkStateView::Ready)],
            Vec::new(),
        ),
        &home,
    );
    driven.frame();
    press(&mut driven, Action::DiscardTask);
    assert!(driven.attach.model.recent.asking);
    let in_the_pane = 70;

    driven.press(in_the_pane, 10);
    assert!(driven.attach.model.recent.open, "recent stays open");
    assert!(!driven.attach.model.recent.has_keys());
    assert!(
        !driven.attach.model.recent.asking,
        "its question is withdrawn"
    );

    driven.press(2, 16);
    assert!(driven.attach.model.recent.has_keys());
}

/// The question opens on its way out, so the enter that opened nothing
/// by habit takes nothing either; the arrow moves onto going ahead.
#[test]
fn a_question_opens_on_its_way_out_and_the_arrow_reaches_going_ahead() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-ask"));
    let model = showing(
        vec![preserved(PROJECT, "t1", "kept", WorkStateView::Shelved)],
        Vec::new(),
    );
    let mut driven = driven(model, &home);

    press(&mut driven, Action::DiscardTask);
    press(&mut driven, Action::Activate);
    assert!(!driven.attach.model.recent.asking);
    assert!(
        driven
            .attach
            .model
            .remembered
            .task_mutation_pending
            .is_empty()
    );

    press(&mut driven, Action::DiscardTask);
    press(&mut driven, Action::FocusNext);
    press(&mut driven, Action::Activate);
    assert!(
        driven
            .attach
            .model
            .remembered
            .task_mutation_pending
            .contains("t1")
    );
}

#[test]
fn discard_asks_on_the_row_and_declining_leaves_everything() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-decline"));
    let model = showing(
        vec![preserved(PROJECT, "t1", "kept", WorkStateView::Shelved)],
        Vec::new(),
    );
    let mut driven = driven(model, &home);

    press(&mut driven, Action::DiscardTask);
    assert!(driven.attach.model.recent.asking);
    let lines = drawn(&driven.attach.model).join(" ");
    assert!(
        lines.contains("discard branch, commits and") && lines.contains("conversation?"),
        "{lines}"
    );
    press(&mut driven, Action::Dismiss);
    assert!(!driven.attach.model.recent.asking);
    assert!(driven.attach.model.recent.open, "declining is not leaving");
    assert!(
        driven
            .attach
            .model
            .remembered
            .task_mutation_pending
            .is_empty()
    );
}

#[test]
fn work_whose_project_is_gone_is_forgotten_on_confirmation() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-forget"));
    let mut gone = preserved(OTHER, "t1", "vanished", WorkStateView::Shelved);
    gone.project_present = false;
    let mut driven = driven(showing(vec![gone], Vec::new()), &home);

    press(&mut driven, Action::DiscardTask);
    let lines = drawn(&driven.attach.model).join(" ");
    assert!(
        lines.contains("forget this record?") && lines.contains("nothing on disk"),
        "{lines}"
    );
    press(&mut driven, Action::FocusNext);
    press(&mut driven, Action::Activate);
    assert!(
        driven
            .attach
            .model
            .remembered
            .task_mutation_pending
            .contains("t1")
    );
}

#[test]
fn enter_joins_a_subagent_and_recent_stays_open() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-join"));
    let mut driven = driven(showing(Vec::new(), vec![subagent_checkout(None)]), &home);

    press(&mut driven, Action::Activate);
    assert!(driven.attach.model.remembered.join_pending);
    assert!(driven.attach.model.recent.open);
}

#[test]
fn deliver_is_only_for_ready_work() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-deliver"));
    let mut driven = driven(
        showing(
            vec![
                changed(preserved(PROJECT, "a", "ready", WorkStateView::Ready), 60),
                changed(
                    preserved(PROJECT, "b", "shelf", WorkStateView::Shelved),
                    120,
                ),
            ],
            Vec::new(),
        ),
        &home,
    );

    press(&mut driven, Action::DeliverTask);
    assert!(
        driven
            .attach
            .model
            .remembered
            .delivery_pending
            .contains("a")
    );
    press(&mut driven, Action::SelectNext);
    press(&mut driven, Action::DeliverTask);
    assert!(
        !driven
            .attach
            .model
            .remembered
            .delivery_pending
            .contains("b")
    );
}

/// The letters a row offers are rows of the index too, and only where
/// they apply: a keyboard user who does not know them still reaches them.
#[test]
fn the_index_offers_what_the_selected_row_offers() {
    let ready = showing(
        vec![preserved(PROJECT, "t1", "ready", WorkStateView::Ready)],
        Vec::new(),
    );
    assert_eq!(
        recent_offered(&ready),
        vec![Action::DeliverTask, Action::DiscardTask]
    );
    let joinable = showing(Vec::new(), vec![subagent_checkout(None)]);
    assert!(recent_offered(&joinable).is_empty());
}

fn many(count: usize) -> WorkspaceModel {
    showing(
        (0..count)
            .map(|at| {
                changed(
                    preserved(
                        PROJECT,
                        &format!("t{at}"),
                        &format!("agent {at}"),
                        WorkStateView::Shelved,
                    ),
                    60 * (at as u64 + 1),
                )
            })
            .collect(),
        Vec::new(),
    )
}

fn drawn_with_hits(
    model: &WorkspaceModel,
    height: u16,
) -> (ratatui::buffer::Buffer, Vec<(Rect, WorkspaceHit)>) {
    let mut terminal = Terminal::new(TestBackend::new(44, height)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                model,
                &[],
                &mut hits,
                &mut render::FrameMetrics::default(),
            )
        })
        .unwrap();
    (terminal.backend().buffer().clone(), hits)
}

/// A list longer than the column draws the one scrollbar the product has,
/// and a list that fits draws none.
#[test]
fn a_list_longer_than_the_column_draws_a_scrollbar() {
    let thumb = theme::glyph(Symbol::ScrollThumb);
    let count = |model: &WorkspaceModel| {
        let (buffer, _) = drawn_with_hits(model, 16);
        buffer
            .content()
            .iter()
            .filter(|cell| cell.symbol() == thumb)
            .count()
    };
    assert!(count(&many(30)) > 0, "thirty rows do not fit in sixteen");
    assert_eq!(count(&many(2)), 0, "two rows do");
}

/// The handle sits on the sidebar's divider, as the code surface's does:
/// dragged along it, it scrolls the list and leaves the selection where it
/// is; dragged across, it resizes the column as the divider does.
#[test]
fn the_handle_on_the_divider_scrolls_along_it_and_resizes_across_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-grab"));
    let mut driven = driven(many(60), &home);
    driven.frame();
    let handle = driven.hit(|hit| *hit == WorkspaceHit::RecentScrollGrab);
    let bar = driven.attach.model.recent.bar.expect("the rows do not fit");
    let foot = bar.track.bottom() - 1;

    driven.press(handle.x, handle.y);
    driven.mouse(handle.x, foot, MouseEventKind::Drag(MouseButton::Left));
    driven.mouse(handle.x, foot, MouseEventKind::Up(MouseButton::Left));
    driven.frame();
    assert_eq!(
        driven.attach.model.recent.scroll.first,
        bar.total - bar.visible,
        "the foot of the track is the last screenful"
    );
    assert_eq!(
        driven.attach.model.recent.selected, None,
        "and the selection is where it was"
    );

    let handle = driven.hit(|hit| *hit == WorkspaceHit::RecentScrollGrab);
    let width = driven.attach.model.sidebar_width;
    driven.press(handle.x, handle.y);
    driven.mouse(
        handle.x + 6,
        handle.y,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        handle.x + 8,
        handle.y,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        handle.x + 8,
        handle.y,
        MouseEventKind::Up(MouseButton::Left),
    );
    assert_ne!(
        driven.attach.model.sidebar_width, width,
        "across, the column moved"
    );
    assert!(driven.attach.model.recent.open, "and recent stayed");
}

/// The wheel scrolls the list and leaves the selection alone; moving the
/// selection with the keys brings the window back to it.
#[test]
fn the_wheel_scrolls_and_the_keys_bring_the_selection_back_into_view() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-wheel"));
    let mut driven = driven(many(60), &home);
    driven.frame();
    for _ in 0..20 {
        driven.mouse(2, 6, MouseEventKind::ScrollDown);
    }
    driven.frame();
    assert_eq!(driven.attach.model.recent.scroll.first, 20);
    assert_eq!(driven.attach.model.recent.selected, None);

    press(&mut driven, Action::SelectNext);
    driven.frame();
    assert_eq!(driven.attach.model.recent.selected.as_deref(), Some("t1"));
    assert_eq!(
        driven.attach.model.recent.scroll.first, 2,
        "the selection moved, and the window went back just far enough to show \
         it: the heading and the first row above it are scrolled past"
    );
}

/// A space rooted below a project's root asks about a directory of the
/// repository rather than its primary checkout; the answer is kept under
/// the primary, which is what every row looks its project up by.
#[test]
fn a_read_of_a_directory_below_the_root_is_kept_under_the_primary() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-rekey"));
    let task = preserved(PROJECT, "t1", "kept", WorkStateView::Shelved);
    let checkout = checkout_of(&task);
    let mut model = showing(vec![task], Vec::new());
    model.recent.reads.clear();
    let below = Path::new(PROJECT).join("crates");
    model.recent.reads.insert(
        below.clone(),
        ProjectRead {
            asked: 7,
            answer: None,
        },
    );
    let mut driven = driven(model, &home);
    driven
        .attach
        .channels
        .checkouts
        .sender
        .send(CheckoutsResolution {
            project: below.clone(),
            asked: 7,
            view: Some(CheckoutsView {
                primary: PathBuf::from(PROJECT),
                checkouts: vec![checkout],
            }),
        })
        .unwrap();
    driven.pump();

    let reads = &driven.attach.model.recent.reads;
    assert!(reads.contains_key(Path::new(PROJECT)) && !reads.contains_key(&below));
    assert_eq!(
        recent_rows(&driven.attach.model)[0].bytes,
        Some(5 * 1024 * 1024 * 1024)
    );
    assert!(driven.attach.model.recent.covers(&below));
}

/// The foot is one line with the two keys that matter on the row:
/// escape leaves every surface alike and delivering is in the action
/// index, so neither takes the room the two need.
#[test]
fn the_foot_offers_resume_and_discard_on_one_line() {
    let mut model = showing(
        vec![preserved(PROJECT, "a", "one", WorkStateView::Ready)],
        Vec::new(),
    );
    model.recent.selected = Some("a".to_owned());
    let lines = drawn(&model);
    let foot = &lines[row_of(&lines, "enter resume")];
    assert!(foot.contains("d discard"), "{foot:?}");
    assert!(
        !foot.contains("deliver") && !foot.contains("close"),
        "{foot:?}"
    );
}

/// The selected row wears the thin bar the space tree's row in front
/// wears, down every line of it; the others leave the column empty.
#[test]
fn the_selected_row_wears_the_trees_thin_bar() {
    let mut model = showing(
        vec![
            preserved(PROJECT, "a", "one", WorkStateView::Ready),
            preserved(PROJECT, "b", "two", WorkStateView::Ready),
        ],
        Vec::new(),
    );
    model.recent.selected = Some("a".to_owned());
    let lines = drawn(&model);
    let bar = crate::ui::theme::glyph(crate::ui::theme::Symbol::BarThin);
    let selected = row_of(&lines, "one");
    assert!(lines[selected].starts_with(&bar), "{lines:?}");
    assert!(lines[selected + 1].starts_with(&bar), "{lines:?}");
    assert!(!lines[row_of(&lines, "two")].starts_with(&bar), "{lines:?}");
}

#[test]
fn with_nothing_kept_it_says_so_in_a_title_alone() {
    let model = showing(Vec::new(), Vec::new());
    let lines = drawn(&model);
    let title = &lines[row_of(&lines, "nothing kept yet")];
    let before = title.find("nothing").unwrap();
    assert!(before > 2, "the title is centred: {title:?}");
    assert!(
        !lines.iter().any(|line| line.contains("picked up again")),
        "{lines:?}"
    );
}

#[test]
fn recent_draws_on_a_small_terminal_without_panicking() {
    let model = showing(
        vec![preserved(PROJECT, "t1", "kept", WorkStateView::Ready)],
        Vec::new(),
    );
    for height in 1..8 {
        sidebar_buffer(&model, height);
    }
}

fn names(model: &WorkspaceModel) -> Vec<String> {
    recent_rows(model).into_iter().map(|row| row.name).collect()
}

fn type_into(driven: &mut super::workspace_tests::Driven<'_>, text: &str) {
    for character in text.chars() {
        let chord = uze_keys::Chord::parse(&character.to_string()).unwrap();
        driven.press_key(key_event(chord));
    }
}

/// An agent nobody named is known by its generated id, so recent lists
/// the named ones until asked for every agent, and asking again goes back.
#[test]
fn recent_lists_named_work_until_asked_for_every_agent() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-named"));
    let unnamed = PreservedWork {
        named: false,
        ..preserved(PROJECT, "x1y2z3", "x1y2z3", WorkStateView::Ready)
    };
    let mut driven = driven(
        showing(
            vec![
                preserved(PROJECT, "a", "parser", WorkStateView::Ready),
                unnamed,
            ],
            Vec::new(),
        ),
        &home,
    );
    assert_eq!(names(&driven.attach.model), ["parser"]);

    press(&mut driven, Action::ToggleAllWork);
    assert_eq!(names(&driven.attach.model).len(), 2);

    press(&mut driven, Action::ToggleAllWork);
    assert_eq!(names(&driven.attach.model), ["parser"]);
}

/// The search narrows by anything a row says — its name, its branch, its
/// project, its state — walks what it let through, and esc takes the
/// words away with it.
#[test]
fn the_search_narrows_by_what_a_row_says_and_esc_clears_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-search"));
    let mut driven = driven(
        showing(
            vec![
                preserved(PROJECT, "a", "parser", WorkStateView::Ready),
                preserved(OTHER, "b", "lexer", WorkStateView::Shelved),
            ],
            Vec::new(),
        ),
        &home,
    );
    press(&mut driven, Action::StartFilter);
    assert!(driven.attach.model.recent.filtering);

    type_into(&mut driven, "LEX");
    assert_eq!(names(&driven.attach.model), ["lexer"], "by name, any case");
    driven.press_key(key_event(uze_keys::Chord::parse("backspace").unwrap()));
    driven.press_key(key_event(uze_keys::Chord::parse("backspace").unwrap()));
    driven.press_key(key_event(uze_keys::Chord::parse("backspace").unwrap()));
    type_into(&mut driven, "agent/a");
    assert_eq!(names(&driven.attach.model), ["parser"], "by branch");
    driven.attach.model.recent.filter = "other".to_owned();
    assert_eq!(names(&driven.attach.model), ["lexer"], "by project");
    driven.attach.model.recent.filter = "ready to deliver".to_owned();
    assert_eq!(names(&driven.attach.model), ["parser"], "by state");

    press(&mut driven, Action::Dismiss);
    assert!(!driven.attach.model.recent.filtering);
    assert!(driven.attach.model.recent.filter.is_empty());
    assert!(
        driven.attach.model.recent.open,
        "esc left the search, not recent"
    );
}

/// The letters typed into the search are the search's: `a` is a letter
/// there, not the key that lists every agent.
#[test]
fn a_letter_typed_into_the_search_is_never_a_key() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-letter"));
    let mut driven = driven(
        showing(
            vec![preserved(PROJECT, "a", "parser", WorkStateView::Ready)],
            Vec::new(),
        ),
        &home,
    );
    press(&mut driven, Action::StartFilter);
    type_into(&mut driven, "ad");
    assert_eq!(driven.attach.model.recent.filter, "ad");
    assert!(!driven.attach.model.recent.show_all);
    assert!(!driven.attach.model.recent.asking);
}

/// The search row leads the column with its key, and the key that lists
/// every agent sits beside it; with the unnamed left out and none named,
/// the list says so rather than that nothing was kept.
#[test]
fn the_search_row_leads_recent_with_the_key_beside_it() {
    let model = showing(
        vec![PreservedWork {
            named: false,
            ..preserved(PROJECT, "x1y2z3", "x1y2z3", WorkStateView::Ready)
        }],
        Vec::new(),
    );
    let lines = drawn(&model);
    let search = &lines[row_of(&lines, "search")];
    assert!(search.contains('/'), "{search:?}");
    assert!(search.contains("a all"), "{search:?}");
    row_of(&lines, "nothing named yet");
}

/// A click selects a row and nothing more, however often it lands: only
/// enter resumes, so a second click never launches an agent by accident.
#[test]
fn a_click_on_the_selected_row_only_selects_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-recent-click"));
    let mut driven = driven(
        showing(
            vec![preserved(PROJECT, "a", "parser", WorkStateView::Shelved)],
            Vec::new(),
        ),
        &home,
    );
    driven.attach.click_recent_row(0);
    driven.attach.click_recent_row(0);
    assert_eq!(driven.attach.model.recent.selected.as_deref(), Some("a"));
    assert!(driven.attach.model.agent_picker.is_none());
    assert!(!driven.attach.model.placement_pending);
    assert!(driven.attach.model.recent.open);
}
