//! Terminal presentation over [`UzeApplication`] — product surface, not a
//! debug console.
//!
//! This module owns only navigation/selection/overlay state and input
//! transitions. Every product operation runs in a short-lived worker against
//! a fresh application facade, so the terminal never reads Store, vendor
//! files, integrations, or `marketplace.json` directly — it calls
//! `UzeApplication` read models exactly like the CLI does, and renders what
//! comes back.
//!
//! Module map — start at [`run`], the entry point:
//! - `ui.rs` (this file): the entry point, plus chrome both surfaces
//!   share — [`TerminalSession`] (the one alternate-screen lifecycle),
//!   row and text helpers, and the sidebar-geometry math
//!   (`clamp_sidebar_width`/`sidebar_width_for`) both menus resize by.
//! - [`orchestrator`]: the terminal workspace client (ADR-038) — tabs,
//!   panes, the persistent runtime client, and the one event loop. Owns
//!   its own model, hit type and render.
//! - `management`: the management surface (routes below), drawn as a
//!   modal over the workspace and driven by its loop: what it keeps
//!   between openings, how one opening is made, and how it is drawn.
//! - [`model`]/[`input`]/[`hit`]/[`worker`]: management's MVU pieces —
//!   `TuiModel` (state), key/mouse handling, hit-testing, and the
//!   intent/worker dispatch that runs product operations off-thread.
//! - [`view`]: one file per management route (Overview, Plugins,
//!   Extensions, Harnesses, Profiles, Keys, Settings).
//! - `overlay`: the dialogs shared across management routes.

use std::{
    io::{self, Stdout},
    path::PathBuf,
};

use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
};

use uze_application::{
    ProcessOutput, ProcessResult, ProcessRunner, ProcessSpec, Result, SystemProcessRunner,
    UzeApplication, UzeHome,
};

mod agent_support;
mod chime;
pub mod extension_host;
mod extension_switch;
mod extension_view;
mod hit;
mod input;
mod keys;
mod management;
mod model;
mod orchestrator;
mod overlay;
mod release_notes;
mod root_picker;
mod selection;
pub(crate) mod theme;

use crate::ui::widget::{TRAILING_PAD, row, text};
use theme::Symbol;
pub mod view;
pub(crate) mod widget;
mod worker;

/// Forces every process the TUI spawns to run silently, regardless of
/// whether the integration asked for inherited output. The TUI owns the
/// terminal's alternate screen for its own rendering; a vendor installer's
/// progress written directly to the real stdout (as `uze setup`'s inherited
/// output is designed to do on the CLI) has nowhere sane to land here — it
/// prints straight through the ratatui frame and corrupts the layout, which
/// is exactly what `SystemProcessRunner`'s `ProcessOutput::Inherit` does.
/// Every `UzeApplication` the TUI constructs uses this instead.
struct SilentProcessRunner;

impl ProcessRunner for SilentProcessRunner {
    fn run(&self, spec: &ProcessSpec) -> Result<ProcessResult> {
        let quiet = ProcessSpec {
            output: ProcessOutput::Quiet,
            ..spec.clone()
        };
        SystemProcessRunner.run(&quiet)
    }
}

/// The TUI's one composition point for `UzeApplication` — every worker
/// thread builds its application through this, never `UzeApplication::from_env`
/// directly, so no code path can accidentally let a provisioning command's
/// output loose on the terminal.
fn tui_application(home: UzeHome) -> Result<UzeApplication> {
    UzeApplication::from_env_with_runner(home, Box::new(SilentProcessRunner))
}

/// Runs the TUI. `home` is passed to workers, which construct the same
/// production application composition root as the CLI.
pub fn run(home: UzeHome) -> Result<()> {
    let session = tracing::info_span!("tui.session");
    let _entered = session.enter();
    // Once per process rather than per attach: attaching again is not a
    // reason to ask GitHub again, and both surfaces read the one answer.
    crate::self_update::watch(home.clone());
    // Before the session starts reading input, which would take the
    // terminal's reply for keystrokes.
    crate::theme::observe_appearance(&home);
    crate::theme::follow_desktop(home.clone());
    let mut terminal = TerminalSession::start()?;
    // Immediately after the screen is entered and before anything draws
    // into it: from here on, a panic on this thread leaves a terminal a
    // message can be read on, and one on a background read leaves the
    // screen alone.
    report_panics_on_a_restored_terminal(terminal.keyboard());
    chime::load(&home);
    extension_switch::load(&home);
    // The client's shape as this user last left it — read once, here, and
    // handed to the attach, which writes every section of it back as it
    // changes (see `uze_application::ClientLayout`).
    let mut layout = tui_application(home.clone())
        .map(|app| app.workspace().client_layout())
        .unwrap_or_default();
    // What the workspace client resolved for itself — the sidebar's
    // tasks, branches and agent statuses among them — which each attach
    // takes over from the last instead of deriving again in front of the
    // user (see `orchestrator::WorkspaceMemory`).
    let mut workspace_memory = orchestrator::WorkspaceMemory::default();
    // The management modal's own half of the same idea, started here
    // rather than on the first opening of the modal: the machine resolves
    // on a thread while the workspace attaches, so the modal is already
    // answered when the operator asks for it, and what it resolved is
    // still there the next time they do (see
    // `management::ManagementMemory`).
    let mut management_memory = management::ManagementMemory::warming(&home);
    // Only the first attach lands the client in a space for the directory
    // `uze` was started in; an attach after the runtime went away takes
    // the session as it stands (see `orchestrator::Landing`).
    let mut landing = orchestrator::Landing::AtLaunchDirectory;
    let outcome = loop {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let launch = uze_terminal::SpaceSeat {
            root: uze_application::space_root(&root),
        };
        match orchestrator::attach_workspace(
            &mut terminal,
            &launch,
            &mut layout,
            &mut workspace_memory,
            &home,
            &mut management_memory,
            landing,
        ) {
            Ok(orchestrator::WorkspaceExit::Quit) => break Ok(()),
            Ok(orchestrator::WorkspaceExit::Disconnected) => {
                landing = orchestrator::Landing::WhereItLeftOff;
            }
            Err(error) => break Err(error),
        }
    };
    // The attach writes its shape as it changes, on a thread it has
    // joined by now, and hands back the last one; one synchronous write
    // on the way out is what makes the last drag or fold survive the
    // process ending a moment later.
    remember_layout(&home, &layout);
    outcome
}

/// Keeps the client's shape for the next run. Best-effort: a layout that
/// cannot be written is a preference lost, never a session lost.
fn remember_layout(home: &UzeHome, layout: &uze_application::ClientLayout) {
    let _ =
        tui_application(home.clone()).and_then(|app| app.workspace().save_client_layout(layout));
}

// --- Terminal lifecycle ------------------------------------------------------

/// Owns the raw-mode/alternate-screen/mouse-capture lifecycle for the whole
/// `run()` call, not per attach: a client that opens and tears down its
/// own alternate screen on every attach makes two consecutive full-screen
/// buffer swaps most terminal emulators render as a visible flash, reading
/// as uze itself closing and reopening. One session, entered once and left
/// once (on quit), makes an attach after the runtime went away just
/// another `draw` into the same already-open screen.
pub(crate) struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// What this terminal turned out to be able to deliver — read by the
    /// Keys screen, which shows a chord the host cannot send as such
    /// rather than letting it look bound.
    keyboard: keys::KeyboardSupport,
}

impl TerminalSession {
    fn start() -> Result<Self> {
        enable_raw_mode().map_err(io_error)?;
        let mut stdout = io::stdout();
        // Asked for before the screen is entered, so the very first
        // keystroke is read the same way as every later one.
        let keyboard = keys::begin_enhanced_input();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::cursor::Hide,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            let _ = disable_raw_mode();
            return Err(io_error(error));
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(io_error)?;
        Ok(Self { terminal, keyboard })
    }

    pub(crate) fn keyboard(&self) -> keys::KeyboardSupport {
        self.keyboard
    }

    pub(crate) fn size(&self) -> Result<ratatui::layout::Size> {
        self.terminal.size().map_err(io_error)
    }

    /// Draws one frame. The only way a frame reaches the terminal: this is
    /// the one ratatui `Terminal` the client constructs, so the control
    /// pass below cannot be forgotten by a surface.
    pub(crate) fn draw(&mut self, render: impl FnOnce(&mut ratatui::Frame<'_>)) -> Result<()> {
        self.terminal
            .draw(|frame| {
                render(frame);
                settle_controls(frame.buffer_mut());
            })
            .map(|_| ())
            .map_err(io_error)
    }

    /// The terminal's own bell, through the handle the frames go through
    /// so it cannot land inside one. What it sounds like — a tone, a
    /// flash, nothing — is the terminal's setting, not ours.
    pub(crate) fn ring(&mut self) {
        use std::io::Write;
        let backend = self.terminal.backend_mut();
        let _ = backend.write_all(b"\x07").and_then(|()| backend.flush());
    }

    /// Writes raw bytes to the host terminal between frames — a control
    /// sequence meant for the terminal itself rather than for a cell.
    pub(crate) fn emit(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let backend = self.terminal.backend_mut();
        let _ = backend.write_all(bytes).and_then(|()| backend.flush());
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal(self.keyboard);
        let _ = self.terminal.show_cursor();
    }
}

/// Hands the terminal back the way it was found: raw mode off, the
/// alternate screen left, the mouse and paste modes and the cursor
/// restored.
///
/// Standalone rather than only [`TerminalSession`]'s `Drop` because the
/// panic hook has to run it too, and a hook holds no session — see
/// [`report_panics_on_a_restored_terminal`]. Writes to `io::stdout()`,
/// which is the same handle the backend holds.
fn restore_terminal(keyboard: keys::KeyboardSupport) {
    keys::end_enhanced_input(keyboard);
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
        crossterm::cursor::Show
    );
}

/// Makes a panic reportable: restore the terminal first, then let the
/// hook that was already installed print.
///
/// Unwinding alone was never the problem — `Drop` restores the terminal
/// correctly. The order is: the default hook prints the message and the
/// location to stderr *before* anything unwinds, so it lands on the
/// alternate screen, and leaving that screen a moment later wipes it. A
/// panic on the draw path therefore presented as uze vanishing with
/// nothing at all to report, which is a bug nobody can file.
///
/// Only a panic on the thread that draws ends the session, so only that
/// one hands the terminal back. A background read's panic is caught where
/// it runs (`orchestrator::answered_or`) and the client carries on drawing:
/// restoring the terminal for it would leave that client painting into a
/// screen it no longer owns, and printing the message would write it into
/// the live alternate screen, where ratatui — repainting only the cells
/// that differ — would leave it. Those go to the log instead.
fn report_panics_on_a_restored_terminal(keyboard: keys::KeyboardSupport) {
    let render_thread = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if std::thread::current().id() == render_thread {
            restore_terminal(keyboard);
            previous(panic);
        } else {
            tracing::error!(
                thread = std::thread::current().name().unwrap_or("unnamed"),
                "background thread panicked: {panic}"
            );
        }
    }));
}

fn io_error(source: io::Error) -> uze_application::UzeError {
    uze_application::UzeError::Write {
        path: PathBuf::from("terminal"),
        source,
    }
}

/// `~/relative/path` when `root` is under the user's home directory, else
/// the path as-is — mirrors what a shell prompt usually shows.
pub(crate) fn display_project_path(root: &std::path::Path) -> String {
    uze_platform::home::shorten(root)
}

// --- Shared helpers ---------------------------------------------------------

/// Narrowest either sidebar (workspace or management) can be dragged. Needs
/// to comfortably fit an agent tab's indented detail line — connector +
/// cwd on the left, the running agent's alias pinned to the row's own
/// right edge (see `orchestrator::render_sidebar`'s agent-tab loop) — the
/// widest row this menu draws; a bound tight enough to only fit a short
/// label broke that layout once the alias moved off the end of the cwd
/// text and onto a fixed right column.
const MIN_SIDEBAR_WIDTH: u16 = 28;
/// Widest either sidebar can be dragged, regardless of how wide the
/// terminal is — it's navigation, not the workspace; past this it's just
/// width the content column could otherwise use.
const MAX_SIDEBAR_WIDTH: u16 = 40;
/// Dragging either sidebar's border never shrinks its content column below
/// this many columns.
const MIN_CONTENT_WIDTH: u16 = 30;

/// Shared by both sidebars' drag-resize — the workspace's and the modal's
/// menu — same bounds, so the two feel identical to drag rather than just
/// similarly shaped.
fn clamp_sidebar_width(width: u16, total_width: u16) -> u16 {
    let max = total_width
        .saturating_sub(MIN_CONTENT_WIDTH)
        .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
    width.clamp(MIN_SIDEBAR_WIDTH, max)
}

/// Both clients' first cut of the frame: the sidebar, at the dragged width
/// or the responsive default, and the column right of it.
///
/// Flush against both edges: a blank row at either one read as wasted
/// room, the top's as a header floating down from the terminal's edge and
/// the bottom's as a stray empty line under everything.
fn sidebar_and_column(frame_area: Rect, sidebar_width_override: Option<u16>) -> (Rect, Rect) {
    let area = frame_area;
    let sidebar_width = sidebar_width_override
        .map(|width| clamp_sidebar_width(width, area.width))
        .unwrap_or_else(|| sidebar_width_for(area.width));
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(10)])
        .split(area);
    (columns[0], columns[1])
}

/// Shared responsive default sidebar width (no user drag override yet) for
/// both sidebars. Every bucket stays at or above `MIN_SIDEBAR_WIDTH` — this
/// path isn't run through `clamp_sidebar_width`, so a bucket smaller than
/// the drag floor would reintroduce the same overflow on a narrow terminal
/// that raising the floor was meant to fix.
fn sidebar_width_for(total_width: u16) -> u16 {
    if total_width < 60 {
        MIN_SIDEBAR_WIDTH
    } else if total_width < 90 {
        30
    } else {
        32
    }
}

/// The inset [`content_area`] keeps on each side of a screen's content:
/// the column the modal's header and footer hang their words from, so
/// every screen's text starts where the modal's own does.
pub(crate) const CONTENT_INSET_LEFT: u16 = 3;
pub(crate) const CONTENT_INSET_RIGHT: u16 = 3;
const CONTENT_INSET_TOP: u16 = 0;

/// [`screen_header`](widget::screen_header) for a management route: its
/// name, and beside it what the screen holds — `summary` when the screen
/// reports one, its subtitle otherwise.
pub(crate) fn render_screen_header(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    route: model::Route,
    summary: Option<Span<'static>>,
) -> Rect {
    let note = summary
        .unwrap_or_else(|| Span::raw(route.subtitle()))
        .style(theme::fg(theme::Token::TextMuted));
    widget::screen_header::inline(frame, area, route.label(), note, None)
}

/// Every content screen's outer inset. No border, no background: content
/// just sits indented on the shared backdrop.
pub(crate) fn content_area(area: Rect) -> Rect {
    Rect::new(
        area.x + CONTENT_INSET_LEFT,
        area.y + CONTENT_INSET_TOP,
        area.width
            .saturating_sub(CONTENT_INSET_LEFT + CONTENT_INSET_RIGHT),
        area.height.saturating_sub(CONTENT_INSET_TOP),
    )
}

/// A panel that opens off the right of a content area — a drawer, or a
/// permanent right-hand column — drawn flush against the frame's own top
/// and right edges instead of stopping at the content inset: text sitting
/// two columns in reads as a margin, a filled slab ending two columns
/// short of the border reads as misaligned. `width` is how much of the
/// content area the panel takes; the inset it swallows on the right comes
/// on top of that, so whatever lays out to its left is unaffected, and
/// the panel's own inner padding keeps its text off the edge.
pub(crate) fn side_panel_area(content: Rect, width: u16) -> Rect {
    let width = width.min(content.width);
    Rect::new(
        content.right().saturating_sub(width),
        content.y.saturating_sub(CONTENT_INSET_TOP),
        width + CONTENT_INSET_RIGHT,
        content.height + CONTENT_INSET_TOP,
    )
}

// --- Row chrome ----------------------------------------------------------
//
// One column, one row at a time: what both sidebars and every extension
// section are laid out with. Here rather than in either surface's own
// renderer because an extension's section is drawn by `extension_view`,
// which is a sibling of both — and two modules deriving the same trailing
// column independently is what this file already exists to prevent.

/// A downward cursor over one column's rows. The sidebar lays itself out a
/// row at a time and simply stops once the column is full, so nothing it
/// draws needs to know in advance how tall everything else came out.
pub(crate) struct Rows {
    x: u16,
    width: u16,
    y: u16,
    bottom: u16,
    /// Rows still to be scrolled past before anything lands on screen. A
    /// column that is scrolled still asks for the rows above its window —
    /// a list only knows what it is showing by walking what comes before
    /// it — and gets [`Slot::Hidden`] for them instead of a rectangle.
    skipped: u16,
}

/// Where a row landed once the column it belongs to is scrolled.
///
/// [`Rows::next`] flattens this back to an `Option` for the columns that
/// never scroll; a scrolled one has to tell "above the window" (keep
/// laying out) from "past the foot" (stop), which one `None` cannot say.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Slot {
    /// Scrolled out above the window: laid out, never drawn.
    Hidden,
    Visible(Rect),
    /// The column is full — nothing after this one fits either.
    Full,
}

impl Slot {
    /// The rectangle to draw into, if this row is on screen at all.
    pub(crate) fn visible(self) -> Option<Rect> {
        match self {
            Self::Visible(rect) => Some(rect),
            Self::Hidden | Self::Full => None,
        }
    }

    pub(crate) fn is_full(self) -> bool {
        self == Self::Full
    }
}

impl Rows {
    pub(crate) fn over(area: Rect) -> Self {
        Self {
            x: area.x,
            width: area.width,
            y: area.y,
            bottom: area.y + area.height,
            skipped: 0,
        }
    }

    /// Scrolls the next `rows` out above the window. Whole rows only: a
    /// column scrolls by its own items, never by half of one.
    pub(crate) fn scroll_past(&mut self, rows: u16) {
        self.skipped = rows;
    }

    pub(crate) fn slot(&mut self, height: u16) -> Slot {
        if self.skipped >= height {
            self.skipped -= height;
            return Slot::Hidden;
        }
        match self.next(height) {
            Some(rect) => Slot::Visible(rect),
            None => Slot::Full,
        }
    }

    pub(crate) fn next(&mut self, height: u16) -> Option<Rect> {
        if self.y + height > self.bottom {
            return None;
        }
        let rect = Rect::new(self.x, self.y, self.width, height);
        self.y += height;
        Some(rect)
    }

    /// One blank row, when the column still has one to spare — scrolled
    /// past like any other row when the column is scrolled.
    pub(crate) fn gap(&mut self) {
        let _ = self.slot(1);
    }

    pub(crate) fn remaining(&self) -> u16 {
        self.bottom.saturating_sub(self.y)
    }
}

/// The list of things worth trying once, at the foot of a sidebar.
///
/// A section, in the same shape and drawn by the same renderer as the
/// commit timeline beside it: a header that folds it, and a row per entry
/// with a mark, a name and a value at the far edge. Reusing that rather
/// than writing a second collapsible thing is the point — a sidebar with
/// two kinds of section is a sidebar whose sections drift.
///
/// Every word of it comes from the keymap: the label is the action's own
/// and the key beside it is whatever is bound now, so a rebinding cannot
/// leave a stale key printed here. What it adds is the only thing the
/// keymap cannot know — whether the operator has ever done it.
pub(crate) struct FirstSteps<'a> {
    /// The steps this surface offers — the fixed list, less any whose
    /// extension is switched off.
    pub(crate) steps: Vec<uze_keys::Action>,
    /// The steps already taken, by action name.
    pub(crate) taken: &'a std::collections::BTreeSet<String>,
    pub(crate) collapsed: bool,
    /// Put away for good, which the header offers only once every step has
    /// been taken.
    pub(crate) closed: bool,
    /// Which keyboard the keys beside the steps are read from. Named from
    /// the mode rather than from what is open, so a dialog cannot change
    /// the key printed here.
    pub(crate) scopes: &'a [uze_keys::Scope],
}

impl FirstSteps<'_> {
    /// The heading, in the section vocabulary the timeline already speaks.
    pub(crate) fn section(&self) -> uze_extensions::view::Section {
        use uze_extensions::view::{Role, RowMark, SectionRow, Span as ViewSpan};

        let keymap = uze_keys::active();
        uze_extensions::view::Section {
            title: "first steps".to_owned(),
            caption: ViewSpan::new(self.caption(), Role::Faint),
            collapsed: self.collapsed,
            // Nothing to drag: the list is as long as it is, and a handle
            // that can only ever be dropped in one place is a control that
            // does nothing.
            resizable: false,
            scroll: 0,
            rows: self
                .steps
                .iter()
                .map(|action| SectionRow {
                    mark: RowMark::Step {
                        done: self.is_taken(*action),
                    },
                    mark_role: Role::Success,
                    name: ViewSpan::new(
                        action.label(),
                        if self.is_taken(*action) {
                            Role::Dim
                        } else {
                            Role::Default
                        },
                    ),
                    trailing: ViewSpan::new(
                        keymap
                            .chord_for(*action, self.scopes)
                            .map(|chord| chord.to_string())
                            .unwrap_or_default(),
                        Role::Faint,
                    ),
                })
                .collect(),
        }
    }

    /// How far along, and — once there is nowhere further — the mark that
    /// puts the list away. It rides in the caption rather than taking a
    /// column of its own, so the header is the same shape as the timeline's
    /// beside it whether the mark is there or not.
    fn caption(&self) -> String {
        let progress = format!("{} of {}", self.done(), self.steps.len());
        if self.complete() {
            format!("{progress} {}", theme::glyph(Symbol::MarkClose))
        } else {
            progress
        }
    }

    /// Every step taken. Only then is the list something to be finished
    /// with rather than folded away.
    pub(crate) fn complete(&self) -> bool {
        self.done() == self.steps.len()
    }

    /// The cells of the header the closing mark occupies, if it is there
    /// at all. Derived from the header the section renderer drew rather
    /// than measured twice: the mark is the caption's own last glyph.
    pub(crate) fn close_rect(&self, header: Rect) -> Option<Rect> {
        let mark = theme::width(Symbol::MarkClose);
        self.complete().then(|| {
            Rect::new(
                header.right().saturating_sub(mark + TRAILING_PAD),
                header.y,
                mark,
                1,
            )
        })
    }

    pub(crate) fn is_taken(&self, action: uze_keys::Action) -> bool {
        self.taken.contains(&action.name())
    }

    fn done(&self) -> usize {
        self.steps
            .iter()
            .filter(|action| self.is_taken(**action))
            .count()
    }

    /// The rows the section comes to. Both sidebars have to know this
    /// before they lay out what sits above it — a strip pinned to the foot
    /// that the list above could grow over would be pinned to nothing.
    pub(crate) fn height(&self) -> u16 {
        if self.collapsed {
            1
        } else {
            // The trailing row is blank on purpose: open, the last step
            // would otherwise sit against the next section's header with
            // nothing saying where one ends and the other begins. The
            // section renderer simply runs out of rows before it reaches
            // it, so no one has to draw the gap.
            1 + self.steps.len() as u16 + 1
        }
    }

    /// Where the section goes at the foot of `column`, or nothing when the
    /// column cannot spare the rows.
    ///
    /// The headroom is the point: a sidebar that is mostly its own footer
    /// is worse than one with no footer, so on a column too short for both
    /// the list wins and the steps stay reachable by their own keys and
    /// through the index.
    pub(crate) fn rect(&self, column: Rect) -> Option<Rect> {
        const HEADROOM: u16 = 5;
        if self.closed {
            return None;
        }
        let height = self.height();
        (column.height >= height + HEADROOM)
            .then(|| Rect::new(column.x, column.bottom() - height, column.width, height))
    }
}

/// The release notice both sidebars carry, sitting on the sections that
/// hold their foot. It borrows a section row's layout — a marker column,
/// the text, anything right-aligned kept `TRAILING_PAD` off the divider —
/// but not a section's header: there is nothing under it to fold, and a
/// band with a chevron that folds nothing reads as a control that does
/// nothing.
pub(crate) struct ReleaseNotice<'a>(pub(crate) &'a crate::self_update::Notice);

/// What a drawn notice answers to.
pub(crate) struct ReleaseTargets {
    /// Every row that opens the release's notes.
    pub(crate) notes: Vec<Rect>,
    /// The mark that puts the notice away. It goes into a frame's hits
    /// ahead of `notes`, because it sits on one of them.
    pub(crate) dismiss: Rect,
}

impl ReleaseNotice<'_> {
    /// Two rows and a blank one under them, which keeps the notice off the
    /// header of the section it sits on. A heading of its own was tried and
    /// read as a third thing to take in, where the version and what to do
    /// about it are the whole of the news.
    const HEIGHT: u16 = 3;

    /// Where it goes at the foot of `column`, or nothing when the column
    /// cannot spare the rows — the same headroom rule the steps keep.
    pub(crate) fn rect(&self, column: Rect) -> Option<Rect> {
        const HEADROOM: u16 = 5;
        (column.height >= Self::HEIGHT + HEADROOM).then(|| {
            Rect::new(
                column.x,
                column.bottom() - Self::HEIGHT,
                column.width,
                Self::HEIGHT,
            )
        })
    }

    /// The mark rides on the version's row rather than on a caption: a
    /// caption is the first thing a narrow column elides, and a mark at its
    /// end went with it — a notice nobody could put away. What gives way on
    /// a narrow column is the text, the way a section row's name does; the
    /// mark never does.
    pub(crate) fn render(&self, frame: &mut ratatui::Frame<'_>, area: Rect) -> ReleaseTargets {
        use ratatui::{text::Line, widgets::Paragraph};

        let notice = self.0;
        let version = Rect::new(area.x, area.y, area.width, 1);
        let action = Rect::new(area.x, area.y + 1, area.width, 1);
        let gutter = theme::width(Symbol::ArrowUp) + 1;
        // The room the version has once its gutter, the gap `push_trailing`
        // keeps and the dismissing mark are paid for.
        let room = usize::from(
            area.width
                .saturating_sub(gutter + 1 + theme::width(Symbol::MarkClose) + TRAILING_PAD),
        );

        let mut spans = vec![
            Span::styled(
                format!("{} ", theme::glyph(Symbol::ArrowUp)),
                theme::fg(theme::Token::Accent),
            ),
            Span::styled(
                text::elide(&format!("v{}", notice.version()), room),
                theme::fg(theme::Token::TextPrimary),
            ),
        ];
        row::push_trailing(
            &mut spans,
            version.width,
            theme::glyph(Symbol::MarkClose),
            theme::color(theme::Token::TextFaint),
        );
        frame.render_widget(Paragraph::new(Line::from(spans)), version);

        // No trailing glyph: the notes open here, in a modal, and an
        // external-link arrow would promise the browser.
        let spans = vec![
            Span::raw(" ".repeat(usize::from(gutter))),
            Span::styled(
                text::elide(
                    notice.action(),
                    usize::from(area.width.saturating_sub(gutter + TRAILING_PAD)),
                ),
                theme::fg(theme::Token::TextDim),
            ),
        ];
        frame.render_widget(Paragraph::new(Line::from(spans)), action);

        let mark = theme::width(Symbol::MarkClose);
        ReleaseTargets {
            notes: vec![version, action],
            dismiss: Rect::new(
                version.right().saturating_sub(mark + TRAILING_PAD),
                version.y,
                mark,
                1,
            ),
        }
    }
}

/// Takes every control a cell still carries out of it, last, over the whole
/// frame: every frame the client draws passes here, through
/// [`TerminalSession::draw`]. ratatui drops the C0 and C1 controls itself, but a bidi override
/// or isolate is zero-width, so it is appended to the cell before it and
/// reaches the terminal, where it reorders the rest of the row: a plugin's
/// name or description could make what follows it read as something else.
/// Width is settled by the time a cell exists, so the control is removed
/// rather than spelled out; the CLI, which lays text out after escaping it,
/// spells it.
pub(crate) fn settle_controls(buffer: &mut ratatui::buffer::Buffer) {
    for cell in &mut buffer.content {
        let symbol = cell.symbol();
        if symbol.chars().any(uze_application::is_terminal_control) {
            let kept: String = symbol
                .chars()
                .filter(|character| !uze_application::is_terminal_control(*character))
                .collect();
            cell.set_symbol(if kept.is_empty() { " " } else { &kept });
        }
    }
}

#[cfg(test)]
mod tests;
