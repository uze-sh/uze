//! The slot-and-delivery engine end to end (L3): the real `uze` binary's
//! terminal server, real Git, an isolated `$UZE_HOME`, and a scripted agent
//! in place of a model — driven through the client protocol the way the
//! TUI drives it, with the application asked what the TUI would ask it.
//!
//! No container and no PTY driver: everything the engine needs is local,
//! and what a container adds — a vendor's binary, a synthetic provider —
//! is not part of slots, rebases, gates or fast-forwards.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    time::{Duration, Instant},
};

use uze_application::{
    DeliveryOutcome, Placement, PlacementKind, UzeApplication, UzeHome, WorkStateView,
};
use uze_terminal::{
    ClientEvent, ClientRequest, PROTOCOL_VERSION, PaneId, Session, Stream, open_space, read_event,
    send_request, socket_path,
};
use uze_testkit::{
    env::ProcessEnvGuard,
    fake_harness::{Action, FakeHarness, Steps},
    temp::TestEnvironment,
};

use super::util::uze_bin;

const WAIT: Duration = Duration::from_secs(30);

/// One repository, one server, one attached client, and the scripts its
/// agents will play.
struct Engine {
    env: TestEnvironment,
    scripts: PathBuf,
    server: Child,
    stream: Stream,
    reader: Stream,
    session: Option<Session>,
    /// The key each launched agent's placement issued, by agent.
    launch_keys: std::collections::BTreeMap<String, String>,
    _guard: ProcessEnvGuard<'static>,
}

impl Engine {
    fn start(lock: &str) -> Self {
        Self::start_with(lock, |_| {})
    }

    /// `start`, with `before_serve` run against the world before the
    /// server is: what the server does as it starts is part of the subject.
    fn start_with(lock: &str, before_serve: impl FnOnce(&TestEnvironment)) -> Self {
        let env = TestEnvironment::isolated();
        // The application runs in this process and spawns Git and the forge
        // CLI, so this process must see the isolated HOME, UZE_HOME and the
        // fake bin on PATH for as long as the engine lives.
        let guard = env.apply();
        let project = env.project.clone();
        let git = |args: &[&str]| {
            let output = env.command("git").args(args).output().unwrap();
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        };
        git(&["init", "--quiet", "-b", "main", "."]);
        git(&["config", "user.name", "Operator"]);
        git(&["config", "user.email", "operator@uze.invalid"]);
        fs::write(project.join("README.md"), "# engine\n").unwrap();
        fs::write(project.join(".gitignore"), "target/\n").unwrap();
        fs::write(project.join("agents.yaml"), format!("workspace:\n{lock}")).unwrap();
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "init"]);

        let scripts = env.root().join("agent-scripts");
        fs::create_dir_all(&scripts).unwrap();
        FakeHarness::scripted_agent(&env.fake_bin, "agent");
        before_serve(&env);

        let server = env
            .command(uze_bin())
            .args(["terminal", "serve", "--root"])
            .arg(&project)
            .env("AGENT_SCRIPTS", &scripts)
            // A pane with no command runs this: a shell nothing on the
            // machine configured, so what it finds on `PATH` is the server's.
            .env("SHELL", "/bin/sh")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the real uze binary serves a terminal");
        let socket = socket_path().unwrap();
        wait_until("the server answers at its endpoint", || listening(&socket));
        let (stream, reader) = connect(&project);
        let mut engine = Self {
            env,
            scripts,
            server,
            stream,
            reader,
            session: None,
            launch_keys: std::collections::BTreeMap::new(),
            _guard: guard,
        };
        engine.wait_for_session();
        engine
    }

    fn project(&self) -> &Path {
        &self.env.project
    }

    fn app(&self) -> UzeApplication {
        UzeApplication::new(UzeHome::at(&self.env.uze_home), Vec::new())
    }

    fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let output = self
            .env
            .command("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} in {}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn wait_for_session(&mut self) {
        let started = Instant::now();
        loop {
            match read_event(&mut self.reader).expect("the server keeps talking") {
                Some(ClientEvent::Snapshot { session })
                | Some(ClientEvent::SessionUpdated { session }) => {
                    self.session = Some(session);
                    return;
                }
                Some(ClientEvent::Error { message }) => panic!("server: {message}"),
                Some(_) => {}
                None => panic!("the server hung up"),
            }
            assert!(started.elapsed() < WAIT, "no session update in time");
        }
    }

    /// Launches an agent the way the TUI does: placement first, then a tab
    /// whose first process is the scripted agent, started in the slot.
    /// `start` is what the agent does before it goes quiet.
    fn launch(&mut self, start: Steps) -> (String, PathBuf) {
        let placement = self
            .app()
            .workspace()
            .place_new_agent(
                self.project(),
                Some(PlacementKind::Isolated),
                "claude-code",
                &[],
            )
            .expect("a slot is acquired");
        let Placement::Isolated { task, .. } = &placement.placement else {
            panic!("{placement:?}");
        };
        let slot = placement.cwd.clone();
        let name = slot.file_name().unwrap().to_string_lossy().into_owned();
        start.write_for(&self.scripts, &name, "start");
        // A reused slot keeps its name, and with it the marker the previous
        // agent left: clearing it is what makes the wait below prove that
        // *this* agent ran.
        let started = self.scripts.join(format!("{name}.started"));
        let _ = fs::remove_file(&started);
        let label = format!(
            "agent {}",
            self.session
                .as_ref()
                .map_or(1, |s| s.selected_space().tabs.len())
        );
        send_request(
            &mut self.stream,
            &ClientRequest::CreateTab {
                label,
                agent: None,
                columns: 80,
                rows: 24,
                cwd: Some(slot.clone()),
                command: Some(vec!["agent".into()]),
                // The launch carries the agent's identity, as the client's
                // does: what the sweep reads back to know the task is still
                // somebody's.
                env: vec![
                    (
                        uze_terminal::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
                        task.as_str().to_owned(),
                    ),
                    (
                        uze_terminal::launch::AGENT_KEY_VARIABLE.to_owned(),
                        placement.launch_key.clone(),
                    ),
                ],
            },
        )
        .unwrap();
        self.launch_keys
            .insert(task.as_str().to_owned(), placement.launch_key.clone());
        self.wait_for_tab_in(&slot);
        wait_until("the agent played its script", || started.exists());
        (task.as_str().to_owned(), slot)
    }

    /// The pane of the tab running in `slot`, from the latest session.
    fn pane_in(&self, slot: &Path) -> PaneId {
        self.find_pane_in(slot).expect("a tab runs in the slot")
    }

    fn find_pane_in(&self, slot: &Path) -> Option<PaneId> {
        let slot = uze_platform::path::canonical(slot).unwrap_or_else(|_| slot.to_path_buf());
        self.session
            .as_ref()?
            .workspace
            .spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .map(|tab| &tab.pane)
            .find(|pane| {
                uze_platform::path::canonical(&pane.cwd).unwrap_or_else(|_| pane.cwd.clone())
                    == slot
            })
            .map(|pane| pane.id)
    }

    /// Reads session updates until `accept` holds: the server also pushes
    /// updates on its own clock (a pane's process name resolving), so the
    /// next event is not necessarily the one a request produced.
    fn wait_for_session_where(&mut self, what: &str, accept: impl Fn(&Session) -> bool) {
        let started = Instant::now();
        while !self.session.as_ref().is_some_and(&accept) {
            assert!(started.elapsed() < WAIT, "no session update where {what}");
            self.wait_for_session();
        }
    }

    /// Reads session updates until a tab runs in `slot`: the server answers
    /// an attach with more than one session event, and the one carrying the
    /// new tab is not necessarily the first to arrive.
    fn wait_for_tab_in(&mut self, slot: &Path) {
        let started = Instant::now();
        while self.find_pane_in(slot).is_none() {
            assert!(
                started.elapsed() < WAIT,
                "no tab appeared in {}",
                slot.display()
            );
            self.wait_for_session();
        }
    }

    /// The checkout directories a pane still sits in — what the TUI hands
    /// the application to say which slots are still somebody's.
    fn occupied(&self) -> Vec<PathBuf> {
        self.session
            .as_ref()
            .into_iter()
            .flat_map(|session| &session.workspace.spaces)
            .flat_map(|space| &space.tabs)
            .map(|tab| tab.pane.cwd.clone())
            .collect()
    }

    /// Closes the tab running in `slot`, the way the operator does.
    fn close_tab_in(&mut self, slot: &Path) {
        let pane = self.pane_in(slot);
        let tab = self
            .session
            .as_ref()
            .unwrap()
            .workspace
            .spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .find(|tab| tab.pane.id == pane)
            .expect("the pane belongs to a tab")
            .id;
        send_request(&mut self.stream, &ClientRequest::CloseTab { tab }).unwrap();
        let slot = slot.to_path_buf();
        self.wait_for_session_where("the closed tab is gone", |_| true);
        while self.find_pane_in(&slot).is_some() {
            self.wait_for_session();
        }
    }

    /// Types one line into the agent's pane, as the TUI types a notice.
    fn tell(&mut self, pane: PaneId, message: &str) {
        let mut bytes = message.as_bytes().to_vec();
        bytes.push(b'\r');
        send_request(&mut self.stream, &ClientRequest::Input { pane, bytes }).unwrap();
    }

    fn state_of(&self, id: &str) -> WorkStateView {
        self.app()
            .workspace()
            .tasks(self.project())
            .into_iter()
            .find(|task| task.id == id)
            .map(|task| task.state)
            .unwrap_or_else(|| panic!("task {id} is recorded"))
    }

    fn evaluate(&self) -> uze_application::Evaluation {
        self.app().workspace().evaluate_tasks(self.project(), &[])
    }

    fn restart_server(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
        let project = self.project().to_path_buf();
        let socket = socket_path().unwrap();
        wait_until("the dead server's endpoint answers nobody", || {
            !listening(&socket)
        });
        let _ = fs::remove_file(&socket);
        self.server = self
            .env
            .command(uze_bin())
            .args(["terminal", "serve", "--root"])
            .arg(&project)
            .env("AGENT_SCRIPTS", &self.scripts)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_until("the new server answers at its endpoint", || {
            listening(&socket)
        });
        let (stream, reader) = connect(&project);
        self.stream = stream;
        self.reader = reader;
        self.wait_for_session();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = send_request(&mut self.stream, &ClientRequest::Detach);
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

/// Whether a server answers at `endpoint`: a socket file can outlive its
/// server, and a pipe is no file at all.
fn listening(endpoint: &Path) -> bool {
    uze_terminal::connect(endpoint).is_ok()
}

fn connect(project: &Path) -> (Stream, Stream) {
    // Straight to the endpoint: `attach` would replace a server that is not
    // this executable, and the one started above is the real binary.
    let mut stream = uze_terminal::connect(&socket_path().unwrap())
        .expect("connects to the server started above");
    let reader = stream.try_clone().unwrap();
    send_request(
        &mut stream,
        &ClientRequest::Attach {
            version: PROTOCOL_VERSION,
            columns: 80,
            rows: 24,
            // A project directory, which is a directory somebody chose:
            // the client asks for a space there.
            seating: uze_terminal::Seating::Open(uze_terminal::SpaceSeat {
                root: project.to_path_buf(),
            }),
            key: uze_terminal::server_key(),
        },
    )
    .unwrap();
    (stream, reader)
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < WAIT, "timed out waiting until {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Evaluates until every listed task reads as `wanted`, or fails.
fn wait_for_states(engine: &Engine, ids: &[&str], wanted: &WorkStateView) {
    wait_until(&format!("tasks {ids:?} read as {wanted:?}"), || {
        engine.evaluate();
        ids.iter().all(|id| &engine.state_of(id) == wanted)
    });
}

/// An agent writing `file` and committing it, named after the file.
fn commit(file: &str, contents: &str) -> Steps {
    Steps::new()
        .write(file, contents)
        .git(&["add", file])
        .git(&["commit", "--quiet", "-m", file])
}

#[test]
fn three_agents_deliver_into_a_linear_target_around_the_operators_edits() {
    let mut engine = Engine::start(
        "  delivery: merge\n  gate:\n    posix: test -f README.md\n    \
         windows: \"if (-not (Test-Path README.md)) { exit 1 }\"\n",
    );
    let project = engine.project().to_path_buf();
    // The operator read the project's gate and approved it (ADR-055).
    let awaiting = engine
        .app()
        .workspace()
        .commands_awaiting_approval(&project)
        .expect("the gate waits for the operator");
    engine
        .app()
        .workspace()
        .approve_commands(&awaiting)
        .unwrap();
    // The operator is mid-edit in the primary the whole time.
    fs::write(project.join("README.md"), "# engine, edited\n").unwrap();
    fs::write(project.join("scratch.txt"), "untracked\n").unwrap();

    let a = engine.launch(commit("a.rs", "fn a() {}\n"));
    let b = engine.launch(commit("b.rs", "fn b() {}\n"));
    let c = engine.launch(commit("c.rs", "fn c() {}\n"));
    let slots = [&a.1, &b.1, &c.1];
    assert!(
        slots
            .iter()
            .all(|slot| slot.starts_with(project.join(".worktrees")))
    );
    assert_eq!(
        slots
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );

    wait_for_states(&engine, &[&a.0, &b.0, &c.0], &WorkStateView::Ready);
    let reports = engine.app().workspace().deliver_ready(&project);
    assert_eq!(reports.len(), 3, "{reports:?}");
    assert!(
        reports
            .iter()
            .all(|report| report.outcome == DeliveryOutcome::Merged),
        "{reports:?}"
    );

    for file in ["a.rs", "b.rs", "c.rs"] {
        assert!(project.join(file).is_file(), "{file} landed in the primary");
    }
    let parents = engine.git(&project, &["log", "--format=%p", "-n", "3"]);
    assert!(
        parents
            .lines()
            .all(|line| line.split_whitespace().count() == 1),
        "linear: {parents}"
    );
    assert_eq!(
        fs::read_to_string(project.join("README.md")).unwrap(),
        "# engine, edited\n",
        "the operator's uncommitted edit survived three deliveries"
    );
    let status = engine.git(&project, &["status", "--porcelain"]);
    assert_eq!(
        status.lines().count(),
        2,
        "only the operator's own changes: {status}"
    );
    for id in [&a.0, &b.0, &c.0] {
        assert_eq!(engine.state_of(id), WorkStateView::Integrated);
    }
}

/// The whole point of a slot: an agent closed without delivering anything
/// gives its checkout back, and so does one that left work behind, once
/// that work is kept on its shelf.
#[test]
fn a_closed_agent_gives_its_slot_back_and_its_work_is_kept_on_a_shelf() {
    let mut engine = Engine::start("  delivery: merge\n");
    let project = engine.project().to_path_buf();

    let (empty, slot) = engine.launch(Steps::new());
    engine.close_tab_in(&slot);
    let occupied = engine.occupied();
    let released = engine
        .app()
        .workspace()
        .release_abandoned_tasks(&project, &occupied, &[]);
    assert_eq!(released.len(), 1, "{released:?}");
    assert!(!released[0].unfinished, "the checkout held nothing");
    assert_eq!(
        engine.state_of(&empty),
        WorkStateView::Closed,
        "it ended holding nothing, which is not the same as delivered"
    );

    let (_, reused) = engine.launch(Steps::new());
    assert_eq!(
        reused, slot,
        "the freed slot is taken instead of a new working tree"
    );
    let occupied = engine.occupied();
    assert!(
        engine
            .app()
            .workspace()
            .release_abandoned_tasks(&project, &occupied, &[])
            .is_empty(),
        "an agent sitting in its slot is not abandoned"
    );

    let (unsaved, kept) = engine.launch(Steps::new().write("draft.rs", "draft\n"));
    engine.close_tab_in(&kept);
    let occupied = engine.occupied();
    let released = engine
        .app()
        .workspace()
        .release_abandoned_tasks(&project, &occupied, &[]);
    assert_eq!(released.len(), 1, "{released:?}");
    assert!(released[0].unfinished, "it held uncommitted work");
    assert_eq!(engine.state_of(&unsaved), WorkStateView::Shelved);
    assert_eq!(
        engine
            .git(
                &project,
                &["show", &format!("refs/uze/shelf/{unsaved}:draft.rs")]
            )
            .trim(),
        "draft",
        "the uncommitted file is kept on the task's shelf"
    );

    let (_, next) = engine.launch(Steps::new());
    assert_eq!(next, kept, "the checkout goes to the next agent");
    assert!(!next.join("draft.rs").exists(), "carrying none of it");
}

/// An agent gives its subagent a checkout through the real binary, from
/// inside its pane, beside a checkout somebody added by hand. A new agent
/// launched next to both takes neither; the agent's branch moves onto the
/// target; joining brings exactly the subagent's commit over with no merge
/// commit; and the subagent's checkout is what the next agent gets.
#[test]
fn a_subagents_checkout_is_split_and_joined_beside_one_made_by_hand() {
    let mut engine = Engine::start("  delivery: merge\n");
    let project = engine.project().to_path_buf();
    engine.git(
        &project,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "by-hand",
            ".worktrees/by-hand",
            "HEAD",
        ],
    );
    let by_hand = project.join(".worktrees/by-hand");
    let split_path = engine.scripts.join("parser.path");
    let uze = uze_bin();
    let (agent, slot) = engine.launch(
        Steps::new()
            .run_capturing(uze, &["agent", "work", "split", "parser"], &split_path)
            .within(&split_path, commit("parser.rs", "fn parse() {}\n"))
            .and(commit("own.rs", "fn own() {}\n")),
    );
    let child = PathBuf::from(fs::read_to_string(&split_path).unwrap().trim());
    assert!(
        child.join("parser.rs").is_file(),
        "the subagent worked in its own checkout"
    );

    let (_, beside) = engine.launch(Steps::new());
    assert_ne!(beside, by_hand, "a checkout made by hand is never taken");
    assert_ne!(beside, child, "a subagent's checkout is held for its agent");
    assert_eq!(
        engine.git(&by_hand, &["branch", "--show-current"]),
        "by-hand"
    );

    fs::write(project.join("NEWS.md"), "moved\n").unwrap();
    engine.git(&project, &["add", "NEWS.md"]);
    engine.git(&project, &["commit", "--quiet", "-m", "target moved"]);
    engine.git(&slot, &["rebase", "--quiet", "main"]);

    let joined = engine
        .env
        .command(uze_bin())
        .args(["agent", "work", "join", "parser"])
        .current_dir(&slot)
        .env(uze_terminal::launch::AGENT_IDENTITY_VARIABLE, &agent)
        .env(
            uze_terminal::launch::AGENT_KEY_VARIABLE,
            &engine.launch_keys[&agent],
        )
        .output()
        .unwrap();
    assert!(
        joined.status.success(),
        "{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    let subjects = engine.git(&slot, &["log", "--format=%s", "main.."]);
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        ["parser.rs", "own.rs"]
    );
    assert!(
        engine
            .git(&slot, &["log", "--merges", "--oneline"])
            .is_empty()
    );

    let (_, next) = engine.launch(Steps::new());
    assert_eq!(
        next, child,
        "the joined subagent's checkout went back to the pool"
    );
}

/// The pass a workspace client actually runs: it names every path it has
/// reason to doubt and gets back the repositories that changed, once
/// each.
///
/// Naming several paths of one repository is the ordinary case — a
/// vanished slot and the space's own root both point at it — and
/// reconciling it twice would release against a store the first pass had
/// already rewritten.
#[test]
fn one_reconciliation_pass_answers_a_repository_once_however_it_is_named() {
    let mut engine = Engine::start("  delivery: merge\n");
    let project = engine.project().to_path_buf();

    let (empty, slot) = engine.launch(Steps::new());
    // A second agent that stays open: the pass must leave it alone.
    let (live, occupied_slot) = engine.launch(Steps::new());
    engine.close_tab_in(&slot);

    // The checkout that just emptied, the repository root, and the slot's
    // own parent: three names for one repository.
    let look_in = vec![slot.clone(), project.clone(), project.join(".worktrees")];
    let held = engine.occupied();
    assert!(
        held.iter().any(|cwd| cwd.starts_with(&occupied_slot)),
        "the live agent's checkout is what `held` is for: {held:?}"
    );
    let reconciliation = engine
        .app()
        .workspace()
        .reconcile_occupancy(&look_in, &held, &[]);

    assert_eq!(
        reconciliation.released.len(),
        1,
        "only the abandoned slot was released: {reconciliation:?}"
    );
    assert!(
        occupied_slot.is_dir(),
        "a checkout a pane still sits in survives the same pass"
    );
    assert_eq!(
        engine.state_of(&live),
        WorkStateView::Running,
        "and its task is untouched"
    );
    assert_eq!(
        reconciliation.changed.len(),
        1,
        "and its repository is reported once, not once per name: {reconciliation:?}"
    );
    assert_eq!(engine.state_of(&empty), WorkStateView::Closed);

    let quiet = engine
        .app()
        .workspace()
        .reconcile_occupancy(&look_in, &engine.occupied(), &[]);
    assert!(
        quiet.changed.is_empty() && quiet.released.is_empty(),
        "a second pass over the same state changes nothing: {quiet:?}"
    );
}

/// The status column is read off the checkout, every time, for as long as
/// somebody is in it: an agent almost never stops at its first delivery,
/// and a row that froze on `delivered` was reporting the past over a slot
/// full of work.
#[test]
fn a_slots_status_follows_the_agent_through_a_delivery_and_past_it() {
    let mut engine = Engine::start("  delivery: merge\n");
    let project = engine.project().to_path_buf();

    let (id, slot) = engine.launch(Steps::new().write("draft.rs", "draft\n"));
    wait_for_states(&engine, &[&id], &WorkStateView::Uncommitted);

    engine.git(&slot, &["add", "draft.rs"]);
    engine.git(&slot, &["commit", "--quiet", "-m", "draft"]);
    wait_for_states(&engine, &[&id], &WorkStateView::Ready);

    let report = engine
        .app()
        .workspace()
        .deliver_task(&project, &id)
        .unwrap();
    assert_eq!(report.outcome, DeliveryOutcome::Merged, "{report:?}");
    assert_eq!(engine.state_of(&id), WorkStateView::Integrated);

    engine.evaluate();
    assert_eq!(
        engine.state_of(&id),
        WorkStateView::Integrated,
        "nothing new leaves the delivery standing"
    );

    // The same agent carries on in the same slot.
    fs::write(slot.join("after.rs"), "fn after() {}\n").unwrap();
    wait_for_states(&engine, &[&id], &WorkStateView::Uncommitted);

    engine.git(&slot, &["add", "after.rs"]);
    engine.git(&slot, &["commit", "--quiet", "-m", "after"]);
    wait_for_states(&engine, &[&id], &WorkStateView::Ready);

    let report = engine
        .app()
        .workspace()
        .deliver_task(&project, &id)
        .unwrap();
    assert_eq!(report.outcome, DeliveryOutcome::Merged, "{report:?}");
    assert!(project.join("after.rs").is_file(), "and the second lands");
}

#[test]
fn a_conflict_goes_to_the_agents_pane_and_comes_back_resolved() {
    let mut engine = Engine::start("  delivery: merge\n");
    let project = engine.project().to_path_buf();
    let (id, slot) = engine.launch(commit("shared.rs", "agent\n"));
    wait_for_states(&engine, &[&id], &WorkStateView::Ready);
    // The target moves under the task, on the same file.
    fs::write(project.join("shared.rs"), "operator\n").unwrap();
    engine.git(&project, &["add", "shared.rs"]);
    engine.git(&project, &["commit", "--quiet", "-m", "operator's shared"]);
    let target_before = engine.git(&project, &["rev-parse", "main"]);

    let evaluation = engine.evaluate();
    assert_eq!(evaluation.notices.len(), 1, "{evaluation:?}");
    let notice = evaluation.notices[0].clone();
    assert_eq!(notice.checkout, slot);
    assert!(matches!(
        engine.state_of(&id),
        WorkStateView::Conflicted { .. }
    ));
    assert_eq!(engine.git(&project, &["rev-parse", "main"]), target_before);

    // What the agent does with the message: resolve, continue, end the turn.
    let name = slot.file_name().unwrap().to_string_lossy().into_owned();
    Steps::new()
        .write("shared.rs", "both\n")
        .git(&["add", "shared.rs"])
        .git_with(&["rebase", "--continue"], &[("GIT_EDITOR", "true")])
        .write_for(&engine.scripts, &name, "conflict");
    let pane = engine.pane_in(&slot);
    engine.tell(pane, &notice.message);
    let resolved = engine.scripts.join(format!("{name}.resolved"));
    wait_until("the agent resolved the rebase", || resolved.exists());

    wait_for_states(&engine, &[&id], &WorkStateView::Ready);
    let report = engine
        .app()
        .workspace()
        .deliver_task(&project, &id)
        .unwrap();
    assert_eq!(report.outcome, DeliveryOutcome::Merged, "{report:?}");
    assert_eq!(
        fs::read_to_string(project.join("shared.rs")).unwrap(),
        "both\n"
    );
}

#[test]
fn a_server_restart_loses_no_task_and_a_dirty_orphan_is_kept() {
    let mut engine = Engine::start("  delivery: handoff\n");
    let project = engine.project().to_path_buf();
    let (id, slot) = engine.launch(commit("kept.rs", "kept\n"));
    wait_for_states(&engine, &[&id], &WorkStateView::Ready);
    // A checkout from before task state existed, with work in it.
    engine.git(
        &project,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "agent/agent-2",
            ".worktrees/agent-2",
            "HEAD",
        ],
    );
    fs::write(
        project.join(".worktrees/agent-2/half-done.rs"),
        "unfinished\n",
    )
    .unwrap();

    engine.restart_server();

    let evaluation = engine.evaluate();
    let kept = evaluation
        .tasks
        .iter()
        .find(|task| task.id == id)
        .expect("the task outlived the server");
    assert_eq!(kept.state, WorkStateView::Ready);
    assert_eq!(kept.checkout.as_deref(), Some(slot.as_path()));
    let legacy = evaluation
        .tasks
        .iter()
        .find(|task| task.branch == "agent/agent-2")
        .expect("the legacy checkout was adopted");
    assert_eq!(legacy.state, WorkStateView::Shelved);
    assert_eq!(legacy.label, "agent-2");
    assert_eq!(
        fs::read_to_string(project.join(".worktrees/agent-2/half-done.rs")).unwrap(),
        "unfinished\n",
        "kept until its work is shelved, with every file preserved"
    );
    let next = engine
        .app()
        .workspace()
        .place_new_agent(&project, Some(PlacementKind::Isolated), "claude-code", &[])
        .expect("a slot is acquired");
    assert!(
        next.cwd != project.join(".worktrees/agent-2"),
        "a checkout whose work is not kept yet is never handed to a new agent"
    );
}

/// `pr` end to end, with no forge CLI on `PATH` at all: UZE publishes and
/// hands the request to the agent, the agent opens it, and the next
/// delivery is a sync that names it.
#[test]
fn pr_publishes_then_hands_the_request_to_its_agent_and_syncs_it_after() {
    let mut engine = Engine::start("  delivery: pr\n");
    let project = engine.project().to_path_buf();
    uze_testkit::git::publish_to_origin(&project, "main");

    let (id, slot) = engine.launch(commit("feature.rs", "feature\n"));
    wait_for_states(&engine, &[&id], &WorkStateView::Ready);
    let local_target = engine.git(&project, &["rev-parse", "main"]);

    let report = engine
        .app()
        .workspace()
        .deliver_task(&project, &id)
        .unwrap();
    let DeliveryOutcome::AwaitingRequest(notice) = &report.outcome else {
        panic!("nothing has opened a request yet: {report:?}");
    };
    assert_eq!(notice.checkout, slot);
    let branch = report.task.published_as.clone().unwrap();
    let remote = engine.git(&project, &["ls-remote", "--heads", "origin"]);
    assert!(remote.contains(&format!("refs/heads/{branch}")), "{remote}");
    assert_eq!(report.task.published_request, None);
    assert_eq!(
        engine.git(&project, &["rev-parse", "main"]),
        local_target,
        "never pulled"
    );

    // The agent takes the message and opens the request; the forge
    // publishes its head, which is all UZE reads to learn the number.
    let name = slot.file_name().unwrap().to_string_lossy().into_owned();
    let tip = engine.git(&project, &["rev-parse", &format!("agent/{id}")]);
    Steps::new()
        .git(&[
            "push",
            "--quiet",
            "origin",
            &format!("{tip}:refs/pull/7/head"),
        ])
        .write_for(&engine.scripts, &name, "request");
    let pane = engine.pane_in(&slot);
    engine.tell(pane, &notice.message);
    let opened = engine.scripts.join(format!("{name}.opened"));
    wait_until("the agent opened the request", || opened.exists());

    let report = engine
        .app()
        .workspace()
        .deliver_task(&project, &id)
        .unwrap();
    assert_eq!(
        report.outcome,
        DeliveryOutcome::Published { branch, request: 7 },
        "{report:?}"
    );
    assert_eq!(
        report.task.published_request,
        Some(7),
        "the task carries it, so the button can name it without asking the remote again"
    );
    assert_eq!(
        engine.git(&project, &["rev-parse", "main"]),
        local_target,
        "a sync still writes nothing local"
    );
}

/// One server, two clients: each keeps its own selection, and a `uze`
/// started inside a pane opens a space in the server instead of a client
/// inside a client.
#[test]
fn two_clients_keep_their_own_focus_and_a_nested_launch_opens_a_space() {
    let mut engine = Engine::start("  delivery: handoff\n");
    let project = engine.project().to_path_buf();
    let first_space = engine.session.as_ref().unwrap().workspace.selected_space;
    assert_eq!(
        engine.session.as_ref().unwrap().selected_space().root,
        uze_platform::path::canonical(&project).unwrap_or(project.clone()),
        "the launch directory is the first space's root"
    );

    // A second client attaches for another directory: it gets a space of
    // its own, selected for it alone.
    let other = engine.env.root().join("other-project");
    fs::create_dir_all(&other).unwrap();
    let (mut second, mut second_reader) = connect(&other);
    let second_view = loop {
        match read_event(&mut second_reader).unwrap() {
            Some(ClientEvent::Snapshot { session }) => break session,
            Some(ClientEvent::Error { message }) => panic!("{message}"),
            Some(_) => {}
            None => panic!("hung up"),
        }
    };
    assert_ne!(second_view.workspace.selected_space, first_space);
    assert_eq!(
        second_view.selected_space().root,
        uze_platform::path::canonical(&other).unwrap_or(other.clone())
    );
    assert_eq!(second_view.workspace.spaces.len(), 2);

    // The first client's own view did not move.
    engine.wait_for_session_where("two spaces exist", |session| {
        session.workspace.spaces.len() == 2
    });
    assert_eq!(
        engine.session.as_ref().unwrap().workspace.selected_space,
        first_space,
        "another client's attach never moves this client's selection"
    );

    // Selecting in one client is invisible to the other.
    send_request(
        &mut second,
        &ClientRequest::SelectSpace { space: first_space },
    )
    .unwrap();
    let moved = loop {
        match read_event(&mut second_reader).unwrap() {
            Some(ClientEvent::SessionUpdated { session }) => break session,
            Some(_) => {}
            None => panic!("hung up"),
        }
    };
    assert_eq!(moved.workspace.selected_space, first_space);
    engine.wait_for_session();
    assert_eq!(
        engine.session.as_ref().unwrap().workspace.selected_space,
        first_space
    );
    let _ = send_request(&mut second, &ClientRequest::Detach);

    // A nested launch: what `uze` does when UZE_PANE is set.
    let nested = engine.env.root().join("nested-project");
    fs::create_dir_all(&nested).unwrap();
    let label = open_space(uze_terminal::SpaceSeat {
        root: nested.clone(),
    })
    .expect("the running server opens a space");
    assert_eq!(label, "nested-project");
    engine.wait_for_session_where("three spaces exist", |session| {
        session.workspace.spaces.len() == 3
    });
    let session = engine.session.as_ref().unwrap();
    assert!(
        session
            .workspace
            .spaces
            .iter()
            .any(|space| space.label == "nested-project")
    );
    assert_eq!(
        session.workspace.selected_space, first_space,
        "a space opened from a pane does not steal this client's focus"
    );
}

/// A harness a person types into a workspace pane, or a program running
/// there starts, goes through the launcher its setup placed, like one
/// started from the menu: the server puts the shims first on every pane's
/// `PATH`, and nothing in the pane's shell is told.
#[test]
fn a_harness_typed_into_a_pane_goes_through_its_launcher() {
    let launched = std::cell::OnceCell::new();
    let mut engine = Engine::start_with("  delivery: merge\n", |env| {
        let marker = env.root().join("claude.env");
        FakeHarness::new(&env.fake_bin, "claude")
            .version_line("9.9.9 (Claude Code)")
            .on_prefix(["update"], Action::Exit(0))
            .on_prefix(["plugin"], Action::Exit(0))
            .on_prefix(
                [""],
                Action::RecordLaunch {
                    into: marker.clone(),
                },
            )
            .build();
        env.run_ok(uze_bin(), &["setup", "claude-code"]);
        launched.set(marker).unwrap();
    });
    let marker = launched.into_inner().unwrap();
    assert!(
        UzeHome::at(&engine.env.uze_home)
            .shims_dir()
            .join(uze_platform::executable::file_name("claude"))
            .exists(),
        "setup placed the harness's launcher"
    );

    let project = engine.project().to_path_buf();
    send_request(
        &mut engine.stream,
        &ClientRequest::CreateTab {
            label: "shell".into(),
            agent: None,
            columns: 80,
            rows: 24,
            cwd: Some(project.clone()),
            command: None,
            env: Vec::new(),
        },
    )
    .unwrap();
    engine.wait_for_tab_in(&project);
    let pane = engine.pane_in(&project);
    engine.tell(pane, "claude");
    wait_until("the typed harness started", || marker.exists());

    let seen = fs::read_to_string(&marker).unwrap();
    let pid = seen
        .lines()
        .find_map(|line| line.strip_prefix("PID="))
        .unwrap();
    assert!(
        seen.lines().any(|line| line == "UZE_SHIM_NAME=claude"),
        "the real binary was reached through its launcher: {seen}"
    );
    let launcher: u32 = seen
        .lines()
        .find_map(|line| line.strip_prefix("UZE_SHIM_PID="))
        .and_then(|launcher| launcher.parse().ok())
        .expect("the launcher stamped its pid");
    assert!(
        uze_platform::process::launched_by(pid.parse().unwrap(), launcher),
        "and the stamp names the launch that ran the harness: {seen}"
    );
    engine.wait_for_session_where("the pane runs claude through its launcher", |session| {
        session
            .workspace
            .spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .any(|tab| {
                tab.pane.id == pane && tab.pane.process == "claude" && tab.pane.through_launcher
            })
    });
}
