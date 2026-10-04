//! Workspace client for the persistent local terminal runtime (ADR-038).
//!
//! Presentation deliberately shares the management surface's palette and
//! layout conventions (hairline dividers, no filled panels) so the modal
//! it opens over itself reads as one product, not two.

use super::agent_support::PromptScope;
use super::tui_application;
use crate::ui::extension_host::WorkspaceHost;
use crate::ui::extension_view;
use crate::ui::root_picker::RootPicker;
use crate::ui::selection::{self, Selection};
use crate::ui::theme::{self, Symbol, Token};
use crate::ui::widget::ToastKind;
use crate::ui::widget::{action_index, text};
use crossterm::event::{
    self, Event, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph, Wrap},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use uze_application::AgentIdentity;
use uze_application::path::Canonical as _;
use uze_application::{
    AgentView, CompletionBehavior, DeliveryOutcome, DeliveryReport, Evaluation, UpstreamSync,
    WorkStateView,
};
use uze_application::{Result, UzeError, UzeHome};
use uze_extensions::{
    ExtensionHit, architect, code, spec,
    view::{ScrollDirection, ViewHit},
};
use uze_keys::{Action, Chord, Key};
use uze_terminal::{
    CellAttributes, ClientEvent, ClientRequest, Cursor, PROTOCOL_VERSION, PaneDamage, PaneId,
    PaneSnapshot, RenderCell, Session, Space, SpaceId, Tab, TabId, TerminalColor, attach,
    read_event, send_request,
};

mod gestures;
mod pickers;
mod surfaces;
mod tabs;

use gestures::*;
use pickers::*;
use surfaces::*;
pub(super) use tabs::*;

mod reads;

pub(super) use reads::*;

mod activity;
mod agents;
mod answers;
mod events;
mod notices;

/// Input/redraw cadence. Unlike the pane content itself — which the server
/// now pushes on PTY output instead of the client polling for it (see
/// ADR-038 follow-up: the previous per-frame `Refresh` request serialized
/// every cell of every pane up to 60x/sec regardless of activity, which is
/// what made typing feel like it hung under any real system load) — this
/// timeout only bounds keyboard/mouse latency.
const POLL: Duration = Duration::from_millis(16);
/// The most input events handled between two frames, so a flood of input
/// cannot hold the frame back indefinitely.
const EVENTS_PER_FRAME: usize = 64;

/// Git inspection runs locally but still launches a process. Refresh often
/// enough to follow commands typed in the active pane without attaching that
/// cost to every PTY damage redraw.
const GIT_BADGE_REFRESH: Duration = Duration::from_millis(750);
/// How far back the sidebar's timeline reads — the most its section can
/// be dragged open to. The section itself never claims the column from
/// the spaces it sits under (see `render::timeline_height`).
const TIMELINE_COMMITS: usize = 30;
/// How often the timeline is re-read while the tab stays put — slower
/// than the badge: telling a delivered commit from one still ahead
/// compares patches across the branch and its target, and history does
/// not move at the pace a working tree does.
const TIMELINE_REFRESH: Duration = Duration::from_secs(3);
/// How often the sidebar's spec summary is re-read while the tab stays
/// put. Slower still: task lists are ticked by hand, a few times an hour,
/// and each read walks every change in flight.
const SPEC_SUMMARY_REFRESH: Duration = Duration::from_secs(5);

/// How far apart two reads of the same thing are kept: never closer than
/// `floor`, and never closer than a few times what the last one took.
///
/// A fixed period assumes the read is cheap. On a large checkout a
/// `status` takes longer than the period itself, and a read that starts
/// the moment the last one ends is a core spent on Git for as long as the
/// workspace is open — for an answer that changes a few times a minute.
fn paced(floor: Duration, took: Duration) -> Duration {
    floor.max(took * uze_extensions::code::PACE)
}

/// The theme's `status.working` frames configure the hidden `indicatif`
/// spinner that schedules this animation. Ratatui owns the alternate screen,
/// so it paints the frame instead of letting indicatif write to stderr.
const AGENT_ACTIVITY_TICK: Duration = Duration::from_millis(120);

/// How long a checkout's measurement is taken to still describe it.
///
/// Long enough that opening the code surface, closing it and opening it
/// again is free — which is how it is used — and short enough that a map
/// is never a picture of a morning ago.
const CODE_MEASURE_FRESH: Duration = Duration::from_secs(30);
/// How long a pressed control wears its pressed skin. Long enough to be
/// seen at a glance, short enough that it reads as the press itself and
/// never as a state the control got stuck in.
const PRESS_FLASH: Duration = Duration::from_millis(140);
/// The least time between two rings of the bell. Agents started together
/// tend to finish together, and one sound says "go look" as well as four.
const CHIME_COOLDOWN: Duration = Duration::from_secs(4);
/// How long a finished turn must stay finished before it rings. The turn
/// detector calls a turn over after [`AGENT_QUIET_AFTER`] without a
/// repaint, which a long tool call or a pause to think also reaches; the
/// sidebar can afford that, because a spinner that comes back costs
/// nothing, but a bell that sounds on every pause is the one thing this
/// feature must not be. A pause the agent resumes from inside this window
/// never rings; one it does not — finished, or waiting on the operator —
/// is exactly what the bell is for.
const CHIME_SETTLE: Duration = Duration::from_secs(10);
/// How long an agent pane must stay quiet before its work reads as
/// finished. Counted in missed beats: at the once-a-second a harness
/// keeps while it waits, this is five of them, which a phase change
/// between tools does not reach and a finished turn passes straight
/// through. Agent harnesses animate while they work — a spinner, an
/// elapsed-token counter — so a pane that is genuinely busy keeps emitting
/// damage well inside this window even across a slow tool call, and a pane
/// that stops emitting has stopped working. Long enough to ride out a
/// harness that redraws less eagerly, short enough that "done" arrives
/// while the user still cares; unlike the previous 10s window, guessing
/// low is no longer terminal because renewed output re-enters `Working`
/// on its own (see [`WorkspaceModel::note_agent_output`]).
const AGENT_QUIET_AFTER: Duration = Duration::from_secs(5);

/// What a pane's repainting has to look like before it reads as an agent
/// at work: a *beat*, not a rate.
///
/// A harness running a turn keeps its own clock on screen — a spinner
/// while it thinks, an elapsed counter while it waits on a tool — and the
/// slowest of those ticks about once a second. An idle one repaints too,
/// but sporadically: a rotating hint, a status line, a pasted image being
/// laid out. What tells them apart is the *rhythm*, and this used to ask
/// the wrong question of it: five frames inside one second, which only a
/// spinner mid-animation can answer. A harness sitting on `· 1m 23s`
/// while a tool call runs paints once a second, never reaches five, and
/// read as stopped — with the terminal moving the whole time.
///
/// So: enough beats to be a rhythm ([`AGENT_BEATS`]) inside a window
/// short enough that only a rhythm fits in it
/// ([`AGENT_BEAT_WINDOW`]), spanning long enough not to be one frame
/// arriving in pieces ([`AGENT_BEAT_SPAN`]). A once-a-second counter
/// passes. A hint that turns over every half minute does not, which is
/// what the old threshold was protecting against — treating any single
/// repaint as work is what left merely-open agents spinning forever.
///
/// Counted, deliberately, rather than measured against the widest gap in
/// the window. Judging the gaps was tried and is worse: a harness that
/// pauses two seconds between phases puts one wide pair in the window,
/// and every call for as long as that pair is retained fails on it — the
/// status dropped out of `Working` and came back once the pair aged off,
/// which is a flicker where the old rule at least held still. A count
/// over a short window says the same thing about cadence and cannot be
/// poisoned by one hiccup.
const AGENT_BEAT_WINDOW: Duration = Duration::from_millis(3500);
const AGENT_BEAT_SPAN: Duration = Duration::from_millis(900);
const AGENT_BEATS: usize = 3;

/// A pane echoes what the user types or pastes, and that echo is damage
/// like any other. Damage inside the window that input opens is treated
/// as that echo, not as the agent working — otherwise composing a prompt,
/// or dropping an image into one, would light the sidebar up as busy.
/// Enter is exempt: it marks the pane busy explicitly. A paste gets the
/// longer window because the harness reflows its whole prompt box around
/// the pasted content, well after the bytes themselves landed.
const AGENT_ECHO_GRACE: Duration = Duration::from_millis(150);
const AGENT_PASTE_GRACE: Duration = Duration::from_millis(750);

/// A pane the client just resized — on attach, on a tab switch, on a
/// terminal resize — redraws because we asked it to, and that redraw is
/// not the agent working either.
///
/// A fixed window could not say how long that takes. Re-laying out a long
/// conversation runs well past a second, so the window shut in the middle
/// of the redraw and the rest of it read as a beat: selecting a finished
/// agent put it straight back on the spinner, which is the one thing
/// looking at it was supposed to settle. So the window *follows* the
/// redraw — each frame pushes it out by [`AGENT_SETTLE_QUIET`] — and
/// [`AGENT_SETTLE_CAP`] is the promise that it ends: past that, a pane
/// still painting is painting for itself, and a turn running through a
/// resize is understated once rather than muted for as long as it runs.
const AGENT_REDRAW_GRACE: Duration = Duration::from_millis(1000);
const AGENT_SETTLE_QUIET: Duration = Duration::from_millis(400);
const AGENT_SETTLE_CAP: Duration = Duration::from_millis(2500);

mod checkouts;
mod input;
mod render;
mod session;
mod work;
mod work_list;
use checkouts::*;
use input::*;
use render::*;
use session::*;
use work::*;
use work_list::*;

/// Why an attach ended.
pub(crate) enum WorkspaceExit {
    /// The terminal runtime stopped answering. `super::run` attaches
    /// again rather than leaving the operator with a frozen session.
    Disconnected,
    Quit,
}

/// How often every visible repository's tasks are re-read even when no
/// pane went quiet — a task delivered from another client, a branch
/// integrated by hand, a checkout removed.
const TASK_REFRESH: Duration = Duration::from_secs(20);

/// The widest a notice may draw in the header. Past this it is elided:
/// the chip shares one row with the tabs, and a message that pushes them
/// off the strip costs more than it says.
const NOTICE_WIDTH: usize = 34;
/// How long an outcome stays on screen before it leaves on its own. Longer
/// than a glance and shorter than a distraction; one that must be answered
/// is not on this clock at all.
const TOAST_TTL: Duration = Duration::from_secs(6);
/// How many outcomes the stack holds. Past this the oldest goes: the
/// reader is looking at the newest, and a column tall enough to need
/// scrolling has stopped being a transient message.
const MAX_TOASTS: usize = 4;

/// An agent's tab, and the project whose space it belongs in.
struct PendingAgentTab {
    project: PathBuf,
    label: String,
    command: Vec<String>,
    cwd: PathBuf,
    agent: String,
    size: (u16, u16),
}

/// Everything reachable with `scopes` open, each with the key that
/// reaches it, and then what was `offered` there without one. The
/// workspace's counterpart to the management model's own
/// `action_index_rows` — the same question, read from the same keymap, so
/// the two surfaces cannot describe themselves differently.
fn action_index_rows(
    scopes: &[uze_keys::Scope],
    offered: &[uze_keys::Action],
    filter: &str,
    disabled: &std::collections::BTreeSet<String>,
) -> Vec<(uze_keys::Action, Option<uze_keys::Chord>)> {
    let keymap = uze_keys::active();
    let mut rows = keymap.available(scopes);
    for action in offered {
        if !rows.iter().any(|(listed, _)| listed == action) {
            rows.push((*action, keymap.chord_for(*action, scopes)));
        }
    }
    rows.retain(|(action, _)| super::extension_switch::offered(*action, disabled));
    action_index::narrowed(rows, filter)
}

/// The open index of everything, in the workspace client.
///
/// Carries the scopes it was opened over: what is reachable is a question
/// about what was open underneath, not about the index itself.
struct ActionIndexOverlay {
    scopes: Vec<uze_keys::Scope>,
    /// What was on offer underneath that holds no key — a work row's rarer
    /// moves, delivering a whole space — which the index is the keyboard's
    /// way to.
    offered: Vec<uze_keys::Action>,
    filter: String,
    selected: usize,
}

/// What an evaluation of `cwd` is reserved under.
///
/// The repository a slot hangs off — [`uze_application::slot_key`], which
/// is where the rule that every slot of a repository is one answer lives.
/// Kept as a name here because it reads as the *key* at every call site,
/// and a reader following one should land on the rule rather than on a
/// path helper.
fn evaluation_key(cwd: &Path) -> PathBuf {
    uze_application::slot_key(cwd)
}

/// What a delivery leaves on screen: the ending, in as few words as name
/// it. No label of its own — whoever shows it decides whether the task it
/// is about still needs naming (see `WorkspaceModel::notice_chip`) — and
/// no restatement of what the pane it came from already says at length.
fn describe_delivery(report: &DeliveryReport) -> String {
    let outcome = describe_delivery_outcome(report);
    match report.warnings.as_slice() {
        [] => outcome,
        warnings => format!("{outcome} · {}", warnings.join(" · ")),
    }
}

fn describe_delivery_outcome(report: &DeliveryReport) -> String {
    match &report.outcome {
        DeliveryOutcome::Handoff => format!("ready on {}", report.task.branch),
        DeliveryOutcome::Merged => format!(
            "merged {} {}",
            theme::glyph(Symbol::ArrowTo),
            report.task.target
        ),
        DeliveryOutcome::Published { request, .. } => {
            format!("synced {} #{request}", theme::glyph(Symbol::ArrowTo))
        }
        // The one line with room for the whole word, and the one place
        // there is no number to say it instead: the forge's own word
        // where `origin` said which forge this is, and the neutral one
        // where it did not — which is the word the agent was handed too.
        DeliveryOutcome::AwaitingRequest(_) => format!(
            "pushed · agent opening the {}",
            report.task.forge.request_term().unwrap_or("request")
        ),
        DeliveryOutcome::Refused(reason) => reason.clone(),
        DeliveryOutcome::ReturnedToAgent(_) => "back to its agent".to_owned(),
    }
}

/// What starting `uze` in a directory asks of the workspace.
///
/// A directory somebody chose is a request: `cd` into a project, start
/// uze, and the project is there. The home directory is not a choice — it
/// is where a shell starts when nobody said otherwise — so starting there
/// lands on the home space when one is open and adds nothing when none
/// is. Without the distinction, a home space closed on purpose came back
/// on the next launch, which is indistinguishable from closing it not
/// having worked.
///
/// Compared the way `uze_terminal::space_label` compares it, so the rule
/// applies to exactly the space that would have been called *home*.
///
/// This never leaves the workspace empty: a server with nothing persisted
/// bootstraps a space at the seat it was started with, before any client
/// attaches.
fn seating_at(seat: uze_terminal::SpaceSeat) -> uze_terminal::Seating {
    let home = uze_platform::home::user_home();
    if home.is_some_and(|home| home == seat.root) {
        return uze_terminal::Seating::At(seat);
    }
    uze_terminal::Seating::Open(seat)
}

/// Whether this attach asks the server for a space rooted at the launch
/// directory.
///
/// Only the process's first attach does. An attach after the runtime went
/// away is a fresh attach of the *same* run, and asking again there would
/// reopen a space the operator closed in between — the launch directory
/// would resurrect it, and closing it would look broken rather than
/// deliberate.
///
/// Even the first attach only *asks*; whether asking may create is
/// [`seating_at`]'s question.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Landing {
    /// The run's first attach: the directory `uze` was started in gets a
    /// space, created when none is open for it.
    AtLaunchDirectory,
    /// An attach after the first: the session already knows where the
    /// operator was, and a space they closed stays closed.
    WhereItLeftOff,
}

/// The active theme, in the shape the terminal runtime speaks: plain
/// triples, since that crate holds no opinion about appearance.
fn active_palette() -> uze_terminal::Palette {
    let theme = uze_theme::active();
    let triple = |token| {
        let rgb = theme.color(token);
        (rgb.0, rgb.1, rgb.2)
    };
    uze_terminal::Palette {
        foreground: triple(Token::TextPrimary),
        background: triple(Token::SurfaceBackground),
        ansi: std::array::from_fn(|index| triple(Token::ANSI[index])),
    }
}

pub(crate) fn attach_workspace(
    terminal: &mut super::TerminalSession,
    // The directory `uze` was started in and the kind its space is created
    // as when this client is the one to create it — decided before the
    // attach, once per run of the loop: one Git read on the way in, never
    // inside it.
    launch: &uze_terminal::SpaceSeat,
    layout: &mut uze_application::ClientLayout,
    memory: &mut WorkspaceMemory,
    home: &UzeHome,
    manage: &mut super::management::ManagementMemory,
    landing: Landing,
) -> Result<WorkspaceExit> {
    // The handshake below must ship the real terminal size: it sizes the PTY
    // used for the session's *already-selected* pane (e.g. a tab restored
    // from a prior attach), and the per-frame resize further down only
    // corrects the size actually visible in that loop's compute_layout call.
    // A placeholder here previously left a stale-selected pane pinned to a
    // wrong fixed size until something happened to trigger a fresh resize.
    // `layout.sidebar.width` carries over whatever the user last dragged
    // it to, so the pane starts at its real width immediately instead of
    // assuming the sidebar's responsive default.
    let size = terminal.size()?;
    let geometry = compute_layout(
        Rect::new(0, 0, size.width, size.height),
        layout.sidebar.width,
    );
    let (columns, rows) = (geometry.pane.width, geometry.pane.height);

    // One server per user; what the launch directory decides is which
    // space this client lands in, and only on the run's first attach (see
    // `Landing`). Resolving the workspace root *before* attaching is what
    // makes a repository and a subdirectory of it the same space rather
    // than two.
    let seat = uze_terminal::SpaceSeat {
        root: uze_application::space_root(&launch.root),
    };
    let mut stream = attach(&seat).map_err(runtime_error)?;
    let read_stream = stream.try_clone().map_err(io_error)?;
    send_request(
        &mut stream,
        &ClientRequest::Attach {
            version: PROTOCOL_VERSION,
            columns,
            rows,
            seating: match landing {
                Landing::AtLaunchDirectory => seating_at(seat),
                Landing::WhereItLeftOff => uze_terminal::Seating::WhereItLeftOff,
            },
        },
    )
    .map_err(runtime_error)?;
    // The server owns what a pane's program is told when it asks the
    // terminal what colours it is drawn in, but not what those colours are —
    // this client does. Sent on every attach, because the server outlives
    // any one client and must not keep answering with a palette nobody is
    // drawing any more.
    send_request(&mut stream, &ClientRequest::SetPalette(active_palette()))
        .map_err(runtime_error)?;
    let (events, receiver) = mpsc::channel();
    let parent = tracing::Span::current();
    thread::spawn(move || {
        let _parent = parent.enter();
        let mut reader = BufReader::new(read_stream);
        while let Ok(Some(event)) = read_event(&mut reader) {
            if events.send(event).is_err() {
                break;
            }
        }
    });
    // Keyed on the root of the space the prompt was typed in — the same
    // answer the management Overview reads against. Writes run on their
    // own thread so a keystroke never waits on the filesystem, and the
    // thread ends when `model` drops its sender at the end of this attach.
    let (prompt_recorder, recorded_prompts) =
        mpsc::channel::<(PathBuf, uze_application::PromptOrigin, String)>();
    let parent = tracing::Span::current();
    thread::spawn({
        let home = home.clone();
        move || {
            let _parent = parent.enter();
            let _span = tracing::info_span!("tui.attach_workspace").entered();
            while let Ok((root, origin, prompt)) = recorded_prompts.recv() {
                let _ = tui_application(home.clone())
                    .and_then(|app| app.workspace().record_prompt(&root, &origin, &prompt));
            }
        }
    });
    // This client's own shape, written the same way and for the same
    // reason: folding a section is a click, and a click never waits on the
    // filesystem. The thread writes the whole layout from the copy it was
    // handed, and every section of it — the modal's included — arrives
    // in the shape, so the copy is exact.
    let (layout_recorder, remembered_layouts) = mpsc::channel::<WorkspaceShape>();
    let parent = tracing::Span::current();
    let layout_writer = thread::spawn({
        let home = home.clone();
        let mut layout = layout.clone();
        move || {
            let _parent = parent.enter();
            let _span = tracing::info_span!("tui.attach_workspace").entered();
            while let Ok(shape) = remembered_layouts.recv() {
                shape.apply_to(&mut layout);
                let _ = tui_application(home.clone())
                    .and_then(|app| app.workspace().save_client_layout(&layout));
            }
        }
    });
    let mut model = WorkspaceModel {
        dirty: true,
        last_size: (columns, rows),
        sidebar_width: layout.sidebar.width,
        first_steps_collapsed: layout.first_steps.collapsed,
        first_steps_closed: layout.first_steps.closed,
        steps_taken: layout.first_steps.taken.clone(),
        disabled_extensions: super::extension_switch::disabled(),
        timeline_collapsed: layout.workspace.timeline_collapsed,
        timeline_rows: layout.workspace.timeline_rows,
        collapsed_space_roots: layout.workspace.collapsed_space_roots.clone(),
        prompt_recorder: Some(prompt_recorder),
        layout_recorder: Some(layout_recorder),
        management_layout: layout.management.clone(),
        remembered: std::mem::take(&mut memory.remembered),
        ..WorkspaceModel::default()
    };
    // A registered harness set doesn't change mid-session, so this is built
    // once per attach.
    let identities = agent_identities(home);
    let activity_spinner = ProgressBar::new_spinner();
    activity_spinner.set_draw_target(ProgressDrawTarget::hidden());
    let activity_frames = theme::frames(Symbol::StatusWorking);
    let activity_ticks: Vec<&str> = activity_frames.iter().map(String::as_str).collect();
    activity_spinner.set_style(ProgressStyle::default_spinner().tick_strings(&activity_ticks));
    let next_activity_tick = Instant::now();
    // The server's session/pane state is persistent — attaching again
    // finds the same shells exactly as they were left. But the client's
    // view of that session and its panes always starts empty (only what it
    // resolved on its own carries over, see `WorkspaceMemory`), so without
    // this wait the very first frame renders before the server's initial
    // `Snapshot` reply lands, flashing the "starting shell…"
    // placeholder and repainting the whole pane a moment later — reading
    // as a lost/reset session even though nothing server-side ever was. A
    // generous timeout is still a safety net, not the expected path: this
    // is a local Unix socket round trip, normally sub-millisecond, and the
    // alternate screen is already open (see `super::TerminalSession`), so
    // this blocks inside a continuously open uze, not a flash back to the
    // shell.
    while model.session.is_none() || model.panes.is_empty() {
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(event) => model.apply(event, &identities),
            Err(_) => break,
        }
    }
    // Attaching makes the whole workspace repaint: the client has no
    // baseline to diff against, and the panes it inherits are mid-screen.
    // None of that is an agent starting a turn, which is what made every
    // open agent spin for a few seconds each time uze was reopened.
    model.note_attach_redraw();
    // The loop's own state, gathered into one value so an event handler
    // reaches for a field instead of closing over a dozen locals — see
    // `session::Attach`.
    let mut attach = Attach {
        model,
        stream,
        home,
        identities,
        // The answer channels outlive this attach with the rest of the
        // memory: a read still running when the runtime goes away lands
        // after the client attaches again.
        channels: &memory.channels,
        spinner: activity_spinner,
        next_tick: next_activity_tick,
        asked_for_a_tab: false,
        manage_memory: manage,
        keyboard: terminal.keyboard(),
    };
    // Every way out of the loop — a quit, a runtime gone, an error — must
    // hand the model's memory back, so the loop runs inside one call whose
    // result is read only after that handover.
    // An event read while draining a burst and put back for after the
    // frame (see the drain at the end of the loop).
    let mut held: Option<Event> = None;
    let outcome: Result<WorkspaceExit> = (|| loop {
        if let Flow::Exit(exit) = attach.pump(&receiver) {
            return Ok(exit);
        }
        let size = terminal.size()?;
        let geometry = compute_layout(
            Rect::new(0, 0, size.width, size.height),
            attach.model.sidebar_width,
        );
        let viewport = Viewport {
            size,
            columns: geometry.pane.width,
            rows: geometry.pane.height,
            layout: geometry,
        };
        if (viewport.columns, viewport.rows) != attach.model.last_size {
            attach.model.last_size = (viewport.columns, viewport.rows);
            attach.model.dirty = true;
            let focused = attach.model.focused_pane();
            resize_pane(
                &mut attach.stream,
                &mut attach.model,
                focused,
                viewport.columns,
                viewport.rows,
            );
        }
        if attach.model.dirty {
            let mut hits = Vec::new();
            let mut metrics = render::FrameMetrics::default();
            let model = &attach.model;
            let identities = &attach.identities;
            terminal.draw(|frame| render(frame, model, identities, &mut hits, &mut metrics))?;
            attach.model.hits = hits;
            attach.model.marquee = metrics.marquee;
            attach.model.tree_overflow = metrics.tree_overflow;
            attach.model.remembered.tree_scroll = attach
                .model
                .remembered
                .tree_scroll
                .min(metrics.tree_overflow);
            if let Some(rendered) = metrics.code {
                attach.model.code_tree_scroll = rendered.navigator_scroll;
                attach.model.code_scrollbars = rendered;
            }
            attach.model.drawer_text = metrics.drawer.unwrap_or_default();
            attach.model.absorb_manage_frame(metrics.manage);
            attach.model.dirty = false;
        }
        attach
            .model
            .follow_extension_switch(super::extension_switch::disabled());
        if attach
            .model
            .take_ring(super::chime::current(), Instant::now())
            | super::chime::take_preview()
        {
            terminal.ring();
        }
        if let Some(text) = attach.model.clipboard.take() {
            terminal.emit(&selection::osc52(&text));
        }
        // Everything already waiting is handled before the next frame: a
        // burst of keys or wheel ticks is one frame, not one per event.
        let mut timeout = POLL;
        let mut handled = 0;
        while handled < EVENTS_PER_FRAME {
            let event = match held.take() {
                Some(event) => event,
                None => {
                    if !event::poll(timeout).map_err(io_error)? {
                        break;
                    }
                    event::read().map_err(io_error)?
                }
            };
            timeout = Duration::ZERO;
            let admitted = burst_admits(handled, &event, attach.model.dirty);
            if admitted == Admit::Hold {
                held = Some(event);
                break;
            }
            handled += 1;
            if let Flow::Exit(exit) = attach.handle(event, &viewport) {
                return Ok(exit);
            }
            if admitted == Admit::HandleAndDraw {
                break;
            }
        }
    })();
    // The modal is closed on the way out so what it arranged is in the
    // shape, and the shape is handed back so the next attach — or the
    // next run — opens on it.
    attach.close_manage();
    attach.model.shape().apply_to(layout);
    // The recorder may still be writing an older shape, and `super::run`
    // writes the final one as soon as this returns: the older write must
    // not land after it.
    drop(attach.model.layout_recorder.take());
    let _ = layout_writer.join();
    memory.remembered = attach.model.remembered;
    outcome
}

/// What a burst of input does with the next event it reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Admit {
    Handle,
    /// Handled, and then the frame is drawn before anything else is.
    HandleAndDraw,
    /// Put back for after the frame.
    Hold,
}

/// A resize ends a burst, since what follows it is measured against a
/// viewport only the next turn computes; a pointer event arriving after
/// something already changed the screen waits for it, since it is aimed
/// at what the next frame draws and would be resolved against the hits of
/// the last one.
fn burst_admits(handled: usize, event: &Event, dirty: bool) -> Admit {
    match event {
        Event::Mouse(_) if handled > 0 && dirty => Admit::Hold,
        Event::Resize(..) => Admit::HandleAndDraw,
        _ => Admit::Handle,
    }
}

/// The client's layout as it stands: the sidebar column, the workspace's
/// own section, the first-steps list, and the management modal's. Sent to
/// the recorder thread on every change and handed back to `super::run`
/// when the attach ends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceShape {
    pub(crate) sidebar: uze_application::SidebarLayout,
    pub(crate) workspace: uze_application::WorkspaceLayout,
    pub(crate) first_steps: uze_application::FirstStepsLayout,
    pub(crate) management: uze_application::ManagementLayout,
}

impl WorkspaceShape {
    fn apply_to(self, layout: &mut uze_application::ClientLayout) {
        layout.sidebar = self.sidebar;
        layout.workspace = self.workspace;
        layout.first_steps = self.first_steps;
        layout.management = self.management;
    }
}

/// `pub(super)` (not private) so `uze_extensions::code` — a crate
/// this one depends on, not a child module of `orchestrator` — can
/// construct `ExtensionHit`s from its own render function; `Extension`
/// below wraps them into the same `hits` vec every other overlay already
/// shares, rather than this workspace client threading a second, parallel
/// hit-testing vec just for one extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkspaceHit {
    /// One of the work modal's projects, by its place in the sidebar.
    WorkProject(usize),
    /// One row of the project in front, by its index there.
    WorkRow(usize),
    /// One of the work modal's buttons, by the action it performs.
    WorkAction(Action),
    /// One of the answers to the question the work modal is asking:
    /// `true` for going ahead.
    WorkAnswer(bool),
    /// The mark on the work modal's title that closes it.
    WorkClose,
    /// Anywhere else on the work modal: answers nothing, and is not a
    /// click outside it.
    WorkBody,
    SelectTab(TabId),
    CloseTab(TabId),
    NewTab,
    /// Opens the agent picker (`WorkspaceModel::agent_picker`) — the
    /// "new" on the selected space's header in the sidebar, creating a
    /// new agent tab inside that space.
    NewAgentMenu,
    /// One row of the open agent picker, by index into its `options`.
    PickAgent(usize),
    /// The agent picker's only row when no harness is set up — opens the
    /// modal on Integrations, where one is.
    SetUpAgent,
    /// A space's header row in the sidebar — click selects it (switching
    /// which space's tabs the tab strip and pane show).
    SelectSpace(SpaceId),
    /// The fold in front of a space's name — minimizes the space to its
    /// header, or opens it again (see `Remembered::collapsed_spaces`).
    ToggleSpaceCollapsed(SpaceId),
    /// The "resume" behind an agent row whose checkout was removed from
    /// under it (see `WorkspaceModel::lost_checkouts`) — opens the agent
    /// picker to put the task it was running back into a slot of its own.
    ResumeLostCheckout(TabId),
    /// One row of the open [`ContextMenu`], by index into its `items` —
    /// generic over whatever action that row is, same pattern
    /// [`WorkspaceHit::PickAgent`] uses for the agent picker.
    ContextMenuAction(usize),
    /// The sidebar's "+ space" control — opens the root picker
    /// ([`WorkspaceModel::root_picker`]), since a space is born from a
    /// directory and that directory is chosen, not typed blind.
    NewSpace,
    /// One row of the open root picker, by index into its current matches
    /// — same pattern [`WorkspaceHit::PickAgent`] uses for the agent
    /// picker.
    PickSpaceRoot(usize),
    /// The tab strip's right-corner button — opens the Git extension's
    /// changes of the active tab's checkout — the code surface
    /// (`WorkspaceModel::code`), opened on its diff.
    OpenChanges,
    /// The tab strip's code button — the same surface, opened on the
    /// checkout's file tree.
    ///
    /// Both are drawn unconditionally. A control that comes and goes with
    /// the work is one the operator has to go looking for; what the
    /// changes chip *says* still varies, which is where that signal
    /// lives now.
    OpenFiles,
    /// The tab strip's architect button, beside the code one.
    OpenArchitect,
    /// The tab strip's spec button, first of the three.
    OpenSpec,
    /// Opens contextual support details for the selected agent tab.
    OpenAgentSupport(Rect),
    /// A prompt listed in the agent drawer, by position in its list.
    DrawerPrompt(usize),
    /// Whose prompts the agent drawer lists.
    DrawerScope(PromptScope),
    /// The rest of the drawer, so a click inside it does not close it.
    DrawerBody,
    /// The task mark on a sidebar agent row — opens the catalog of what
    /// every glyph in both columns means, anchored to the mark. A status
    /// column is a wordless vocabulary; this is where it is written down.
    OpenStatusCatalog(Rect),
    /// The tab strip's delivery button — delivers the selected tab's task
    /// the way the project's completion says. Present only when the task
    /// is deliverable.
    Deliver(TabId),
    /// A hit the open extension's own render pass produced (a file row, a
    /// worktree header, its tree/diff resize handle, its close button —
    /// see `uze_extensions::ExtensionHit`), wrapped instead of given its
    /// own `WorkspaceHit` variant per extension. The one Git extension
    /// today owns every `ExtensionHit` variant that exists; a second
    /// extension adds to that enum, not to this one.
    Extension(ExtensionHit),
    /// The sidebar header's trailing control — opens the management
    /// modal (`WorkspaceModel::manage`), the same as the `SwitchMode`
    /// action does.
    OpenManage,
    /// The open management modal itself. A click inside it is the
    /// modal's to answer against its own hit list; one outside closes it.
    ManageSurface,
    /// One toast's own offer, by its place in the stack as drawn.
    ToastAction(usize),
    /// The mark that puts one toast away.
    DismissToast(usize),
    /// The mark on the modal's title that closes it.
    CloseManage,
    /// The first-steps section's header, which folds it.
    ToggleFirstSteps,
    /// The mark on that header, which puts the section away for good.
    CloseFirstSteps,
    /// The release notice's row, which opens that release's notes.
    OpenReleaseNotes,
    /// The mark on the notice's header, which puts it away.
    DismissRelease,
    /// One entry of the sidebar's quick strip — performed exactly as the
    /// keyboard performs it, which is why it carries the action rather
    /// than naming a surface: a control that took its own path to the
    /// same place is a second implementation to keep agreeing.
    QuickAction(Action),
    /// One row of the open index, by position in it.
    ActionIndexEntry(usize),
    /// The release notes modal's own area: a click on it is reading, and
    /// only one outside it closes the modal.
    ReleaseNotesBody,
    ResizeSidebar,
}

/// Resolved entirely through the generic `IntegrationPort` contract
/// (`.id()`/`.display_name()`/`.aliases()`) — never a hardcoded vendor list,
/// which `src/` is not allowed to hold (see
/// `tests/integrations/identity.rs::cli_and_tui_never_names_a_vendor_harness`).
/// A registry that fails to construct (rare — see `src/shim.rs`'s identical
/// `.ok()` fallback) just yields no identities rather than failing the
/// whole workspace session.
fn agent_identities(home: &UzeHome) -> Vec<AgentIdentity> {
    tui_application(home.clone())
        .map(|app| app.workspace().agent_identities())
        .unwrap_or_default()
}

/// The harnesses the agent picker offers — one row per configured
/// [`AgentIdentity`], `command` set to the program that identity says an
/// agent of it is launched by.
fn agent_options(home: &UzeHome) -> Vec<AgentOption> {
    agent_identities(home)
        .into_iter()
        .filter(|identity| identity.configured)
        .map(|identity| AgentOption {
            display_name: identity.display_name.to_owned(),
            integration: identity.integration.to_owned(),
            command: vec![identity.launch.to_string_lossy().into_owned()],
            continuity_gap: identity.continuity_gap,
        })
        .collect()
}

/// The recognized agent, if any, running in `tab`'s pane — matched against
/// `identities` by the live foreground process name (a shim-launched
/// process reports its invoked alias there via `UZE_SHIM_NAME`, not its raw
/// `comm` — see `uze_terminal::PaneRuntime::foreground_status`). Returns
/// the harness's short binary/alias name (`claude`, `codex`, …) — what
/// decides whether a tab lists under "agents" or "shell" at all.
fn agent_identity_for_tab<'a>(identities: &'a [AgentIdentity], tab: &Tab) -> Option<&'a str> {
    agent_for_tab(identities, tab).map(|identity| identity.binary)
}

/// The harness running in `tab`, as [`agent_identity_for_tab`] recognizes
/// it — the whole identity, for a caller that names it to a person.
fn agent_for_tab<'a>(identities: &'a [AgentIdentity], tab: &Tab) -> Option<&'a AgentIdentity> {
    identities
        .iter()
        .find(|identity| tab.pane.process.eq_ignore_ascii_case(identity.binary))
}

/// What the sidebar shows beside one agent tab. These four states are the
/// whole vocabulary, and they are mutually exclusive by the precedence
/// encoded in [`WorkspaceModel::agent_tab_status`] — the single place that
/// decides, so the glyph can never disagree with the model that produced
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AgentTabStatus {
    /// The agent is producing output right now.
    Working,
    /// The agent finished while the user was looking somewhere else, and
    /// the user has not opened the tab since.
    Completed,
    /// The tab the user is on, with nothing in flight.
    Selected,
    /// A quiet agent tab the user is not on.
    Idle,
}

impl AgentTabStatus {
    /// The indicator column, including its trailing space. `tick` only
    /// matters for [`AgentTabStatus::Working`], whose glyph animates.
    pub(super) fn glyph(self, tick: usize) -> String {
        match self {
            AgentTabStatus::Working => format!("{} ", agent_activity_frame(tick)),
            AgentTabStatus::Completed => format!("{} ", theme::glyph(Symbol::StatusCompleted)),
            AgentTabStatus::Selected => format!("{} ", theme::glyph(Symbol::StatusSelected)),
            AgentTabStatus::Idle => format!("{} ", theme::glyph(Symbol::StatusIdle)),
        }
    }

    /// Idle is the only state drawn faint: the other three all report
    /// something the user asked for or needs to notice.
    pub(super) fn color(self) -> Color {
        match self {
            AgentTabStatus::Idle => theme::color(Token::TextFaint),
            _ => theme::color(Token::Accent),
        }
    }
}

/// How long damage in a pane still reads as something the client asked
/// for rather than the agent working: the deadline now, and the furthest
/// it may ever be pushed to.
///
/// Two instants because the window has to answer two things at once — a
/// redraw is over when the painting stops, and no redraw runs forever.
#[derive(Clone, Copy, Debug)]
struct EchoWindow {
    until: Instant,
    cap: Instant,
}

/// One agent pane's recent repaints and the deadline its busy state runs
/// to. Both halves are load-bearing: the repaints are the evidence that
/// the pane keeps a beat rather than having blinked once, and the
/// deadline is what carries "working" across the gaps in that beat.
#[derive(Debug, Default)]
struct AgentActivity {
    repaints: VecDeque<Instant>,
    working_until: Option<Instant>,
}

impl AgentActivity {
    fn is_working(&self) -> bool {
        self.working_until.is_some()
    }

    /// Records one repaint and reports whether the pane is now keeping a
    /// beat — the shape a harness paints in while a turn is running (see
    /// [`AGENT_BEATS`]).
    fn note_repaint(&mut self, now: Instant) -> bool {
        self.forget_repaints_before(now);
        self.repaints.push_back(now);
        let Some(oldest) = self.repaints.front() else {
            return false;
        };
        let spread = now.duration_since(*oldest);
        self.repaints.len() >= AGENT_BEATS && spread >= AGENT_BEAT_SPAN
    }

    /// Drops the deadline once it has passed, reporting whether this call
    /// is the one that ended the pane's turn, and forgets repaints too old
    /// to still be evidence of animation.
    fn expire(&mut self, now: Instant) -> bool {
        let ended = self.working_until.is_some_and(|deadline| now >= deadline);
        if ended {
            self.working_until = None;
        }
        self.forget_repaints_before(now);
        ended
    }

    fn forget_repaints_before(&mut self, now: Instant) {
        while self
            .repaints
            .front()
            .is_some_and(|at| now.duration_since(*at) >= AGENT_BEAT_WINDOW)
        {
            self.repaints.pop_front();
        }
    }
}

/// Resizes one pane and records that whatever it repaints next is the
/// harness reacting to us. Every resize path goes through here: a pane
/// redrawing on our command is the client's own doing, and counting it as
/// activity is what made every open agent spin for a moment whenever the
/// workspace was opened, a tab selected, or the terminal resized.
fn resize_pane<W: io::Write>(
    stream: &mut W,
    model: &mut WorkspaceModel,
    pane: PaneId,
    columns: u16,
    rows: u16,
) {
    let _ = send_request(
        stream,
        &ClientRequest::Resize {
            pane,
            columns,
            rows,
        },
    );
    model.note_pane_redraw(pane);
}

/// Whether this damage describes an agent painting a frame at all. Damage
/// that redescribes the entire grid is the server having no comparable
/// baseline to diff against — the first push after an attach, or a resize —
/// and counting those is what lit every open agent up as busy for a few
/// seconds whenever the workspace was reopened.
fn is_incremental_repaint(damage: &PaneDamage) -> bool {
    let grid = usize::from(damage.columns) * usize::from(damage.rows);
    !damage.changed.is_empty() && damage.changed.len() < grid
}

fn workspace_has_active_agent_operation(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
) -> bool {
    model.tabs().any(|tab| {
        agent_identity_for_tab(identities, tab).is_some() && model.agent_is_working(tab.pane.id)
    })
}

/// The agent the workspace is *about*, paired with that agent's own
/// pane's working directory.
///
/// The context agent ([`context_agent`]), never whichever tab is
/// selected: a shell opened alongside an agent is part of that agent's
/// context — the whole reason the strip it sits on is the agent's own —
/// so stepping into one must not take the "✦" away and must not change
/// which agent the support dropdown is about. Reading the selected tab
/// instead made the badge blink out on every hop to a shell and back.
///
/// `None` is the one context with no agent to support: the space's own
/// row, which holds shells and nothing else.
fn selected_agent_context(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
) -> Option<SupportKey> {
    let session = model.session.as_ref()?;
    let agent = context_agent(model, identities)?;
    let tab = session
        .selected_space()
        .tabs
        .iter()
        .find(|tab| tab.id == agent)?;
    let identity = agent_for_tab(identities, tab)?;
    Some((identity.integration.to_owned(), tab.pane.cwd.clone()))
}

/// The drawer for the agent in front: the same agent
/// [`selected_agent_context`] resolves, with what its prompts are kept
/// under. Opens on the agent's own prompts when UZE launched it, and on the
/// space's when nothing identifies it.
fn selected_agent_drawer(
    model: &WorkspaceModel,
    identities: &[AgentIdentity],
) -> Option<AgentSupportDropdown> {
    let key = selected_agent_context(model, identities)?;
    let space = model.session.as_ref()?.selected_space();
    let tab = context_agent(model, identities)
        .and_then(|tab| space.tabs.iter().find(|candidate| candidate.id == tab))?;
    let agent = launched_agent_id(tab).map(str::to_owned);
    // The tab the operator last chose, when this agent can show it.
    let scope = match model.remembered.drawer_scope {
        Some(PromptScope::Agent) | None if agent.is_some() => PromptScope::Agent,
        _ => PromptScope::Space,
    };
    let support = model
        .remembered
        .agent_support
        .as_ref()
        .filter(|resolution| resolution.key == key)
        .and_then(|resolution| resolution.support.clone());
    Some(AgentSupportDropdown {
        support,
        scope,
        agent,
        space_root: space.root.clone(),
        path: crate::ui::display_project_path(&key.1),
        key,
        selected: 0,
        clearing: false,
    })
}

/// Every live agent pane as `(integration, directory)` — the same pair
/// [`selected_agent_context`] resolves, for every tab rather than for the
/// one the workspace is about, since an agent nobody is looking at is
/// exactly the one whose conversation would otherwise go unrecorded.
fn agent_contexts(model: &WorkspaceModel, identities: &[AgentIdentity]) -> Vec<LaunchedAgent> {
    model
        .tabs()
        .filter_map(|tab| {
            let integration = agent_for_tab(identities, tab)?.integration;
            let id = launched_agent_id(tab)?.to_owned();
            // The directory as it was given, not the kernel's note about
            // what became of it: a removed checkout is still where the
            // record says the agent is.
            let cwd = named_checkout(&tab.pane.cwd);
            Some(LaunchedAgent {
                integration: integration.to_owned(),
                id,
                cwd,
            })
        })
        .collect()
}

/// The agent a tab was launched for, as the server echoes the launch: the
/// identity the client stamped when it created the tab. `None` for a
/// shell, and for a tab whose agent exited and was respawned as one.
fn launched_agent_id(tab: &Tab) -> Option<&str> {
    tab.env
        .iter()
        .find(|(name, _)| name == uze_terminal::launch::AGENT_IDENTITY_VARIABLE)
        .map(|(_, id)| id.as_str())
}

/// What the header is saying: the work in flight, and nothing else.
///
/// One line, one slot, no clock. It is about something that is happening
/// now, so there is only ever one of them and it goes when the work does
/// — everything that *happened* is a toast, which stacks and retires
/// itself (see [`RaisedToast`]).
/// A short message on screen, and — for one about a single task — enough
/// to tell whether that task is the one currently in front of the
/// operator.
struct Notice {
    text: String,
}

/// An outcome raised for the reader, with the clock that retires it.
///
/// Kept apart from [`Notice`] on purpose: a notice is about work *in
/// flight* and lives in the header for as long as that takes, while this
/// is about work that finished and leaves on its own. Collapsing them
/// would mean one of the two rules — "no deadline" and "gone in six
/// seconds" — winning over a case it is wrong for.
struct RaisedToast {
    kind: crate::ui::widget::ToastKind,
    text: String,
    /// The line under the title: which task, which remote, what the reason
    /// was.
    detail: String,
    raised: Instant,
    /// What the reader may do about it, and what that means. `None` for an
    /// outcome there is nothing to do about.
    offer: Option<(String, WorkspaceHit)>,
    /// Whether it leaves on a clock. An outcome the reader has to answer
    /// stays until they do.
    stays: bool,
}

/// The active notice as the header draws it, in the message zone left of
/// the actions — text and whether it is still running, and nothing about
/// placement: what the workspace says never decides where a button sits.
pub(super) struct NoticeChip {
    /// Already elided to [`NOTICE_WIDTH`], and already carrying the label
    /// of the task it is about when that is not the task on screen.
    pub(super) text: String,
    pub(super) busy: bool,
}

/// One background read's answer channel, kept as a pair so the two ends
/// live and die together.
struct Answers<T> {
    sender: mpsc::Sender<T>,
    receiver: mpsc::Receiver<T>,
}

impl<T> Default for Answers<T> {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }
}

/// What the workspace client keeps between attaches.
///
/// Owned by `super::run` for the life of the process, not by one call to
/// [`attach_workspace`]: an attach after the runtime went away is a fresh
/// attach, and everything this client had resolved on its own — every
/// task, branch, badge and agent status in the sidebar — used to leave
/// with the model that held it. The next attach then redrew
/// every agent row from its bare working directory and filled the captions
/// in again one answer at a time, reading as the whole workspace being
/// resolved from scratch. What comes from the server (the session, the
/// pane grids) is deliberately *not* here: the attach re-reads it, and a
/// stale copy would be worse than a short wait for the real one. Nor is
/// the client's own shape — the sidebar's width, the timeline's fold —
/// which is a preference rather than a resolution, and lives in the
/// `uze_application::ClientLayout` both surfaces share.
#[derive(Default)]
pub(crate) struct WorkspaceMemory {
    /// The model's own remembered half, moved into the attach's model and
    /// handed back when it ends.
    remembered: Remembered,
    channels: Channels,
}

/// The channels background reads answer on, one per kind of answer.
///
/// Kept with the memory rather than with one attach, so a read still
/// running when the user leaves lands after they come back instead of
/// vanishing with a dropped receiver — which would also have left its key
/// reserved in the pending sets forever.
#[derive(Default)]
struct Channels {
    support: Answers<SupportResolution>,
    /// The agent drawer's prompt history, read and cleared.
    prompts: Answers<PromptHistoryResolution>,
    tasks: Answers<WorkResolution>,
    deliveries: Answers<DeliveryResolution>,
    /// Finishing and discarding a preserved task. Off-thread for the same
    /// reason a delivery is: a discard is `git worktree remove` and a
    /// recursive directory removal.
    mutations: Answers<MutationResolution>,
    /// The badge and the timeline behind it. Git is read off-thread like
    /// everything else expensive here; it was the last subsystem still
    /// answering inline on the render path.
    git: Answers<GitResolution>,
    commit_details: Answers<CommitDetailResolution>,
    release_notes: Answers<ReleaseNotesResolution>,
    code_changes: Answers<ChangesResolution>,
    code_diffs: Answers<DiffResolution>,
    /// The surface's file reads and writes, off-thread for the same
    /// reason its changes are.
    code_files: Answers<FileResolution>,
    /// Preserved work across the machine. Off-thread like every other
    /// read: it opens `$UZE_HOME` and walks every project UZE has
    /// recorded, which is not something a frame may wait on.
    preserved: Answers<PreservedResolution>,
    /// Every checkout of a project, measured: a walk of every directory.
    checkouts: Answers<CheckoutsResolution>,
    /// Adopting, removing and cleaning up checkouts.
    checkout_changes: Answers<CheckoutChangeResolution>,
    occupancy: Answers<OccupancyResolution>,
    placements: Answers<PlacementResolution>,
    artifacts: Answers<ArtifactsResolution>,
    spec: Answers<SpecResolution>,
    spec_summaries: Answers<SpecSummaryResolution>,
    /// The code surface's map, measured once per checkout it is opened on.
    code_measures: Answers<MeasureResolution>,
    /// Keeping each project's `AGENTS.md` workspace section in step.
    policy_regions: Answers<PolicyRegionResolution>,
    /// A project's gates this machine cannot run, read where it opens.
    unspelled_gates: Answers<UnspelledGates>,
    /// The names a harness launched through a shim runs under, asked once.
    launchers: Answers<Vec<String>>,
}

/// The half of [`WorkspaceModel`] that outlives one attach. Everything
/// else belongs to one attach: the server's view of the session,
/// presentation state such as open overlays and drags, and per-attach
/// transients like echo windows and hit rects.
#[derive(Default)]
struct Remembered {
    /// Per-agent-pane repaint evidence and busy deadline. A pane starts
    /// working either because the user submitted a line into it or because
    /// it started animating on its own — an agent that resumes work with
    /// nothing typed (a hook, a queued turn, a subagent reporting back) is
    /// working just as much as one answering a prompt. Sustained repainting
    /// extends the deadline; a pane that only blinks does not, which is why
    /// merely having an agent process open — or reattaching to one — is
    /// deliberately not activity.
    agent_activity: BTreeMap<PaneId, AgentActivity>,
    /// Agent panes that stopped working while the user was looking
    /// somewhere else. The sidebar keeps their check visible until the tab
    /// is actually on screen, making completion discoverable without
    /// leaving a stale busy spinner.
    completed_agent_panes: BTreeSet<PaneId>,
    /// The most recently resolved agent support answer, tagged with the
    /// `(harness, cwd)` it answers — never assumed to apply to a different
    /// selection.
    agent_support: Option<SupportResolution>,
    /// The key a background resolution is currently in flight for, so the
    /// per-frame check cannot queue the same read repeatedly.
    agent_support_pending: Option<SupportKey>,
    /// The last prompt history the drawer read, tagged with its space.
    drawer_prompts: Option<PromptHistoryResolution>,
    /// Whose prompts the drawer listed last, so it opens on them again.
    drawer_scope: Option<PromptScope>,
    /// Cached Git summary for the selected agent/shell tab's live cwd.
    /// Stored client-side because it is display chrome, not terminal session
    /// state that belongs in `uze-terminal`.
    git_badge: Option<GitBadge>,
    /// The checkout a background Git read is out for, so the workspace
    /// asks once rather than once per frame — see [`spawn_git_read`].
    git_pending: Option<PathBuf>,
    /// The sidebar's spec summary for the checkout in front, and the one
    /// being read — kept like the badge, so a tab switch shows the last
    /// answer until the new one lands rather than an empty column.
    spec_summary: Option<SpecSummaryState>,
    spec_summary_pending: Option<PathBuf>,
    /// How long the last of those reads took (see [`paced`]).
    git_took: Duration,
    /// Per-pane reconstruction of the line being typed, flushed on Enter.
    prompt_buffers: BTreeMap<PaneId, PromptBuffer>,
    /// Where the viewer was on each checkout's code surface, keyed by the
    /// checkout — reviewing one agent's work and coming back to another's
    /// is two places, not one. Kept here rather than in the client layout
    /// because a path a file once had is a claim about a tree that is
    /// being rewritten while nobody is looking; within a session it is
    /// worth returning to, and across runs it is worth nothing.
    code_places: BTreeMap<PathBuf, code::CodePlace>,
    /// Where the viewer was on each checkout's architect surface, keyed
    /// the same way and for the same reason: a drawing is walked into —
    /// a level entered, a box selected, the board moved to it — and a
    /// surface that forgets all of that on the way out is one nobody
    /// leaves to check something.
    architect_places: BTreeMap<PathBuf, architect::ArchitectPlace>,
    /// The same for the spec surface: which change, and which of its
    /// documents.
    spec_places: BTreeMap<PathBuf, spec::SpecPlace>,
    /// Preserved work across the machine, as last swept. The last good
    /// answer stays drawn while a new one is in flight, so opening the
    /// list never shows an empty one it is about to fill.
    preserved_work: Vec<uze_application::PreservedWork>,
    /// Whether a sweep is out, so the list asks once rather than once per
    /// frame.
    preserved_pending: bool,
    /// How many checkouts reads were asked for: only the answer to a
    /// project's last one is drawn, so a read that began before a change
    /// landed never replaces the one that began after it.
    checkouts_asked: u64,
    /// Whether a change to the checkouts is out: a clean-up walks and
    /// removes several directories, and a second one started beside it
    /// would inspect what the first is removing.
    checkout_change_pending: bool,
    /// Every repository's tasks as last evaluated, keyed by its primary
    /// checkout. Display state: the truth is Git and the task store.
    tasks: BTreeMap<PathBuf, Vec<AgentView>>,
    /// The branch checked out at each evaluation key (see
    /// [`evaluation_key`]) — the primary's for every slot of a repository,
    /// a directory's own outside any slot. Read for an agent outside any
    /// slot, whose caption has no task to take a branch from.
    branches: BTreeMap<PathBuf, String>,
    /// The delivery target of the repository at each evaluation key —
    /// what the timeline marks a commit as ahead of.
    targets: BTreeMap<PathBuf, String>,
    /// How the branch in [`Self::branches`] stands against its upstream,
    /// under the same key, for the keys where that branch is the delivery
    /// target and tracks something. Read for an agent outside any slot:
    /// the operator's own tree is the one a pull or a push is due on.
    upstream_syncs: BTreeMap<PathBuf, UpstreamSync>,
    /// Repositories an evaluation is in flight for, so a quiet pane and
    /// the clock cannot queue the same read twice.
    task_eval_pending: BTreeSet<PathBuf>,
    /// Repositories asked about again while their read was in flight, by
    /// the directory asked from. That read may have started before the
    /// push or pull the second question was about, so its answer is not
    /// the last word: the question is asked once more when it lands.
    task_eval_again: BTreeMap<PathBuf, PathBuf>,
    /// The directories an evaluation has answered for at least once,
    /// whatever it found — a directory that is no repository is answered
    /// too, and must not be asked again on every frame.
    evaluated: BTreeSet<PathBuf>,
    /// Shell tabs told to take an agent label, and the label each was
    /// told, until the session confirms it — so two updates arriving
    /// before the rename lands do not ask twice.
    label_adoptions: BTreeMap<TabId, String>,
    /// What [`adopt_task_names`] last put on each tab. Its own ledger
    /// rather than a second use of `label_adoptions`: that one is retained
    /// against a *shell* label still being generated, which would drop
    /// these the moment they land.
    task_name_adoptions: BTreeMap<TabId, String>,
    last_task_refresh: Option<Instant>,
    /// Tasks a delivery is in flight for.
    delivery_pending: BTreeSet<String>,
    /// Tasks a finish or a discard is in flight for. Its own set rather
    /// than a flag: what a second Enter must not start is a second
    /// removal of *this* task, and the answer arrives keyed by the task
    /// it was asked about.
    task_mutation_pending: BTreeSet<String>,
    /// A one-line message and when it appeared.
    notice: Option<Notice>,
    /// Outcomes waiting to be read, newest last. A queue rather than one
    /// slot, because two things finishing at once is the ordinary case and
    /// the notice's single slot loses one of them.
    toasts: VecDeque<RaisedToast>,
    /// The `AGENTS.md` files already reported as holding a workspace section
    /// edited by hand, so the report is made once a session rather than on
    /// every refresh.
    policy_region_reported: BTreeSet<PathBuf>,
    /// The projects already said to have a gate this machine cannot run,
    /// so it is said once a session.
    unspelled_gates_reported: BTreeSet<PathBuf>,
    /// The names a harness launched through the workspace's shim runs
    /// under, once the registry has answered, and whether it was asked.
    launchers: Option<Vec<String>>,
    launchers_asked: bool,
    /// Panes already told their harness bypassed the shim, so it is said
    /// once rather than on every status tick.
    bypass_reported: BTreeSet<uze_terminal::PaneId>,
    /// The checkout each open pane was first seen in — a pane's slot does
    /// not change when it `cd`s. A directory fact, and the only thing it
    /// answers is slot occupancy; which agent a pane is for is what the
    /// session's tab says (see [`launched_agent_id`]).
    pane_checkouts: BTreeMap<PaneId, PathBuf>,
    /// The slot directories a pane still holds. A checkout that leaves this
    /// set lost its last pane, which is what ends the task running there.
    occupied_checkouts: BTreeSet<PathBuf>,
    /// The agents a tab still echoes. An agent that leaves this set lost
    /// its last tab, which is what ends an agent in the root: it holds no
    /// checkout, so nothing in `occupied_checkouts` would say so.
    echoed_agents: BTreeSet<String>,
    /// Panes whose checkout is gone from under them — removed outside UZE
    /// while the agent ran. The process is still there, standing in a
    /// directory that no longer exists; its row says so instead of
    /// showing the kernel's own `(deleted)` path.
    lost_checkouts: BTreeSet<PaneId>,
    /// Whether the sweep for tasks nobody's session restored has run.
    slots_swept: bool,
    /// Which tab each agent was last left on: the agent's own tab, or one
    /// of the shells opened beside it in its strip.
    ///
    /// A space holds one `selected_tab`, so walking from agent A to agent
    /// B and back used to land on A's own tab — the shell the user had
    /// been working in beside it was forgotten the moment they looked at
    /// something else. This is what puts them back where they were.
    /// Rebuilt from every session update rather than maintained by hand,
    /// so an agent that closes takes its entry with it.
    strip_selection: BTreeMap<TabId, TabId>,
    /// The first row of the space tree the sidebar shows — where the wheel
    /// over it has scrolled to. Held to `tree_overflow`, so the tree can
    /// never be scrolled off its own foot.
    tree_scroll: u16,
}

#[derive(Default)]
struct WorkspaceModel {
    /// What this client resolved on its own and keeps across attaches
    /// (see [`WorkspaceMemory`]).
    remembered: Remembered,
    session: Option<Session>,
    panes: BTreeMap<PaneId, PaneSnapshot>,
    last_size: (u16, u16),
    error: Option<String>,
    tick: usize,
    /// Until when each pane's own repaints are the echo of input we
    /// forwarded to it, rather than the agent working (see
    /// [`AGENT_ECHO_GRACE`] and [`AGENT_PASTE_GRACE`]).
    input_echo_until: BTreeMap<PaneId, EchoWindow>,
    hits: Vec<(Rect, WorkspaceHit)>,
    /// Whether the last frame drew a caption sliding under the pointer
    /// (see `render::FrameMetrics::marquee`). The clock turns for it the
    /// way it turns for a spinner — without this the caption moved one
    /// column and then stopped, because nothing else asked for the next
    /// frame.
    marquee: bool,
    /// The piece of chrome the pointer is over, read from the same hit
    /// list a click reads. Only ever set while no modal is open: what
    /// sits under an overlay is not what the pointer is on.
    hovered: Option<WorkspaceHit>,
    /// The last control pressed, and when. A press flashes for
    /// [`PRESS_FLASH`] so pressing is visible in itself — most of these
    /// buttons answer somewhere else on the frame, or after a round trip,
    /// and a control that looks identical the instant after it is pressed
    /// is one the operator presses again.
    pressed: Option<(WorkspaceHit, Instant)>,
    /// Set whenever applying an event (or a resize) changes what should be
    /// on screen; the input loop only redraws when this is true. Redrawing
    /// unconditionally at the input-poll rate was the other half of the
    /// workspace client's earlier CPU/latency problem (see [`POLL`]):
    /// ratatui re-copying and diffing a full grid ~60x/sec regardless of
    /// whether anything changed.
    dirty: bool,
    /// User-dragged sidebar width; `None` falls back to `sidebar_width_for`.
    /// Client-local presentation state — never sent to the server.
    sidebar_width: Option<u16>,
    /// What the sidebar's foot says about releases, as of `release_revision`.
    release: Option<crate::self_update::Notice>,
    release_revision: u64,
    /// The spaces minimized to their header row, by root, flipped by the
    /// fold in front of the name. A view of the column, not of the work:
    /// the space's agents keep running and the chords still reach them.
    /// A preference rather than a resolution, so it is kept in the shared
    /// `uze_application::ClientLayout` — the sidebar's width and the
    /// timeline's fold are the same kind of thing, and a fold the
    /// operator has to make again on every launch is not remembered at
    /// all.
    collapsed_space_roots: BTreeSet<PathBuf>,
    /// Whether the sidebar's first-steps section is folded to its header.
    first_steps_collapsed: bool,
    /// Whether the sidebar's spec section is open. Folded until asked,
    /// and not remembered across launches yet: the other two sections'
    /// folds live in a record, and a record grows by a shape.
    spec_summary_open: bool,
    /// Whether it has been put away for good, which is offered only once
    /// every step has been taken.
    first_steps_closed: bool,
    /// The steps already taken, by action name — shared with the
    /// management modal, because it is one list drawn at the foot of both
    /// sidebars and a step taken in one surface is taken.
    steps_taken: std::collections::BTreeSet<String>,
    /// The extensions switched off, as this client last took them from
    /// `extension_switch`. Everything that offers an extension — a key, a
    /// button, an entry in the index, a sidebar section, the reads that
    /// feed one — asks this, so switching one off is one place changing.
    disabled_extensions: std::collections::BTreeSet<String>,
    dragging_sidebar: bool,
    /// What's being renamed (a tab or a space) and its live edit buffer.
    /// While set, all keyboard input edits this instead of reaching the
    /// pane, and any click elsewhere cancels it (same "click outside
    /// discards" rule the management modal's dialogs use).
    renaming: Option<(RenameTarget, RenameBuffer)>,
    /// Open state of the sidebar's "+ space" prompt — the directory the next
    /// space is born from, chosen from a live listing that narrows as it is
    /// typed. Same "click outside discards" rule as `renaming`.
    root_picker: Option<RootPicker>,
    last_click: Option<(std::time::Instant, WorkspaceHit)>,
    /// Open state of the "+ new agent" popup; `None` when closed. Same
    /// "click outside discards" rule as `renaming`.
    agent_picker: Option<AgentPicker>,
    /// Contextual support information for the active harness tab.
    support_dropdown: Option<AgentSupportDropdown>,
    /// The open status catalog and the glyph it hangs off — the legend for
    /// the two status columns a sidebar agent row carries. Informational
    /// and anchored, like `support_dropdown`: any click or key dismisses
    /// it. Just the anchor, since the catalog itself is generated from the
    /// same tables the sidebar draws with and holds no state of its own.
    status_catalog: Option<Rect>,
    /// Open state of the right-click close-confirmation popup; `None` when
    /// closed. Same "click outside discards" rule as `renaming`.
    context_menu: Option<ContextMenu>,
    /// Open state of the code surface; `None` when closed. Drawn where the
    /// pane is, with the sidebar and the strip live around it — so unlike
    /// `renaming`/`agent_picker`/`context_menu` a click outside it is not a
    /// dismissal but a click on whatever it landed on. `Esc`, either
    /// shortcut that opens it, or putting another tab in front closes it.
    code: Option<code::CodeView>,
    /// Open state of the architect surface. It borrows the code surface's
    /// frame — the navigator width, its scroll, the scrollbars — because
    /// the two are never open together and the frame is the host's, not
    /// either extension's.
    architect: Option<architect::ArchitectView>,
    /// Open state of the spec surface, on the same borrowed frame.
    spec: Option<spec::SpecView>,
    /// The checkout it belongs to, and whether it has been read yet — as
    /// `architect_root` and `architect_asked` are for the architect.
    spec_root: Option<PathBuf>,
    spec_asked: bool,
    /// The tab the open surface stands in for. It is drawn where that
    /// tab's pane is, about that tab's checkout, so a different tab coming
    /// to the front — however it got there — puts it away rather than
    /// leaving it answering for a checkout nobody is looking at.
    extension_tab: Option<TabId>,
    /// The diagram held by the pointer. A press on it is not yet a click
    /// or a drag — the first movement says which, as it does for
    /// [`EdgeDrag`] — so the click it might turn out to be is kept here
    /// until release.
    architect_grab: Option<DiagramGrab>,
    /// The checkout the open architect surface belongs to, and whether
    /// its artifacts have been asked for yet. Asked once per opening: the
    /// answer carries the root, so one that arrives for a surface since
    /// closed or reopened elsewhere is dropped rather than drawn.
    architect_root: Option<PathBuf>,
    architect_asked: bool,
    /// User-dragged navigator width; `None` falls back to its own
    /// responsive default. Mirrors `sidebar_width`/`dragging_sidebar`
    /// above, kept on the model rather than on the view itself so it
    /// survives closing and reopening the surface within the same
    /// session, the same way the sidebar's width survives switching tabs.
    code_tree_width: Option<u16>,
    /// Where the navigator is scrolled to. The host's, not the
    /// extension's, for the reason the width is: how far a list of rows
    /// can scroll is a question about how many fit, and only the render
    /// knows — which is also why a frame hands it back settled (see
    /// `render::FrameMetrics`).
    code_tree_scroll: extension_view::NavigatorScroll,
    /// The scrollbars the last frame drew, so a drag can answer *where in
    /// the content* the pointer went. Geometry belongs to the render, so
    /// it travels from there rather than being derived twice.
    code_scrollbars: extension_view::Rendered,
    /// The text the agent drawer drew last frame, for a drag over it to
    /// resolve against, and for its release to copy out of.
    drawer_text: crate::ui::agent_support::DrawerText,
    /// Where the pointer asked for the code surface's row menu, which is
    /// where it opens. `None` when the keyboard asked, and the menu opens
    /// under its row instead.
    code_menu_at: Option<Rect>,
    /// A press on the navigator's edge, waiting to find out what it is.
    code_edge_drag: Option<EdgeDrag>,
    /// Whether the content's own scrollbar is being held. Unambiguous, so
    /// it needs nothing but a flag.
    dragging_code_content: bool,
    /// Whether the pointer is held since a press on a toast, so the rest
    /// of that gesture is the toast's and reaches nothing beneath it.
    pressing_toast: bool,
    /// Text being selected with the pointer — in a pane, or in what an
    /// open surface drew — and, once released, the selection still drawn
    /// until the next press or key.
    selection: Option<Selection>,
    /// What a release selected, waiting for the frame loop to hand it to
    /// the host terminal's clipboard through the handle the frames go
    /// through, so it cannot land inside one.
    clipboard: Option<String>,
    /// An in-progress tab-reorder drag; `None` when no tab is being
    /// dragged. Client-local presentation state — nothing is sent to the
    /// server until release (see `TabDragGroup`/`DraggingTab`).
    dragging_tab: Option<DraggingTab>,
    /// An in-progress space-reorder drag, begun on a space's header row;
    /// sent to the server on release, like `dragging_tab`.
    dragging_space: Option<DraggingSpace>,
    /// The commit a background `git show` is out for; see
    /// [`CommitDetailResolution`] for why the answer names it back.
    commit_detail_pending: Option<String>,
    /// Whether a re-read of the surface's changes half is out, and
    /// whether one of its file requests is. Two flags because the two
    /// halves have two cadences and can be in flight at once.
    code_changes_pending: bool,
    /// Whether a one-file diff read is out. Its own flag, apart from the
    /// refresh's: a click must not wait behind a `status` it does not
    /// need.
    code_diff_pending: bool,
    code_request_pending: bool,
    /// The checkout the map's measurement was asked about. Not a flag:
    /// the answer outside a repository is nothing, and a flag cleared on
    /// nothing would ask again every frame.
    code_measure_asked: Option<PathBuf>,
    /// The last measurement of each checkout, and when it was taken.
    ///
    /// Kept because measuring is a `git grep` that opens every file the
    /// checkout has, and the surface it feeds is opened and closed all
    /// day. Without it, every open paid that again and showed no map
    /// until it landed — the one thing the cost was supposed to buy.
    /// Handed to a surface the moment it opens, and taken again in the
    /// background once it is old enough to have missed something.
    code_measures: BTreeMap<PathBuf, (Instant, code::Measure)>,
    /// Sink for recorded prompts. `None` leaves the history untouched —
    /// the default, so tests exercise the submission path without writing
    /// to a real UZE home.
    prompt_recorder: Option<mpsc::Sender<(PathBuf, uze_application::PromptOrigin, String)>>,
    /// Sink for the sidebar's own remembered shape, written when the user
    /// changes it. `None` leaves the stored layout untouched — the
    /// default, so tests fold and drag without writing to a real UZE home.
    layout_recorder: Option<mpsc::Sender<WorkspaceShape>>,
    /// Agent panes that went quiet since the last tick — the moment
    /// readiness is re-read.
    recently_quiet: Vec<PaneId>,
    /// Agent panes whose turn ended and has not yet stayed ended for
    /// [`CHIME_SETTLE`], by when it ended.
    unsettled_turns: BTreeMap<PaneId, Instant>,
    /// When the bell last rang, for [`CHIME_COOLDOWN`].
    chimed_at: Option<Instant>,
    /// An agent's tab waiting for the space it belongs in to exist.
    ///
    /// `CreateSpace` answers on the session's own clock, and a `CreateTab`
    /// sent before that lands in whichever space is selected — which is
    /// the bug this whole path exists to fix, reintroduced by racing it.
    pending_agent_tab: Option<PendingAgentTab>,
    /// Open state of the work modal; `None` when closed.
    work: Option<WorkOverlay>,
    /// Everything that can be done here, each with the key that reaches
    /// it. The workspace had no help surface at all — `alt+shift+i`
    /// delivers every task in a space, and there was no way to find that
    /// out — so this is the one place that answers "what can I do", in the
    /// mode where the keyboard mostly belongs to something else.
    action_index: Option<ActionIndexOverlay>,
    /// The notes of the release the sidebar's notice names, open over
    /// everything; `None` when closed.
    release_notes: Option<crate::ui::release_notes::ReleaseNotesModal>,
    /// What each root the picker landed on allows, once a worker answered:
    /// asked once per root and kept for the attach, so walking back over a
    /// directory never asks Git again.
    /// Whether the pane set has moved since occupancy was last worked out.
    ///
    /// The loop runs at 60Hz and the pane set changes when a tab opens or
    /// closes — a few times a session. Without this the sweep below cloned
    /// every pane's path and rebuilt two collections on every one of those
    /// ticks, to conclude nothing had happened.
    occupancy_stale: bool,
    /// Whether a reconciliation is out; only one at a time, and never on
    /// this thread — it releases slots and collects garbage, both of which
    /// ask Git.
    occupancy_pending: bool,
    /// Whether a slot is being acquired for a new agent. One at a time:
    /// two acquisitions racing over the same pool is how two agents end
    /// up in one checkout.
    placement_pending: bool,
    /// Whether the sidebar's timeline section shows only its header —
    /// folded by clicking that header (see `ViewHit::ToggleSection`).
    /// A preference, kept in the shared `ClientLayout` rather than in
    /// this attach's memory (see `shape`).
    timeline_collapsed: bool,
    /// How many commit rows the user dragged the timeline section to;
    /// `None` leaves it to `render::timeline_height`'s own default.
    /// Mirrors `sidebar_width`/`dragging_sidebar`, kept like
    /// `timeline_collapsed`.
    timeline_rows: Option<u16>,
    dragging_timeline: bool,
    /// The first commit the timeline section shows — where the wheel has
    /// scrolled it to. Clamped when drawn, so a history that shrank under
    /// it still shows its tail rather than nothing.
    timeline_scroll: usize,
    /// Rows of the tree the last frame could not show (see
    /// `render::FrameMetrics`). Zero while the whole tree fits, which is
    /// also what makes the wheel a no-op there.
    tree_overflow: u16,
    /// The open commit popup and the timeline row it hangs off (see
    /// `ViewHit::SelectItem`). Informational and anchored like
    /// `support_dropdown`: any click or key dismisses it.
    commit_detail: Option<CommitDetailPopup>,
    /// The management modal, while it is open. Sealed: every key and
    /// every click inside it is the modal's, and the workspace behind it
    /// keeps drawing but answers nothing.
    manage: Option<super::model::TuiModel>,
    /// Where the last frame drew the modal, for the click that lands
    /// beside it — or on its close mark — to be told apart from one
    /// inside.
    manage_chrome: Option<super::widget::modal::Chrome>,
    /// Whether the pointer is on the modal's close mark. Kept apart from
    /// `hovered` because the modal seals the client: its motion never
    /// reaches the hover the rest of the chrome reads.
    manage_close_hovered: bool,
    /// The modal's shape as it was last closed, kept here so the layout
    /// file is written from this model alone (see `shape`).
    management_layout: uze_application::ManagementLayout,
}

/// What [`WorkspaceModel::commit_detail`] holds while a commit is open —
/// the account itself, the timeline row it was opened from, and how far
/// its text has been scrolled.
pub(super) struct CommitDetailPopup {
    pub(super) detail: code::CommitDetail,
    /// The repository's delivery target, so the popup can single its
    /// label out among the refs standing at the commit.
    pub(super) target: Option<String>,
    pub(super) anchor: Rect,
    pub(super) scroll: u16,
}

/// Client-side reconstruction of what the user typed into a pane before
/// Enter.
///
/// UZE forwards keystrokes to a PTY whose line editor it cannot observe, so
/// this models only an ordinary single-line edit: printable characters,
/// backspace/delete, and horizontal cursor movement. Anything that could
/// rewrite the line invisibly — history recall, completion, a kill ring, a
/// control chord this does not encode — marks the buffer untrusted, and an
/// untrusted buffer is discarded at Enter. Recording nothing is always
/// preferable to recording a prompt the user never typed.
struct PromptBuffer {
    characters: Vec<char>,
    cursor: usize,
    trusted: bool,
}

impl Default for PromptBuffer {
    fn default() -> Self {
        Self {
            characters: Vec::new(),
            cursor: 0,
            trusted: true,
        }
    }
}

impl PromptBuffer {
    /// Mirrors one keystroke into the buffer. Takes a chord rather than a
    /// key event because reconstructing what someone typed is the same
    /// vocabulary question as binding it — and a buffer that read keys
    /// directly would be a second place the keyboard is understood.
    fn apply(&mut self, chord: Chord) {
        if chord.mods.ctrl || chord.mods.alt {
            self.trusted = false;
            return;
        }
        match chord.key {
            Key::Char(character) => {
                // The chord vocabulary folds case, so the buffer takes the
                // character as it was typed rather than as it was bound.
                let character = if chord.mods.shift {
                    character.to_ascii_uppercase()
                } else {
                    character
                };
                self.characters.insert(self.cursor, character);
                self.cursor += 1;
            }
            Key::Space => {
                self.characters.insert(self.cursor, ' ');
                self.cursor += 1;
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.characters.remove(self.cursor);
                }
            }
            Key::Delete => {
                if self.cursor < self.characters.len() {
                    self.characters.remove(self.cursor);
                }
            }
            Key::Left => self.cursor = self.cursor.saturating_sub(1),
            Key::Right => self.cursor = (self.cursor + 1).min(self.characters.len()),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.characters.len(),
            _ => self.trusted = false,
        }
    }

    fn paste(&mut self, text: &str) {
        for character in text.chars().map(|c| if c == '\r' { '\n' } else { c }) {
            self.characters.insert(self.cursor, character);
            self.cursor += 1;
        }
    }

    /// The text to record, or `None` when there is nothing trustworthy to
    /// record — including the line-continuation case, where the Enter
    /// inserts a newline instead of submitting and the buffer must survive.
    fn submit(&mut self) -> Option<String> {
        if self.trusted && self.cursor > 0 && self.characters[self.cursor - 1] == '\\' {
            self.characters[self.cursor - 1] = '\n';
            return None;
        }
        let flushed = std::mem::take(self);
        flushed
            .trusted
            .then(|| flushed.characters.into_iter().collect())
    }
}

struct GitBadge {
    cwd: PathBuf,
    summary: Option<code::ChangeSummary>,
    /// The same checkout's recent history, for the same tab: what the
    /// sidebar's timeline section draws. Re-read on its own, slower
    /// cadence (`TIMELINE_REFRESH`).
    timeline: Option<code::Timeline>,
    timeline_checked_at: Instant,
    checked_at: Instant,
}
impl WorkspaceModel {
    /// Records, for every open space, which tab its context agent is
    /// currently on — the agent's own tab, or a shell opened beside it.
    ///
    /// Rebuilt wholesale from the session rather than updated at the point
    /// of each selection: selection also moves for reasons this client
    /// never sees a click for (a tab opening, a tab closing, another
    /// client attached to the same session), and a map maintained by hand
    /// would quietly drift from all three. Agents that no longer exist are
    /// dropped in the same pass.
    fn note_strip_selection(&mut self, identities: &[AgentIdentity]) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let mut live = BTreeMap::new();
        for space in &session.workspace.spaces {
            if let Some(agent) = space_context_agent(space, identities) {
                live.insert(agent, space.selected_tab);
            }
        }
        // A space the user is not looking at keeps whatever it had: only
        // the spaces this update actually described are re-stated, and an
        // agent that has gone is one no space names any more.
        let known: BTreeSet<TabId> = self.tabs().map(|tab| tab.id).collect();
        self.remembered
            .strip_selection
            .retain(|agent, _| known.contains(agent));
        self.remembered.strip_selection.extend(live);
    }

    /// Whether `tab` is the agent its own space is currently in context of
    /// — true while the user is inside that agent, including from one of
    /// the shells opened beside it.
    fn is_context_agent(&self, tab: TabId, identities: &[AgentIdentity]) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session.workspace.spaces.iter().any(|space| {
                space.tabs.iter().any(|candidate| candidate.id == tab)
                    && space_context_agent(space, identities) == Some(tab)
            })
        })
    }

    /// Where selecting `agent` from the sidebar should actually land — the
    /// tab it was last left on, when that tab is still open beside it, and
    /// otherwise the agent itself.
    fn strip_tab_for(&self, agent: TabId, identities: &[AgentIdentity]) -> TabId {
        let Some(remembered) = self.remembered.strip_selection.get(&agent).copied() else {
            return agent;
        };
        if remembered == agent {
            return agent;
        }
        // Only a tab the agent's strip still draws: a shell can be dragged
        // into another strip, or have a harness started in it and become an
        // agent of its own, and following it either way would land every
        // click on this agent's row in a different agent.
        let belongs = self.session.as_ref().is_some_and(|session| {
            session.workspace.spaces.iter().any(|space| {
                strip_tabs(space, Some(agent), identities)
                    .iter()
                    .any(|tab| tab.id == remembered)
            })
        });
        if belongs { remembered } else { agent }
    }
    /// None of the modal overlays that own mouse input while they're open
    /// (rename buffer, new-space root picker, agent picker, support
    /// dropdown, status catalog, isolation tip,
    /// context menu, Git overlay) are
    /// currently up — the precondition for forwarding a drag/release/scroll
    /// that isn't already claimed by one of them straight into the focused
    /// pane's PTY instead of dropping it.
    fn no_modal_open(&self) -> bool {
        self.chrome_answers()
            && self.code.is_none()
            && self.architect.is_none()
            && self.spec.is_none()
    }

    /// Whether the sidebar and the strip answer the pointer: nothing is
    /// drawn over them. An open extension is not — it stands where the
    /// pane is, and only the pane is covered.
    fn chrome_answers(&self) -> bool {
        self.renaming.is_none()
            && self.root_picker.is_none()
            && self.agent_picker.is_none()
            && self.support_dropdown.is_none()
            && self.status_catalog.is_none()
            && self.work.is_none()
            && self.context_menu.is_none()
            && self.action_index.is_none()
            && self.release_notes.is_none()
            && self.manage.is_none()
            && !self.commit_detail_open()
    }

    /// The open space rooted at `project`, by canonical root.
    ///
    /// Prefers the selected space when several are rooted there — which
    /// the first round of `add-space-kinds` allowed on purpose — because
    /// the one the operator is looking at is the one they meant.
    fn space_rooted_at(&self, project: &Path) -> Option<SpaceId> {
        let canonical = |root: &Path| root.canonical().unwrap_or_else(|_| root.to_path_buf());
        let wanted = canonical(project);
        let session = self.session.as_ref()?;
        let matching: Vec<&Space> = session
            .workspace
            .spaces
            .iter()
            .filter(|space| canonical(&space.root) == wanted)
            .collect();
        matching
            .iter()
            .find(|space| space.id == session.workspace.selected_space)
            .or_else(|| matching.first())
            .map(|space| space.id)
    }

    /// Every space's label and root, as `space_rooted_at` sees them. For
    /// the journal alone: a lookup that answered `None` is only readable
    /// afterwards beside what it was looking through.
    fn space_roots(&self) -> Vec<(String, PathBuf)> {
        self.session.as_ref().map_or_else(Vec::new, |session| {
            session
                .workspace
                .spaces
                .iter()
                .map(|space| (space.label.clone(), space.root.clone()))
                .collect()
        })
    }

    fn hit_at(&self, column: u16, row: u16) -> Option<WorkspaceHit> {
        self.hit_rect_at(column, row).map(|(_, hit)| hit)
    }

    /// The same hit, with the rectangle the last frame drew it into.
    ///
    /// Several answers anchor a popup to that rectangle, so the geometry
    /// travels with the hit rather than being re-derived by whoever needs
    /// it — the split that let the renderer and the input loop disagree
    /// about where a row was.
    fn hit_rect_at(&self, column: u16, row: u16) -> Option<(Rect, WorkspaceHit)> {
        self.hits
            .iter()
            .find(|(rect, _)| rect.contains(Position::new(column, row)))
            .map(|(rect, hit)| (*rect, *hit))
    }

    /// The control still wearing its pressed skin, if any. Read at draw
    /// time rather than cleared at press time, so the flash ends with the
    /// clock instead of with whatever the press went on to do.
    fn pressed_hit(&self) -> Option<WorkspaceHit> {
        self.pressed
            .filter(|(_, at)| at.elapsed() < PRESS_FLASH)
            .map(|(hit, _)| hit)
    }

    /// Drops a press whose flash has run out, and says whether that
    /// changed the frame — the one repaint the flash needs to end on,
    /// since nothing else is necessarily moving when it does.
    fn expire_press(&mut self, now: Instant) -> bool {
        let expired = self
            .pressed
            .is_some_and(|(_, at)| now.duration_since(at) >= PRESS_FLASH);
        if expired {
            self.pressed = None;
        }
        expired
    }

    /// Whether `(column, row)` is over the sidebar's timeline section —
    /// any of its rows, since the wheel over a section scrolls that
    /// section wherever inside it the pointer happens to be.
    fn over_timeline(&self, column: u16, row: u16) -> bool {
        matches!(
            self.hit_at(column, row),
            Some(WorkspaceHit::Extension(ExtensionHit::CodeTimeline(_)))
        )
    }

    /// How many commit rows the timeline drew last frame — what the wheel
    /// scrolls by pages of, read off the hits rather than re-laid-out.
    fn timeline_rows_shown(&self) -> usize {
        self.hits
            .iter()
            .filter(|(_, hit)| {
                matches!(
                    hit,
                    WorkspaceHit::Extension(ExtensionHit::CodeTimeline(ViewHit::SelectItem(_)))
                )
            })
            .count()
    }
    fn focused_pane(&self) -> PaneId {
        self.session
            .as_ref()
            .map(|session| session.selected_tab().pane.id)
            .unwrap_or(PaneId(1))
    }
    fn selected_tab(&self) -> Option<TabId> {
        self.session
            .as_ref()
            .map(|session| session.selected_tab().id)
    }
    /// Whether a space is minimized to its header row. Asked by root,
    /// which is what the fold is kept by (see `collapsed_space_roots`).
    pub(super) fn space_folded(&self, space: &Space) -> bool {
        self.collapsed_space_roots.contains(&space.root)
    }

    /// The root a space is over — the name its fold is kept under.
    fn space_root(&self, space: SpaceId) -> Option<PathBuf> {
        self.session
            .iter()
            .flat_map(|session| &session.workspace.spaces)
            .find(|candidate| candidate.id == space)
            .map(|space| space.root.clone())
    }

    /// Every tab of every space, in the order the session lists them.
    pub(super) fn tabs(&self) -> impl Iterator<Item = &Tab> {
        self.session
            .iter()
            .flat_map(|session| &session.workspace.spaces)
            .flat_map(|space| &space.tabs)
    }

    pub(super) fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs().find(|tab| tab.id == id)
    }

    fn tab_of_pane(&self, pane: PaneId) -> Option<&Tab> {
        self.tabs().find(|tab| tab.pane.id == pane)
    }

    /// The pane a tab — and the sidebar row standing for it — shows.
    pub(super) fn pane_for_tab(&self, tab: TabId) -> Option<PaneId> {
        self.tab(tab).map(|tab| tab.pane.id)
    }
    /// The list at the foot of the sidebar, as it stands.
    fn first_steps(&self) -> crate::ui::FirstSteps<'_> {
        crate::ui::FirstSteps {
            steps: render::FIRST_STEPS
                .into_iter()
                .filter(|action| self.offers_action(*action))
                .collect(),
            taken: &self.steps_taken,
            collapsed: self.first_steps_collapsed,
            closed: self.first_steps_closed,
            scopes: render::FIRST_STEP_SCOPES,
        }
    }

    /// Records that a step was taken, whichever way it was reached.
    fn note_step(&mut self, action: Action) -> bool {
        render::FIRST_STEPS.contains(&action) && self.steps_taken.insert(action.name())
    }

    /// What this client owns of the shared layout, as it stands.
    fn shape(&self) -> WorkspaceShape {
        WorkspaceShape {
            sidebar: uze_application::SidebarLayout {
                width: self.sidebar_width,
            },
            workspace: uze_application::WorkspaceLayout {
                timeline_collapsed: self.timeline_collapsed,
                timeline_rows: self.timeline_rows,
                collapsed_space_roots: self.collapsed_space_roots.clone(),
            },
            first_steps: uze_application::FirstStepsLayout {
                collapsed: self.first_steps_collapsed,
                closed: self.first_steps_closed,
                taken: self.steps_taken.clone(),
            },
            // Live while the modal is open, last-closed otherwise: a
            // drawer dragged in the modal is in the file the moment the
            // next fold or drag anywhere sends a shape.
            management: self.manage.as_ref().map_or_else(
                || self.management_layout.clone(),
                |manage| manage.management_layout(),
            ),
        }
    }

    /// What the last frame drew of the modal — its hit list goes to the
    /// modal's own model, where its clicks are resolved, and its geometry
    /// stays here, where the click beside it is.
    fn absorb_manage_frame(&mut self, frame: Option<ManageFrame>) {
        match (frame, self.manage.as_mut()) {
            (Some(frame), Some(manage)) => {
                manage.hits = frame.hits;
                self.manage_chrome = Some(frame.chrome);
            }
            _ => self.manage_chrome = None,
        }
    }

    /// Keeps the sidebar's shape for the next run — sent, never written
    /// here, so a fold costs a channel send on the input path (see
    /// `layout_recorder`).
    fn remember_sidebar(&self) {
        if let Some(recorder) = self.layout_recorder.as_ref() {
            let _ = recorder.send(self.shape());
        }
    }
}

// --- Layout --------------------------------------------------------------

// --- Rendering -------------------------------------------------------------

fn io_error(source: io::Error) -> UzeError {
    UzeError::Write {
        path: "terminal".into(),
        source,
    }
}
fn runtime_error(error: uze_terminal::RuntimeError) -> UzeError {
    UzeError::TerminalRuntime(error.to_string())
}

#[cfg(test)]
mod tests;
