//! What the extension surfaces show inside the host: the code, spec and
//! architect surfaces, where they resume, and what reaches them.

use super::WorkspaceHit;
use super::support::*;
use super::{
    ExtensionHit, SpecResolution, SpecSummaryState, WorkspaceModel, open_architect, open_code,
    open_spec,
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
        let view = model.code.as_mut().expect("the surface is open");
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
        crate::ui::extension_view::content_space(Rect::new(0, 0, 120, 40), model.code_tree_width);

    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    answer_listing(&mut model, "/repo/.worktrees/a");
    // Walked away from the row the tree opened on, which is the part
    // that must survive the round trip.
    code::handle_command(
        model.code.as_mut().expect("open"),
        uze_extensions::view::Command::SelectNext,
        space,
    );
    let walked_to = model.code.as_ref().expect("open").place();

    model.close_code();
    assert!(model.code.is_none());

    // Another agent's checkout is a different place, not this one.
    model.session.as_mut().expect("session").select_tab(second);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    answer_listing(&mut model, "/repo/.worktrees/b");
    assert_ne!(
        model.code.as_ref().expect("open").place(),
        walked_to,
        "a checkout never visited opens on its own first row"
    );
    model.close_code();

    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    assert_eq!(
        model.code.as_ref().expect("open").place(),
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
        let view = model.architect.as_mut().expect("the surface is open");
        view.absorb(architect::ArtifactsAnswer {
            branch: "main".to_owned(),
            artifacts: architect::Artifacts::Found {
                artifacts: vec![
                    architect::Artifact::read("one.mmd", "flowchart TD\n a --> b"),
                    architect::Artifact::read("two.mmd", "flowchart TD\n c --> d"),
                ],
                project: PathBuf::from("/repo"),
            },
        });
    };
    let space = uze_extensions::view::Size {
        width: 120,
        height: 40,
    };

    model.session.as_mut().expect("session").select_tab(first);
    open_architect(&mut model);
    answer_artifacts(&mut model);
    // Moved off the diagram the surface opens on, which is the part
    // that must survive the round trip.
    architect::handle_command(
        model.architect.as_mut().expect("open"),
        Command::NextView,
        space,
    );
    let moved_to = model.architect.as_ref().expect("open").place();

    model.close_architect();
    assert!(model.architect.is_none());

    // Another agent's checkout is a different place, not this one.
    model.session.as_mut().expect("session").select_tab(second);
    open_architect(&mut model);
    answer_artifacts(&mut model);
    assert_ne!(
        model.architect.as_ref().expect("open").place(),
        moved_to,
        "a checkout never visited opens on its own first diagram"
    );
    model.close_architect();

    model.session.as_mut().expect("session").select_tab(first);
    open_architect(&mut model);
    answer_artifacts(&mut model);
    assert_eq!(
        model.architect.as_ref().expect("open").place(),
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
    use std::sync::mpsc;

    use uze_extensions::code;

    let root = PathBuf::from("/repo/.worktrees/a");
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    model.remembered.code_places.insert(
        root.clone(),
        code::CodePlace::at(&root, &root.join("src/ui/widget/row.rs"), false),
    );
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);

    let (sender, _receiver) = mpsc::channel();
    model.schedule_file_request(&sender);

    let held = model.code_request_pending;
    let waiting = model
        .code
        .as_ref()
        .expect("the surface is open")
        .peek_request();
    assert!(
        waiting.is_none(),
        "every directory went out together, and the read behind them with it: {waiting:?}"
    );
    assert!(
        held,
        "the read is what took the one-at-a-time chain, not the listings"
    );
}

/// Measuring a checkout opens every file it has, and the surface it
/// feeds is opened and closed all day. The second open shows the map
/// it already has rather than paying for it again and showing none
/// until it lands.
#[test]
fn a_checkout_measured_once_is_not_measured_again_on_the_way_back_in() {
    use std::sync::mpsc;

    use uze_extensions::code;

    use crate::ui::orchestrator::MeasureResolution;

    let root = PathBuf::from("/repo/.worktrees/a");
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);

    let measured = code::Measure {
        root: root.clone(),
        files: vec![code::FileMeasure {
            path: "src/main.rs".to_owned(),
            lines: 120,
            commits: 3,
            changed: false,
        }],
    };
    model.absorb_measure(MeasureResolution {
        root: root.clone(),
        measure: Some(measured),
    });
    assert!(model.code.as_ref().expect("open").has_map());

    model.close_code();
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    assert!(
        model.code.as_ref().expect("open").has_map(),
        "the map is there the moment the surface is"
    );

    model
        .code
        .as_mut()
        .expect("open")
        .show(uze_extensions::code::ContentMode::Map);
    let (sender, _receiver) = mpsc::channel();
    model.schedule_code_measure(&sender);
    assert!(
        model.code_measure_asked.is_none(),
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
    use uze_extensions::{ExtensionHit, code, view::ViewHit};

    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-content-groove"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let mut view = code::CodeView::opening(
        PathBuf::from("/repo"),
        "/repo".to_owned(),
        code::ContentMode::Contents,
    );
    view.take_request();
    model.code = Some(view);

    let mut driven = driven(model, &home);
    driven.frame();
    let pane = compute_layout(driven.area, driven.attach.model.sidebar_width).pane;
    let track = Rect::new(pane.right(), pane.y + 1, 1, pane.height - 1);
    let bar = crate::ui::widget::Scrollbar::measure(track, usize::from(track.height), 500)
        .expect("five hundred lines in a pane is a scrollbar");
    driven.attach.model.code_scrollbars.content_bar = Some(bar);
    driven.attach.model.hits.insert(
        0,
        (
            track,
            WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::DragContentScrollbar)),
        ),
    );
    let place = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .code
            .as_ref()
            .expect("still open")
            .place()
    };
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
    model.session.as_mut().expect("session").select_tab(first);

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
    model.code = Some(view);

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
            WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::SelectSubject(_)))
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
    model.session.as_mut().expect("session").select_tab(first);
    let cwd = model.focused_cwd().expect("a tab in front");
    model.remembered.spec_summary = Some(SpecSummaryState {
        cwd,
        summary: Some(Summary {
            changes: vec![ChangeSummary {
                name: "mine".to_owned(),
                progress: Some(Progress { done: 1, total: 4 }),
            }],
            done: 1,
            total: 4,
        }),
        checked_at: std::time::Instant::now(),
    });
    model.spec = Some(uze_extensions::spec::SpecView::opening("/repo".to_owned()));
    let mut driven = driven(model, &home);
    driven.frame();
    let header = driven.hit(|hit| {
        *hit == WorkspaceHit::Extension(ExtensionHit::SpecSummary(ViewHit::ToggleSection))
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
    model.session.as_mut().expect("session").select_tab(first);
    model.timeline_collapsed = false;
    let cwd = model.focused_cwd().expect("a tab in front");
    model.remembered.spec_summary = Some(SpecSummaryState {
        cwd,
        summary: Some(Summary {
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
        }),
        checked_at: std::time::Instant::now(),
    });
    let mut driven = driven(model, &home);
    driven.frame();

    let drawn = |driven: &Driven<'_>, wanted: ViewHit| {
        driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| *hit == WorkspaceHit::Extension(ExtensionHit::SpecSummary(wanted)))
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
    let place = driven
        .attach
        .model
        .spec
        .as_ref()
        .expect("the surface opened")
        .place();
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
            .find(|(_, hit)| *hit == WorkspaceHit::Extension(ExtensionHit::SpecSummary(wanted)))
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
    let answer = |root: PathBuf| SpecResolution {
        root: root.clone(),
        answer: uze_extensions::spec::SpecAnswer {
            root,
            branch: "main".to_owned(),
            theme: String::new(),
            found: uze_extensions::spec::Found::NoLayout,
            subjects: Vec::new(),
        },
    };
    model.session.as_mut().expect("session").select_tab(first);
    open_spec(&mut model);
    let asked_for = model.spec_root.clone().expect("a root");
    model.close_spec();
    assert!(!model.absorb_spec(answer(asked_for.clone())));
    assert!(model.spec.is_none(), "nothing reopened");

    model.session.as_mut().expect("session").select_tab(second);
    open_spec(&mut model);
    if model.spec_root.as_ref() != Some(&asked_for) {
        assert!(
            !model.absorb_spec(answer(asked_for.clone())),
            "an answer about another checkout is not drawn"
        );
    }
    let here = model.spec_root.clone().expect("a root");
    assert!(model.absorb_spec(answer(here)), "its own answer lands");
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
        model.spec.as_mut().expect("open").absorb(spec::SpecAnswer {
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
        });
    };
    let space = uze_extensions::view::Size {
        width: 120,
        height: 40,
    };

    model.session.as_mut().expect("session").select_tab(first);
    open_spec(&mut model);
    answer(&mut model);
    for command in [Command::Expand, Command::SelectNext, Command::SelectNext] {
        spec::handle_command(model.spec.as_mut().expect("open"), command, space);
    }
    let left_on = model.spec.as_ref().expect("open").place();
    model.close_spec();

    open_spec(&mut model);
    answer(&mut model);
    assert_eq!(model.spec.as_ref().expect("open").place(), left_on);
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
    use uze_extensions::{ExtensionHit, code, view::ViewHit};

    let root = PathBuf::from("/repo/.worktrees/a");
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    let view = model.code.as_mut().expect("the surface is open");
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
        model.code_scrollbars.content_gutter, 0,
        "a drawing carries no line numbers, so it has no gutter"
    );
    let (rect, line, cell) = model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::PlaceCaret { line, cell })) => {
                Some((*rect, *line, *cell))
            }
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
            model.code_scrollbars.content_gutter,
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
    model.session.as_mut().expect("session").select_tab(first);

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
    model.code = Some(view);
    full_frame(&mut model);

    let row = model
        .hits
        .iter()
        .find(|(_, hit)| {
            matches!(
                hit,
                WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::SelectItem(_)))
            )
        })
        .expect("the explorer drew a clickable file row")
        .0;
    assert!(
        matches!(
            model.hit_at(row.x + 2, row.y),
            Some(WorkspaceHit::Extension(ExtensionHit::Code(
                ViewHit::SelectItem(_)
            )))
        ),
        "a click on the row resolves to that row, not to something under it"
    );

    let caret = model.hits.iter().find(|(_, hit)| {
        matches!(
            hit,
            WorkspaceHit::Extension(ExtensionHit::Code(ViewHit::PlaceCaret { .. }))
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
    let mut view = architect::ArchitectView::opening("~/repo".to_owned());
    view.absorb(architect::ArtifactsAnswer {
        branch: "main".to_owned(),
        artifacts: architect::Artifacts::Found {
            artifacts: [
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
            project: PathBuf::from("/repo"),
        },
    });
    let mut model = model_of(session("/repo"));
    model.architect = Some(view);
    // Roomy, because the board is drawn beside the sidebar: at the
    // default width the menu has no room to name what it chooses.
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    let space = crate::ui::extension_view::board_space(compute_layout(driven.area, None).pane);
    let highlighted = |driven: &Driven<'_>| {
        architect::view(driven.attach.model.architect.as_ref().unwrap(), space)
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
                *hit == WorkspaceHit::Extension(ExtensionHit::Architect(ViewHit::SelectItem(item)))
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
            *hit == WorkspaceHit::Extension(ExtensionHit::Architect(ViewHit::ChooseItem))
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
    let mut view = architect::ArchitectView::opening("~/repo".to_owned());
    view.absorb(architect::ArtifactsAnswer {
        branch: "main".to_owned(),
        artifacts: architect::Artifacts::Found {
            artifacts: [architect::Artifact::read("crate-layering.mmd", source)].into(),
            project: PathBuf::from("/repo"),
        },
    });
    let mut model = model_of(session("/repo"));
    model.architect = Some(view);
    let mut driven = driven(model, &home).on_a_roomy_terminal();

    driven.frame();
    assert!(
        driven.attach.model.code_scrollbars.text_rows.is_empty(),
        "a diagram has no text to mark"
    );

    let space = driven.attach.model.code_scrollbars.content_space;
    if let Some(view) = driven.attach.model.architect.as_mut() {
        architect::handle_mouse(view, Some(ViewHit::SelectMode(2)), space);
    }
    driven.frame();
    let row_of = |driven: &Driven<'_>, line: usize| {
        driven
            .attach
            .model
            .code_scrollbars
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
