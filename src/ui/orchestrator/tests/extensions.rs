//! What the extension surfaces show inside the host: the code, spec and
//! architect surfaces, where they resume, and what reaches them.

use super::WorkspaceHit;
use super::support::*;
use super::{
    ExtensionHit, WorkspaceModel, open_code, open_spec,
    render::{self, compute_layout},
};
use crate::ui::theme::{self, Token};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use uze_core::UzeHome;
use uze_extensions::view::ViewHit;

/// Glance at the code, close it, come back: the commonest gesture in
/// the product, and the one that used to cost the whole walk down the
/// tree again. The place is per checkout, so another agent's surface
/// is another place rather than the same one moved.
#[test]
fn coming_back_to_a_checkouts_code_returns_to_where_it_was_left() {
    use uze_extensions::{DirEntry, code};

    let (mut model, first, second) = two_agents_with_shells();
    let answer_listing = |model: &mut WorkspaceModel, root: &str| {
        let view = code_view_mut(model).expect("the surface is open");
        view.take_request();
        view.absorb(code::FileAnswer::Listed {
            chain: Vec::new(),
            path: PathBuf::from(root),
            entries: Ok(vec![
                DirEntry {
                    directory: false,
                    name: "a.rs".to_owned(),
                },
                DirEntry {
                    directory: false,
                    name: "b.rs".to_owned(),
                },
            ]),
        });
    };

    let space =
        crate::ui::extension_view::content_space(Rect::new(0, 0, 120, 40), model.navigator_width);

    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    answer_listing(&mut model, "/repo/.worktrees/a");
    // Walked away from the row the tree opened on, which is the part
    // that must survive the round trip.
    code::handle_command(
        code_view_mut(&mut model).expect("open"),
        uze_extensions::view::Command::SelectNext,
        space,
    );
    let walked_to = code_view(&model).expect("open").place();

    model.close_surface();
    assert!(!model.showing(uze_extensions::code::CATALOG.id));

    // Another agent's checkout is a different place, not this one.
    select_tab(&mut model, second);
    open_code(&mut model, uze_extensions::code::FILES);
    answer_listing(&mut model, "/repo/.worktrees/b");
    assert_ne!(
        code_view(&model).expect("open").place(),
        walked_to,
        "a checkout never visited opens on its own first row"
    );
    model.close_surface();

    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    assert_eq!(
        code_view(&model).expect("open").place(),
        walked_to,
        "and the one left mid-walk is where it was left"
    );
}

/// The same gesture on the architect surface. A drawing is walked
/// into — a diagram chosen, a level entered, a box selected, the
/// board moved to it — and a surface that forgot all of that on the
/// way out is one nobody leaves to go and check something.
#[test]
fn coming_back_to_a_checkouts_architect_returns_to_the_diagram_it_was_left_on() {
    use uze_extensions::{architect, view::Command};

    let (mut model, first, second) = two_agents_with_shells();
    let answer_artifacts = |model: &mut WorkspaceModel| {
        open_architect_with(
            model,
            vec![
                architect::Artifact::read("one.mmd", "flowchart TD\n a --> b"),
                architect::Artifact::read("two.mmd", "flowchart TD\n c --> d"),
            ],
        );
    };
    let space = uze_extensions::view::Size {
        width: 120,
        height: 40,
    };

    select_tab(&mut model, first);
    answer_artifacts(&mut model);
    // Moved off the diagram the surface opens on, which is the part
    // that must survive the round trip.
    model.tell_surface(Command::NextView, space);
    let moved_to = architect_place(&model);

    model.close_surface();
    assert!(!model.showing(architect::CATALOG.id));

    // Another agent's checkout is a different place, not this one.
    select_tab(&mut model, second);
    answer_artifacts(&mut model);
    assert_ne!(
        architect_place(&model),
        moved_to,
        "a checkout never visited opens on its own first diagram"
    );
    model.close_surface();

    select_tab(&mut model, first);
    answer_artifacts(&mut model);
    assert_eq!(
        architect_place(&model),
        moved_to,
        "and the one left mid-walk is where it was left"
    );
}

/// Opening a surface back where it was left asks for every directory
/// that was open, and those answers do not wait on each other.
///
/// One request per pass made that one *frame* per directory: a third
/// of a second of a tree filling in a row at a time, for a tenth of a
/// millisecond of work. Reads keep the one-at-a-time chain, because a
/// save and the re-read that follows it have an order.
#[test]
fn every_directory_a_resumed_tree_needs_is_asked_for_at_once() {
    use uze_extensions::code;

    let root = PathBuf::from("/repo/.worktrees/a");
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.remembered.declared.places.insert(
        (
            super::Owner::Builtin(code::CATALOG.id),
            code::SURFACE.to_owned(),
            root.clone(),
        ),
        uze_extensions::contract::Opaque::new(code::CodePlace::at(
            &root,
            &root.join("src/ui/widget/row.rs"),
            false,
        )),
    );
    open_code(&mut model, uze_extensions::code::FILES);

    let handed = code_engine(&mut model).jobs().len();
    let waiting = code_view(&model)
        .expect("the surface is open")
        .peek_request()
        .cloned();
    assert!(
        waiting.is_none(),
        "every directory went out together, and the read behind them with it: {waiting:?}"
    );
    assert!(
        handed > 2,
        "the directories on the way, and the read: {handed}"
    );
}

/// Measuring a checkout opens every file it has, and the surface it
/// feeds is opened and closed all day. The second open shows the map
/// it already has rather than paying for it again and showing none
/// until it lands.
#[test]
fn a_checkout_measured_once_is_not_measured_again_on_the_way_back_in() {
    use uze_extensions::code;
    use uze_extensions::contract::Opaque;

    let root = PathBuf::from("/repo/.worktrees/a");
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);

    let measured = code::Measure {
        root: root.clone(),
        files: vec![code::FileMeasure {
            path: "src/main.rs".to_owned(),
            lines: 120,
            commits: 3,
            changed: false,
        }],
    };
    code_engine(&mut model).absorb(Opaque::new(code::MeasureRead {
        root: root.clone(),
        measure: Some(measured),
    }));
    assert!(code_view(&model).expect("open").has_map());

    model.close_surface();
    open_code(&mut model, uze_extensions::code::FILES);
    assert!(
        code_view(&model).expect("open").has_map(),
        "the map is there the moment the surface is"
    );

    // What opening asked for, out of the way.
    code_engine(&mut model).jobs();
    code_view_mut(&mut model)
        .expect("open")
        .show(uze_extensions::code::ContentMode::Map);
    assert!(
        code_engine(&mut model).jobs().is_empty(),
        "and nothing was asked of the checkout again"
    );
}

/// The content's groove hugs the frame's edge, in the column the pane
/// keeps from it (see `extension_view`'s own test of where it is
/// drawn) — and a press and the wheel there are still the surface's.
/// Routed by the pane alone, both fell through to the chrome and the
/// groove drawn to be dragged did nothing.
#[test]
fn the_content_scrollbar_answers_in_the_panes_margin() {
    use uze_extensions::{code, view::ViewHit};

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-content-groove"));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    let mut view = code::CodeView::opening(
        PathBuf::from("/repo"),
        "/repo".to_owned(),
        code::ContentMode::Contents,
    );
    view.take_request();
    show_code(&mut model, view);

    let mut driven = driven(model, &home);
    driven.frame();
    let pane = compute_layout(
        driven.area,
        driven.attach.model.sidebar_width,
        driven.attach.model.dock(),
    )
    .pane;
    let track = Rect::new(pane.right(), pane.y + 1, 1, pane.height - 1);
    let bar = crate::ui::widget::Scrollbar::measure(track, usize::from(track.height), 500)
        .expect("five hundred lines in a pane is a scrollbar");
    driven.attach.model.surface_frame.content_bar = Some(bar);
    driven.attach.model.hits.insert(
        0,
        (
            track,
            code_hit(&driven.attach.model, ViewHit::DragContentScrollbar),
        ),
    );
    let place = |driven: &Driven<'_>| code_view(&driven.attach.model).expect("still open").place();
    let top = place(&driven);

    driven.press(track.x, track.bottom() - 1);
    assert_ne!(place(&driven), top, "a press low on the groove scrolls");

    driven.mouse(track.x, track.y, MouseEventKind::Up(MouseButton::Left));
    driven.press(track.x, track.y);
    assert_eq!(place(&driven), top, "and one at its top goes back");

    driven.mouse(track.x, track.y, MouseEventKind::Up(MouseButton::Left));
    driven.mouse(track.x, track.y, MouseEventKind::ScrollDown);
    assert_ne!(place(&driven), top, "the wheel over it scrolls the content");
}

/// An open surface's navigation takes the bar's leading slot from the
/// agent tabs: the halves are on the bar's row, the list's heading that
/// named the half is gone, and a click on one reaches the surface.
#[test]
fn the_code_halves_take_the_bars_leading_slot() {
    use uze_extensions::{DirEntry, ExtensionHit, code, view::ViewHit};

    let root = PathBuf::from("/repo");
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);

    let mut view = code::CodeView::opening(
        root.clone(),
        "/repo".to_owned(),
        code::ContentMode::Contents,
    );
    view.take_request();
    view.absorb(code::FileAnswer::Listed {
        chain: Vec::new(),
        path: root.clone(),
        entries: Ok(vec![DirEntry {
            directory: false,
            name: "main.rs".to_owned(),
        }]),
    });
    // A measured checkout is what makes the map one of the halves.
    view.absorb_measure(code::Measure {
        root,
        files: vec![code::FileMeasure {
            path: "main.rs".to_owned(),
            lines: 1,
            commits: 1,
            changed: false,
        }],
    });
    show_code(&mut model, view);

    let rows = frame_rows(&mut model);
    let layout = full_frame(&mut model);
    let bar = layout.tab_strip.y as usize;
    assert!(
        rows[bar].contains("files") && rows[bar].contains("map"),
        "the halves are on the bar's row: {:?}",
        rows[bar]
    );
    assert!(
        !model.hits.iter().any(|(rect, hit)| {
            matches!(hit, WorkspaceHit::SelectTab(_) | WorkspaceHit::NewTab)
                && rect.y == layout.tab_strip.y
        }),
        "in place of the agent tabs: {:?}",
        rows[bar]
    );
    assert!(
        !rows[layout.pane.y as usize].contains("FILES"),
        "and the list's heading that named the half is gone: {:?}",
        rows[layout.pane.y as usize]
    );

    let control = model.hits.iter().find(|(_, hit)| {
        matches!(
            hit,
            WorkspaceHit::Extension(ExtensionHit {
                hit: ViewHit::SelectSubject(_),
                ..
            })
        )
    });
    let (rect, _) = control.expect("the control can be pointed at");
    assert_eq!(rect.y as usize, bar, "on that same row");
}

/// The sidebar's sections are the sidebar's while a surface is open:
/// their targets are an extension's too, and a press on one once went to
/// the open surface instead, which answered nothing.
#[test]
fn a_sidebar_section_answers_while_a_surface_is_open() {
    use uze_extensions::spec::{ChangeSummary, Progress, Summary};
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-spec-section-under-surface",
    ));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    count_spec(
        &mut model,
        Summary {
            changes: vec![ChangeSummary {
                name: "mine".to_owned(),
                progress: Some(Progress { done: 1, total: 4 }),
            }],
            done: 1,
            total: 4,
        },
    );
    open_spec(&mut model);
    let summary = spec_slot(&model, uze_extensions::spec::SUMMARY);
    let mut driven = driven(model, &home);
    driven.frame();
    let header = driven.hit(|hit| {
        *hit == WorkspaceHit::Extension(ExtensionHit {
            slot: summary,
            hit: ViewHit::ToggleSection,
        })
    });
    driven.press(header.x, header.y);
    assert!(
        driven.attach.model.spec_summary_open,
        "opened from its header"
    );
}

/// The sidebar's spec section says, folded, how far the checkout's own
/// changes have got; opened, it lists them beside an open timeline;
/// and a change opens the surface on it.
#[test]
fn the_spec_section_totals_the_work_and_opens_the_surface_on_a_change() {
    use uze_extensions::spec::{ChangeSummary, Progress, Summary};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-spec-section"));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.timeline_collapsed = false;
    count_spec(
        &mut model,
        Summary {
            changes: vec![
                ChangeSummary {
                    name: "mine".to_owned(),
                    progress: Some(Progress { done: 1, total: 4 }),
                },
                ChangeSummary {
                    name: "theirs".to_owned(),
                    progress: Some(Progress { done: 2, total: 2 }),
                },
            ],
            done: 3,
            total: 6,
        },
    );
    let summary = spec_slot(&model, uze_extensions::spec::SUMMARY);
    let mut driven = driven(model, &home);
    driven.frame();

    let drawn = |driven: &Driven<'_>, wanted: ViewHit| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| {
                *hit == WorkspaceHit::Extension(ExtensionHit {
                    slot: summary,
                    hit: wanted,
                })
            })
            .map(|(rect, _)| *rect)
            .unwrap_or_else(|| panic!("{wanted:?} is not in the sidebar"))
    };
    let header = drawn(&driven, ViewHit::ToggleSection);
    driven.press(header.x, header.y);
    assert!(
        driven.attach.model.spec_summary_open,
        "opened from its header"
    );
    assert!(
        !driven.attach.model.timeline_collapsed,
        "the timeline stays open beside it"
    );

    driven.frame();
    let row = drawn(&driven, ViewHit::SelectItem(1));
    driven.press(row.x, row.y);
    assert!(
        driven
            .attach
            .model
            .showing(uze_extensions::spec::CATALOG.id),
        "the surface opened"
    );
    let place = spec_place(&driven.attach.model);
    assert_eq!(
        place,
        Some(uze_extensions::spec::SpecPlace::change("theirs")),
        "on the change that was clicked"
    );

    // With the pointer gone, the row the surface is showing stays lit
    // and the other rests.
    driven.attach.model.hovered = None;
    let area = driven.area;
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    let mut hits = Vec::new();
    terminal
        .draw(|frame| {
            render::render(
                frame,
                &driven.attach.model,
                &identities_fixture(),
                &mut hits,
                &mut render::FrameMetrics::default(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let bright_name = |wanted: ViewHit, name: &str| {
        let (rect, _) = hits
            .iter()
            .find(|(_, hit)| {
                *hit == WorkspaceHit::Extension(ExtensionHit {
                    slot: summary,
                    hit: wanted,
                })
            })
            .unwrap_or_else(|| panic!("{wanted:?} is not in the sidebar"));
        let text: String = (rect.x..rect.right())
            .map(|column| buffer[(column, rect.y)].symbol())
            .collect();
        let byte = text.find(name).expect("the name is drawn");
        let column = rect.x + text[..byte].chars().count() as u16;
        buffer[(column, rect.y)].fg == theme::color(Token::TextBright)
    };
    assert!(bright_name(ViewHit::SelectItem(1), "theirs"), "on show");
    assert!(!bright_name(ViewHit::SelectItem(0), "mine"), "at rest");
}

/// An answer carries the checkout it was read for, so one landing
/// after the surface was closed, or reopened on another checkout, is
/// dropped rather than drawn.
#[test]
fn a_spec_answer_for_a_surface_since_closed_or_moved_is_dropped() {
    let (mut model, first, second) = two_agents_with_shells();
    let read = |root: PathBuf| {
        uze_extensions::contract::Opaque::new(uze_extensions::spec::SpecRead {
            root: root.clone(),
            answer: uze_extensions::spec::SpecAnswer {
                root,
                branch: "main".to_owned(),
                theme: String::new(),
                found: uze_extensions::spec::Found::NoLayout,
                subjects: Vec::new(),
            },
        })
    };
    let reading = |model: &WorkspaceModel| {
        let space = uze_extensions::view::Size {
            width: 80,
            height: 24,
        };
        matches!(
            model.view_in_pane(space).map(|view| view.content),
            Some(uze_extensions::view::Content::Message { text, .. }) if text.starts_with("Reading")
        )
    };
    select_tab(&mut model, first);
    open_spec(&mut model);
    let asked_for = model.open_root.clone().expect("a root");
    model.close_surface();
    spec_engine(&mut model).absorb(read(asked_for.clone()));
    assert!(
        !model.showing(uze_extensions::spec::CATALOG.id),
        "nothing reopened"
    );

    select_tab(&mut model, second);
    open_spec(&mut model);
    let here = model.open_root.clone().expect("a root");
    if here != asked_for {
        spec_engine(&mut model).absorb(read(asked_for.clone()));
        assert!(
            reading(&model),
            "an answer about another checkout is not drawn"
        );
    }
    spec_engine(&mut model).absorb(read(here));
    assert!(!reading(&model), "its own answer lands");
}

/// Coming back to a checkout's spec surface returns to the document it
/// was left on, by name.
#[test]
fn coming_back_to_a_checkouts_spec_returns_to_the_document_it_was_left_on() {
    use uze_extensions::{spec, view::Command};

    let (mut model, first, _second) = two_agents_with_shells();
    let answer = |model: &mut WorkspaceModel| {
        let artifact = |name: &str| spec::Artifact {
            path: PathBuf::from(format!("/repo/openspec/changes/x/{name}.md")),
            relative: format!("{name}.md"),
            role: match name {
                "proposal" => spec::Role::Why,
                _ => spec::Role::How,
            },
            name: name.to_owned(),
            rank: 0,
            text: Ok(format!("# {name}\n")),
        };
        answer_spec(
            model,
            spec::SpecAnswer {
                root: PathBuf::from("/repo"),
                branch: "main".to_owned(),
                theme: String::new(),
                found: spec::Found::Units {
                    dialects: vec!["OpenSpec"],
                    units: vec![spec::Unit {
                        subject: spec::Subject::Changes,
                        name: "x".to_owned(),
                        relative: "openspec/changes/x".to_owned(),
                        dialect: "OpenSpec",
                        archives: true,
                        artifacts: vec![artifact("proposal"), artifact("design")],
                        progress: None,
                        own: false,
                        standing: None,
                    }],
                },
                subjects: vec![spec::Subject::Changes],
            },
        );
    };
    let space = uze_extensions::view::Size {
        width: 120,
        height: 40,
    };

    select_tab(&mut model, first);
    open_spec(&mut model);
    answer(&mut model);
    for command in [Command::Expand, Command::SelectNext, Command::SelectNext] {
        model.tell_surface(command, space);
    }
    let left_on = spec_place(&model);
    model.close_surface();

    open_spec(&mut model);
    answer(&mut model);
    assert_eq!(spec_place(&model), left_on);
    assert!(left_on.is_some());
}

/// A click on the map lands on the column the pointer is over.
///
/// A drawing has no line numbers and so no gutter, and the rule that
/// turned a pointer into a position assumed one either way: every
/// click resolved seven cells to the left of where it landed, which
/// on a map is a tile or two over — and against the right-hand edge,
/// a tile nobody could reach at all.
#[test]
fn a_click_on_the_map_resolves_to_the_column_under_the_pointer() {
    use uze_extensions::{code, view::ViewHit};

    let root = PathBuf::from("/repo/.worktrees/a");
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    let view = code_view_mut(&mut model).expect("the surface is open");
    view.absorb_measure(code::Measure {
        root,
        files: (0..6)
            .map(|n| code::FileMeasure {
                path: format!("src/m{n}.rs"),
                lines: 400 + n * 100,
                commits: n,
                changed: false,
            })
            .collect(),
    });
    view.show(code::ContentMode::Map);
    full_frame(&mut model);

    assert_eq!(
        model.surface_frame.content_gutter, 0,
        "a drawing carries no line numbers, so it has no gutter"
    );
    let (rect, line, cell) = model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit {
                hit: ViewHit::PlaceCaret { line, cell },
                ..
            }) => Some((*rect, *line, *cell)),
            _ => None,
        })
        .expect("the map's rows are clickable");
    let _ = line;
    let far_right = rect.x + rect.width - 1;
    assert_eq!(
        crate::ui::extension_view::caret_cell_at(
            rect,
            cell,
            far_right,
            model.surface_frame.content_gutter,
        ),
        usize::from(rect.width - 1),
        "the last column of a row is the last cell of the drawing"
    );
}

/// A click inside the explorer has to resolve to the row the frame
/// drew, not to something laid out under it. The overlay covers the
/// whole frame and pushes its own hits into the shared table, so
/// "does a click reach the extension" is a question only the real
/// render can answer.
#[test]
fn a_click_inside_the_explorer_reaches_the_extension() {
    use uze_extensions::{DirEntry, ExtensionHit, code, view::ViewHit};

    let root = PathBuf::from("/repo");
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);

    let mut view = code::CodeView::opening(
        root.clone(),
        "/repo".to_owned(),
        code::ContentMode::Contents,
    );
    // Answered by hand rather than off a disk: what is under test is
    // where the click lands, and a temp directory would only add a
    // way for the test to fail for reasons of its own.
    view.take_request();
    view.absorb(code::FileAnswer::Listed {
        chain: Vec::new(),
        path: root,
        entries: Ok(vec![
            DirEntry {
                directory: true,
                name: "src".to_owned(),
            },
            DirEntry {
                directory: false,
                name: "README.md".to_owned(),
            },
        ]),
    });
    show_code(&mut model, view);
    full_frame(&mut model);

    let row = model
        .hits
        .iter()
        .find(|(_, hit)| {
            matches!(
                hit,
                WorkspaceHit::Extension(ExtensionHit {
                    hit: ViewHit::SelectItem(_),
                    ..
                })
            )
        })
        .expect("the explorer drew a clickable file row")
        .0;
    assert!(
        matches!(
            model.hit_at(row.x + 2, row.y),
            Some(WorkspaceHit::Extension(ExtensionHit {
                hit: ViewHit::SelectItem(_),
                ..
            }))
        ),
        "a click on the row resolves to that row, not to something under it"
    );

    let caret = model.hits.iter().find(|(_, hit)| {
        matches!(
            hit,
            WorkspaceHit::Extension(ExtensionHit {
                hit: ViewHit::PlaceCaret { .. },
                ..
            })
        )
    });
    assert!(
        caret.is_none(),
        "with no file open there is nothing to put a caret in"
    );
}

/// A list opened over the architect's board follows the pointer, the
/// way every other dropdown in this client does.
///
/// The hit list carries two orders at once: the workspace reads its
/// own chrome latest-drawn first, while an extension hands its hits
/// down topmost first, an open list spliced in front of the board it
/// covers. Read the wrong way round, a pointer over a row of the list
/// finds the drawing underneath it and the highlight never moves.
#[test]
fn a_list_open_over_the_architects_board_follows_the_pointer() {
    use uze_extensions::{architect, view::Choosing};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-architect-hover"));
    let mut model = agent_session_in("/repo");
    open_architect_with(
        &mut model,
        [
            (
                "crate-layering.mmd",
                include_str!("../../../../docs/architecture/crate-layering.mmd"),
            ),
            (
                "install-pipeline.mmd",
                include_str!("../../../../docs/architecture/install-pipeline.mmd"),
            ),
        ]
        .map(|(origin, source)| architect::Artifact::read(origin, source))
        .into(),
    );
    let slot = architect_slot(&model);
    // Roomy, because the board is drawn beside the sidebar: at the
    // default width the menu has no room to name what it chooses.
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    let space =
        crate::ui::extension_view::board_space(compute_layout(driven.area, None, None).pane);
    let highlighted = |driven: &Driven<'_>| {
        architect_view(&driven.attach.model, space)
            .navigator
            .expect("a menu")
            .choosing
    };
    let row_of = |driven: &Driven<'_>, item: usize| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| {
                *hit == WorkspaceHit::Extension(ExtensionHit {
                    slot,
                    hit: ViewHit::SelectItem(item),
                })
            })
            .map(|(rect, _)| *rect)
            .unwrap_or_else(|| panic!("no row for artifact {item}"))
    };

    driven.frame();
    let selector = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| {
            *hit == WorkspaceHit::Extension(ExtensionHit {
                slot,
                hit: ViewHit::ChooseItem,
            })
        })
        .map(|(rect, _)| *rect)
        .expect("two artifacts in the area, so it opens");
    driven.press(selector.x + 1, selector.y);
    driven.frame();
    assert_eq!(highlighted(&driven), Some(Choosing::Item(0)));

    let other = row_of(&driven, 1);
    driven.mouse(other.x + 1, other.y, MouseEventKind::Moved);
    assert_eq!(
        highlighted(&driven),
        Some(Choosing::Item(1)),
        "the row under the pointer is the highlighted one"
    );
    driven.frame();
    let first = row_of(&driven, 0);
    driven.mouse(first.x + 1, first.y, MouseEventKind::Moved);
    assert_eq!(highlighted(&driven), Some(Choosing::Item(0)), "and back");
}

/// The architect's source is text like any other: press, drag, let go,
/// and what was passed over is on the clipboard. Its diagram is a
/// drawing, which a press points at and never marks.
#[test]
fn the_architects_source_is_marked_and_copied_and_its_diagram_is_not() {
    use uze_extensions::architect;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-architect-copy"));
    let source = include_str!("../../../../docs/architecture/crate-layering.mmd");
    let mut model = agent_session_in("/repo");
    open_architect_with(
        &mut model,
        vec![architect::Artifact::read("crate-layering.mmd", source)],
    );
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    driven.frame();
    assert!(
        driven.attach.model.surface_frame.text_rows.is_empty(),
        "a diagram has no text to mark"
    );

    let space = driven.attach.model.surface_frame.content_space;
    let slot = architect_slot(&driven.attach.model);
    driven
        .attach
        .model
        .hit_surface(slot, ViewHit::SelectMode(2), space);
    driven.frame();
    let row_of = |driven: &Driven<'_>, line: usize| {
        driven
            .attach
            .model
            .surface_frame
            .text_rows
            .iter()
            .find(|row| row.line == line)
            .cloned()
            .unwrap_or_else(|| panic!("line {line} of the source is drawn"))
    };
    let (title, rule) = (row_of(&driven, 1), row_of(&driven, 2));
    driven.press(title.glyphs[0].x, title.area.y);
    driven.mouse(
        rule.glyphs[2].x,
        rule.area.y,
        MouseEventKind::Drag(MouseButton::Left),
    );
    driven.mouse(
        rule.glyphs[2].x,
        rule.area.y,
        MouseEventKind::Up(MouseButton::Left),
    );

    let expected: Vec<&str> = source.lines().skip(1).take(2).collect();
    assert_eq!(
        driven.attach.model.clipboard.as_deref(),
        Some(expected.join("\n").as_str())
    );
}

/// The architect is hosted like any extension: opening it asks its read as
/// a job through the workspace's host, and the answer reaches its engine.
#[test]
fn opening_the_architect_reads_its_artifacts_as_a_job() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-architect-job"));
    let mut model = agent_session_in("/repo");
    // The spec counts the checkout in front on the same pass, and the
    // code reads its history; off, only the architect's read is handed
    // out.
    model.follow_extension_switch(
        [
            uze_extensions::spec::CATALOG.id.to_owned(),
            uze_extensions::code::CATALOG.id.to_owned(),
        ]
        .into(),
    );
    super::open_architect(&mut model);
    assert!(model.showing(uze_extensions::architect::CATALOG.id));
    let reading = |model: &WorkspaceModel| {
        matches!(
            architect_view(model, uze_extensions::view::Size { width: 80, height: 24 }).content,
            uze_extensions::view::Content::Message { text, .. } if text.starts_with("Reading")
        )
    };
    assert!(reading(&model));
    let (sender, receiver) = std::sync::mpsc::channel();
    assert_eq!(model.schedule_engine_jobs(&home, &sender), 1);
    assert_eq!(model.schedule_engine_jobs(&home, &sender), 0, "asked once");
    let answer = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the read answers");
    assert!(model.absorb_engine_answer(answer));
    assert!(!reading(&model), "the answer was drawn");
}

/// The code surface's tree is listed by its jobs, and drawn once they
/// answer: nothing of it is read on the thread that draws.
#[test]
fn opening_the_code_surface_lists_its_tree_as_jobs() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-code-jobs"));
    let root = uze_testkit::temp::scratch("orchestrator-code-jobs-checkout");
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    let mut model = agent_session_in(root.to_str().unwrap());
    model.follow_extension_switch([uze_extensions::spec::CATALOG.id.to_owned()].into());
    open_code(&mut model, uze_extensions::code::FILES);
    assert!(model.showing(uze_extensions::code::CATALOG.id));
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut handed = model.schedule_engine_jobs(&home, &sender);
    assert!(handed >= 1, "the surface asked for its tree");
    while handed > 0 {
        let answer = receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("every job answers");
        assert!(model.absorb_engine_answer(answer));
        handed = handed - 1 + model.schedule_engine_jobs(&home, &sender);
    }
    let view = uze_extensions::code::view(
        code_view(&model).expect("open"),
        uze_extensions::view::Size {
            width: 80,
            height: 24,
        },
    );
    let rows = view.navigator.expect("a tree").rows;
    assert!(
        format!("{rows:?}").contains("main.rs"),
        "the listing was drawn: {rows:?}"
    );
}

/// A board held and moved follows the pointer; one pressed and let go
/// where it was is a click on what was under it.
#[test]
fn the_architects_board_is_dragged_through_its_slot() {
    use uze_extensions::architect;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-architect-drag"));
    let mut model = agent_session_in("/repo");
    open_architect_with(
        &mut model,
        vec![architect::Artifact::read(
            "crate-layering.mmd",
            include_str!("../../../../docs/architecture/crate-layering.mmd"),
        )],
    );
    let slot = architect_slot(&model);
    let mut driven = driven(model, &home).on_a_roomy_terminal();
    driven.frame();
    let board = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| {
            matches!(
                hit,
                WorkspaceHit::Extension(ExtensionHit {
                    slot: hit_slot,
                    hit: ViewHit::PlaceCaret { .. },
                }) if *hit_slot == slot
            )
        })
        .map(|(rect, _)| *rect)
        .expect("the board is drawn");
    let before = architect_place(&driven.attach.model);
    let (column, row) = (board.x + board.width / 2, board.y + board.height / 2);
    driven.press(column, row);
    assert!(
        driven.attach.model.board_grab.is_some(),
        "held, not clicked"
    );
    driven.mouse(column + 4, row + 2, MouseEventKind::Drag(MouseButton::Left));
    driven.mouse(column + 4, row + 2, MouseEventKind::Up(MouseButton::Left));
    assert!(driven.attach.model.board_grab.is_none());
    assert_ne!(
        architect_place(&driven.attach.model),
        before,
        "the board moved with the pointer"
    );
}

/// The spec summary is counted for the checkout in front as its engine's
/// job, once while it stands, and the answer reaches the sidebar.
#[test]
fn the_spec_summary_is_counted_as_a_job_for_the_checkout_in_front() {
    let repository = uze_testkit::git::Repository::new("orchestrator-spec-count");
    let target = repository.branch();
    repository.commit_file("openspec/config.yaml", "schema: spec-driven\n");
    repository.git(&["checkout", "--quiet", "-b", "work"]);
    repository.commit_file("openspec/changes/mine/tasks.md", "- [x] a\n- [ ] b\n");
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-spec-count-home"));
    let root = repository.root().display().to_string();
    let mut model = agent_session_in(&root);
    // What the checkout delivers to, as an evaluation left it: what the
    // count is measured against.
    model
        .remembered
        .targets
        .insert(super::evaluation_key(repository.root()), target);
    // The code reads the same checkout's history on the same pass.
    model.follow_extension_switch([uze_extensions::code::CATALOG.id.to_owned()].into());
    let (sender, receiver) = std::sync::mpsc::channel();
    assert_eq!(model.schedule_engine_jobs(&home, &sender), 1);
    assert_eq!(
        model.schedule_engine_jobs(&home, &sender),
        0,
        "one count at a time"
    );
    let answer = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the count answers");
    assert!(model.absorb_engine_answer(answer));
    let (_, section) = model.spec_summary().expect("the section is drawn");
    assert_eq!(section.rows.len(), 1);
    assert_eq!(model.schedule_engine_jobs(&home, &sender), 0, "fresh");
}
