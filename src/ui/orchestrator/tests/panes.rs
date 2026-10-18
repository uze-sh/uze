//! Terminal panes: what a pane is recognized as, closing and replacing
//! shells, frames asked for, and input forwarded to a pane.

use super::support::*;
use super::{
    AGENT_BEAT_SPAN, AGENT_BEATS, WorkspaceModel, agent_identity_for_tab, blank_pane,
    can_close_tab_from_menu, forward_paste, forward_scroll, next_agent_label, pane_relative,
    selected_pane_cwd, tab_needs_replacement_shell,
};
use crossterm::event::MouseEventKind;
use ratatui::layout::Rect;
use std::time::Instant;
use uze_core::UzeHome;
use uze_terminal::{
    ClientEvent, ClientRequest, Cursor, MouseMode, Pane, PaneDamage, PaneId, Tab, TabId,
};

#[test]
fn pane_relative_is_1_indexed_and_excludes_anything_outside_the_pane() {
    let pane = Rect::new(10, 2, 40, 20);
    assert_eq!(
        pane_relative(mouse_at(10, 2, MouseEventKind::Moved), pane),
        Some((1, 1))
    );
    assert_eq!(
        pane_relative(mouse_at(49, 21, MouseEventKind::Moved), pane),
        Some((40, 20))
    );
    // One past the pane's own bottom-right corner in either axis, and
    // anything left of/above its origin (the sidebar, tab strip) — all
    // outside.
    assert_eq!(
        pane_relative(mouse_at(50, 21, MouseEventKind::Moved), pane),
        None
    );
    assert_eq!(
        pane_relative(mouse_at(49, 22, MouseEventKind::Moved), pane),
        None
    );
    assert_eq!(
        pane_relative(mouse_at(9, 5, MouseEventKind::Moved), pane),
        None
    );
    assert_eq!(
        pane_relative(mouse_at(15, 1, MouseEventKind::Moved), pane),
        None
    );
}

/// An agent launched through UZE's own launcher is still the same agent
/// on the tab: what a pane is recognized by is the process it is
/// running, which the launcher preserves, and never the path it was
/// started from.
#[test]
fn launching_through_the_launcher_leaves_the_pane_recognizable() {
    let tab = tab_with("agent 1", "claude");
    assert_eq!(agent_identity_for_tab(&identities(), &tab), Some("claude"));
}

fn tab_with(label: &str, process: &str) -> Tab {
    let pane = Pane {
        id: PaneId(1),
        cwd: "/tmp".into(),
        columns: 80,
        rows: 24,
        process: process.to_owned(),
        through_launcher: true,
    };
    Tab {
        id: TabId(1),
        label: label.to_owned(),
        agent: None,
        env: Vec::new(),
        pane,
    }
}

#[test]
fn recognizes_a_shim_launched_process_by_its_live_alias() {
    // What `UZE_SHIM_NAME` resolves `pane.process` to for a shim-
    // launched pane (see `src/shim.rs`) — a plain shell tab where
    // someone manually typed `claude`, unrelated to the picker.
    let tab = tab_with("shell 2", "claude");
    assert_eq!(agent_identity_for_tab(&identities(), &tab), Some("claude"));
}

#[test]
fn new_agent_labels_are_numbered_independently_of_harnesses() {
    let mut session = session("/tmp");
    let model = model_of(session.clone());
    assert_eq!(next_agent_label(&model), "agent 1");

    session.add_tab(
        session.workspace.selected_space,
        "agent 1".into(),
        None,
        80,
        24,
        "/tmp".into(),
    );
    let model = model_of(session);
    assert_eq!(next_agent_label(&model), "agent 2");
}

/// A harness in a pane that did not come through the workspace's shim
/// is said once for that pane, and one that did is never said at all.
#[test]
fn a_harness_that_bypassed_the_launcher_is_said_once_per_pane() {
    let mut session = session("/tmp");
    session.workspace.spaces[0].tabs[0].pane.process = "claude".into();
    session.workspace.spaces[0].tabs[0].pane.through_launcher = false;
    let mut model = model_of(session.clone());
    model.remembered.launchers = Some(vec!["claude".to_owned()]);

    model.note_launcher_bypass();
    model.note_launcher_bypass();
    assert_eq!(model.toast_stack().len(), 1, "said once");
    assert!(
        model.remembered.toasts[0].text.contains("claude"),
        "the notice names the harness"
    );

    let mut through = model_of({
        let mut session = session;
        session.workspace.spaces[0].tabs[0].pane.through_launcher = true;
        session
    });
    through.remembered.launchers = Some(vec!["claude".to_owned()]);
    through.note_launcher_bypass();
    assert!(through.toast_stack().is_empty(), "nothing to say");
}

#[test]
fn a_lone_agent_can_close_when_it_is_replaced_by_a_shell() {
    let mut session = session("/tmp");
    session.workspace.spaces[0].tabs[0].label = "Claude Code".into();
    session.workspace.spaces[0].tabs[0].pane.process = "claude".into();
    let tab = session.workspace.spaces[0].selected_tab;
    let model = model_of(session);

    assert!(tab_needs_replacement_shell(&model, &identities(), tab));
    assert!(can_close_tab_from_menu(&model, &identities(), tab));
}

#[test]
fn a_lone_plain_shell_stays_non_closable() {
    let session = session("/tmp");
    let tab = session.workspace.spaces[0].selected_tab;
    let model = model_of(session);

    assert!(!tab_needs_replacement_shell(&model, &identities(), tab));
    assert!(!can_close_tab_from_menu(&model, &identities(), tab));
}

/// A space always keeps a shell of its own: closing the last one beside
/// its agents opens another first, while a shell with a sibling of its
/// kind needs none — and an agent whose only company is its own shells
/// does, because those go with it.
#[test]
fn closing_a_spaces_last_own_shell_is_replaced_and_no_other_close_is() {
    let session_of_one_agent = || {
        let mut solo = session("/tmp");
        solo.workspace.spaces[0].tabs[0].pane.process = "claude".into();
        solo
    };
    let mut session = session("/tmp");
    let space = session.workspace.selected_space;
    let shell = session.workspace.spaces[0].tabs[0].id;
    let pane = session.add_tab(space, "Claude Code".into(), None, 80, 24, "/tmp".into());
    session.update_pane_status(pane, "/tmp".into(), "claude".into());
    let agent = session.workspace.spaces[0].tabs[1].id;
    let model = model_of(session.clone());
    assert!(
        tab_needs_replacement_shell(&model, &identities(), shell),
        "the space's only shell of its own"
    );
    assert!(
        !tab_needs_replacement_shell(&model, &identities(), agent),
        "the agent goes, the shell stays"
    );

    session.add_tab(space, "shell 2".into(), None, 80, 24, "/tmp".into());
    let model = model_of(session.clone());
    assert!(
        !tab_needs_replacement_shell(&model, &identities(), shell),
        "another shell of its own remains"
    );

    let mut solo = session_of_one_agent();
    let lone = solo.workspace.spaces[0].tabs[0].id;
    let space = solo.workspace.selected_space;
    solo.add_tab(space, "shell 2".into(), Some(lone), 80, 24, "/tmp".into());
    let model = model_of(solo);
    assert!(
        tab_needs_replacement_shell(&model, &identities(), lone),
        "the agent's own shell goes with it, leaving the space none"
    );
}

/// The shell that stands in for a closed agent is the *space's*, so it
/// opens where the space is — never in the agent's own checkout, which
/// is the slot on its way back to the pool.
#[test]
fn the_shell_replacing_an_agent_opens_where_the_space_is() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-replacement-cwd"));
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let bootstrap = session.workspace.spaces[0].tabs[0].id;
    let pane = session.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
    session.update_pane_status(pane, "/repo/.worktrees/abc".into(), "agent".into());
    let agent = session.workspace.spaces[0].tabs[1].id;
    session.add_tab(
        space,
        "shell 2".into(),
        Some(agent),
        80,
        24,
        "/repo/.worktrees/abc".into(),
    );
    // The space keeps no shell of its own, so closing the agent has to
    // open one — the case this is about.
    session.remove_tab(bootstrap).expect("the bootstrap goes");
    session.workspace.spaces[0].selected_tab = agent;
    let mut driven = driven(model_of(session), &home);
    let close = uze_keys::active()
        .chord_for(uze_keys::Action::CloseTab, &[uze_keys::Scope::Workspace])
        .expect("closing a tab is reachable from the keyboard");

    driven.press_key(key_event(close));

    let sent = driven.sent();
    let opened = sent.iter().find_map(|request| match request {
        ClientRequest::CreateTab {
            agent: None, cwd, ..
        } => Some(cwd.clone()),
        _ => None,
    });
    assert_eq!(
        opened,
        Some(Some("/repo".into())),
        "the space's root, not the checkout that is going: {sent:?}"
    );
}

/// Closed from the keyboard, the last shell of a space's own is
/// replaced before it goes, so the header still has somewhere to land.
#[test]
fn the_close_chord_on_a_spaces_last_shell_opens_another_first() {
    let home = UzeHome::at(uze_testkit::temp::scratch("orchestrator-last-shell"));
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let shell = session.workspace.spaces[0].tabs[0].id;
    let pane = session.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
    session.update_pane_status(pane, "/repo".into(), "agent".into());
    session.workspace.spaces[0].selected_tab = shell;
    let mut driven = driven(model_of(session), &home);
    let close = uze_keys::active()
        .chord_for(uze_keys::Action::CloseTab, &[uze_keys::Scope::Workspace])
        .expect("closing a tab is reachable from the keyboard");

    driven.press_key(key_event(close));

    let sent = driven.sent();
    let opened = sent.iter().position(|request| {
        matches!(
            request,
            ClientRequest::CreateTab {
                agent: None,
                command: None,
                ..
            }
        )
    });
    let closed = sent
        .iter()
        .position(|request| matches!(request, ClientRequest::CloseTab { tab } if *tab == shell));
    assert!(
        matches!((opened, closed), (Some(opened), Some(closed)) if opened < closed),
        "a shell of its own opens before the last one closes: {sent:?}"
    );
}

#[test]
fn a_plain_shell_matches_neither_signal() {
    let tab = tab_with("shell", "zsh");
    assert_eq!(agent_identity_for_tab(&identities(), &tab), None);
}

#[test]
fn new_tabs_use_the_selected_panes_live_directory() {
    let mut session = session("/tmp/root");
    assert!(session.update_pane_status(PaneId(1), "/tmp/project/src".into(), "zsh".into()));
    let model = model_of(session);

    assert_eq!(selected_pane_cwd(&model), Some("/tmp/project/src".into()));
}

#[test]
fn an_unrecognized_process_name_does_not_match() {
    // The exact motivating case: Claude Code's live comm resolves to
    // its own version string, not `claude` — recognizable only via the
    // shim-identity signal (`claude` from `UZE_SHIM_NAME`), not this
    // raw process read alone.
    let tab = tab_with("shell", "2.1.251");
    assert_eq!(agent_identity_for_tab(&identities(), &tab), None);
}

#[test]
fn only_the_focused_panes_paint_asks_for_a_frame() {
    let mut model = model_of(session("/tmp"));
    let focused = model.focused_pane();
    let background = PaneId(focused.0 + 100);

    model.dirty = false;
    model.apply(painted(background), &[]);
    assert!(!model.dirty, "a pane nobody sees asked for a frame");
    assert!(model.panes.contains_key(&background));

    model.apply(painted(focused), &[]);
    assert!(model.dirty);
}

#[test]
fn a_background_agent_starting_to_work_still_asks_for_a_frame() {
    let (mut model, first, second) = two_agents_with_shells();
    let identities = identities_fixture();
    let session = model.session.as_mut().expect("session");
    session.select_tab(second);
    let background = session
        .workspace
        .spaces
        .iter()
        .flat_map(|space| &space.tabs)
        .find(|tab| tab.id == first)
        .expect("first agent")
        .pane
        .id;
    assert_ne!(background, model.focused_pane());

    let start = Instant::now();
    let beat = AGENT_BEAT_SPAN / (AGENT_BEATS as u32 - 1);
    let mut asked = Vec::new();
    for n in 0..=AGENT_BEATS as u32 {
        let ClientEvent::Damage(damage) = painted(background) else {
            unreachable!()
        };
        asked.push(model.absorb_damage(damage, &identities, start + beat * n));
    }
    assert!(model.agent_is_working(background));
    assert_eq!(
        asked.iter().filter(|seen| **seen).count(),
        1,
        "only the paint that started the turn changes the sidebar: {asked:?}"
    );
}

#[test]
fn damage_updates_the_tracked_panes_mouse_and_bracketed_paste_mode() {
    // Regression: `mouse`/`bracketed_paste` ride along on every
    // `PaneDamage`, not just the initial full `Snapshot` — a pane's own
    // program typically turns these on shortly after it starts, which
    // is after the client's one-time first snapshot already fired. A
    // client that only reads these off `Snapshot` would forward mouse
    // clicks and pastes into the pane forever as if it never asked.
    let mut model = WorkspaceModel {
        panes: [(PaneId(1), blank_pane(PaneId(1), 80, 24))].into(),
        ..WorkspaceModel::default()
    };
    assert!(!model.panes[&PaneId(1)].mouse.reports_clicks);
    assert!(!model.panes[&PaneId(1)].bracketed_paste);

    model.apply(
        ClientEvent::Damage(PaneDamage {
            pane: PaneId(1),
            columns: 80,
            rows: 24,
            cursor: Cursor { column: 0, row: 0 },
            alternate_screen: false,
            mouse: MouseMode {
                reports_clicks: true,
                reports_drag: false,
                sgr: true,
            },
            bracketed_paste: true,
            changed: Vec::new(),
        }),
        &[],
    );

    assert!(model.panes[&PaneId(1)].mouse.reports_clicks);
    assert!(model.panes[&PaneId(1)].bracketed_paste);
}

#[test]
fn forward_paste_frames_the_bytes_only_when_the_pane_asked_for_bracketed_paste() {
    let mut plain = blank_pane(PaneId(1), 80, 24);
    plain.bracketed_paste = false;
    let plain_model = WorkspaceModel {
        session: Some(session("/tmp")),
        panes: [(PaneId(1), plain)].into(),
        ..WorkspaceModel::default()
    };
    let mut stream = Vec::new();
    forward_paste(&mut stream, &plain_model, "hello");
    assert_eq!(decode_input_bytes(&stream), b"hello".to_vec());

    let mut bracketed = blank_pane(PaneId(1), 80, 24);
    bracketed.bracketed_paste = true;
    let bracketed_model = WorkspaceModel {
        session: Some(session("/tmp")),
        panes: [(PaneId(1), bracketed)].into(),
        ..WorkspaceModel::default()
    };
    let mut stream = Vec::new();
    forward_paste(&mut stream, &bracketed_model, "hello");
    assert_eq!(
        decode_input_bytes(&stream),
        b"\x1b[200~hello\x1b[201~".to_vec()
    );
}

#[test]
fn scroll_uses_arrow_keys_for_an_alternate_screen_without_mouse_reporting() {
    let mut pane = blank_pane(PaneId(1), 80, 24);
    pane.alternate_screen = true;
    let model = WorkspaceModel {
        session: Some(session("/tmp")),
        panes: [(PaneId(1), pane)].into(),
        ..WorkspaceModel::default()
    };
    let mut stream = Vec::new();
    forward_scroll(
        &mut stream,
        &model,
        Rect::new(0, 0, 80, 24),
        mouse_at(4, 5, MouseEventKind::ScrollUp),
    );
    assert_eq!(decode_input_bytes(&stream), b"\x1b[A".to_vec());
}

#[test]
fn scroll_uses_terminal_scrollback_for_a_normal_screen_without_mouse_reporting() {
    let model = WorkspaceModel {
        session: Some(session("/tmp")),
        panes: [(PaneId(1), blank_pane(PaneId(1), 80, 24))].into(),
        ..WorkspaceModel::default()
    };
    let mut stream = Vec::new();
    forward_scroll(
        &mut stream,
        &model,
        Rect::new(0, 0, 80, 24),
        mouse_at(4, 5, MouseEventKind::ScrollDown),
    );
    assert_eq!(
        decode_request(&stream),
        ClientRequest::Scroll {
            pane: PaneId(1),
            lines: -3,
        }
    );
}

/// Mirrors `uze_terminal::runtime`'s length-prefixed bincode framing
/// (a 4-byte little-endian length, then the payload) — `send_request`
/// writes real wire frames, not bare JSON, so a test reading `stream`
/// back has to strip the same prefix.
fn decode_input_bytes(stream: &[u8]) -> Vec<u8> {
    match decode_request(stream) {
        ClientRequest::Input { bytes, .. } => bytes,
        other => panic!("expected ClientRequest::Input, got {other:?}"),
    }
}

fn decode_request(stream: &[u8]) -> ClientRequest {
    let (len_bytes, payload) = stream.split_at(4);
    let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
    assert_eq!(payload.len(), len, "one ClientRequest frame");
    bincode::deserialize(payload).expect("one ClientRequest frame")
}
