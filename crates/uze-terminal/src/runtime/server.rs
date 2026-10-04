//! The server: the session, its clients and its panes, and the threads that keep them current.

use super::*;

pub(super) struct Server {
    pub(super) session: Mutex<Session>,
    pub(super) panes: Mutex<BTreeMap<PaneId, Arc<PaneRuntime>>>,
    pub(super) clients: Mutex<Vec<Client>>,
    pub(super) next_client: std::sync::atomic::AtomicU64,
    pub(super) stopped: Mutex<bool>,
    pub(super) stop_requested: Condvar,
    pub(super) socket: PathBuf,
    /// Held for as long as this server exists — see [`WorkspaceLock`].
    pub(super) _workspace: WorkspaceLock,
    /// Serializes [`Server::persist`], so two structural changes landing at
    /// once cannot rename an older picture of the workspace over a newer
    /// one, and holds the bytes last written: selecting a tab broadcasts
    /// the session without changing anything persisted, and an fsync of
    /// the same bytes on every switch was latency a person felt.
    pub(super) persisting: Mutex<Option<Vec<u8>>>,
    /// Cloned into every [`PaneRuntime`] so its PTY reader thread can report
    /// new output; [`spawn_damage_broadcaster`] owns the matching receiver.
    pub(super) damage: mpsc::Sender<PaneId>,
    /// What a pane's own program is told when it asks the terminal what
    /// colours it is drawn in. Shared with every pane already running, so a
    /// client changing theme changes the answer everywhere at once rather
    /// than only for panes opened afterwards.
    pub(super) palette: Arc<Mutex<Palette>>,
    /// The workspace this runtime could not carry across, held until a
    /// client is there to be told.
    ///
    /// Held rather than logged, and held rather than dropped: the runtime
    /// starts before any client attaches, and the one time this happened
    /// the only record was a `tracing::warn!` to a file sink that is off
    /// unless `UZE_LOG` is set. An operator watched every space disappear
    /// with no sentence anywhere.
    pub(super) set_aside: Mutex<Option<uze_document::SetAside>>,
}

impl Server {
    pub(super) fn new(
        seat: SpaceSeat,
        socket: PathBuf,
    ) -> Result<(Self, mpsc::Receiver<PaneId>), RuntimeError> {
        // Taken before anything is read: restoring a workspace a live
        // server already holds is what turns one set of agents into two.
        let workspace_lock = WorkspaceLock::acquire()?;
        // A previous run's shape, if this workspace has one — see
        // `persisted_state_path` for why a crash, a `kill -9`, or a reboot
        // still leaves this behind even though nothing else about a pane's
        // running state survives any of those.
        let (restored, set_aside) = load_persisted_workspace_at(&persisted_state_path());
        let (session, launches) = restored
            .and_then(|persisted| Session::restore(persisted.spaces))
            .unwrap_or_else(|| {
                let (columns, rows) = PLACEHOLDER_PANE_SIZE;
                let session = Session::new(seat, columns, rows);
                let bootstrap = session.selected_tab().pane.id;
                (session, vec![(bootstrap, Launch::Shell)])
            });
        let (damage, damage_events) = mpsc::channel();
        let server = Self {
            session: Mutex::new(session),
            panes: Mutex::new(BTreeMap::new()),
            clients: Mutex::new(Vec::new()),
            next_client: std::sync::atomic::AtomicU64::new(1),
            stopped: Mutex::new(false),
            stop_requested: Condvar::new(),
            socket,
            _workspace: workspace_lock,
            persisting: Mutex::new(None),
            damage,
            palette: Arc::new(Mutex::new(Palette::default())),
            set_aside: Mutex::new(set_aside),
        };
        for (pane, launch) in launches {
            // A persisted program is a guess (an agent binary that may
            // since be uninstalled or renamed, or a best-effort relaunch
            // built from a live process name — see
            // `relaunch_command_for_process`) — one bad guess must never
            // keep the rest of a restored workspace from coming back, so a
            // failed program retries as a shell; a shell failing to spawn is
            // fatal.
            let guessed = matches!(launch, Launch::Program { .. });
            let spawned = server.spawn_pane(pane, launch);
            if spawned.is_err() && guessed {
                let _ = server.spawn_pane(pane, Launch::Shell);
            } else {
                spawned?;
            }
        }
        Ok((server, damage_events))
    }

    /// Best-effort snapshot of the current space/tab shape to
    /// [`persisted_state_path`] — called from [`Server::broadcast_session`]
    /// (every structural change: a tab/space created, closed, renamed, or
    /// moved to a new cwd), so whatever's on disk is never more than one
    /// change stale, however this process eventually stops.
    pub(super) fn persist(&self) {
        let mut written = self.persisting.lock().expect("persist state poisoned");
        let path = persisted_state_path();
        let panes = self.panes.lock().expect("panes poisoned");
        let session = self.session.lock().expect("session poisoned");
        let workspace =
            PersistedWorkspace {
                schema_version: WORKSPACE_SCHEMA_VERSION,
                spaces: session
                    .workspace
                    .spaces
                    .iter()
                    .map(|space| SpaceSeed {
                        label: space.label.clone(),
                        root: space.root.clone(),
                        tabs: space
                            .tabs
                            .iter()
                            .map(|tab| {
                                // By position, since a restored tab is minted a
                                // fresh id — and against this same list, which
                                // is the one `Session::restore` will rebuild.
                                let agent = tab.agent.and_then(|agent| {
                                    space.tabs.iter().position(|other| other.id == agent)
                                });
                                // A shell with something other than a shell now
                                // running in it (someone typed `claude` straight
                                // into it) had an agent as much as a launched
                                // one did — restoring it to a bare shell would
                                // silently drop that. What was typed was
                                // launched by nobody, so it carries no
                                // environment.
                                let launch =
                                    match panes.get(&tab.pane.id).map(|runtime| &runtime.launch) {
                                        Some(launch @ Launch::Program { .. }) => launch.clone(),
                                        _ => relaunch_command_for_process(&tab.pane.process)
                                            .map_or(Launch::Shell, |argv| Launch::Program {
                                                argv,
                                                env: Vec::new(),
                                            }),
                                    };
                                TabSeed {
                                    label: tab.label.clone(),
                                    cwd: tab.pane.cwd.clone(),
                                    agent,
                                    launch,
                                }
                            })
                            .collect(),
                    })
                    .collect(),
            };
        drop(session);
        drop(panes);
        match serde_json::to_vec(&workspace) {
            Ok(json) if written.as_ref() == Some(&json) => {}
            Ok(json) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match write_atomically(&path, &json) {
                    Ok(()) => *written = Some(json),
                    Err(error) => {
                        tracing::warn!(path = %path.display(), %error, "could not persist the workspace")
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not describe the workspace to persist it")
            }
        }
    }

    pub(super) fn handle_client(self: Arc<Self>, stream: Stream) {
        let _span = tracing::info_span!("terminal.client").entered();
        let reader_stream = match stream.try_clone() {
            Ok(value) => value,
            Err(_) => return,
        };
        let (outbox, receiver) = Outbox::new();
        let events = Arc::new(outbox);
        let backlog = events.backlog();
        thread::spawn(move || forward_events(stream, &receiver, &backlog));

        // A deadline on the handshake only — see [`HANDSHAKE_DEADLINE`] —
        // and a frame limit sized for what a handshake actually says rather
        // than for the largest repaint this wire ever carries: nothing has
        // vouched for this peer yet.
        let mut reader = BufReader::new(Handshake::new(reader_stream, HANDSHAKE_DEADLINE));
        let first = read_message_within::<_, ClientRequest>(&mut reader, MAX_HANDSHAKE_FRAME);
        let attached = match first {
            // Stopping needs no client and no session: the workspace claim
            // makes a live server refuse every replacement of the same
            // build, so `uze workspace stop` failing to be heard by one
            // nobody attached to left no way back in but a manual `kill`.
            Ok(Some(ClientRequest::Stop)) => {
                // Answered on the socket rather than through the writer
                // thread: the acknowledgement has to be on the wire before
                // the accept loop is woken, or the process can exit out
                // from under a frame still sitting in a channel. Nothing
                // else is ever sent on this connection — no client was
                // registered — so there is nothing for this to interleave
                // with.
                let answered = write_message(reader.get_mut().socket(), &ClientEvent::Stopped);
                if let Err(error) = answered {
                    tracing::warn!(%error, "could not acknowledge a stop request");
                }
                self.shut_down();
                return;
            }
            Ok(Some(ClientRequest::Attach {
                version,
                columns,
                rows,
                seating,
            })) if version == PROTOCOL_VERSION => {
                let client = self
                    .next_client
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let drawn = (columns > 0 && rows > 0)
                    .then(|| (within_pane_bounds(columns), within_pane_bounds(rows)));
                let mut selection = Selection::default();
                let mut sized_at_creation = false;
                match seating {
                    // Nothing to say about where to land: the session's own
                    // selection answers.
                    Seating::WhereItLeftOff => {}
                    // A place, not a request. A seat with no space is a
                    // workspace this client has nothing to add to, so it
                    // lands where the session already is.
                    Seating::At(seat) => {
                        selection.space = self
                            .session
                            .lock()
                            .expect("session poisoned")
                            .space_for(&seat);
                    }
                    Seating::Open(seat) => {
                        match self.ensure_space(&seat, drawn.unwrap_or(PLACEHOLDER_PANE_SIZE)) {
                            Ok(OpenedSpace::Existing(space)) => selection.space = Some(space),
                            Ok(OpenedSpace::Created(NewSpace { space, .. })) => {
                                selection.space = Some(space);
                                sized_at_creation = true;
                            }
                            Err(error) => {
                                events.reply(ClientEvent::Error {
                                    message: format!(
                                        "could not open a space at {}: {error}",
                                        seat.root.display()
                                    ),
                                });
                            }
                        }
                    }
                }
                self.clients.lock().expect("clients poisoned").push(Client {
                    id: client,
                    events: Arc::clone(&events),
                    selection,
                });
                if let Some((columns, rows)) = drawn
                    && !sized_at_creation
                {
                    self.resize_pane(self.selected_pane_of(client), columns, rows);
                }
                self.broadcast_snapshot();
                // Said once, to the first client that arrives. The runtime
                // starts before anyone is watching, so this waits rather
                // than going to a log nobody turned on — which is how an
                // operator once watched every space disappear with no
                // sentence anywhere.
                if let Some(moved) = self.set_aside.lock().expect("set-aside poisoned").take() {
                    events.reply(ClientEvent::WorkspaceSetAside {
                        kept_at: moved.path,
                        reason: moved.reason,
                    });
                }
                Some(client)
            }
            Ok(Some(ClientRequest::Attach { .. })) => {
                events.reply(ClientEvent::Error {
                    message: "incompatible terminal runtime protocol".into(),
                });
                None
            }
            _ => None,
        };
        let Some(client) = attached else {
            return;
        };
        // Attached, so silence is a person reading rather than a peer
        // holding threads it never intends to use.
        reader.get_mut().attached();

        // A selection is drawn for every client, but only the one that
        // made it can end it; one that leaves, however it leaves, takes
        // its selections with it.
        let mut selecting = std::collections::BTreeSet::new();
        while let Ok(Some(request)) = read_message::<_, ClientRequest>(&mut reader) {
            // A keystroke is a request too, and there are thousands: debug
            // level, so a trace of the server is what a person did to it
            // unless they asked for every byte.
            let _span =
                tracing::debug_span!("terminal.request", kind = request.kind(), client).entered();
            match request {
                ClientRequest::Detach => {
                    events.reply(ClientEvent::Detached);
                    break;
                }
                ClientRequest::SetPalette(palette) => self.set_palette(palette),
                ClientRequest::Input { pane, bytes } => self.write_input(pane, &bytes),
                ClientRequest::Scroll { pane, lines } => self.scroll_pane(pane, lines),
                ClientRequest::Select { pane, gesture } => {
                    selecting.insert(pane);
                    self.select_in_pane(pane, gesture);
                }
                ClientRequest::CopySelection { pane } => {
                    let text = self
                        .runtime(pane)
                        .map(|runtime| runtime.selected_text())
                        .unwrap_or_default();
                    // A drag over nothing but blanks selected nothing worth
                    // keeping drawn.
                    if text.is_empty() {
                        self.select_in_pane(pane, SelectionGesture::Clear);
                    }
                    events.reply(ClientEvent::SelectionText { pane, text });
                }
                ClientRequest::Resize {
                    pane,
                    columns,
                    rows,
                } => self.resize_pane(pane, within_pane_bounds(columns), within_pane_bounds(rows)),
                ClientRequest::CreateTab {
                    label,
                    agent,
                    columns,
                    rows,
                    cwd,
                    command,
                    env,
                } => {
                    // Refused before anything is created: a tab that exists
                    // without the launch it was asked for is worse than no
                    // tab, and a shell carrying a launch environment would
                    // report as an agent it is not.
                    let launch = match crate::launch::validate(command, env) {
                        Ok(launch) => launch,
                        Err(refusal) => {
                            events.reply(ClientEvent::Error {
                                message: refusal.to_string(),
                            });
                            continue;
                        }
                    };
                    let (pane, tab, space) = {
                        let mut session = self.session.lock().expect("session poisoned");
                        let space = self
                            .selection_of(client)
                            .space
                            .filter(|space| session.space(*space).is_some())
                            .unwrap_or(session.workspace.selected_space);
                        let cwd = cwd.unwrap_or_else(|| {
                            session
                                .space(space)
                                .map(|space| space.root.clone())
                                .unwrap_or_else(|| PathBuf::from("."))
                        });
                        let pane = session.add_tab(
                            space,
                            label,
                            agent,
                            within_pane_bounds(columns),
                            within_pane_bounds(rows),
                            cwd,
                        );
                        let tab = session
                            .space(space)
                            .expect("the space the tab was added to")
                            .selected_tab;
                        (pane, tab, space)
                    };
                    self.update_selection(client, |selection| {
                        selection.space = Some(space);
                        selection.tabs.insert(space, tab);
                    });
                    if self.spawn_pane(pane, launch).is_err() {
                        events.reply(ClientEvent::Error {
                            message: "could not create terminal pane".into(),
                        });
                    }
                    self.broadcast_session();
                }
                ClientRequest::SelectTab { tab } => {
                    let located = {
                        let mut session = self.session.lock().expect("session poisoned");
                        session.select_tab(tab);
                        session
                            .workspace
                            .spaces
                            .iter()
                            .find(|space| space.tabs.iter().any(|t| t.id == tab))
                            .map(|space| space.id)
                    };
                    if let Some(space) = located {
                        self.update_selection(client, |selection| {
                            selection.space = Some(space);
                            selection.tabs.insert(space, tab);
                        });
                        self.broadcast_session();
                    }
                }
                ClientRequest::CloseTab { tab } => {
                    let removed = self
                        .session
                        .lock()
                        .expect("session poisoned")
                        .remove_tab(tab);
                    match removed {
                        Some(panes) => {
                            self.stop_runtimes(&panes);
                            self.broadcast_session();
                        }
                        None => {
                            events.reply(ClientEvent::Error {
                                // An agent's shells go with it, so what is
                                // refused is a close that would empty the
                                // space — not always a single tab.
                                message: "cannot close a space's last tabs".into(),
                            });
                        }
                    }
                }
                ClientRequest::RenameTab { tab, label } => {
                    let changed = self
                        .session
                        .lock()
                        .expect("session poisoned")
                        .rename_tab(tab, label);
                    if changed {
                        self.broadcast_session();
                    }
                }
                ClientRequest::ReorderTab { tab, before } => {
                    let changed = self
                        .session
                        .lock()
                        .expect("session poisoned")
                        .reorder_tab(tab, before);
                    if changed {
                        self.broadcast_session();
                    }
                }
                ClientRequest::ReorderSpace { space, before } => {
                    let changed = self
                        .session
                        .lock()
                        .expect("session poisoned")
                        .reorder_space(space, before);
                    if changed {
                        self.broadcast_session();
                    }
                }
                ClientRequest::CreateSpace {
                    label,
                    seat,
                    columns,
                    rows,
                } => {
                    // Always a new space, even over a directory another
                    // space already holds (see `Session::create_space`):
                    // the prompt asked for a space, and one repository is
                    // routinely worth two — one per branch, one per thing
                    // being tried. `ensure_space` is the other question.
                    let root = seat.root.clone();
                    let (created, named) = {
                        let mut session = self.session.lock().expect("session poisoned");
                        let created = session.create_space(
                            label,
                            seat,
                            within_pane_bounds(columns),
                            within_pane_bounds(rows),
                        );
                        let named = session
                            .space(created.space)
                            .map(|space| space.label.clone())
                            .unwrap_or_default();
                        (created, named)
                    };
                    // The one record of a space appearing, with the name it
                    // ended up with: a repeated label is numbered here, and
                    // "where did `project 2` come from" is a question only
                    // this line can answer after the fact.
                    tracing::info!(root = %root.display(), label = %named, "a space was created");
                    self.spawn_new_space(client, created, &events);
                    self.broadcast_session();
                }
                ClientRequest::SelectSpace { space } => {
                    let exists = {
                        let mut session = self.session.lock().expect("session poisoned");
                        session.select_space(space);
                        session.space(space).is_some()
                    };
                    if exists {
                        self.update_selection(client, |selection| selection.space = Some(space));
                        self.broadcast_session();
                    }
                }
                ClientRequest::CloseSpace {
                    space,
                    replacement,
                    columns,
                    rows,
                } => {
                    let removed = self.session.lock().expect("session poisoned").remove_space(
                        space,
                        replacement,
                        within_pane_bounds(columns),
                        within_pane_bounds(rows),
                    );
                    let Some(removed) = removed else {
                        continue;
                    };
                    self.stop_runtimes(&removed.panes);
                    if let Some(created) = removed.replacement {
                        self.spawn_new_space(client, created, &events);
                    }
                    self.broadcast_session();
                }
                ClientRequest::RenameSpace { space, label } => {
                    let changed = self
                        .session
                        .lock()
                        .expect("session poisoned")
                        .rename_space(space, label);
                    if changed {
                        self.broadcast_session();
                    }
                }
                ClientRequest::Stop => {
                    events.reply(ClientEvent::Stopped);
                    self.shut_down();
                    break;
                }
                ClientRequest::Attach { .. } => {}
            }
        }
        for pane in selecting {
            self.select_in_pane(pane, SelectionGesture::Clear);
        }
        self.clients
            .lock()
            .expect("clients poisoned")
            .retain(|attached| attached.id != client);
    }

    /// The space at `seat`, created — with its first shell pane, spawned at
    /// `size` — when none is.
    pub(super) fn ensure_space(
        &self,
        seat: &SpaceSeat,
        (columns, rows): (u16, u16),
    ) -> Result<OpenedSpace, RuntimeError> {
        let opened =
            self.session
                .lock()
                .expect("session poisoned")
                .open_space(seat.clone(), columns, rows);
        if let OpenedSpace::Created(NewSpace { pane, .. }) = opened {
            self.spawn_pane(pane, Launch::Shell)?;
        }
        Ok(opened)
    }

    /// Starts a space a client just brought into being and puts that client
    /// in it.
    pub(super) fn spawn_new_space(&self, client: u64, created: NewSpace, events: &Outbox) {
        if self.spawn_pane(created.pane, Launch::Shell).is_err() {
            events.reply(ClientEvent::Error {
                message: "could not create terminal pane".into(),
            });
        }
        self.update_selection(client, |selection| selection.space = Some(created.space));
    }

    pub(super) fn stop_runtimes(&self, panes: &[PaneId]) {
        let mut runtimes = self.panes.lock().expect("panes poisoned");
        for pane in panes {
            if let Some(runtime) = runtimes.remove(pane) {
                runtime.stop();
            }
        }
    }

    pub(super) fn selection_of(&self, client: u64) -> Selection {
        self.clients
            .lock()
            .expect("clients poisoned")
            .iter()
            .find(|attached| attached.id == client)
            .map(|attached| attached.selection.clone())
            .unwrap_or_default()
    }

    pub(super) fn update_selection(&self, client: u64, change: impl FnOnce(&mut Selection)) {
        if let Some(attached) = self
            .clients
            .lock()
            .expect("clients poisoned")
            .iter_mut()
            .find(|attached| attached.id == client)
        {
            change(&mut attached.selection);
        }
    }

    /// The session as `client` sees it: the shared structure with this
    /// client's own selection overlaid wherever it still points at
    /// something that exists.
    pub(super) fn view_of(&self, client: u64) -> Session {
        let selection = self.selection_of(client);
        let session = self.session.lock().expect("session poisoned");
        view_for(&session, &selection)
    }

    pub(super) fn selected_pane_of(&self, client: u64) -> PaneId {
        self.view_of(client).selected_tab().pane.id
    }
    /// Takes the attached client's palette. Every pane shares the one
    /// `Arc`, so panes that were already running answer with it too — a
    /// theme switch that only reached panes opened afterwards would leave
    /// the older ones telling their programs a colour nobody draws.
    pub(super) fn set_palette(&self, palette: Palette) {
        if let Ok(mut held) = self.palette.lock() {
            *held = palette;
        }
    }

    pub(super) fn spawn_pane(&self, pane_id: PaneId, launch: Launch) -> Result<(), RuntimeError> {
        let pane = self
            .session
            .lock()
            .expect("session poisoned")
            .pane(pane_id)
            .cloned()
            .ok_or_else(|| RuntimeError::Protocol("unknown pane".into()))?;
        let runtime = PaneRuntime::spawn(
            pane_id,
            pane.cwd,
            // The session's own record of a pane's size is bounded here too,
            // not only where a request arrives: it can come back from a
            // persisted workspace written by an older build that never
            // clamped one.
            spawnable_pane_bounds(pane.columns),
            spawnable_pane_bounds(pane.rows),
            self.damage.clone(),
            launch,
            Arc::clone(&self.palette),
        )?;
        {
            let mut session = self.session.lock().expect("session poisoned");
            // The tab reports the launch the server made, from the one
            // place that makes it: a respawn as a plain shell clears it.
            session.record_launch(pane_id, runtime.launch.env().to_vec());
            // Best-effort: label the sidebar tree with the real shell name
            // immediately instead of leaving the "shell" placeholder until
            // the next status tick.
            if let Some((cwd, process)) = runtime.foreground_status() {
                session.update_pane_status(pane_id, cwd, process);
            }
            if let Some(through) = runtime.foreground_through_launcher() {
                session.update_pane_launcher(pane_id, through);
            }
        }
        let runtime = Arc::new(runtime);
        let replaced = {
            let mut panes = self.panes.lock().expect("panes poisoned");
            // A tab closed while this pane was being spawned has already
            // stopped the runtimes it knew of; this one it never saw.
            let closed = self
                .session
                .lock()
                .expect("session poisoned")
                .pane(pane_id)
                .is_none();
            if closed {
                drop(panes);
                runtime.stop();
                return Err(RuntimeError::Protocol("unknown pane".into()));
            }
            panes.insert(pane_id, Arc::clone(&runtime))
        };
        // The replaced runtime's reader and reaper end with it rather than
        // with the server.
        if let Some(replaced) = replaced {
            replaced.stop();
        }
        // The reader is live before the pane is registered, and damage for
        // a pane the broadcaster cannot find yet is dropped. A program that
        // prints once and then waits — a harness's banner, then its prompt —
        // would never be drawn, so the pane is flushed once it can be found.
        let _ = self.damage.send(pane_id);
        Ok(())
    }

    /// Re-probes every pane's foreground process/cwd (see
    /// [`PaneRuntime::foreground_status`]) and broadcasts the session only
    /// if the sidebar tree would actually show something different —
    /// called on a slow tick (see [`spawn_status_ticker`]), never from the
    /// input/damage hot paths.
    pub(super) fn refresh_pane_status(&self) {
        self.restore_finished_agent_panes();
        // Probed after the map's lock is released: each probe reads `/proc`,
        // and the input and damage paths wait on that lock.
        let runtimes: Vec<(PaneId, Arc<PaneRuntime>)> = self
            .panes
            .lock()
            .expect("panes poisoned")
            .iter()
            .map(|(&id, runtime)| (id, Arc::clone(runtime)))
            .collect();
        let probes: Vec<(PaneId, PathBuf, String, bool)> = runtimes
            .iter()
            .filter_map(|(id, runtime)| {
                let through = runtime.foreground_through_launcher().unwrap_or(false);
                runtime
                    .foreground_status()
                    .map(|(cwd, process)| (*id, cwd, process, through))
            })
            .collect();
        if probes.is_empty() {
            return;
        }
        let mut changed = false;
        let mut session = self.session.lock().expect("session poisoned");
        for (pane, cwd, process, through) in probes {
            changed |= session.update_pane_status(pane, cwd, process);
            changed |= session.update_pane_launcher(pane, through);
        }
        drop(session);
        if changed {
            self.broadcast_session();
        }
    }

    /// An agent tab starts the agent directly as the PTY child so terminal
    /// input, including Ctrl+C, reaches it naturally. Once that child exits,
    /// there is no shell left in the PTY to accept the next command. Replace
    /// only those finished direct-agent panes with a fresh shell; ordinary
    /// shell panes intentionally stay closed when their shell exits.
    pub(super) fn restore_finished_agent_panes(&self) {
        let finished: Vec<PaneId> = self
            .panes
            .lock()
            .expect("panes poisoned")
            .iter()
            .filter(|(_, runtime)| runtime.finished_agent())
            .map(|(&pane, runtime)| {
                runtime.end_leftovers();
                pane
            })
            .collect();
        let mut restored = false;
        for pane in finished {
            if self.spawn_pane(pane, Launch::Shell).is_ok() {
                restored = true;
                self.broadcast_pane_damage(pane);
            }
        }
        if restored {
            self.broadcast_session();
        }
    }

    /// The pane's runtime, with the map's lock already released: a PTY
    /// write blocks for as long as the program in the pane is not reading,
    /// and holding `panes` across it would freeze every other pane.
    pub(super) fn runtime(&self, pane: PaneId) -> Option<Arc<PaneRuntime>> {
        self.panes
            .lock()
            .expect("panes poisoned")
            .get(&pane)
            .cloned()
    }

    pub(super) fn write_input(&self, pane: PaneId, bytes: &[u8]) {
        if let Some(runtime) = self.runtime(pane) {
            runtime.write(bytes);
        }
    }

    pub(super) fn scroll_pane(&self, pane: PaneId, lines: i32) {
        let changed = self
            .runtime(pane)
            .is_some_and(|runtime| runtime.scroll(lines));
        if changed {
            self.broadcast_pane_damage(pane);
        }
    }

    pub(super) fn select_in_pane(&self, pane: PaneId, gesture: SelectionGesture) {
        let changed = self
            .runtime(pane)
            .is_some_and(|runtime| runtime.select(gesture));
        if changed {
            self.broadcast_pane_damage(pane);
        }
    }

    pub(super) fn resize_pane(&self, pane: PaneId, columns: u16, rows: u16) {
        if let Some(runtime) = self.runtime(pane) {
            runtime.resize(columns, rows);
        }
        // A resize doesn't guarantee new PTY output on its own (an idle
        // shell prompt emits nothing after its terminal shrinks/grows), so
        // push the new dimensions immediately instead of waiting for the
        // next damage notification.
        self.broadcast_pane_damage(pane);
    }

    /// Sends only `pane`'s changed cells to every attached client — the
    /// steady-state update path, driven by PTY output instead of a client
    /// poll. Session/tab structure is unaffected, so only this pane's cells
    /// go out, and only the ones that actually changed since the last
    /// event this pane sent (see [`PaneRuntime::damage_since_last`]).
    pub(super) fn broadcast_pane_damage(&self, pane: PaneId) {
        let Some(runtime) = self.runtime(pane) else {
            return;
        };
        runtime.offer_damage(|damage| {
            self.clients
                .lock()
                .expect("clients poisoned")
                .retain(|client| client.events.offer(ClientEvent::Damage(damage.clone())));
        });
    }

    /// Sends just the tab/selection structure to every attached client —
    /// used by tab create/select/close. None of those change any pane's
    /// cells, and every open pane (selected or not) already stays current
    /// through its own damage pushes, so resending every pane's whole grid
    /// here (as tab-switching did before) was pure waste: a `SelectTab`
    /// that changes nothing about pane content was serializing thousands
    /// of unchanged cells per tab, which is what made switching tabs feel
    /// slow.
    pub(super) fn broadcast_session(&self) {
        self.persist();
        let session = self.session.lock().expect("session poisoned").clone();
        self.clients
            .lock()
            .expect("clients poisoned")
            .retain(|client| {
                client.events.offer(ClientEvent::SessionUpdated {
                    session: view_for(&session, &client.selection),
                })
            });
    }

    /// Repaints every pane on every attached client, as one frame per pane.
    ///
    /// One frame carrying them all is what [`MAX_FRAME`] does *not* bound:
    /// the cap is tied to a repaint of the largest single pane a client may
    /// ask for, so three panes at [`MAX_PANE_DIMENSION`] — a size
    /// `within_pane_bounds` permits, and one that is persisted across
    /// restarts — made the frame unsendable and every attached client sit
    /// frozen on live-looking chrome. Per pane, that cap is the real bound
    /// again.
    ///
    /// The `Snapshot` goes out first: it is what tells a client to forget
    /// the panes it has, and the repaints that follow are what give it the
    /// new ones. They are ordinary `Damage` frames naming every cell, which
    /// is what a client already applies to a pane it has never heard of
    /// (and what a resize already sends).
    pub(super) fn broadcast_snapshot(&self) {
        let session = self.session.lock().expect("session poisoned").clone();
        // The runtimes, not their grids: a repaint is built and handed on
        // one pane at a time, so the largest thing alive at once stays one
        // pane's worth rather than the whole workspace's.
        let panes: Vec<Arc<PaneRuntime>> = self
            .panes
            .lock()
            .expect("panes poisoned")
            .values()
            .cloned()
            .collect();
        self.clients
            .lock()
            .expect("clients poisoned")
            .retain(|client| {
                client.events.offer(ClientEvent::Snapshot {
                    session: view_for(&session, &client.selection),
                })
            });
        for pane in panes {
            pane.offer_repaint(|repaint| {
                self.clients
                    .lock()
                    .expect("clients poisoned")
                    .retain(|client| client.events.offer(ClientEvent::Damage(repaint.clone())));
            });
        }
    }

    /// Sends the whole workspace to every client that fell behind and has
    /// since drained what it was sent. Built from `snapshot`, not
    /// `offer_repaint`: the damage baseline is shared by every
    /// client, and resetting it for one would cost the others a change.
    pub(super) fn has_stale_client(&self) -> bool {
        self.clients
            .lock()
            .expect("clients poisoned")
            .iter()
            .any(|client| client.events.is_stale())
    }

    pub(super) fn resync_stale_clients(&self) {
        let stale = self
            .clients
            .lock()
            .expect("clients poisoned")
            .iter()
            .any(|client| client.events.awaits_resync());
        if !stale {
            return;
        }
        let session = self.session.lock().expect("session poisoned").clone();
        let repaints: Vec<PaneDamage> = self
            .panes
            .lock()
            .expect("panes poisoned")
            .values()
            .map(|pane| whole_pane(pane.snapshot()))
            .collect();
        self.clients
            .lock()
            .expect("clients poisoned")
            .retain(|client| {
                !client.events.awaits_resync()
                    || client
                        .events
                        .resync(view_for(&session, &client.selection), &repaints)
            });
    }

    /// Ends every pane, and returns once each has been reaped: the server
    /// exits right after, and a reaper still running when it does would
    /// leave the pane's process group behind. All at once, so shutdown
    /// waits for the slowest pane rather than for their sum.
    pub(super) fn stop_panes(&self) {
        let reapers: Vec<_> = self
            .panes
            .lock()
            .expect("panes poisoned")
            .values()
            .map(|pane| pane.stop())
            .collect();
        for reaper in reapers {
            let _ = reaper.join();
        }
    }

    /// Takes the server down: no new work, no live panes, [`serve`]
    /// released, and one connection of its own so [`accept_connections`]
    /// wakes from `accept` and reads the flag instead of blocking until
    /// somebody happens to attach.
    pub(super) fn shut_down(&self) {
        *self.stopped.lock().expect("stop state poisoned") = true;
        self.stop_requested.notify_all();
        self.stop_panes();
        let _ = transport::connect(&self.socket);
    }

    /// Blocks until [`Server::shut_down`] runs, and returns holding the
    /// stop flag.
    pub(super) fn await_stop(&self) -> std::sync::MutexGuard<'_, bool> {
        self.stop_requested
            .wait_while(
                self.stopped.lock().expect("stop state poisoned"),
                |stopped| !*stopped,
            )
            .expect("stop state poisoned")
    }
}

/// Coalesces damage notifications from every pane's PTY reader thread and
/// broadcasts one diff per dirty pane: whatever arrived while the previous
/// batch was being sent goes out together, so a continuously noisy pane is
/// bounded by how fast a batch is sent rather than by its output —
/// output-driven redraws instead of a fixed-rate client poll (the source of
/// the workspace client's earlier busy-refresh/CPU-starvation bug). The
/// 8ms tick only runs while a client is stale, to resynchronize it.
pub(super) fn spawn_damage_broadcaster(server: Arc<Server>, damage: mpsc::Receiver<PaneId>) {
    thread::spawn(move || {
        let mut dirty = std::collections::BTreeSet::new();
        loop {
            // Idle, nothing wakes the thread but new damage. A stale client
            // is the exception: its catching up sends no damage, so the
            // resync below has to be looked for on the tick.
            let next = if server.has_stale_client() {
                damage.recv_timeout(Duration::from_millis(8))
            } else {
                damage
                    .recv()
                    .map_err(|mpsc::RecvError| mpsc::RecvTimeoutError::Disconnected)
            };
            match next {
                Ok(pane) => {
                    dirty.insert(pane);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // Absorb whatever else arrived while broadcasting the last
            // batch, without blocking — this is what keeps a continuously
            // noisy pane (e.g. `yes`) flushing on this ~8ms cadence instead
            // of starving until output goes quiet.
            while let Ok(pane) = damage.try_recv() {
                dirty.insert(pane);
            }
            for pane in std::mem::take(&mut dirty) {
                server.broadcast_pane_damage(pane);
            }
            server.resync_stale_clients();
        }
    });
}

/// Drives [`Server::refresh_pane_status`] on a slow, fixed cadence — cwd and
/// foreground-process are sidebar-tree labels, not terminal content, so
/// they don't need (and shouldn't cost) damage-path freshness.
pub(super) const STATUS_PROBE_INTERVAL: Duration = Duration::from_secs(1);

pub(super) fn spawn_status_ticker(server: Arc<Server>) {
    thread::spawn(move || {
        loop {
            server.refresh_pane_status();
            // Woken by the stop itself, so a stopping server does not probe
            // its panes once more after taking them down.
            let (stopped, _) = server
                .stop_requested
                .wait_timeout_while(
                    server.stopped.lock().expect("stop state poisoned"),
                    STATUS_PROBE_INTERVAL,
                    |stopped| !*stopped,
                )
                .expect("stop state poisoned");
            if *stopped {
                break;
            }
        }
    });
}

/// Puts back the directory the endpoint lives in, held to the same
/// ownership and mode [`socket_path`] demanded of it in the first place — a
/// cleaner that took the socket usually took the directory too.
#[cfg(unix)]
pub(super) fn restore_endpoint_directory(socket: &Path) -> io::Result<()> {
    let Some(directory) = socket.parent() else {
        return Ok(());
    };
    fs::create_dir_all(directory)?;
    private_directory(directory, current_uid())
}

/// The process group `pid` leads, when it leads one of its own: a pane's
/// program is started in a session of its own, so its group is its pid.
/// `None` for anything else — above all this process's own group, which a
/// group signal must never reach.
#[cfg(unix)]
pub(super) fn own_process_group(pid: u32) -> Option<libc::pid_t> {
    let pid = libc::pid_t::try_from(pid).ok().filter(|pid| *pid > 1)?;
    // SAFETY: `getpgid` reads the group of a positive pid and touches no
    // memory of ours; `getpgrp` takes no arguments and cannot fail.
    let (group, ours) = unsafe { (libc::getpgid(pid), libc::getpgrp()) };
    (group == pid && group != ours).then_some(group)
}

/// The real user id of this process.
#[cfg(unix)]
pub(super) fn current_uid() -> libc::uid_t {
    // SAFETY: `getuid` takes no arguments, cannot fail, and touches no
    // memory of ours.
    unsafe { libc::getuid() }
}

/// What identifies the socket a server bound, so a later look at the same
/// path can tell "still the one I am listening on" from "gone".
#[cfg(unix)]
pub(super) fn socket_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).ok()?;
    Some((metadata.dev(), metadata.ino()))
}

/// Puts the server back at its endpoint when the endpoint stops being the
/// one it bound.
///
/// The recorded WSL case: `systemd-tmpfiles` wipes `/tmp` about forty
/// seconds after login, taking the socket out from under a live server. The
/// server never notices — it holds an open listener on an unlinked inode —
/// and a client finding no socket would start a second one. The
/// [`WorkspaceLock`] tells that client a server is alive, and stops a
/// second server from restoring the same workspace; this is the other half:
/// the original reappears where clients look for it.
///
/// Reclaiming rather than yielding is safe *because* of that lock. This
/// process holds it, so anything now sitting at the path is not another
/// server of this workspace.
///
/// The stop flag is held across the whole check-and-rebind, not merely
/// read at the top. Reading it and then rebinding races the teardown at
/// the end of [`serve`]: a shutdown landing between the two leaves a server
/// on its way out rebinding a socket nothing will ever remove, and an
/// endpoint nobody serves. `serve` takes the same lock before it
/// clears the endpoint, which makes "rebound, then cleared" and "stopped,
/// so never rebound" the only two orderings there are.
#[cfg(unix)]
pub(super) fn spawn_endpoint_watch(server: Arc<Server>) {
    thread::spawn(move || {
        let mut bound = socket_identity(&server.socket);
        loop {
            thread::sleep(STATUS_PROBE_INTERVAL);
            let stopped = server.stopped.lock().expect("stop state poisoned");
            if *stopped {
                break;
            }
            if socket_identity(&server.socket) == bound {
                continue;
            }
            match restore_endpoint_directory(&server.socket)
                .map_err(RuntimeError::from)
                .and_then(|()| bind_endpoint(&server.socket))
            {
                Ok(listener) => {
                    tracing::warn!(
                        socket = %server.socket.display(),
                        "the terminal endpoint vanished under a live server; rebound it"
                    );
                    bound = socket_identity(&server.socket);
                    let accepting = Arc::clone(&server);
                    // The listener this replaces is left blocked in
                    // `accept` on an inode nothing can reach any more, so it
                    // costs one idle thread and answers nobody.
                    thread::spawn(move || accept_connections(listener, accepting));
                }
                Err(error) => {
                    tracing::warn!(%error, "could not rebind the terminal endpoint")
                }
            }
        }
    });
}

/// Stops the server when an `uze` of any build asks it to through the
/// named stop event — the cooperative half of retiring a server, which on
/// Unix is `SIGTERM`. The event's name is derived from the endpoint, and
/// its shape is a thing no build changes.
#[cfg(windows)]
pub(super) fn spawn_stop_event(server: Arc<Server>) {
    let name = windows::stop_event_name(&server.socket);
    thread::spawn(move || {
        if windows::wait_for_stop(&name).is_ok() {
            server.persist();
            server.shut_down();
        }
    });
}
