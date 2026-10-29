//! The surface buttons and chords: opening, switching and closing the
//! extension surfaces, and an extension switched off.

use super::WorkspaceHit;
use super::support::*;
use super::{
    ExtensionHit, GitBadge, GitResolution, WorkStateView, WorkspaceModel, open_architect,
    open_code, open_spec,
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
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);

    let layout = full_frame(&mut model);
    let extension_hits: Vec<Rect> = model
        .hits
        .iter()
        .filter(|(_, hit)| matches!(hit, WorkspaceHit::Extension(ExtensionHit { .. })))
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
        (|model: &mut WorkspaceModel| open_code(model, uze_extensions::code::FILES))
            as fn(&mut WorkspaceModel),
        |model| open_spec(model),
        |model| open_architect(model),
    ] {
        let (mut model, first, _second) = two_agents_with_shells();
        select_tab(&mut model, first);
        open(&mut model);
        let layout = full_frame(&mut model);
        let over: Vec<_> = model
            .hits
            .iter()
            .filter(|(rect, hit)| model.is_open_surfaces(*hit) && layout.sidebar.intersects(*rect))
            .collect();
        assert!(over.is_empty(), "over the sidebar: {over:?}");
    }
}

/// The strip lights one thing, the one in the pane: the selected tab,
/// or — while a surface stands over it — that surface's button.
#[test]
fn the_strip_lights_one_thing_the_one_in_the_pane() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    let raised = crate::ui::theme::color(Token::SurfaceRaised);
    let lit = crate::ui::theme::color(Token::TextBright);
    let ink = crate::ui::theme::color(Token::SurfaceBackground);
    let resting = crate::ui::theme::color(Token::TextSecondary);

    let tab = hit_rect(&model, WorkspaceHit::SelectTab(first));
    let button = hit_rect(&model, WorkspaceHit::OpenFiles);
    assert_eq!(chip_colors(&model, tab).1, raised, "the tab, at first");
    assert_eq!(chip_colors(&model, button), (resting, raised));

    open_code(&mut model, uze_extensions::code::FILES);
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
    select_tab(&mut model, first);
    let mut driven = driven(model, &home);
    let showing = |driven: &Driven<'_>| {
        let model = &driven.attach.model;
        match (
            code_view(model).map(CodeView::showing),
            model.showing(uze_extensions::architect::CATALOG.id),
            model.showing(uze_extensions::spec::CATALOG.id),
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
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::CHANGES);
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
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id),
        "closed in one click"
    );
}

#[test]
fn the_spec_button_opens_and_closes_the_spec_surface() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-spec-button"));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
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
    assert!(
        driven
            .attach
            .model
            .showing(uze_extensions::spec::CATALOG.id),
        "opened by its button"
    );
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id)
    );
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::architect::CATALOG.id)
    );

    driven.frame();
    let button = hit_rect(&driven.attach.model, WorkspaceHit::OpenSpec);
    driven.press(button.x, button.y);
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::spec::CATALOG.id),
        "closed in one click"
    );
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
    select_tab(&mut model, first);
    open_architect(&mut model);
    model.follow_extension_switch(switched_off(&[uze_extensions::architect::CATALOG.id]));
    assert!(
        !model.showing(uze_extensions::architect::CATALOG.id),
        "the open surface closed"
    );

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
        !driven
            .attach
            .model
            .showing(uze_extensions::architect::CATALOG.id),
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
    select_tab(&mut model, first);
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
    let home = UzeHome::at(uze_testkit::temp::scratch(
        "orchestrator-switched-off-reads",
    ));
    let (git, _git_answers) = std::sync::mpsc::channel();
    let (engines, _engine_answers) = std::sync::mpsc::channel();

    model.schedule_git_read(&git);
    assert_eq!(
        model.schedule_engine_jobs(&home, &engines),
        0,
        "the spec counts nothing"
    );

    assert!(model.remembered.git_pending.is_none());
    assert!(!model.absorb_git_read(GitResolution {
        cwd: PathBuf::from("/repo"),
        summary: None,
        took: Duration::ZERO,
    }));
    assert!(model.remembered.git_badge.is_none());
}

/// Switching back on puts it all back: the button returns and a key
/// opens the surface again.
#[test]
fn an_extension_switched_back_on_opens_again() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.follow_extension_switch(switched_off(&[uze_extensions::spec::CATALOG.id]));
    open_spec(&mut model);
    assert!(!model.showing(uze_extensions::spec::CATALOG.id));

    model.follow_extension_switch(std::collections::BTreeSet::new());
    open_spec(&mut model);
    assert!(model.showing(uze_extensions::spec::CATALOG.id));
}

/// The surface seals the keyboard: a letter it has no use for is not
/// typed into the program underneath it.
#[test]
fn a_key_typed_over_the_spec_surface_does_not_reach_the_pane() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-spec-seals"));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_spec(&mut model);
    let mut driven = driven(model, &home);
    driven.frame();
    driven.sent();

    driven.press_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

    assert!(
        driven
            .attach
            .model
            .showing(uze_extensions::spec::CATALOG.id)
    );
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
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    model.close_surface();
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
    select_tab(&mut model, first);
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
    assert!(
        driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id),
        "the first opens"
    );
    driven.frame();
    driven.press(button.x, button.y);
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id),
        "the second closes"
    );
}

/// Choosing the tab already in front is choosing to see it: that is how
/// the pane is had back from a surface.
#[test]
fn choosing_a_tab_puts_the_open_surface_away() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-surface-tab"));
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
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
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id)
    );
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
    select_tab(&mut model, first);
    open_architect(&mut model);

    let mut session = model.session.clone().expect("session");
    session.rename_tab(first, "renamed".to_owned());
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    assert!(
        model.showing(uze_extensions::architect::CATALOG.id),
        "the same tab in front keeps it"
    );

    let mut session = model.session.clone().expect("session");
    session.select_tab(second);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    assert!(!model.showing(uze_extensions::architect::CATALOG.id));
}

/// Moving between agents never closes what one of them was showing: the
/// tab coming back to the front has its surface standing again, and the
/// tab that never had one stays bare.
#[test]
fn a_tab_coming_back_to_the_front_shows_the_surface_it_was_left_with() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    let bring_to_front = |model: &mut WorkspaceModel, tab| {
        let mut session = model.session.clone().expect("session");
        session.select_tab(tab);
        model.apply(
            ClientEvent::SessionUpdated { session },
            &identities_fixture(),
        );
    };

    bring_to_front(&mut model, second);
    assert!(!model.showing(uze_extensions::architect::CATALOG.id));

    bring_to_front(&mut model, first);
    assert!(
        model.showing(uze_extensions::architect::CATALOG.id),
        "the first agent's surface stands again"
    );

    model.close_surface();
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, first);
    assert!(
        !model.showing(uze_extensions::architect::CATALOG.id),
        "a surface closed by hand stays closed"
    );
}

/// A tab coming back while the space's overview stands keeps the overview
/// in front, and its surface is not lost for that: it stands again the next
/// time the tab comes back.
#[test]
fn a_surface_kept_back_by_the_overview_is_not_forgotten() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    let bring_to_front = |model: &mut WorkspaceModel, tab| {
        let mut session = model.session.clone().expect("session");
        session.select_tab(tab);
        model.apply(
            ClientEvent::SessionUpdated { session },
            &identities_fixture(),
        );
    };

    bring_to_front(&mut model, second);
    let space = model.session.as_ref().expect("session").selected_space().id;
    model.open_space_overview(space, second, PathBuf::from("/repo"), true);
    bring_to_front(&mut model, first);
    assert!(!model.showing(uze_extensions::architect::CATALOG.id));

    model.put_away_space_overview();
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, first);
    assert!(
        model.showing(uze_extensions::architect::CATALOG.id),
        "the first agent's surface stands again"
    );
}

/// The same, the way it is done: a click on another agent leaves the
/// surface to the agent it was opened on, rather than closing it before
/// the move is even confirmed.
#[test]
fn clicking_away_and_back_keeps_the_surface_of_the_agent_left() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-surface-left-open"));
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    let mut driven = driven(model, &home);
    let click_and_confirm = |driven: &mut Driven<'_>, tab| {
        driven.frame();
        let row = driven
            .attach
            .model
            .hits
            .iter()
            .find(|(_, hit)| matches!(hit, WorkspaceHit::SelectTab(hit) if *hit == tab))
            .map(|(rect, _)| *rect)
            .expect("the agent has a row");
        driven.press(row.x, row.y);
        let selected = driven
            .sent()
            .into_iter()
            .find_map(|request| match request {
                ClientRequest::SelectTab { tab } => Some(tab),
                _ => None,
            })
            .expect("the click asked for a tab");
        let mut session = driven.attach.model.session.clone().expect("session");
        session.select_tab(selected);
        driven.attach.model.apply(
            ClientEvent::SessionUpdated { session },
            &identities_fixture(),
        );
    };

    click_and_confirm(&mut driven, second);
    assert!(
        !driven
            .attach
            .model
            .showing(uze_extensions::architect::CATALOG.id)
    );

    click_and_confirm(&mut driven, first);
    assert!(
        driven
            .attach
            .model
            .showing(uze_extensions::architect::CATALOG.id),
        "the first agent's surface stands again"
    );
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
    select_tab(&mut model, first);

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
        select_tab(&mut model, first);
        let mut view = code::CodeView::opening(
            PathBuf::from("/repo"),
            "/repo".to_owned(),
            code::ContentMode::Contents,
        );
        view.take_request();
        show_code(&mut model, view);
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

    assert!(
        !clicked
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id),
        "the click closed it"
    );
    assert!(
        !pressed
            .attach
            .model
            .showing(uze_extensions::code::CATALOG.id),
        "as the key does"
    );
}

/// A space's header is the same gesture one level up: another space's
/// leaves the surface to the agent it stands for, and the header of the
/// space already in front takes the pane back.
#[test]
fn clicking_another_spaces_header_keeps_the_surface_and_its_own_takes_it_away() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-surface-space"));
    let mut model = three_spaces();
    let (one, three) = {
        let spaces = &model.session.as_ref().expect("session").workspace.spaces;
        (spaces[0].id, spaces[2].id)
    };
    open_architect(&mut model);
    let mut driven = driven(model, &home);
    let click_and_confirm = |driven: &mut Driven<'_>, space| {
        // Each click is its own gesture, never the second half of a
        // double click on the same header.
        driven.attach.model.last_click = None;
        driven.frame();
        let header = space_header(&driven.attach.model.hits, space);
        driven.press(header.x, header.y);
        let mut session = driven.attach.model.session.clone().expect("session");
        for request in driven.sent() {
            match request {
                ClientRequest::SelectSpace { space } => session.workspace.selected_space = space,
                ClientRequest::SelectTab { tab } => {
                    session.select_tab(tab);
                }
                _ => {}
            }
        }
        driven.attach.model.apply(
            ClientEvent::SessionUpdated { session },
            &identities_fixture(),
        );
    };
    let showing = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .showing(uze_extensions::architect::CATALOG.id)
    };

    click_and_confirm(&mut driven, one);
    assert!(!showing(&driven));
    click_and_confirm(&mut driven, three);
    assert!(showing(&driven), "the surface stands again");
    click_and_confirm(&mut driven, three);
    assert!(
        !showing(&driven),
        "its own space's header takes the pane back"
    );
}

/// What a closed tab was showing goes with it.
#[test]
fn a_closed_tab_leaves_no_surface_behind() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);

    let mut session = model.session.clone().expect("session");
    session.select_tab(second);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    let mut session = model.session.clone().expect("session");
    session.remove_tab(first);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
    assert!(model.remembered.declared.standing.is_empty());
}

/// The session's word that `tab` is in front, as the server sends it.
fn bring_to_front(model: &mut WorkspaceModel, tab: uze_terminal::TabId) {
    let mut session = model.session.clone().expect("session");
    session.select_tab(tab);
    model.apply(
        ClientEvent::SessionUpdated { session },
        &identities_fixture(),
    );
}

fn showing_architect(model: &WorkspaceModel) -> bool {
    model.showing(uze_extensions::architect::CATALOG.id)
}

/// A return is not a door: code stands again on the half it was left on,
/// not on the changes every fresh opening starts from.
#[test]
fn returning_to_an_agent_shows_code_on_the_half_it_was_left_on() {
    use uze_extensions::code::ContentMode;
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    assert_eq!(
        code_view(&model).expect("open").showing(),
        ContentMode::Contents
    );

    bring_to_front(&mut model, second);
    assert!(code_view(&model).is_none());
    bring_to_front(&mut model, first);
    assert_eq!(
        code_view(&model).expect("it stands again").showing(),
        ContentMode::Contents,
        "on files, where it was left"
    );
}

/// Two agents, two surfaces: each tab gets back its own, however many
/// times the operator goes between them.
#[test]
fn each_agent_keeps_its_own_surface() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    bring_to_front(&mut model, second);
    open_architect(&mut model);

    for _ in 0..2 {
        bring_to_front(&mut model, first);
        assert!(model.showing(uze_extensions::code::CATALOG.id));
        bring_to_front(&mut model, second);
        assert!(showing_architect(&model));
    }
}

/// The overview opened over a surface puts it away for its tab, and the
/// overview put away gives it back.
#[test]
fn the_overview_opened_over_a_surface_gives_it_back_when_put_away() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.settle_pane();
    open_architect(&mut model);
    let space = model.session.as_ref().expect("session").selected_space().id;

    model.open_space_overview(space, first, PathBuf::from("/repo"), true);
    assert!(!showing_architect(&model));
    model.dismiss_space_overview();
    assert!(showing_architect(&model));
}

/// A surface opened on a tab is what the tab keeps from then on, even
/// one opened while an older one waited under the overview.
#[test]
fn a_surface_opened_where_one_was_kept_replaces_it() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    bring_to_front(&mut model, second);
    let space = model.session.as_ref().expect("session").selected_space().id;
    model.open_space_overview(space, second, PathBuf::from("/repo"), true);
    bring_to_front(&mut model, first);
    assert!(!showing_architect(&model), "the overview holds the pane");

    open_spec(&mut model);
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, first);
    assert!(model.showing(uze_extensions::spec::CATALOG.id));
    assert!(!showing_architect(&model));
}

/// A surface closed by hand stays closed: leaving and coming back does
/// not bring back what was kept before it.
#[test]
fn a_surface_closed_by_hand_is_not_brought_back() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    bring_to_front(&mut model, second);
    let space = model.session.as_ref().expect("session").selected_space().id;
    model.open_space_overview(space, second, PathBuf::from("/repo"), true);
    bring_to_front(&mut model, first);

    open_spec(&mut model);
    model.close_surface();
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, first);
    assert!(!showing_architect(&model));
    assert!(!model.showing(uze_extensions::spec::CATALOG.id));
}

/// A client attaching again finds the tab in front as it was left.
#[test]
fn reattaching_shows_the_surface_of_the_tab_in_front() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    let session = model.session.clone().expect("session");

    model.suspend_surface();
    let mut again = WorkspaceModel {
        remembered: std::mem::take(&mut model.remembered),
        ..WorkspaceModel::default()
    };
    again.apply(ClientEvent::Snapshot { session }, &identities_fixture());
    assert!(showing_architect(&again));
}

/// A workspace set aside starts its tabs from one again, and the server
/// paints the new one before it says so: a tab under a number the old one
/// used, working somewhere else, is not given the old tab's surface.
#[test]
fn a_tab_number_reused_by_another_runtime_does_not_get_the_old_surface() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    bring_to_front(&mut model, second);

    let mut fresh = model.session.clone().expect("session");
    for tab in fresh
        .workspace
        .spaces
        .iter_mut()
        .flat_map(|space| space.tabs.iter_mut())
    {
        if tab.id == first {
            tab.pane.cwd = "/elsewhere".into();
        }
    }
    fresh.select_tab(first);
    model.apply(
        ClientEvent::Snapshot { session: fresh },
        &identities_fixture(),
    );
    model.apply(
        ClientEvent::WorkspaceSetAside {
            kept_at: PathBuf::from("/kept"),
            reason: "unreadable".to_owned(),
        },
        &identities_fixture(),
    );
    assert!(!showing_architect(&model));
}

/// Dismissing the overview over a tab gives the pane back at once, not a
/// frame later.
#[test]
fn dismissing_the_overview_gives_the_surface_back_at_once() {
    let (mut model, first, _second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.settle_pane();
    open_architect(&mut model);
    let space = model.session.as_ref().expect("session").selected_space().id;
    model.open_space_overview(space, first, PathBuf::from("/repo"), true);
    model.dismiss_space_overview();
    assert!(showing_architect(&model));
}

/// Leaving a tab by a way that puts the overview away first does not give
/// that tab its surface back for the moment before the move is confirmed.
#[test]
fn leaving_from_under_the_overview_does_not_flash_the_surface() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    model.settle_pane();
    open_architect(&mut model);
    let space = model.session.as_ref().expect("session").selected_space().id;
    model.open_space_overview(space, first, PathBuf::from("/repo"), true);

    model.put_away_space_overview();
    assert!(!showing_architect(&model), "nothing stands before the move");
    bring_to_front(&mut model, second);
    assert!(!showing_architect(&model));
    bring_to_front(&mut model, first);
    assert!(showing_architect(&model), "and it is there when the tab is");
}

/// An extension switched off takes with it what tabs kept of it, so
/// switching it back on does not bring an old surface back.
#[test]
fn an_extension_switched_off_forgets_what_tabs_kept_of_it() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_architect(&mut model);
    bring_to_front(&mut model, second);
    model.follow_extension_switch(switched_off(&[uze_extensions::architect::CATALOG.id]));
    model.follow_extension_switch(std::collections::BTreeSet::new());
    bring_to_front(&mut model, first);
    assert!(!showing_architect(&model));
}

/// The list's scroll is the host's, and travels with the tab too.
#[test]
fn a_navigator_scroll_comes_back_with_its_tab() {
    let (mut model, first, second) = two_agents_with_shells();
    select_tab(&mut model, first);
    open_code(&mut model, uze_extensions::code::FILES);
    model.navigator_scroll.first = 7;
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, first);
    assert_eq!(model.navigator_scroll.first, 7);
}

/// The shell opened beside `agent`, the one the strip draws after it.
fn shell_beside(model: &WorkspaceModel, agent: uze_terminal::TabId) -> uze_terminal::TabId {
    model
        .tabs()
        .find(|tab| tab.agent == Some(agent))
        .map(|tab| tab.id)
        .expect("the agent has a shell beside it")
}

/// A surface opened from the shell beside an agent is that agent's: the
/// sidebar marks the agent while the shell is in front, so going to the
/// agent's own tab is choosing a terminal of the agent already in front,
/// and the surface does not wait on the shell to stand over it again.
#[test]
fn a_surface_opened_on_an_agents_shell_does_not_come_back_over_the_shell() {
    let (mut model, first, _second) = two_agents_with_shells();
    let shell = shell_beside(&model, first);
    select_tab(&mut model, shell);
    model.settle_pane();
    open_architect(&mut model);
    assert!(showing_architect(&model));

    bring_to_front(&mut model, first);
    assert!(!showing_architect(&model), "the agent's own pane is shown");
    bring_to_front(&mut model, shell);
    assert!(!showing_architect(&model), "and the shell's, when chosen");
}

/// One agent, one surface: whichever of its tabs it was opened on, an
/// agent never keeps a second one waiting on another of its tabs.
#[test]
fn an_agent_and_its_shell_never_keep_two_surfaces() {
    let (mut model, first, second) = two_agents_with_shells();
    let shell = shell_beside(&model, first);
    select_tab(&mut model, first);
    model.settle_pane();
    open_code(&mut model, uze_extensions::code::FILES);
    bring_to_front(&mut model, shell);
    open_architect(&mut model);

    bring_to_front(&mut model, first);
    assert!(model.open_slot.is_none(), "no surface left over the agent");
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, shell);
    assert!(model.open_slot.is_none(), "nor over its shell");
}

/// What an agent was left showing stands again whichever of its tabs the
/// way back lands on: from the sidebar, that is the shell it was left in.
#[test]
fn a_surface_opened_on_an_agents_shell_comes_back_with_the_agent() {
    let (mut model, first, second) = two_agents_with_shells();
    let shell = shell_beside(&model, first);
    select_tab(&mut model, shell);
    model.settle_pane();
    open_architect(&mut model);

    bring_to_front(&mut model, second);
    assert!(!showing_architect(&model));
    bring_to_front(&mut model, first);
    assert!(showing_architect(&model), "on the agent's own tab");
    bring_to_front(&mut model, second);
    bring_to_front(&mut model, shell);
    assert!(showing_architect(&model), "and on the shell beside it");
}
