//! The surface buttons and chords: opening, switching and closing the
//! extension surfaces, and an extension switched off.

use super::WorkspaceHit;
use super::support::*;
use super::{
    ExtensionHit, GitAnswer, GitBadge, GitResolution, WorkStateView, WorkspaceModel,
    open_architect, open_code, open_spec,
    render::{LeadingSlot, render_tab_strip},
};
use crate::ui::theme::{self, Token};
use crossterm::event::MouseEventKind;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uze_core::UzeHome;
use uze_terminal::{ClientEvent, ClientRequest};

/// The colours one header chip is drawn in: the surface under its
/// padding, and the label's own foreground. Read off the cells rather
/// than off the skin function, so the test proves what reaches the
/// screen.
fn chip_colors(model: &WorkspaceModel, rect: Rect) -> (Color, Color) {
    let mut terminal = Terminal::new(TestBackend::new(80, 3)).unwrap();
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
    (buffer[(rect.x + 1, rect.y)].fg, buffer[(rect.x, rect.y)].bg)
}

/// An open surface stands where the pane is, and its navigation in the
/// bar's leading slot: the sidebar and the bar's trailing controls are
/// still drawn and still answer, and nothing the surface draws reaches
/// anywhere else.
#[test]
fn an_open_surface_is_drawn_where_the_pane_is() {
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);

    let layout = full_frame(&mut model);
    let extension_hits: Vec<Rect> = model
        .hits
        .iter()
        .filter(|(_, hit)| matches!(hit, WorkspaceHit::Extension(ExtensionHit::Code(_))))
        .map(|(rect, _)| *rect)
        .collect();
    assert!(!extension_hits.is_empty(), "the surface is drawn");
    let bar_row = Rect::new(
        layout.tab_strip.x,
        layout.tab_strip.y,
        layout.tab_strip.width,
        1,
    );
    assert!(
        extension_hits.iter().all(|rect| {
            layout.pane.intersection(*rect) == *rect || bar_row.intersection(*rect) == *rect
        }),
        "inside the pane or the bar's row: {extension_hits:?} vs {:?}",
        layout.pane
    );
    assert!(
        model
            .hits
            .iter()
            .any(|(_, hit)| *hit == WorkspaceHit::OpenFiles),
        "the bar's trailing controls are still there to be clicked"
    );
    assert!(
        model
            .hits
            .iter()
            .any(|(rect, hit)| *hit == WorkspaceHit::SelectTab(first)
                && layout.sidebar.intersection(*rect) == *rect),
        "and so is the sidebar"
    );
}

/// Nothing an open surface draws a target for lies over the sidebar,
/// so its sections answer as they do with no surface open.
#[test]
fn an_open_surface_leaves_the_sidebar_its_own() {
    for open in [
        (|model: &mut WorkspaceModel| open_code(model, uze_extensions::code::ContentMode::Contents))
            as fn(&mut WorkspaceModel),
        |model| model.spec = Some(uze_extensions::spec::SpecView::opening("/repo".to_owned())),
        |model| {
            model.architect = Some(uze_extensions::architect::ArchitectView::opening(
                "/repo".to_owned(),
            ))
        },
    ] {
        let (mut model, first, _second) = two_agents_with_shells();
        model.session.as_mut().expect("session").select_tab(first);
        open(&mut model);
        let layout = full_frame(&mut model);
        let over: Vec<_> = model
            .hits
            .iter()
            .filter(|(rect, hit)| hit.is_open_surfaces() && layout.sidebar.intersects(*rect))
            .collect();
        assert!(over.is_empty(), "over the sidebar: {over:?}");
    }
}

/// The strip lights one thing, the one in the pane: the selected tab,
/// or — while a surface stands over it — that surface's button.
#[test]
fn the_strip_lights_one_thing_the_one_in_the_pane() {
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let raised = crate::ui::theme::color(Token::SurfaceRaised);
    let lit = crate::ui::theme::color(Token::TextBright);
    let ink = crate::ui::theme::color(Token::SurfaceBackground);
    let resting = crate::ui::theme::color(Token::TextSecondary);

    let tab = hit_rect(&model, WorkspaceHit::SelectTab(first));
    let button = hit_rect(&model, WorkspaceHit::OpenFiles);
    assert_eq!(chip_colors(&model, tab).1, raised, "the tab, at first");
    assert_eq!(chip_colors(&model, button), (resting, raised));

    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    let layout = full_frame(&mut model);
    assert!(
        !model.hits.iter().any(|(rect, hit)| {
            *hit == WorkspaceHit::SelectTab(first) && rect.y == layout.tab_strip.y
        }),
        "the tab gives the bar up to the surface's navigation"
    );
    let button = hit_rect(&model, WorkspaceHit::OpenFiles);
    assert_eq!(
        chip_colors(&model, button),
        (ink, lit),
        "to the surface, filled"
    );
}

/// Each surface's chord leads to it from the other, without closing
/// first: the pane shows one surface, and the keys walk between them.
/// Only the chord of the half already showing closes it.
#[test]
fn the_surface_chords_walk_between_surfaces_without_closing_first() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use uze_extensions::code::CodeView;
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-surface-chords"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let mut driven = driven(model, &home);
    let showing = |driven: &Driven<'_>| {
        let model = &driven.attach.model;
        match (
            model.code.as_ref().map(CodeView::showing),
            model.architect.is_some(),
            model.spec.is_some(),
        ) {
            (Some(mode), false, false) => format!("code {mode:?}"),
            (None, true, false) => "architect".to_owned(),
            (None, false, true) => "spec".to_owned(),
            (None, false, false) => "pane".to_owned(),
            _ => panic!("two surfaces at once"),
        }
    };
    for (key, expected) in [
        ('e', "code Contents"),
        ('g', "code Diff"),
        ('e', "code Contents"),
        ('a', "architect"),
        ('g', "code Diff"),
        ('a', "architect"),
        ('e', "code Contents"),
        ('e', "pane"),
        ('g', "code Diff"),
        ('g', "pane"),
        ('a', "architect"),
        ('a', "pane"),
        ('x', "spec"),
        ('a', "architect"),
        ('x', "spec"),
        ('e', "code Contents"),
        ('x', "spec"),
        ('g', "code Diff"),
        ('x', "spec"),
        ('x', "pane"),
    ] {
        driven.frame();
        driven.press_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::ALT));
        assert_eq!(showing(&driven), expected, "after alt+{key}");
    }
}

/// The lit button is the surface, whichever half of it is showing:
/// one click puts it away. It used to be the files door, so from the
/// changes it switched to the files and only a second click closed.
#[test]
fn a_lit_surface_button_closes_it_in_one_click() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-lit-button"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Diff);
    let mut driven = driven(model, &home);
    driven.frame();

    let button = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::OpenFiles)
        .map(|(rect, _)| *rect)
        .expect("the code button");
    driven.press(button.x, button.y);
    assert!(driven.attach.model.code.is_none(), "closed in one click");
}

#[test]
fn the_spec_button_opens_and_closes_the_spec_surface() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-spec-button"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let mut driven = driven(model, &home);
    driven.frame();
    let order: Vec<WorkspaceHit> = driven
        .attach
        .model
        .hits
        .iter()
        .filter(|(_, hit)| {
            matches!(
                hit,
                WorkspaceHit::OpenSpec | WorkspaceHit::OpenArchitect | WorkspaceHit::OpenFiles
            )
        })
        .map(|(rect, hit)| (rect.x, *hit))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect();
    assert_eq!(
        order,
        [
            WorkspaceHit::OpenSpec,
            WorkspaceHit::OpenArchitect,
            WorkspaceHit::OpenFiles
        ],
        "intent, description, the code itself"
    );

    let button = hit_rect(&driven.attach.model, WorkspaceHit::OpenSpec);
    driven.press(button.x, button.y);
    assert!(driven.attach.model.spec.is_some(), "opened by its button");
    assert!(driven.attach.model.code.is_none());
    assert!(driven.attach.model.architect.is_none());

    driven.frame();
    let button = hit_rect(&driven.attach.model, WorkspaceHit::OpenSpec);
    driven.press(button.x, button.y);
    assert!(driven.attach.model.spec.is_none(), "closed in one click");
}

fn switched_off(ids: &[&str]) -> std::collections::BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

/// Switching an extension off takes every way in with it: the surface
/// standing in the pane closes, its button leaves the strip, its key
/// reaches the program underneath, and the index stops listing it.
#[test]
fn an_extension_switched_off_leaves_the_strip_the_keys_and_the_index() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-extension-off"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_architect(&mut model);
    model.follow_extension_switch(switched_off(&[uze_extensions::architect::CATALOG.id]));
    assert!(model.architect.is_none(), "the open surface closed");

    let mut driven = driven(model, &home);
    driven.frame();
    let hits: Vec<WorkspaceHit> = driven
        .attach
        .model
        .hits
        .iter()
        .map(|(_, hit)| *hit)
        .collect();
    assert!(!hits.contains(&WorkspaceHit::OpenArchitect));
    assert!(hits.contains(&WorkspaceHit::OpenSpec));
    assert!(hits.contains(&WorkspaceHit::OpenFiles));

    driven.sent();
    driven.press_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    assert!(
        driven.attach.model.architect.is_none(),
        "its key opens nothing"
    );
    assert!(
        driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::Input { .. })),
        "the chord is the program's again"
    );

    let listed: Vec<uze_keys::Action> = super::action_index_rows(
        &[
            uze_keys::Scope::Global,
            uze_keys::Scope::Workspace,
            uze_keys::Scope::Surfaces,
        ],
        &[],
        "",
        &driven.attach.model.disabled_extensions,
    )
    .into_iter()
    .map(|(action, _)| action)
    .collect();
    assert!(!listed.contains(&uze_keys::Action::ToggleArchitect));
    assert!(listed.contains(&uze_keys::Action::ToggleSpec));
}

#[test]
fn every_extension_switched_off_takes_the_whole_button_group() {
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-extensions-all-off",
    ));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    model.follow_extension_switch(switched_off(&[
        uze_extensions::code::CATALOG.id,
        uze_extensions::architect::CATALOG.id,
        uze_extensions::spec::CATALOG.id,
    ]));
    let mut driven = driven(model, &home);
    driven.frame();
    assert!(!driven.attach.model.hits.iter().any(|(_, hit)| matches!(
        hit,
        WorkspaceHit::OpenSpec | WorkspaceHit::OpenArchitect | WorkspaceHit::OpenFiles
    )));
    assert!(
        !driven
            .attach
            .model
            .first_steps()
            .steps
            .iter()
            .any(|step| matches!(
                step,
                uze_keys::Action::ToggleChanges | uze_keys::Action::ToggleFiles
            )),
        "a first step nobody can take is not offered"
    );
}

/// Nothing is read for a sidebar section whose extension is off, and
/// an answer already on its way when it was switched off is dropped.
#[test]
fn a_switched_off_extension_reads_nothing_for_the_sidebar() {
    let mut model = agent_session_in("/repo");
    model.follow_extension_switch(switched_off(&[
        uze_extensions::code::CATALOG.id,
        uze_extensions::spec::CATALOG.id,
    ]));
    let (git, _git_answers) = std::sync::mpsc::channel();
    let (summary, _summary_answers) = std::sync::mpsc::channel();

    model.schedule_git_read(&git);
    model.schedule_spec_summary(&summary);

    assert!(model.remembered.git_pending.is_none());
    assert!(model.remembered.spec_summary_pending.is_none());
    assert!(!model.absorb_git_read(GitResolution {
        cwd: PathBuf::from("/repo"),
        answer: GitAnswer::Summary(None),
        took: Duration::ZERO,
    }));
    assert!(model.remembered.git_badge.is_none());
}

/// Switching back on puts it all back: the button returns and a key
/// opens the surface again.
#[test]
fn an_extension_switched_back_on_opens_again() {
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    model.follow_extension_switch(switched_off(&[uze_extensions::spec::CATALOG.id]));
    open_spec(&mut model);
    assert!(model.spec.is_none());

    model.follow_extension_switch(std::collections::BTreeSet::new());
    open_spec(&mut model);
    assert!(model.spec.is_some());
}

/// The surface seals the keyboard: a letter it has no use for is not
/// typed into the program underneath it.
#[test]
fn a_key_typed_over_the_spec_surface_does_not_reach_the_pane() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-spec-seals"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_spec(&mut model);
    let mut driven = driven(model, &home);
    driven.frame();
    driven.sent();

    driven.press_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

    assert!(driven.attach.model.spec.is_some());
    assert!(
        !driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::Input { .. })),
        "nothing reached the pane"
    );
}

/// A surface's button is a switch: the press is answered by it
/// lighting or going out, and no press flash is drawn between the
/// two — that flash was a third look, between lit and unlit.
#[test]
fn a_surface_button_goes_out_without_a_press_flash() {
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    model.close_code();
    model.pressed = Some((WorkspaceHit::OpenFiles, std::time::Instant::now()));
    model.hovered = Some(WorkspaceHit::OpenFiles);

    let button = hit_rect(&model, WorkspaceHit::OpenFiles);
    assert_eq!(
        chip_colors(&model, button),
        (
            crate::ui::theme::color(Token::TextSecondary),
            crate::ui::theme::color(Token::SurfaceHover)
        ),
        "straight to what a pointer over it looks like"
    );
}

/// Two quick clicks on a switch are two presses: it opens, then puts
/// the surface away. The second used to be read as a double click,
/// which a surface button has no meaning for, and was dropped.
#[test]
fn two_quick_clicks_on_a_surface_button_open_and_close_it() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-quick-clicks"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    let mut driven = driven(model, &home);
    driven.frame();
    let button = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::OpenFiles)
        .map(|(rect, _)| *rect)
        .expect("the code button");

    driven.press(button.x, button.y);
    assert!(driven.attach.model.code.is_some(), "the first opens");
    driven.frame();
    driven.press(button.x, button.y);
    assert!(driven.attach.model.code.is_none(), "the second closes");
}

/// Choosing a tab is choosing to see it — the one already in front
/// included, which is how the pane is had back from a surface.
#[test]
fn choosing_a_tab_puts_the_open_surface_away() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-surface-tab"));
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_code(&mut model, uze_extensions::code::ContentMode::Contents);
    let mut driven = driven(model, &home);
    driven.frame();

    let tab = driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::SelectTab(first))
        .map(|(rect, _)| *rect)
        .expect("the agent's own row");
    driven.press(tab.x, tab.y);
    assert!(driven.attach.model.code.is_none());
}

/// A command handed to a new shell is typed into the pane that shell
/// turns out to be, and only there: a session update that brings no
/// new pane leaves it waiting, and the line carries no Enter, since the
/// person runs it.
#[test]
fn a_command_waits_for_its_new_shell_and_is_typed_unrun() {
    let mut model = model_of(session("/tmp/project"));
    let identities = identities_fixture();
    model.typing = Some(crate::ui::orchestrator::PendingTyping {
        known: model.pane_ids(),
        text: "sudo apt-get install -y jq".to_owned(),
    });

    let unchanged = model.session.clone().expect("session");
    model.apply(
        ClientEvent::SessionUpdated { session: unchanged },
        &identities,
    );
    assert!(model.typed.is_none(), "no new pane yet");
    assert!(model.typing.is_some());

    let mut opened = model.session.clone().expect("session");
    let space = opened.selected_space().id;
    let shell = opened.add_tab(space, "shell 2".into(), None, 80, 24, "/tmp/project".into());
    model.apply(ClientEvent::SessionUpdated { session: opened }, &identities);
    assert_eq!(
        model.typed,
        Some((shell, b"sudo apt-get install -y jq".to_vec())),
        "typed into the new shell, with no Enter"
    );
    assert!(model.typing.is_none());
}

/// A surface is about the tab it was opened on. Another tab coming to
/// the front — by whatever way it got there — takes the pane back.
#[test]
fn another_tab_in_front_puts_the_open_surface_away() {
    let (mut model, first, second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);
    open_architect(&mut model);

    let mut session = model.session.clone().expect("session");
    session.rename_tab(first, "renamed".to_owned());
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    assert!(model.architect.is_some(), "the same tab in front keeps it");

    let mut session = model.session.clone().expect("session");
    session.select_tab(second);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    assert!(model.architect.is_none());
}

/// A group of buttons is one ground with a seam where the fills
/// meet, and the seam is the only divider it has. A glyph drawn
/// between two members belongs to neither: hovering one leaves that
/// column wearing the resting fill, a sliver of a third material
/// wedged between two buttons — which is what the pair at the tab
/// side's end looked like, and what the pair at the other end never
/// did. One construction now answers for both.
#[test]
fn a_button_group_is_divided_by_where_its_fill_changes_and_by_nothing_drawn() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    let grounds = |model: &WorkspaceModel, rect: Rect| {
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
        (rect.x..rect.right())
            .map(|column| buffer[(column, rect.y)].bg)
            .collect::<Vec<_>>()
    };

    let (left, right) = (WorkspaceHit::OpenArchitect, WorkspaceHit::OpenFiles);
    let (first, second) = (hit_rect(&model, left), hit_rect(&model, right));
    assert_eq!(
        first.right(),
        second.x,
        "the members meet — there is no column between them"
    );

    model.hovered = Some(left);
    let lit = grounds(&model, first);
    let rest = grounds(&model, second);
    assert!(
        lit.iter().all(|ground| *ground == lit[0]),
        "the hovered member lights whole, padding included: {lit:?}"
    );
    assert!(
        rest.iter().all(|ground| *ground == rest[0]),
        "and its neighbour stays whole at rest: {rest:?}"
    );
    assert_ne!(
        lit[0], rest[0],
        "so the boundary between them is where the fill changes"
    );
}

/// The changes count is a door that is not dressed as one. Three
/// filled controls in a row and a count that grew a plate the moment
/// the pointer went near it read as a third button; the hue does the
/// work instead — held back at rest and restored under the pointer —
/// and no ground is ever drawn behind it.
///
/// The colour cannot simply be dropped at rest the way a grey label's
/// could: green is additions and red is deletions, so the hue *is*
/// the badge's message. Muted, then, never grey.
#[test]
fn the_changes_count_answers_with_its_hue_and_never_with_a_ground() {
    let mut model = agent_with_task(WorkStateView::Ready, 3);
    model.remembered.git_badge = Some(GitBadge {
        cwd: PathBuf::from("/repo"),
        summary: Some(uze_extensions::code::ChangeSummary {
            additions: 476,
            deletions: 286,
        }),
        timeline: None,
        timeline_checked_at: Instant::now(),
        checked_at: Instant::now(),
    });
    let (_, hits) = tab_strip(&model);
    let rect = hits
        .iter()
        .find(|(_, hit)| *hit == WorkspaceHit::OpenChanges)
        .expect("work to report brings the count")
        .0;

    // Read off the cells rather than off the code that chose them, so
    // the test proves what reaches the screen. The additions sit one
    // column past the badge's own leading pad.
    let cells = |model: &WorkspaceModel| {
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
        (cell.fg, cell.bg)
    };

    assert_eq!(
        cells(&model),
        (theme::color(Token::StateSuccessMuted), Color::Reset),
        "at rest: the hue held back, and the strip's own backdrop under it"
    );

    model.hovered = Some(WorkspaceHit::OpenChanges);
    assert_eq!(
        cells(&model),
        (theme::color(Token::StateSuccess), Color::Reset),
        "under the pointer: the hue restored, and still no ground"
    );

    model.hovered = None;
    model.pressed = Some((WorkspaceHit::OpenChanges, Instant::now()));
    assert_eq!(
        cells(&model).1,
        Color::Reset,
        "and a press draws no plate either"
    );
}

/// The changes badge is a count that is also a door: it says how much
/// changed, so with nothing to say it says nothing. Two things that
/// must not cost: reachability — the code button never leaves the
/// pair, and the diff is one mode switch away inside the surface it
/// opens — and steadiness, since the badge comes and goes on a Git
/// read nobody asked for. It takes its room to the left, so the pair
/// sits at the same columns whether or not there is work to report.
#[test]
fn the_changes_badge_comes_with_the_work_and_the_code_button_never_leaves() {
    let (mut model, first, _second) = two_agents_with_shells();
    model.session.as_mut().expect("session").select_tab(first);

    let button = |model: &WorkspaceModel, wanted: WorkspaceHit| {
        model
            .hits
            .iter()
            .find(|(_, hit)| *hit == wanted)
            .map(|(rect, _)| *rect)
    };

    model.remembered.git_badge = None;
    full_frame(&mut model);
    let clean =
        button(&model, WorkspaceHit::OpenFiles).expect("a clean checkout still offers its files");
    assert!(
        button(&model, WorkspaceHit::OpenChanges).is_none(),
        "and says nothing about changes rather than saying zero"
    );

    model.remembered.git_badge = Some(GitBadge {
        cwd: PathBuf::from("/repo/.worktrees/a"),
        summary: Some(uze_extensions::code::ChangeSummary {
            additions: 3,
            deletions: 1,
        }),
        timeline: None,
        timeline_checked_at: Instant::now(),
        checked_at: Instant::now(),
    });
    full_frame(&mut model);
    let changes =
        button(&model, WorkspaceHit::OpenChanges).expect("work arriving brings the badge");
    let files =
        button(&model, WorkspaceHit::OpenFiles).expect("and leaves the code button where it was");
    assert!(
        changes.right() <= files.x,
        "the badge takes its room to the left of the buttons"
    );
    assert_eq!(
        files, clean,
        "so the pair never moves under the pointer when work arrives"
    );
}

/// A key the open surface's footer names is pressed by clicking it,
/// through the door the keystroke takes, and lights under the pointer.
#[test]
fn clicking_a_surface_footer_hint_is_pressing_its_key() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use uze_extensions::code;
    use uze_keys::Action;

    let with_code_open = || {
        let (mut model, first, _second) = two_agents_with_shells();
        model.session.as_mut().expect("session").select_tab(first);
        let mut view = code::CodeView::opening(
            PathBuf::from("/repo"),
            "/repo".to_owned(),
            code::ContentMode::Contents,
        );
        view.take_request();
        model.code = Some(view);
        model
    };
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-hint-click"));
    // Wide enough that the footer draws every key it names, `esc`
    // last among them.
    let wide = Rect::new(0, 0, 200, 40);

    let mut clicked = driven(with_code_open(), &home);
    clicked.area = wide;
    clicked.frame();
    let close = clicked.hit(|hit| *hit == WorkspaceHit::HintAction(Action::Dismiss));
    clicked.mouse(close.x, close.y, MouseEventKind::Moved);
    assert_eq!(
        clicked.attach.model.hovered,
        Some(WorkspaceHit::HintAction(Action::Dismiss)),
        "the key answers the pointer before it is clicked"
    );
    clicked.press(close.x, close.y);

    let mut pressed = driven(with_code_open(), &home);
    pressed.area = wide;
    pressed.frame();
    pressed.press_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(clicked.attach.model.code.is_none(), "the click closed it");
    assert!(pressed.attach.model.code.is_none(), "as the key does");
}
