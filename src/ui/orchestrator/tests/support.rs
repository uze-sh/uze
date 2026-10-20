//! Fixtures every domain of the workspace client's tests shares: sessions,
//! models, frames drawn at a size, and an attach driven the way the loop
//! drives it.

use super::WorkspaceHit;
use super::{
    AgentIdentity, AgentView, Attach, ExtensionHit, Flow, GitBadge, PlacementResolution, Viewport,
    WorkStateView, WorkspaceModel, agent_activity_frame,
    render::{
        self, FrameMetrics, LeadingSlot, WorkspaceLayout, compute_layout, render_sidebar,
        render_tab_strip,
    },
};
use crate::ui::theme::{self, Token};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::{Path, PathBuf};
use std::time::Instant;
use uze_application::CompletionBehavior;
use uze_core::UzeHome;
use uze_extensions::view::ViewHit;
use uze_terminal::{
    CellAttributes, ClientEvent, ClientRequest, Cursor, MouseMode, Pane, PaneDamage, PaneId,
    RenderCell, Session, SpaceId, Tab, TabId, TerminalColor,
};

/// A fresh one-space session whose shell stands in `root`, at the size
/// every test frame is drawn at.
pub(super) fn session(root: impl AsRef<Path>) -> Session {
    Session::new(
        uze_terminal::SpaceSeat {
            project: root.as_ref().to_path_buf(),
        },
        80,
        24,
    )
}

/// A model attached to `session` and nothing else, having read it as a
/// session update does.
pub(super) fn model_of(session: Session) -> WorkspaceModel {
    let mut model = WorkspaceModel {
        session: Some(session),
        ..WorkspaceModel::default()
    };
    model.note_space_projects(&identities_fixture());
    model
}

pub(super) fn identities_fixture() -> Vec<AgentIdentity> {
    vec![AgentIdentity {
        binary: "agent",
        integration: "agent",
        display_name: "Agent",
        launch: std::path::PathBuf::from("agent"),
        continuity_gap: None,
        configured: true,
    }]
}

/// A one-tab session whose only tab `agent_identity_for_tab` resolves
/// to the fixture identity by its probed process.
pub(super) fn agent_session() -> WorkspaceModel {
    let mut session = session("/tmp");
    session.workspace.spaces[0].tabs[0].label = "Agent".into();
    session.workspace.spaces[0].tabs[0].pane.process = "agent".into();
    model_of(session)
}

/// Damage carrying one changed cell — the smallest thing that still
/// counts as a pane having painted something.
pub(super) fn painted(pane: PaneId) -> ClientEvent {
    ClientEvent::Damage(PaneDamage {
        pane,
        columns: 80,
        rows: 24,
        cursor: Cursor { column: 0, row: 0 },
        alternate_screen: false,
        mouse: MouseMode {
            reports_clicks: false,
            reports_drag: false,
            sgr: false,
        },
        bracketed_paste: false,
        changed: vec![(
            0,
            0,
            RenderCell {
                character: 'x',
                foreground: TerminalColor::DefaultForeground,
                background: TerminalColor::DefaultBackground,
                attributes: CellAttributes::default(),
            },
        )],
    })
}

/// A task view as the application would answer it for the slot at
/// `checkout`, in `state`.
pub(super) fn task_in(
    checkout: &str,
    label: &str,
    state: WorkStateView,
    ahead: usize,
) -> AgentView {
    AgentView {
        id: "t1".into(),
        label: label.into(),
        branch: "agent/t1".into(),
        target: "main".into(),
        checkout: Some(PathBuf::from(checkout)),
        state,
        action: None,
        completion: Some(CompletionBehavior::Merge),
        isolated: true,
        parent: None,
        ahead,
        published_as: None,
        published_request: None,
        unsynced: None,
        created_at_unix: 1,
    }
}

/// A one-agent session in the slot `/repo/.worktrees/ai`, whose task is
/// in `state`.
pub(super) fn agent_with_task(state: WorkStateView, ahead: usize) -> WorkspaceModel {
    let mut model = agent_session_in("/repo/.worktrees/ai");
    stamp_first_tab(&mut model, "t1");
    model.remembered.tasks.insert(
        PathBuf::from("/repo"),
        vec![task_in(
            "/repo/.worktrees/ai",
            "fix-auth-redirect",
            state,
            ahead,
        )],
    );
    model
}

/// The tab strip as text and hits.
pub(super) fn tab_strip(model: &WorkspaceModel) -> (Vec<String>, Vec<(Rect, WorkspaceHit)>) {
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
    let rows = (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect();
    (rows, hits)
}

/// Where the strip drew the given control last frame.
pub(super) fn hit_rect(model: &WorkspaceModel, wanted: WorkspaceHit) -> Rect {
    let (_, hits) = tab_strip(model);
    hits.iter()
        .find(|(_, hit)| *hit == wanted)
        .map(|(rect, _)| *rect)
        .unwrap_or_else(|| panic!("{wanted:?} is not on the strip"))
}

/// A space holding two agents, each with one shell of its own, plus
/// the shell the space was born with. Returns the model and the two
/// agent tabs, in creation order.
pub(super) fn two_agents_with_shells() -> (WorkspaceModel, TabId, TabId) {
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let agent = |session: &mut Session, label: &str, cwd: &str| {
        let pane = session.add_tab(space, label.into(), None, 80, 24, cwd.into());
        let id = session.selected_space().selected_tab;
        // What makes a tab an agent is what is running in its pane —
        // the same live probe `agent_identity_for_tab` reads.
        session.update_pane_status(pane, cwd.into(), "agent".into());
        session.add_tab(
            space,
            format!("{label} shell"),
            Some(id),
            80,
            24,
            cwd.into(),
        );
        id
    };
    let first = agent(&mut session, "Agent one", "/repo/.worktrees/a");
    let second = agent(&mut session, "Agent two", "/repo/.worktrees/b");
    let model = model_of(session);
    (model, first, second)
}

/// Renders the whole frame (sidebar + tab strip + pane) the way the
/// real workspace loop does each frame, stores the resulting hits on
/// `model` — mirroring `model.hits = hits;` in `attach_workspace`'s own
/// loop — and returns the matching layout, the pair the drag-reorder
/// helpers need to classify a hit's rect.
pub(super) fn full_frame(model: &mut WorkspaceModel) -> WorkspaceLayout {
    full_frame_at(model, Rect::new(0, 0, 80, 24))
}

pub(super) fn full_frame_at(model: &mut WorkspaceModel, area: Rect) -> WorkspaceLayout {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    let mut hits = Vec::new();
    let mut metrics = render::FrameMetrics::default();
    terminal
        .draw(|frame| render::render(frame, model, &identities_fixture(), &mut hits, &mut metrics))
        .unwrap();
    model.hits = hits;
    // What the loop itself keeps from a frame — the extension surface
    // reports the geometry a click has to be resolved against, and a
    // test that dropped it would resolve clicks against the default.
    if let Some(rendered) = metrics.code {
        model.code_tree_scroll = rendered.navigator_scroll;
        model.code_scrollbars = rendered;
    }
    if let Some(frame) = metrics.recent {
        model.recent.scroll = frame.scroll;
        model.recent.bar = frame.bar;
    }
    model.absorb_manage_frame(metrics.manage);
    model.absorb_overview_frame(metrics.overview);
    compute_layout(area, model.sidebar_width)
}

/// The whole frame as text, row by row — for asserting not just that
/// something was drawn, but where.
pub(super) fn frame_rows(model: &mut WorkspaceModel) -> Vec<String> {
    frame_rows_at(model, Rect::new(0, 0, 80, 24))
}

/// [`frame_rows`] on a terminal of `area`'s size.
pub(super) fn frame_rows_at(model: &mut WorkspaceModel, area: Rect) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            render::render(
                frame,
                model,
                &identities_fixture(),
                &mut Vec::new(),
                &mut render::FrameMetrics::default(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect()
}

pub(super) fn label_every_tab(model: &mut WorkspaceModel, label: &str) {
    if let Some(session) = model.session.as_mut() {
        for space in &mut session.workspace.spaces {
            for tab in &mut space.tabs {
                tab.label = label.to_owned();
            }
        }
    }
}

/// A space with two agents in its root, one per harness: `agent 1` running
/// `claude` (the space's context agent, selected) and `agent 2` running
/// `codex`, both in the space's own root.
pub(super) fn agents_in_the_root_session() -> WorkspaceModel {
    let mut session = session("/repo");
    let space = session.workspace.selected_space;
    let first = session.add_tab(space, "agent 1".into(), None, 80, 24, "/repo".into());
    session.update_pane_status(first, "/repo".into(), "claude".into());
    let second = session.add_tab(space, "agent 2".into(), None, 80, 24, "/repo".into());
    session.update_pane_status(second, "/repo".into(), "codex".into());
    session.workspace.spaces[0].selected_tab = TabId(3);
    // Every agent UZE launches carries the identity its launch
    // stamped, whether or not it has a checkout — it is what a task
    // is later recorded against, and what `Isolate` isolates.
    for (tab, id) in session.workspace.spaces[0]
        .tabs
        .iter_mut()
        .filter(|tab| tab.pane.process != "shell")
        .zip(["a1", "a2"])
    {
        tab.env = vec![(
            uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
            id.to_owned(),
        )];
    }
    let mut model = model_of(session);
    model
        .remembered
        .branches
        .insert(PathBuf::from("/repo"), "main".into());
    model
}

/// The two harnesses the agents above run, so the sidebar is drawn
/// among these rather than the one-identity fixture the tree tests
/// share.
pub(super) fn identities_in_the_root() -> Vec<AgentIdentity> {
    vec![
        AgentIdentity {
            binary: "claude",
            integration: "claude-code",
            display_name: "Claude Code",
            launch: std::path::PathBuf::from("/uze/shims/claude"),
            continuity_gap: None,
            configured: true,
        },
        AgentIdentity {
            binary: "codex",
            integration: "codex",
            display_name: "Codex",
            launch: std::path::PathBuf::from("codex"),
            continuity_gap: None,
            configured: true,
        },
    ]
}

/// One space, both groups: the agents in its root above, the isolated
/// ones below, with one blank row between them. A separator between
/// collections, never between siblings — so a space whose agents are
/// all in one group has none, which is what a blank row after every
/// agent used to look like.
pub(super) fn a_space_with_both_groups() -> WorkspaceModel {
    let mut model = agents_in_the_root_session();
    // `agent 2` is given a checkout of its own: the identity its
    // launch stamped is what the task is recorded against, and the
    // task's branch is what puts the row in the other group.
    let tab = model.session.as_ref().unwrap().workspace.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.label == "agent 2")
        .expect("the second agent")
        .id;
    let stamped = model.session.as_mut().unwrap().workspace.spaces[0]
        .tabs
        .iter_mut()
        .find(|candidate| candidate.id == tab)
        .expect("the second agent");
    stamped.env = vec![(
        uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
        "t1".to_owned(),
    )];
    stamped.pane.cwd = PathBuf::from("/repo/.worktrees/ai");
    model.remembered.tasks.insert(
        PathBuf::from("/repo"),
        vec![task_in(
            "/repo/.worktrees/ai",
            "fix-auth-redirect",
            WorkStateView::Running,
            0,
        )],
    );
    model
}

/// A tick on which the working spinner shows its fullest frame, the one
/// glyph no separator or caption shares.
pub(super) fn a_tick_showing_the_fullest_frame() -> usize {
    let fullest = theme::frame(theme::Symbol::StatusWorking, 3);
    (0..1_000)
        .find(|&tick| agent_activity_frame(tick) == fullest)
        .expect("the cycle reaches every frame")
}

pub(super) fn preserved(
    project: &str,
    id: &str,
    label: &str,
    state: WorkStateView,
) -> uze_application::PreservedWork {
    uze_application::PreservedWork {
        project: PathBuf::from(project),
        id: id.to_owned(),
        label: label.to_owned(),
        branch: Some(format!("agent/{id}")),
        checkout: Some(PathBuf::from(project).join(".worktrees").join(id)),
        state,
        created_at_unix: 1,
        last_active_unix: 1,
        harness: "claude-code".to_owned(),
        project_present: true,
        named: true,
    }
}

/// The first tab of the first space — the one tab most fixtures have.
pub(super) fn first_tab(model: &WorkspaceModel) -> &Tab {
    &model.session.as_ref().expect("a session").workspace.spaces[0].tabs[0]
}

pub(super) fn first_tab_mut(model: &mut WorkspaceModel) -> &mut Tab {
    &mut model.session.as_mut().expect("a session").workspace.spaces[0].tabs[0]
}

/// Marks the first tab as launched for `id`: what the server echoes
/// back for a tab the client created with that identity stamped.
pub(super) fn stamp_first_tab(model: &mut WorkspaceModel, id: &str) {
    let tab = first_tab_mut(model);
    tab.env = vec![(
        uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
        id.to_owned(),
    )];
}

pub(super) fn agent_session_in(cwd: &str) -> WorkspaceModel {
    let mut session = session("/repo");
    let tab = &mut session.workspace.spaces[0].tabs[0];
    tab.label = "Agent".into();
    tab.pane.process = "agent".into();
    tab.pane.cwd = cwd.into();
    model_of(session)
}

/// Two agents in one space, the first of them selected: `Agent` in
/// `first`, `Second` in `second`. The second resolves by its pane's
/// process rather than its label, so the two never answer to the same
/// row search.
pub(super) fn two_agent_session(first: &str, second: &str) -> WorkspaceModel {
    let mut model = agent_session_in(first);
    let space = &mut model.session.as_mut().unwrap().workspace.spaces[0];
    let mut tab = space.tabs[0].clone();
    tab.id = TabId(2);
    tab.label = "Second".into();
    tab.pane = Pane {
        id: PaneId(2),
        cwd: second.into(),
        columns: 80,
        rows: 24,
        process: "agent".to_owned(),
        through_launcher: true,
    };
    space.tabs.push(tab);
    model
}

/// Moves the keystrokes to the second agent [`two_agent_session`] built.
pub(super) fn select_second_agent(model: &mut WorkspaceModel) {
    model.session.as_mut().unwrap().workspace.spaces[0].selected_tab = TabId(2);
}

/// A one-agent session in `/repo` whose checkout's history is
/// `subjects`, newest first, every commit landed `3h` ago.
pub(super) fn session_with_timeline(subjects: &[&str]) -> WorkspaceModel {
    let mut model = agent_session_in("/repo");
    model.remembered.git_badge = Some(GitBadge {
        cwd: PathBuf::from("/repo"),
        summary: None,
        timeline: Some(uze_extensions::code::Timeline {
            branch: "main".to_owned(),
            commits: subjects
                .iter()
                .enumerate()
                .map(|(index, subject)| uze_extensions::code::Commit {
                    hash: format!("{index:07x}"),
                    subject: (*subject).to_owned(),
                    age: "3h".to_owned(),
                    ahead: false,
                })
                .collect(),
        }),
        timeline_checked_at: Instant::now(),
        checked_at: Instant::now(),
    });
    model
}

/// A sidebar row without its right-hand divider and the padding
/// before it.
pub(super) fn inside(row: &str) -> &str {
    row.trim_end_matches('│').trim_end()
}

pub(super) fn timeline_hit(hits: &[(Rect, WorkspaceHit)]) -> Option<Rect> {
    hits.iter()
        .find(|(_, hit)| {
            *hit == WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::ToggleSection))
        })
        .map(|(rect, _)| *rect)
}

pub(super) fn manage_route(driven: &Driven<'_>) -> crate::ui::model::Route {
    driven
        .attach
        .model
        .manage
        .as_ref()
        .expect("the modal is open")
        .route
}

/// The keystroke a chord is: the inverse of `keys::chord_of`, so a test
/// can press what the keymap says rather than a key typed by hand.
pub(super) fn key_event(chord: uze_keys::Chord) -> crossterm::event::KeyEvent {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use uze_keys::Key;
    let code = match chord.key {
        Key::Char(character) => KeyCode::Char(character),
        Key::F(number) => KeyCode::F(number),
        Key::Enter => KeyCode::Enter,
        Key::Esc => KeyCode::Esc,
        Key::Tab => KeyCode::Tab,
        Key::Space => KeyCode::Char(' '),
        Key::Backspace => KeyCode::Backspace,
        Key::Delete => KeyCode::Delete,
        Key::Insert => KeyCode::Insert,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
    };
    let mut modifiers = KeyModifiers::NONE;
    if chord.mods.ctrl {
        modifiers |= KeyModifiers::CONTROL;
    }
    if chord.mods.alt {
        modifiers |= KeyModifiers::ALT;
    }
    if chord.mods.shift {
        modifiers |= KeyModifiers::SHIFT;
    }
    KeyEvent::new(code, modifiers)
}

/// The sidebar drawn once, among `identities`: what it looks like, as
/// cells and as text, and what the frame recorded while drawing it —
/// the hits a click resolves against and the bounds only the render
/// knows, like the tree's own scroll overflow.
pub(super) struct Sidebar {
    pub(super) buffer: ratatui::buffer::Buffer,
    pub(super) rows: Vec<String>,
    pub(super) hits: Vec<(Rect, WorkspaceHit)>,
    pub(super) metrics: FrameMetrics,
}

pub(super) fn sidebar(model: &WorkspaceModel, identities: &[AgentIdentity]) -> Sidebar {
    let mut terminal = Terminal::new(TestBackend::new(40, 24)).unwrap();
    let mut hits = Vec::new();
    let mut metrics = FrameMetrics::default();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                model,
                identities,
                &mut hits,
                &mut metrics,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    Sidebar {
        rows: buffer_rows(&buffer),
        buffer,
        hits,
        metrics,
    }
}

/// A sidebar row with the column's own chrome taken off it: the rail
/// beside the selected space, the divider down the right edge, and
/// the padding between. What is left is what the row *says*.
pub(super) fn without_chrome(row: &str) -> &str {
    let rail = theme::glyph(crate::ui::theme::Symbol::BarThin);
    let divider = theme::glyph(crate::ui::theme::Symbol::TreeColumnDivider);
    row.trim_matches(|character: char| {
        character == ' ' || rail.contains(character) || divider.contains(character)
    })
}

/// The rows whose gutter is lit — drawn in the hue of the space's own
/// kind, in the sidebar's leading column `column`, which every space's
/// gutter runs down.
pub(super) fn lit_gutter_rows(buffer: &ratatui::buffer::Buffer, column: u16) -> Vec<u16> {
    let hue = theme::color(Token::Accent);
    (0..buffer.area.height)
        .filter(|row| {
            let cell = &buffer[(column, *row)];
            cell.symbol() != " " && cell.fg == hue
        })
        .collect()
}

/// The column every space's gutter runs down, by the first header drawn.
pub(super) fn gutter_column(hits: &[(Rect, WorkspaceHit)]) -> u16 {
    hits.iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::SelectSpace(_)))
        .map(|(rect, _)| rect.x)
        .expect("a space header is drawn")
}

pub(super) fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|row| {
            (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect()
        })
        .collect()
}

pub(super) fn mouse_at(
    column: u16,
    row: u16,
    kind: MouseEventKind,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::empty(),
    }
}

pub(super) fn identities() -> Vec<AgentIdentity> {
    vec![
        AgentIdentity {
            binary: "claude",
            integration: "claude-code",
            display_name: "Claude Code",
            launch: std::path::PathBuf::from("/uze/shims/claude"),
            continuity_gap: None,
            configured: true,
        },
        AgentIdentity {
            binary: "codex",
            integration: "codex",
            display_name: "Codex",
            launch: std::path::PathBuf::from("codex"),
            continuity_gap: Some("no launcher".to_owned()),
            configured: true,
        },
    ]
}

/// One attached client, driven the way the real loop drives it: hits
/// from a real frame, a socket pair standing in for the server, and
/// the channels a background read answers through.
pub(super) struct Driven<'a> {
    pub(super) attach: Attach<'a>,
    server: uze_terminal::Stream,
    events: std::sync::mpsc::Receiver<ClientEvent>,
    /// The reader thread's end, held so the channel stays connected.
    /// Dropping it is exactly what the real reader does when the
    /// socket stops answering, which is how this client learns the
    /// terminal server is gone.
    pub(super) events_sender: Option<std::sync::mpsc::Sender<ClientEvent>>,
    /// The terminal every frame is drawn at and every click resolved
    /// against. Small by default, because most of what this drives
    /// does not depend on the room: the ones that do say so with
    /// [`Driven::on_a_roomy_terminal`].
    pub(super) area: Rect,
}

impl Driven<'_> {
    /// Draws at a terminal big enough for the surfaces that keep a
    /// margin — the manage modal takes the whole frame below
    /// `management::ROOMY_*`, so the gestures that need something
    /// beside it need a screen that has one.
    pub(super) fn on_a_roomy_terminal(mut self) -> Self {
        self.area = Rect::new(0, 0, 120, 40);
        self
    }

    /// Draws the frame the next click is tested against, storing its
    /// hits on the model exactly as the attach loop does.
    pub(super) fn frame(&mut self) {
        full_frame_at(&mut self.attach.model, self.area);
    }

    pub(super) fn press(&mut self, column: u16, row: u16) {
        self.mouse(column, row, MouseEventKind::Down(MouseButton::Left));
    }

    /// Any other mouse event at the same viewport the click helpers
    /// use — the rest of a drag, which `press` alone cannot say.
    pub(super) fn mouse(&mut self, column: u16, row: u16, kind: MouseEventKind) {
        let area = self.area;
        let layout = compute_layout(area, self.attach.model.sidebar_width);
        let viewport = Viewport {
            size: ratatui::layout::Size::new(area.width, area.height),
            columns: layout.pane.width,
            rows: layout.pane.height,
            layout,
        };
        let event = crossterm::event::Event::Mouse(mouse_at(column, row, kind));
        let _ = self.attach.handle(event, &viewport);
    }

    /// One key, through the same dispatch the attach loop uses.
    pub(super) fn press_key(&mut self, key: crossterm::event::KeyEvent) {
        let area = self.area;
        let layout = compute_layout(area, self.attach.model.sidebar_width);
        let viewport = Viewport {
            size: ratatui::layout::Size::new(area.width, area.height),
            columns: layout.pane.width,
            rows: layout.pane.height,
            layout,
        };
        let _ = self
            .attach
            .handle(crossterm::event::Event::Key(key), &viewport);
    }

    /// One turn of everything that is not an event — what absorbs a
    /// placement once its thread has answered.
    pub(super) fn pump(&mut self) -> Flow {
        self.attach.pump(&self.events)
    }

    /// The terminal server exiting under the client.
    pub(super) fn runtime_gone(&mut self) {
        self.events_sender = None;
    }

    /// Every request written to the server since the last read.
    pub(super) fn sent(&mut self) -> Vec<ClientRequest> {
        // Whatever has arrived: a read that waits past this is one
        // nothing was written for.
        self.server
            .set_read_timeout(Some(std::time::Duration::from_millis(10)))
            .unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(read) = std::io::Read::read(&mut self.server, &mut chunk) {
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        let mut requests = Vec::new();
        let mut rest = buffer.as_slice();
        while rest.len() >= 4 {
            let (length, payload) = rest.split_at(4);
            let length = u32::from_le_bytes(length.try_into().unwrap()) as usize;
            assert!(payload.len() >= length, "a whole frame");
            requests.push(bincode::deserialize(&payload[..length]).expect("a request"));
            rest = &payload[length..];
        }
        requests
    }

    /// Hands one already-received placement back to the client, the
    /// way the loop's own `pump` absorbs it.
    pub(super) fn placements_answered(&mut self, resolution: PlacementResolution) {
        self.attach
            .channels
            .placements
            .sender
            .send(resolution)
            .unwrap();
        self.pump();
    }

    /// The rect of the one hit of its kind the last frame drew.
    pub(super) fn hit(&self, wanted: impl Fn(&WorkspaceHit) -> bool) -> Rect {
        let found: Vec<Rect> = self
            .attach
            .model
            .hits
            .iter()
            .filter(|(_, hit)| wanted(hit))
            .map(|(rect, _)| *rect)
            .collect();
        assert_eq!(found.len(), 1, "exactly one such hit: {found:?}");
        found[0]
    }
}

/// The picker offers only harnesses set up on this machine, so a test
/// that launches one sets them up first.
pub(super) fn set_up_every_harness(home: &UzeHome) {
    for identity in crate::ui::orchestrator::agent_identities(home) {
        set_up_harness(home, &identity);
    }
}

/// What setup leaves behind for the workspace to read a harness as set
/// up: a verified provisioning record and the launcher in the shims
/// directory.
pub(super) fn set_up_harness(home: &UzeHome, identity: &AgentIdentity) {
    uze_core::state::record_provisioning(
        home,
        identity.integration,
        &uze_core::provisioning::ProvisioningResult::verified(
            uze_core::provisioning::ProvisionAction::None,
            "test",
            uze_core::integration::HarnessDetection {
                present: true,
                version: None,
            },
        ),
    )
    .unwrap();
    std::fs::create_dir_all(home.shims_dir()).unwrap();
    std::fs::write(home.shim_path(identity.binary), "").unwrap();
}

pub(super) fn driven(model: WorkspaceModel, home: &UzeHome) -> Driven<'_> {
    let (client, server) = uze_terminal::stream_pair().unwrap();
    let (events, events_rx) = std::sync::mpsc::channel();
    Driven {
        attach: Attach {
            model,
            stream: client,
            home,
            identities: identities_fixture(),
            // Leaked like the management memory below: the attach
            // borrows its channels for as long as it lives.
            channels: Box::leak(Box::default()),
            spinner: indicatif::ProgressBar::hidden(),
            next_tick: Instant::now(),
            asked_for_a_tab: false,
            // Leaked on purpose: the attach borrows the memory for as
            // long as it lives, and a test's lives until the process
            // does.
            manage_memory: Box::leak(Box::new(
                crate::ui::management::ManagementMemory::unresolved(),
            )),
            keyboard: crate::ui::keys::KeyboardSupport::default(),
            theme_generation: uze_theme::generation(),
        },
        server,
        events: events_rx,
        events_sender: Some(events),
        area: Rect::new(0, 0, 80, 24),
    }
}

/// Three spaces, `one`, `two` and `three`, one agent each; the
/// last one created is selected.
pub(super) fn three_spaces() -> WorkspaceModel {
    let mut session = session("/one");
    let pane = session.selected_tab().pane.id;
    session.update_pane_status(pane, "/one".into(), "agent".into());
    for name in ["two", "three"] {
        let root = PathBuf::from(format!("/{name}"));
        session.create_space(Some(name.into()), root.clone(), 80, 24);
        let pane = session.selected_tab().pane.id;
        session.update_pane_status(pane, root, "agent".into());
    }
    model_of(session)
}

/// The header row a space was drawn at, by the frame's own hits.
pub(super) fn space_header(hits: &[(Rect, WorkspaceHit)], wanted: SpaceId) -> Rect {
    hits.iter()
        .filter(|(_, hit)| matches!(hit, WorkspaceHit::SelectSpace(space) if *space == wanted))
        .map(|(rect, _)| *rect)
        .min_by_key(|rect| rect.y)
        .expect("the space has a header")
}

/// A session whose one agent sits in `checkout`, removed from under
/// it and bound to `task` — the state the "resume" is drawn from.
pub(super) fn agent_over_a_lost_checkout(
    checkout: &Path,
    primary: &Path,
    task: AgentView,
) -> WorkspaceModel {
    // The space is rooted at the project, as an operator working on it
    // has it: an agent belongs to its project, and that is what decides
    // which space its tab opens in.
    let mut session = session(primary);
    let tab = &mut session.workspace.spaces[0].tabs[0];
    tab.label = "Agent".into();
    tab.pane.process = "agent".into();
    tab.pane.cwd = format!("{} (deleted)", checkout.display()).into();
    let mut model = model_of(session);
    let pane = first_tab(&model).pane.id;
    // Rows under the one that lost its checkout: what the picker
    // opens over, and what its own rows have to answer ahead of.
    if let Some(session) = model.session.as_mut() {
        let space = session.workspace.selected_space;
        for label in ["Agent two", "Agent three"] {
            let opened = session.add_tab(space, label.into(), None, 80, 24, primary.to_path_buf());
            session.update_pane_status(opened, primary.to_path_buf(), "agent".into());
        }
    }
    model
        .remembered
        .pane_checkouts
        .insert(pane, checkout.to_path_buf());
    stamp_first_tab(&mut model, &task.id);
    model.remembered.lost_checkouts.insert(pane);
    model
        .remembered
        .tasks
        .insert(primary.to_path_buf(), vec![task]);
    model
}

/// The first target a drawn toast registered: its `✕`, ahead of the
/// box behind it.
pub(super) fn toast_close(driven: &Driven<'_>) -> Rect {
    driven
        .attach
        .model
        .hits
        .iter()
        .find(|(_, hit)| matches!(hit, WorkspaceHit::DismissToast(_)))
        .map(|(rect, _)| *rect)
        .expect("the toast registered a target")
}

/// Selects `tab` and reads the session again, as an update from the
/// server would have the client do.
pub(super) fn select_tab(model: &mut WorkspaceModel, tab: TabId) {
    model.session.as_mut().expect("session").select_tab(tab);
    model.note_space_projects(&identities_fixture());
}
