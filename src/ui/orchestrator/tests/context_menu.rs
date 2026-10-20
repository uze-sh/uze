//! The context menu a row opens, and what it offers.

use super::WorkspaceHit;
use super::support::*;
use super::{AgentView, WorkStateView, WorkspaceModel};
use crossterm::event::{MouseButton, MouseEventKind};
use std::path::{Path, PathBuf};
use uze_core::UzeHome;
use uze_terminal::{ClientRequest, TabId};

/// The context menu answers the keyboard like the agent picker does:
/// the selection moves within the items and stops at their ends,
/// Enter acts on the highlighted one and closes the menu.
#[test]
fn the_context_menu_is_driven_by_the_keyboard() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-menu-keys"));
    let mut driven = driven(model_of(session("/repo")), &home);
    driven.frame();
    let row = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| matches!(hit, WorkspaceHit::SelectSpace(_)).then_some(*rect))
        .expect("the space row is a target");
    driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
    let scopes = [
        uze_keys::Scope::Global,
        uze_keys::Scope::Workspace,
        uze_keys::Scope::ContextMenu,
    ];
    let press = |driven: &mut Driven<'_>, action| {
        let chord = uze_keys::active()
            .chord_for(action, &scopes)
            .expect("the menu is reachable from the keyboard");
        driven.press_key(key_event(chord));
    };
    let selected = |driven: &Driven<'_>| {
        driven
            .attach
            .model
            .context_menu
            .as_ref()
            .map(|menu| menu.selected)
    };

    press(&mut driven, uze_keys::Action::SelectPrevious);
    assert_eq!(selected(&driven), Some(0), "it stops at the first item");
    press(&mut driven, uze_keys::Action::SelectNext);
    press(&mut driven, uze_keys::Action::SelectNext);
    assert_eq!(selected(&driven), Some(1), "and at the last");

    let _ = driven.sent();
    press(&mut driven, uze_keys::Action::Activate);
    assert!(
        driven.attach.model.context_menu.is_none(),
        "acting closes it"
    );
    assert!(
        driven
            .sent()
            .iter()
            .any(|request| matches!(request, ClientRequest::CloseSpace { .. })),
        "Enter acted on the highlighted item, delete"
    );
}

/// `Isolate` is offered on an agent's row only where it can be
/// honoured: the directory has to be a repository with a commit to
/// branch from, and the agent must not already have a checkout. The
/// carry row stands beside it whenever it does — whether the tree is
/// dirty is a Git question, and a row gated on the last evaluation's
/// answer would be missing exactly when the operator has just edited
/// something.
#[test]
fn isolate_is_offered_only_where_a_slot_could_be_cut() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-menu-isolate"));
    // The agent is selected first: the strip shows one agent's context
    // at a time, so the chip a menu opens on is the selected agent's.
    let menu_on = |mut model: WorkspaceModel, wanted: TabId| {
        select_tab(&mut model, wanted);
        let mut driven = driven(model, &home);
        driven.frame();
        let row = driven
            .attach
            .model
            .hits
            .iter()
            .find_map(|(rect, hit)| match hit {
                WorkspaceHit::SelectTab(tab) if *tab == wanted => Some(*rect),
                _ => None,
            })
            .expect("the agent's row is a target");
        driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
        driven
            .attach
            .model
            .context_menu
            .as_ref()
            .expect("a menu opened on the agent's row")
            .items
            .clone()
    };

    // Nothing evaluated yet: the directory has not been answered for,
    // so nothing claims a slot can be cut there.
    let mut unanswered = agents_in_the_root_session();
    unanswered.remembered.branches.clear();
    assert!(
        !menu_on(unanswered, TabId(2)).contains(&uze_keys::Action::IsolateAgent),
        "outside a repository the row is absent"
    );

    // A branch was read at the root: a directory with one is a
    // repository with a commit. One row, because the tree holds
    // nothing to decide about.
    assert_eq!(
        menu_on(agents_in_the_root_session(), TabId(2)),
        vec![
            uze_keys::Action::RenameSelection,
            uze_keys::Action::IsolateAgent,
            uze_keys::Action::CloseTab,
        ],
        "isolating takes the work with it, and says so in one row"
    );

    // The tree the agent stands in has uncommitted work, so leaving it
    // behind is an answer worth offering — and only now.
    let mut dirty = agents_in_the_root_session();
    dirty.remembered.tasks.insert(
        PathBuf::from("/repo"),
        vec![AgentView {
            id: "a1".into(),
            isolated: false,
            parent: None,
            checkout: Some(PathBuf::from("/repo")),
            state: WorkStateView::Uncommitted,
            ..task_in("/repo", "agent 1", WorkStateView::Uncommitted, 0)
        }],
    );
    assert_eq!(
        menu_on(dirty, TabId(2)),
        vec![
            uze_keys::Action::RenameSelection,
            uze_keys::Action::IsolateAgent,
            uze_keys::Action::IsolateAgentAtCommit,
            uze_keys::Action::CloseTab,
        ],
        "the exception stands beside it where there is something to leave"
    );

    // An agent already in a checkout of its own has nothing to be
    // given.
    assert!(
        !menu_on(a_space_with_both_groups(), TabId(3)).contains(&uze_keys::Action::IsolateAgent),
        "an isolated agent is not offered it again"
    );
}

/// A lone space can be deleted like any other: its menu offers it, and
/// deleting it names a space at home to take its place, so the
/// workspace is never left with nowhere to land.
#[test]
fn a_lone_space_offers_delete_and_is_replaced_by_home() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-close-last-space"));
    let model = model_of(session("/repo"));
    let mut driven = driven(model, &home);
    driven.frame();
    let (row, space) = driven
        .attach
        .model
        .hits
        .iter()
        .find_map(|(rect, hit)| match hit {
            WorkspaceHit::SelectSpace(space) => Some((*rect, *space)),
            _ => None,
        })
        .expect("the space row is a target");

    driven.mouse(row.x + 1, row.y, MouseEventKind::Down(MouseButton::Right));
    let menu = driven
        .attach
        .model
        .context_menu
        .as_ref()
        .expect("a menu opened on the space row");
    assert_eq!(
        menu.items,
        vec![
            uze_keys::Action::RenameSelection,
            uze_keys::Action::CloseTab
        ]
    );

    let mut requests = Vec::new();
    crate::ui::orchestrator::dispatch_menu_action(
        &mut requests,
        &mut driven.attach.model,
        &identities_fixture(),
        crate::ui::orchestrator::MenuTarget::Space(space),
        uze_keys::Action::CloseTab,
    );
    let request: ClientRequest =
        bincode::deserialize(&requests[4..]).expect("one request was written");
    let ClientRequest::CloseSpace {
        space: closed,
        replacement,
        ..
    } = request
    else {
        panic!("expected CloseSpace, got {request:?}");
    };
    assert_eq!(closed, space);
    assert_eq!(
        Some(replacement.as_path()),
        uze_platform::home::user_home().as_deref(),
        "the workspace lands at home"
    );
}

/// Starting `uze` somewhere is a request for a space there, except
/// where nobody chose the directory. A shell opens at home, so a
/// launch from home is "start the app", not "add my home directory to
/// the workspace" — and a home space closed on purpose used to be
/// remade by the next launch, which reads as the close not working.
#[test]
fn starting_at_home_lands_in_the_workspace_rather_than_adding_to_it() {
    use uze_terminal::{Seating, SpaceSeat};

    let home = uze_platform::home::user_home().expect("a home directory");
    let seat = |project: &Path| SpaceSeat {
        project: project.to_path_buf(),
    };

    assert_eq!(
        crate::ui::orchestrator::seating_at(seat(&home)),
        Seating::WhereItLeftOff,
        "the home directory is where a shell starts, not a space to open"
    );
    let project = home.join("some-project");
    assert_eq!(
        crate::ui::orchestrator::seating_at(seat(&project)),
        Seating::Open(seat(&project)),
        "a directory somebody chose is a request for a space in it"
    );
}
