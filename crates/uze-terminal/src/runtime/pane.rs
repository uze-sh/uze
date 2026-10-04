//! A pane: the pseudoterminal, the terminal emulator over it, and the snapshots and damage drawn from it.

use super::*;

pub(super) struct PaneRuntime {
    pub(super) id: PaneId,
    /// Taken only when the pane is let go (see its `Drop`).
    pub(super) master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    pub(super) writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Shared with the thread that reaps it once the pane is stopped.
    pub(super) child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    /// Read while the leader is alive: once a finished leader is reaped,
    /// its group can no longer be asked for, though what it left running
    /// is still in it. See [`PaneRuntime::end_leftovers`].
    /// The pane's program and everything it starts, ended together. Read
    /// while the leader is alive: once a finished leader is reaped, what it
    /// left running is still in it. See [`PaneRuntime::end_leftovers`].
    pub(super) group: Option<Arc<uze_platform::process::Group>>,
    pub(super) terminal: Arc<Mutex<Term<ReplySink>>>,
    /// What this pane was spawned as — kept so a workspace restart can
    /// respawn the same launch in the same tab (see [`Server::persist`]),
    /// and so a finished program can be told from a shell.
    pub(super) launch: Launch,
    /// The last snapshot actually sent to clients, so
    /// [`PaneRuntime::damage_since_last`] can diff against what they
    /// already have instead of resending every cell on every PTY read.
    pub(super) last_sent: Mutex<Option<PaneSnapshot>>,
    /// Shared with the thread that reads the pane's output, which has to
    /// follow the content under a selection the moment it is drawn. Always
    /// locked after `terminal`, never before it.
    pub(super) selection: Arc<Mutex<PaneSelection>>,
}

/// Closing a pseudoterminal's master can wait: ConPTY's
/// `ClosePseudoConsole` returns only once the output it still holds has
/// been read. Done on whichever thread let the pane go, it held that
/// request for as long; done on the reader's own, it would wait on itself.
/// So the master is closed on a thread of its own while the reader keeps
/// draining to its end, which is also what lets the reader end at all.
impl Drop for PaneRuntime {
    fn drop(&mut self) {
        if let Some(master) = self.master.get_mut().ok().and_then(Option::take) {
            thread::spawn(move || drop(master));
        }
    }
}

/// Answers a pane's own program, including its OSC 10/11 colour queries.
///
/// The palette is shared rather than copied: a client that changes theme
/// sends the new one, and every pane already running has to start answering
/// with it. Two hardcoded colours used to live here, transcribed from the
/// TUI's palette — a program asking what the background is would have been
/// told a colour nobody was drawing the moment either copy moved.
#[derive(Clone)]
pub(super) struct ReplySink {
    pub(super) replies: mpsc::Sender<Vec<u8>>,
    pub(super) palette: Arc<Mutex<Palette>>,
}

impl ReplySink {
    pub(super) fn new(replies: mpsc::Sender<Vec<u8>>, palette: Arc<Mutex<Palette>>) -> Self {
        Self { replies, palette }
    }

    pub(super) fn color(&self, index: usize) -> Option<Rgb> {
        let palette = self.palette.lock().ok()?;
        let (r, g, b) = if index == NamedColor::Foreground as usize {
            palette.foreground
        } else if index == NamedColor::Background as usize {
            palette.background
        } else {
            *palette.ansi.get(index)?
        };
        Some(Rgb { r, g, b })
    }
}

impl EventListener for ReplySink {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(reply) => {
                let _ = self.replies.send(reply.into_bytes());
            }
            // `Term::dynamic_color_sequence` (OSC 10/11/12 queries) never
            // sends a `PtyWrite` itself — it hands back a formatting
            // closure expecting the *caller* to resolve the color and
            // write the reply. Left unhandled, a query like Codex's OSC 11
            // background probe just hangs until it times out server-side,
            // so the query answers here instead of falling through.
            Event::ColorRequest(index, format) => {
                if let Some(color) = self.color(index) {
                    let _ = self.replies.send(format(color).into_bytes());
                }
            }
            _ => {}
        }
    }
}

impl PaneRuntime {
    pub(super) fn spawn(
        id: PaneId,
        cwd: PathBuf,
        columns: u16,
        rows: u16,
        damage: mpsc::Sender<PaneId>,
        launch: Launch,
        palette: Arc<Mutex<Palette>>,
    ) -> Result<Self, RuntimeError> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| RuntimeError::Pty(error.to_string()))?;
        let mut command = match launch.argv().split_first() {
            Some((program, args)) => {
                let mut builder = CommandBuilder::new(program);
                builder.args(args);
                builder
            }
            None => CommandBuilder::new(host::default_shell()),
        };
        command.cwd(cwd);
        // `CommandBuilder` seeds a pane from *this* process's environment,
        // and this process is the server — started by whatever `uze`
        // invocation first needed one, which in this project is routinely a
        // `uze` run from inside a shimmed agent. Without this every plain
        // shell would inherit that agent's identity stamp, report as the
        // agent in the sidebar, persist as one, and be relaunched as one on
        // the next restart. A pane's environment may only carry what that
        // pane's own launch put there.
        for inherited in crate::launch::STAMPED_VARIABLES {
            command.env_remove(inherited);
        }
        if let Some(first) = PANE_PATH_FIRST.get() {
            command.env("PATH", path_with_first(env::var_os("PATH"), first));
        }
        for (name, value) in launch.env() {
            command.env(name, value);
        }
        // What tells a `uze` started inside this pane that it is inside one,
        // so it opens a space here instead of a client within a client.
        command.env(crate::launch::PANE_VARIABLE, id.0.to_string());
        host::prepare_pane(&mut command);
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| RuntimeError::Pty(error.to_string()))?;
        let group = child
            .process_id()
            .and_then(uze_platform::process::Group::adopt)
            .map(Arc::new);
        let endpoints = pair.master.try_clone_reader().and_then(|reader| {
            pair.master
                .take_writer()
                .map(|writer| (reader, Arc::new(Mutex::new(writer))))
        });
        let (reader, writer) = match endpoints {
            Ok(endpoints) => endpoints,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RuntimeError::Pty(error.to_string()));
            }
        };
        let (reply_sender, reply_receiver) = mpsc::channel();
        let terminal = Arc::new(Mutex::new(Term::new(
            Config::default(),
            &TermSize::new(columns as usize, rows as usize),
            ReplySink::new(reply_sender, palette),
        )));
        let parser_terminal = Arc::clone(&terminal);
        let selection = Arc::new(Mutex::new(PaneSelection::default()));
        let parser_selection = Arc::clone(&selection);
        thread::spawn(move || {
            let mut reader = reader;
            let mut parser: Processor = Processor::new();
            let mut buffer = [0; 8192];
            loop {
                match std::io::Read::read(&mut reader, &mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let mut terminal = parser_terminal.lock().expect("terminal poisoned");
                        parser.advance(&mut *terminal, &buffer[..read]);
                        parser_selection
                            .lock()
                            .expect("selection poisoned")
                            .observe(&terminal);
                        drop(terminal);
                        let _ = damage.send(id);
                    }
                }
            }
        });
        let reply_writer = Arc::clone(&writer);
        thread::spawn(move || {
            while let Ok(bytes) = reply_receiver.recv() {
                if let Ok(mut writer) = reply_writer.lock() {
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
            }
        });
        Ok(Self {
            id,
            master: Mutex::new(Some(pair.master)),
            writer,
            child: Arc::new(Mutex::new(child)),
            group,
            terminal,
            launch,
            last_sent: Mutex::new(None),
            selection,
        })
    }

    pub(super) fn write(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    pub(super) fn scroll(&self, lines: i32) -> bool {
        let mut terminal = self.terminal.lock().expect("terminal poisoned");
        let before = terminal.grid().display_offset();
        terminal.scroll_display(Scroll::Delta(lines));
        terminal.grid().display_offset() != before
    }

    pub(super) fn select(&self, gesture: SelectionGesture) -> bool {
        let mut terminal = self.terminal.lock().expect("terminal poisoned");
        self.selection
            .lock()
            .expect("selection poisoned")
            .apply(&mut terminal, gesture)
    }

    pub(super) fn selected_text(&self) -> String {
        let terminal = self.terminal.lock().expect("terminal poisoned");
        self.selection
            .lock()
            .expect("selection poisoned")
            .text(&terminal)
    }

    pub(super) fn resize(&self, columns: u16, rows: u16) {
        if let Some(master) = self.master.lock().expect("master poisoned").as_ref() {
            let _ = master.resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        self.terminal
            .lock()
            .expect("terminal poisoned")
            .resize(TermSize::new(columns as usize, rows as usize));
    }
    /// Ends the pane's process and reaps it, on a thread of its own.
    ///
    /// `kill` is a SIGHUP with a grace period of up to a fifth of a second
    /// and then a SIGKILL nobody waits on: done inline it held the request
    /// that closed the tab for that long, and left every pane whose program
    /// outlived the grace period a zombie for the life of the server.
    /// Ends the pane's process and everything it started, and reaps it,
    /// on a thread of its own: done inline it held the request that closed
    /// the tab, and left a program that outlived its hangup a zombie for
    /// the life of the server.
    pub(super) fn stop(&self) -> thread::JoinHandle<()> {
        let child = Arc::clone(&self.child);
        let group = self.group.clone();
        thread::spawn(move || {
            let mut child = child.lock().expect("child poisoned");
            // Waited on whether or not the kill landed: a program that
            // already exited is exactly the zombie this is here to reap.
            let _ = child.kill();
            // What the leader started and left behind: a harness's workers
            // that ignore the hangup would otherwise outlive the pane, and
            // hold its terminal open so its reader never ends either.
            if let Some(group) = group {
                group.end();
            }
            let _ = child.wait();
        })
    }

    /// Ends what a finished agent left running in its group: workers that
    /// ignore the hangup keep the old terminal open, and its reader alive.
    /// Called only right after [`PaneRuntime::finished_agent`] reaped the
    /// leader.
    pub(super) fn end_leftovers(&self) {
        if let Some(group) = &self.group {
            group.end();
        }
    }

    pub(super) fn finished_agent(&self) -> bool {
        matches!(self.launch, Launch::Program { .. })
            && self
                .child
                .lock()
                .expect("child poisoned")
                .try_wait()
                .ok()
                .flatten()
                .is_some()
    }

    /// Best-effort `(cwd, process name)` for whatever is currently running
    /// in the foreground of this pane — the same two facts `tmux` shows as
    /// `pane_current_path`/`pane_current_command`, asked of the kernel
    /// through [`uze_platform::probe`]. `None` when the platform cannot answer, or
    /// when the process exited between the group-leader lookup and the read.
    pub(super) fn foreground_status(&self) -> Option<(PathBuf, String)> {
        let pgid = self.foreground_process()?;
        let cwd = uze_platform::probe::current_directory_of(pgid)?;
        let process =
            shim_launched_name(pgid).or_else(|| uze_platform::probe::command_name_of(pgid))?;
        Some((cwd, process))
    }

    /// Whether the foreground process carries the stamp of the launcher
    /// that started it, read from its own environment (`/proc` on Linux,
    /// `KERN_PROCARGS2` on macOS). `None` when there is no foreground
    /// process to ask.
    ///
    /// The shim itself, caught before its `exec`, is the launcher at work:
    /// it already answers to the harness's name (`comm` is the symlink it
    /// was run through) but carries no stamp yet, because the stamp is only
    /// in the environment it hands the harness. The probe made right after
    /// a spawn lands in exactly that window, so reading it as a bypass
    /// warned about every agent the workspace launched.
    pub(super) fn foreground_through_launcher(&self) -> Option<bool> {
        let pgid = self.foreground_process()?;
        Some(shim_launched_name(pgid).is_some() || runs_uze(pgid))
    }

    /// The process in the foreground of this pane, as the platform knows
    /// it (see [`host::foreground`]).
    fn foreground_process(&self) -> Option<u32> {
        let master = self.master.lock().expect("master poisoned");
        host::foreground(&**master.as_ref()?, self.group.as_deref())
    }

    pub(super) fn snapshot(&self) -> PaneSnapshot {
        let terminal = self.terminal.lock().expect("terminal poisoned");
        snapshot_selected(
            self.id,
            &terminal,
            &self.selection.lock().expect("selection poisoned"),
        )
    }

    /// Hands `send` a full snapshot and remembers it as the baseline for
    /// the next [`PaneRuntime::damage_since_last`] diff — used for the rare
    /// whole-session broadcasts (attach, tab create/select), which a newly
    /// attached client has no prior state to diff against.
    pub(super) fn offer_repaint(&self, send: impl FnOnce(PaneDamage)) {
        let mut last_sent = self.last_sent.lock().expect("last_sent poisoned");
        let current = self.snapshot();
        *last_sent = Some(current.clone());
        send(whole_pane(current));
    }

    /// Hands `send` the damage since the last baseline. The baseline lock
    /// is held from the snapshot through the send, because two threads
    /// diffing the same pane at once would otherwise store the older
    /// snapshot as the baseline, or deliver their events out of order, and
    /// either leaves stale cells on screen until the pane next changes.
    ///
    /// Damage that changes nothing a client draws — no cell, and the
    /// cursor, the modes and the shape all as last sent — is not sent: a
    /// program repainting what is already there still woke the PTY reader,
    /// and every client then drew a frame for nothing.
    pub(super) fn offer_damage(&self, send: impl FnOnce(&PaneDamage)) {
        let mut last_sent = self.last_sent.lock().expect("last_sent poisoned");
        let before = last_sent.as_ref().map(drawn_state);
        let damage = self.diff_against(&mut last_sent);
        let unchanged = damage.changed.is_empty()
            && before
                == Some((
                    damage.columns,
                    damage.rows,
                    damage.cursor,
                    damage.alternate_screen,
                    damage.mouse,
                    damage.bracketed_paste,
                ));
        if !unchanged {
            send(&damage);
        }
    }

    #[cfg(test)]
    pub(super) fn damage_since_last(&self) -> PaneDamage {
        self.diff_against(&mut self.last_sent.lock().expect("last_sent poisoned"))
    }

    /// The steady-state update: only the cells that changed since the
    /// baseline this pane last sent (a full snapshot, or a previous
    /// damage event). Falls back to "every cell changed" the first time,
    /// or whenever dimensions moved since the baseline — a resize can't be
    /// expressed as a sparse diff against a differently-shaped grid.
    pub(super) fn diff_against(&self, last_sent: &mut Option<PaneSnapshot>) -> PaneDamage {
        let current = self.snapshot();
        let same_shape = last_sent.as_ref().is_some_and(|previous| {
            previous.columns == current.columns && previous.rows == current.rows
        });
        let changed = match last_sent.as_ref() {
            Some(previous) if same_shape => current
                .cells
                .iter()
                .zip(previous.cells.iter())
                .enumerate()
                .filter(|(_, (new, old))| new != old)
                .map(|(index, (new, _))| cell_coordinates(index, current.columns, new.clone()))
                .collect(),
            _ => whole_pane(current.clone()).changed,
        };
        let damage = PaneDamage {
            pane: self.id,
            columns: current.columns,
            rows: current.rows,
            cursor: current.cursor,
            alternate_screen: current.alternate_screen,
            mouse: current.mouse,
            bracketed_paste: current.bracketed_paste,
            changed,
        };
        *last_sent = Some(current);
        damage
    }
}

/// The alias uze's PATH shim (`src/shim.rs`) launched this process group's
/// leader under, if any — read from `UZE_SHIM_NAME` in its live
/// environment. The shim sets this immediately before `exec`ing into the
/// real binary, so it survives on the same pid for the rest of the
/// process's life, unlike `comm`, which a harness is free to overwrite
/// (Claude Code sets its own title to its version string, erasing the name
/// a person actually typed). `None` for anything not launched through the
/// shim — a bypassed launch, a harness that isn't shimmed, or a plain
/// shell — in which case `foreground_status` falls back to `comm`.
///
/// The name is accepted only from the process the shim launched.
/// `UZE_SHIM_NAME` is an ordinary environment variable: every child of a
/// shimmed agent inherits it, so a shell running *under* one would
/// otherwise answer with its ancestor's identity. `UZE_SHIM_PID` carries
/// the shim's own pid, and only the program it ran in its place
/// (`uze_platform::process::launched_by`) answers to it.
pub(super) fn shim_launched_name(pgid: u32) -> Option<String> {
    let stamped: u32 =
        uze_platform::probe::environment_value_of(pgid, crate::launch::SHIM_PID_VARIABLE)?
            .trim()
            .parse()
            .ok()?;
    if !uze_platform::process::launched_by(pgid, stamped) {
        return None;
    }
    uze_platform::probe::environment_value_of(pgid, crate::launch::SHIM_NAME_VARIABLE)
}

pub(super) fn cell_coordinates(
    index: usize,
    columns: u16,
    cell: RenderCell,
) -> (u16, u16, RenderCell) {
    let row = (index / usize::from(columns)) as u16;
    let column = (index % usize::from(columns)) as u16;
    (row, column, cell)
}

/// Writes one client's events onto its socket until there are no more, or
/// until one of them cannot be written.
///
/// A frame that will not go out ends the connection rather than the writer
/// alone. Dropping only this thread's dup of the socket leaves the reader
/// half open, so the peer sees no EOF: its `Attach` succeeded, nothing
/// follows, and it sits on chrome that still looks live while the events it
/// will never read pile up in a channel nobody drains. Shutting both
/// halves is what makes the failure arrive where a client can act on it —
/// as the disconnect it actually is.
///
/// "No more" is the channel hanging up, which happens once the last
/// [`Outbox`] is dropped — by the handler that served this client, and by
/// [`Server::clients`]. That is why this is handed a [`Backlog`] and not
/// the outbox itself: see the note on `Backlog`.
pub(super) fn forward_events(
    mut socket: Stream,
    events: &mpsc::Receiver<ClientEvent>,
    backlog: &Backlog,
) {
    while let Ok(event) = events.recv() {
        backlog.delivered();
        if let Err(error) = write_message(&mut socket, &event) {
            tracing::warn!(%error, "dropping a terminal client an event could not reach");
            let _ = socket.shutdown(std::net::Shutdown::Both);
            return;
        }
    }
}

/// A pane's whole grid said as damage — every cell "changed" — which is
/// how [`Server::broadcast_snapshot`] repaints one pane in one frame. The
/// same thing a resize already sends, and the shape the frame limit is
/// measured against (`a_full_repaint_of_the_largest_pane_fits_in_one_frame`
/// weighs a damage cell, the widest of the two).
pub(super) fn whole_pane(snapshot: PaneSnapshot) -> PaneDamage {
    let columns = snapshot.columns;
    PaneDamage {
        pane: snapshot.pane,
        columns,
        rows: snapshot.rows,
        cursor: snapshot.cursor,
        alternate_screen: snapshot.alternate_screen,
        mouse: snapshot.mouse,
        bracketed_paste: snapshot.bracketed_paste,
        changed: snapshot
            .cells
            .into_iter()
            .enumerate()
            .map(|(index, cell)| cell_coordinates(index, columns, cell))
            .collect(),
    }
}

#[cfg(test)]
pub(super) fn snapshot(pane: PaneId, terminal: &Term<ReplySink>) -> PaneSnapshot {
    snapshot_selected(pane, terminal, &PaneSelection::default())
}

pub(super) fn snapshot_selected(
    pane: PaneId,
    terminal: &Term<ReplySink>,
    selection: &PaneSelection,
) -> PaneSnapshot {
    let content = terminal.renderable_content();
    let highlight = selection.highlight(terminal);
    let columns = terminal.grid().columns() as u16;
    let rows = terminal.grid().screen_lines() as u16;
    let cells = terminal
        .grid()
        .display_iter()
        .map(|indexed| {
            let cell = indexed.cell;
            RenderCell {
                character: cell.c,
                foreground: color(cell.fg),
                background: color(cell.bg),
                attributes: CellAttributes {
                    bold: cell.flags.contains(Flags::BOLD),
                    dim: cell.flags.contains(Flags::DIM),
                    italic: cell.flags.contains(Flags::ITALIC),
                    underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
                    inverse: cell.flags.contains(Flags::INVERSE),
                    hidden: cell.flags.contains(Flags::HIDDEN),
                    strikeout: cell.flags.contains(Flags::STRIKEOUT),
                    selected: highlight.contains(indexed.point),
                },
            }
        })
        .collect();
    let mode = content.mode;
    PaneSnapshot {
        pane,
        columns,
        rows,
        cursor: Cursor {
            column: content.cursor.point.column.0 as u16,
            row: content.cursor.point.line.0 as u16,
        },
        alternate_screen: mode.contains(TermMode::ALT_SCREEN),
        mouse: mouse_mode(mode),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        cells,
    }
}

pub(super) fn mouse_mode(mode: TermMode) -> MouseMode {
    MouseMode {
        reports_clicks: mode.intersects(
            TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION,
        ),
        reports_drag: mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION),
        sgr: mode.contains(TermMode::SGR_MOUSE),
    }
}

pub(super) fn color(color: EngineColor) -> TerminalColor {
    match color {
        EngineColor::Indexed(index) => TerminalColor::Indexed(index),
        EngineColor::Spec(rgb) => TerminalColor::Rgb {
            red: rgb.r,
            green: rgb.g,
            blue: rgb.b,
        },
        EngineColor::Named(NamedColor::Background) => TerminalColor::DefaultBackground,
        EngineColor::Named(NamedColor::Foreground) => TerminalColor::DefaultForeground,
        EngineColor::Named(named) => TerminalColor::Indexed(named as u8),
    }
}

/// `session` with `selection` overlaid: the client's space when it still
/// exists, and its tab in every space where the tab still exists.
pub(super) fn view_for(session: &Session, selection: &Selection) -> Session {
    let mut view = session.clone();
    if let Some(space) = selection.space
        && view.workspace.spaces.iter().any(|s| s.id == space)
    {
        view.workspace.selected_space = space;
    }
    for space in &mut view.workspace.spaces {
        if let Some(tab) = selection.tabs.get(&space.id)
            && space.tabs.iter().any(|t| t.id == *tab)
        {
            space.selected_tab = *tab;
        }
    }
    view
}

/// Everything about a pane a client draws besides its cells.
fn drawn_state(snapshot: &PaneSnapshot) -> (u16, u16, Cursor, bool, MouseMode, bool) {
    (
        snapshot.columns,
        snapshot.rows,
        snapshot.cursor,
        snapshot.alternate_screen,
        snapshot.mouse,
        snapshot.bracketed_paste,
    )
}
