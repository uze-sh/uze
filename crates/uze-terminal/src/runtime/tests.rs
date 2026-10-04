#[test]
fn the_named_directory_leads_the_pane_path_once() {
    let first = Path::new("/home/x/.uze/shims");
    let path = |directories: &[&str]| std::env::join_paths(directories).unwrap();
    let joined = crate::runtime::path_with_first(
        Some(path(&["/usr/bin", "/home/x/.uze/shims", "/bin"])),
        first,
    );
    assert_eq!(joined, path(&["/home/x/.uze/shims", "/usr/bin", "/bin"]));
    assert_eq!(
        crate::runtime::path_with_first(None, first),
        std::ffi::OsString::from("/home/x/.uze/shims")
    );
}

use super::{
    ANSWERS_WITHIN, Arrival, Launch, Listener, MAX_FRAME, MAX_PANE_DIMENSION, Outbox, PaneRuntime,
    PersistedWorkspace, ReplySink, RuntimeError, Selection, Server, WORKSPACE_SCHEMA_VERSION,
    WorkspaceLock, arrival, bind_endpoint, forward_events, held_by_a_server, identity_of,
    load_persisted_workspace_at, persisted_state_path, read_event, read_message,
    relaunch_command_for_process, send_request, serves_this_build, snapshot, socket_path, view_for,
    workspace_is_claimed, write_atomically, write_message,
};
// Only the Unix tests below tell a listener apart by what it runs.
#[cfg(any(target_os = "linux", target_os = "macos"))]
use super::identify;
use super::{listener_at, retire, workspace_lock_path};
use std::sync::{Arc, Mutex};

// The tests below run on every platform, a real server and real panes
// included. The few gated to Unix drive POSIX programs (`sh`, `sleep`, a
// FIFO, a process group, a zombie) to reach what they prove, and each says
// so where it is gated.

use crate::Palette;

/// A sink over the default palette, for the tests that only need a
/// terminal to parse into.
fn reply_sink(sender: std::sync::mpsc::Sender<Vec<u8>>) -> ReplySink {
    ReplySink::new(sender, Arc::new(Mutex::new(Palette::default())))
}
use crate::state::PLACEHOLDER_PANE_SIZE;
use crate::state::{SpaceSeed, TabSeed};
use crate::{MouseMode, PaneId, TerminalColor};
use crate::{Session, SpaceId, TabId};
use alacritty_terminal::{
    Term,
    grid::Scroll,
    term::{Config, test::TermSize},
    vte::ansi::Processor,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{path::Path, thread, time::Duration};

/// A client's selection overlays the shared session wherever it still
/// points at something, and falls back to the server's default where
/// it does not — the rule that lets two terminals look at two agents.
#[test]
fn a_clients_view_overlays_its_own_selection_and_heals_a_stale_one() {
    let mut session = Session::new(seat_at(Path::new("/tmp/a")), 80, 24);
    let first_space = session.workspace.selected_space;
    session.create_space(
        Some("b".into()),
        crate::SpaceSeat {
            root: "/tmp/b".into(),
        },
        80,
        24,
    );
    let second_space = session.workspace.selected_space;
    session.add_tab(second_space, "extra".into(), None, 80, 24, "/tmp/b".into());
    let extra_tab = session.selected_tab().id;
    let first_tab_of_second = session.space(second_space).unwrap().tabs[0].id;

    let selection = Selection {
        space: Some(first_space),
        tabs: BTreeMap::from([(second_space, first_tab_of_second)]),
    };
    let view = view_for(&session, &selection);
    assert_eq!(view.workspace.selected_space, first_space);
    assert_eq!(
        view.space(second_space).unwrap().selected_tab,
        first_tab_of_second
    );
    assert_eq!(
        session.workspace.selected_space, second_space,
        "the shared default is untouched"
    );
    assert_eq!(session.space(second_space).unwrap().selected_tab, extra_tab);

    let stale = Selection {
        space: Some(SpaceId(99)),
        tabs: BTreeMap::from([(second_space, TabId(99))]),
    };
    let healed = view_for(&session, &stale);
    assert_eq!(healed.workspace.selected_space, second_space);
    assert_eq!(healed.space(second_space).unwrap().selected_tab, extra_tab);
}

#[test]
fn endpoint_identity_is_project_specific() {
    assert_eq!(
        identity_of(Path::new("/tmp/a")),
        identity_of(Path::new("/tmp/a"))
    );
    assert_ne!(
        identity_of(Path::new("/tmp/a")),
        identity_of(Path::new("/tmp/b"))
    );
}

/// One workspace, one endpoint, whatever each terminal's environment
/// says. `XDG_RUNTIME_DIR` and `TMPDIR` are set per session and can
/// differ between two terminals of one login — and then the two
/// computed two different sockets for one `UZE_HOME`. The second
/// found nothing listening at a path the first had never bound, while
/// the claim beside the workspace told it a server was alive, so it
/// connected to nothing and answered `No such file or directory`.
#[test]
fn two_terminals_that_disagree_about_the_environment_share_one_endpoint() {
    let home = uze_testkit::temp::socket_scratch("endpoint-home");
    let elsewhere = uze_testkit::temp::socket_scratch("endpoint-xdg");
    let one = {
        let mut env = uze_testkit::env::scope();
        env.set("UZE_HOME", &home);
        env.set("XDG_RUNTIME_DIR", &elsewhere);
        socket_path().expect("an endpoint can always be named")
    };
    let other = {
        let mut env = uze_testkit::env::scope();
        env.set("UZE_HOME", &home);
        env.remove("XDG_RUNTIME_DIR");
        socket_path().expect("an endpoint can always be named")
    };

    assert_eq!(one, other, "the workspace decides, not the session");

    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

/// "Nothing is running" is the ordinary state of `uze workspace stop`,
/// and it used to exit non-zero: a machine that has not opened the TUI
/// since boot has no socket, and a `/tmp` cleaner taking the socket out
/// from under a live server leaves one nobody answers. Both reached the
/// operator as `could not acquire package: terminal runtime I/O error`,
/// from a command that stops a terminal.
#[test]
fn stopping_a_runtime_that_is_not_running_is_not_a_failure() {
    let scratch = uze_testkit::temp::socket_scratch("stop-idempotent");
    let mut env = uze_testkit::env::scope();
    // The endpoint follows `UZE_HOME`, and `stop` ends whatever serves
    // it: without a scratch home this test would stop the developer's
    // own session.
    env.set("UZE_HOME", &scratch);
    env.set("XDG_RUNTIME_DIR", &scratch);

    let socket = socket_path().expect("an endpoint can always be named");
    let _ = std::fs::remove_file(&socket);
    assert!(
        super::stop().is_ok(),
        "no endpoint at all is nothing to stop, not a failure"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The kernel names whoever listens on a socket, and the process table
/// says what that process runs: this very executable, a `uze` of
/// another build, or something nobody can vouch for as `uze` at all.
// Unix only: A process that is not `uze`, run as a POSIX shell (`ReadyProcess`).
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_listener_is_told_apart_by_what_it_runs() {
    let scratch = uze_testkit::temp::socket_scratch("identify");
    std::fs::create_dir_all(&scratch).unwrap();
    let socket = super::transport::scratch_endpoint(&scratch, "test.sock");
    let listener = super::transport::bind(&socket).unwrap();
    assert_eq!(
        listener_at(&socket),
        Listener::ThisBuild(std::process::id()),
        "a listener running this very executable is this build"
    );
    drop(listener);

    let stranger = ReadyProcess::spawn(Path::new("/bin/sh"));
    assert_eq!(identify(stranger.pid()), Listener::Unrecognized);
    stranger.finish();

    let uze_home = scratch.join("home");
    std::fs::create_dir_all(&uze_home).unwrap();
    let another_build = ClaimHolder::spawn_as(&another_build_of_this_binary(&scratch), &uze_home);
    let pid = another_build.pid();
    assert_eq!(identify(pid), Listener::AnotherBuild(pid));
    another_build.release();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A server of another build — a `make install` over a running one — is
/// ended, and not merely abandoned: it holds the workspace claim, and a
/// fresh server cannot restore the workspace until it lets go.
#[test]
fn a_server_of_another_build_is_retired_and_lets_go_of_the_workspace() {
    let scratch = uze_testkit::temp::socket_scratch("retire");
    let uze_home = scratch.join("home");
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let mut old = ClaimHolder::spawn_as(&another_build_of_this_binary(&scratch), &uze_home);
    assert!(workspace_is_claimed());

    retire(
        old.pid(),
        &super::transport::scratch_endpoint(&scratch, "test.sock"),
    );

    assert!(
        !workspace_is_claimed(),
        "the retired server let go of the workspace before retire returned"
    );
    assert!(!old.process.wait().unwrap().success(), "it was ended");

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A server holding the workspace at an endpoint this build does not
/// name — what every change to the endpoint's own rules leaves behind
/// — is still what `stop` stops. It used to look at the new endpoint,
/// find nothing, and report success while the workspace stayed shut:
/// the operator was told there was nothing to stop, could not open
/// uze, and restarting the machine was the only way out.
#[test]
fn a_server_answering_at_no_endpoint_this_build_names_is_still_stopped() {
    let scratch = uze_testkit::temp::socket_scratch("stop-claimed");
    let uze_home = scratch.join("home");
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let mut old = ClaimHolder::spawn_as(&another_build_of_this_binary(&scratch), &uze_home);
    assert!(workspace_is_claimed());
    assert_eq!(
        super::claim_holder(),
        Some(old.pid()),
        "the claim names who holds it, which `flock` cannot"
    );
    // It bound no endpoint at all, which is what an endpoint named by
    // other rules looks like from here.
    assert!(!socket_path().unwrap().exists());

    assert!(super::stop().is_ok(), "stopping it is not a failure");

    assert!(
        !workspace_is_claimed(),
        "and the workspace is free for the next server"
    );
    assert!(!old.process.wait().unwrap().success(), "it was ended");

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The server every machine already has: one from a release that
/// predates the claim's record of who holds it. It answers at an
/// endpoint this build does not compute and names nobody, so nothing
/// here can end it — and saying "nothing to stop" is what sent an
/// operator to restart their machine. It is said instead.
#[test]
fn a_claim_this_build_cannot_name_is_reported_rather_than_called_stopped() {
    let scratch = uze_testkit::temp::socket_scratch("stop-unnamed");
    let uze_home = scratch.join("home");
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let holder = ClaimHolder::spawn_as(&another_build_of_this_binary(&scratch), &uze_home);
    // What a server older than `record_claimant` leaves: the lock held,
    // and nothing written in it.
    std::fs::write(super::workspace_lock_path(), b"").unwrap();
    assert!(workspace_is_claimed());
    assert_eq!(super::claim_holder(), None);

    let refused = super::stop().expect_err("a claim nobody can name is not a clean stop");
    let said = refused.to_string();
    assert!(
        said.contains("serving this workspace") && said.contains(&super::host::find_server()),
        "the message names the situation and how to end it: {said}"
    );
    assert!(
        workspace_is_claimed(),
        "and nothing was signalled on a claim this build cannot vouch for"
    );

    holder.release();
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn transcript_preserves_style_cursor_and_alternate_screen() {
    let (sender, _receiver) = std::sync::mpsc::channel();
    let mut terminal = Term::new(Config::default(), &TermSize::new(12, 3), reply_sink(sender));
    let mut parser: Processor = Processor::new();
    parser.advance(&mut terminal, b"\x1b[31mred\x1b[0m\x1b[2;5H!");
    let normal = snapshot(PaneId(1), &terminal);
    assert_eq!(normal.cells[0].character, 'r');
    assert_eq!(normal.cells[0].foreground, TerminalColor::Indexed(1));
    assert_eq!(normal.cursor.row, 1);
    assert_eq!(normal.cursor.column, 5);
    assert!(!normal.alternate_screen);
    parser.advance(&mut terminal, b"\x1b[?1049h");
    assert!(snapshot(PaneId(1), &terminal).alternate_screen);
    parser.advance(&mut terminal, b"\x1b[?1049l");
    assert!(!snapshot(PaneId(1), &terminal).alternate_screen);
}

#[test]
fn mouse_mode_reflects_what_the_pane_actually_asked_for() {
    let (sender, _receiver) = std::sync::mpsc::channel();
    let mut terminal = Term::new(Config::default(), &TermSize::new(12, 3), reply_sink(sender));
    let mut parser: Processor = Processor::new();
    assert_eq!(snapshot(PaneId(1), &terminal).mouse, MouseMode::default());

    // Click reporting (1000) plus SGR extended coordinates (1006), no
    // drag/motion — a plain click-tracking app (a pager's mouse mode,
    // say), not one that also wants motion while a button is held.
    parser.advance(&mut terminal, b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        snapshot(PaneId(1), &terminal).mouse,
        MouseMode {
            reports_clicks: true,
            reports_drag: false,
            sgr: true,
        }
    );

    // Drag reporting (1002) layers on top — the shape ratatui/textual/
    // ink-style TUIs (Codex, OpenCode) actually request for click-and-
    // drag UI like tab strips.
    parser.advance(&mut terminal, b"\x1b[?1002h");
    assert!(snapshot(PaneId(1), &terminal).mouse.reports_drag);

    parser.advance(&mut terminal, b"\x1b[?1000l\x1b[?1002l\x1b[?1006l");
    assert_eq!(snapshot(PaneId(1), &terminal).mouse, MouseMode::default());
}

#[test]
fn bracketed_paste_reflects_what_the_pane_actually_asked_for() {
    // A readline-style program (Claude Code, Codex) turns this on
    // during its own startup — the client mirrors it onto the real
    // terminal so a physical paste (including a terminal's own
    // clipboard-image-to-text conversion) reaches the pane framed the
    // way the program expects, instead of arriving as a flood of
    // individual keystrokes a plain shell would.
    let (sender, _receiver) = std::sync::mpsc::channel();
    let mut terminal = Term::new(Config::default(), &TermSize::new(12, 3), reply_sink(sender));
    let mut parser: Processor = Processor::new();
    assert!(!snapshot(PaneId(1), &terminal).bracketed_paste);

    parser.advance(&mut terminal, b"\x1b[?2004h");
    assert!(snapshot(PaneId(1), &terminal).bracketed_paste);

    parser.advance(&mut terminal, b"\x1b[?2004l");
    assert!(!snapshot(PaneId(1), &terminal).bracketed_paste);
}

#[test]
fn osc_background_and_foreground_queries_get_answered_instead_of_hanging() {
    // Regression: `Term::dynamic_color_sequence` (what OSC 10/11
    // queries dispatch to) never emits `Event::PtyWrite` itself — it
    // hands back a formatting closure via `Event::ColorRequest` that
    // the `EventListener` must resolve and write back. A listener that
    // only forwards `PtyWrite` (as `ReplySink` used to) silently drops
    // it, which is exactly what left a pane's own OSC 11 background
    // probe — used by adaptive TUIs like Codex to pick a light- or
    // dark-themed surface — unanswered.
    let (sender, receiver) = std::sync::mpsc::channel();
    // A palette no default would ever produce, so the reply can only be
    // coming from what the client set.
    let palette = Arc::new(Mutex::new(Palette {
        foreground: (0x11, 0x22, 0x33),
        background: (0x44, 0x55, 0x66),
        ..Palette::default()
    }));
    let mut terminal = Term::new(
        Config::default(),
        &TermSize::new(12, 3),
        ReplySink::new(sender, Arc::clone(&palette)),
    );
    let mut parser: Processor = Processor::new();

    parser.advance(&mut terminal, b"\x1b]10;?\x1b\\");
    assert_eq!(
        receiver.try_recv().expect("OSC 10 reply"),
        b"\x1b]10;rgb:1111/2222/3333\x1b\\".to_vec()
    );

    parser.advance(&mut terminal, b"\x1b]11;?\x1b\\");
    assert_eq!(
        receiver.try_recv().expect("OSC 11 reply"),
        b"\x1b]11;rgb:4444/5555/6666\x1b\\".to_vec()
    );

    // A theme changed after the pane started reaches it too: the palette
    // is shared, not copied into the sink.
    palette.lock().expect("palette").background = (0xaa, 0xbb, 0xcc);
    parser.advance(&mut terminal, b"\x1b]11;?\x1b\\");
    assert_eq!(
        receiver
            .try_recv()
            .expect("OSC 11 reply after a theme change"),
        b"\x1b]11;rgb:aaaa/bbbb/cccc\x1b\\".to_vec()
    );
}

#[test]
fn resize_changes_snapshot_dimensions() {
    let (sender, _receiver) = std::sync::mpsc::channel();
    let mut terminal = Term::new(Config::default(), &TermSize::new(8, 2), reply_sink(sender));
    terminal.resize(TermSize::new(20, 4));
    let rendered = snapshot(PaneId(1), &terminal);
    assert_eq!((rendered.columns, rendered.rows), (20, 4));
}

#[test]
fn snapshot_renders_the_scrollback_viewport() {
    let (sender, _receiver) = std::sync::mpsc::channel();
    let mut terminal = Term::new(Config::default(), &TermSize::new(8, 2), reply_sink(sender));
    let mut parser: Processor = Processor::new();
    parser.advance(&mut terminal, b"first\r\nsecond\r\nthird");

    terminal.scroll_display(Scroll::Delta(1));
    let rendered: String = snapshot(PaneId(1), &terminal)
        .cells
        .into_iter()
        .map(|cell| cell.character)
        .collect();

    assert!(rendered.contains("first"));
    assert!(rendered.contains("second"));
    assert!(!rendered.contains("third"));
}

/// The argv that runs a line in this platform's shell, written once for
/// each shell.
fn shell_argv(posix: &str, windows: &str) -> Vec<String> {
    let (program, arguments) =
        uze_platform::shell::invocation(uze_platform::shell::spelling(posix, windows));
    std::iter::once(program).chain(arguments).collect()
}

/// A pane whose screen did not move offers nothing the second time: the
/// baseline goes out, and an identical one after it does not.
#[test]
fn damage_that_changes_nothing_drawn_is_not_offered() {
    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(12),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Program {
            argv: shell_argv("sleep 30", "Start-Sleep 30"),
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();

    let mut offered = 0;
    pane.offer_damage(|_| offered += 1);
    pane.offer_damage(|_| offered += 1);
    pane.stop();

    assert_eq!(offered, 1, "an unchanged screen was offered again");
}

#[test]
fn damage_since_last_is_sparse_after_a_small_change() {
    let (damage, damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(9),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Shell,
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    // Baseline covers every cell — a fresh client has nothing to diff against.
    let baseline = pane.damage_since_last();
    assert_eq!(baseline.changed.len(), 80 * 24);

    pane.write(b"printf uze-diff-probe\\r");
    let probe = std::iter::from_fn(|| damage_events.recv_timeout(Duration::from_secs(10)).ok())
        .map(|_| pane.damage_since_last())
        .find(|damage| {
            damage
                .changed
                .iter()
                .any(|(_, _, cell)| cell.character == 'u')
        });
    pane.stop();
    let probe = probe.expect("the echoed command never reached the grid");
    assert!(
        !probe.changed.is_empty(),
        "expected the echoed command to show up as changed cells"
    );
    assert!(
        probe.changed.len() < 80 * 24,
        "a one-line echo must not redescribe the whole grid, got {} changed cells",
        probe.changed.len()
    );
}

#[test]
fn pane_process_keeps_output_until_explicit_stop() {
    let (damage, damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(7),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Shell,
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    pane.write(b"printf uze-runtime-live\\r");
    let rendered = std::iter::from_fn(|| damage_events.recv_timeout(Duration::from_secs(10)).ok())
        .any(|_| {
            pane.snapshot()
                .cells
                .into_iter()
                .map(|cell| cell.character)
                .collect::<String>()
                .contains("uze-runtime-live")
        });
    pane.stop();
    assert!(rendered, "the printed line never reached the grid");
}

// Unix only: Reads the foreground of a pane running `/bin/sh` against the terminal's own process group leader.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn foreground_status_reports_the_spawned_shell_and_its_cwd() {
    // This is the *fallback* identity path: no shim identity present,
    // so the kernel's `comm` for the spawned shell is what gets
    // reported. The spawned child inherits this process's environment,
    // and on a dogfooding machine that environment carries the
    // `UZE_SHIM_NAME` of the session running the test suite itself —
    // which `foreground_status` rightly prefers (see the sibling test),
    // making this assertion read the developer's own session instead of
    // the shell it just spawned. Clearing it under the shared env lock
    // is what makes the fallback the thing actually under test.
    let mut env = uze_testkit::env::scope();
    env.remove("UZE_SHIM_NAME");
    // The shell a pane launches is the operator's `$SHELL`, which on a
    // runner is whatever it happens to report. Pinned to a shell whose
    // process takes the name of its own file: macOS's `/bin/sh` is a
    // shim that runs as `bash`.
    let shell = ["/bin/bash", "/bin/sh"]
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
        .expect("a POSIX shell");
    env.set("SHELL", shell);
    let (damage, _damage_events) = std::sync::mpsc::channel();
    // Canonicalized, because the assertion below compares this against
    // what the kernel reports, and the kernel answers with the real
    // path: `/tmp` is a symlink to `/private/tmp` on macOS, so spawning
    // in `/tmp` and expecting `/tmp` back never matches there.
    let pane_cwd = uze_platform::path::canonical(&std::env::temp_dir())
        .expect("the system temp directory must resolve");
    let pane = PaneRuntime::spawn(
        PaneId(11),
        pane_cwd.clone(),
        80,
        24,
        damage,
        Launch::Shell,
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    let expected_name = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a shell path names a file")
        .to_owned();

    // Poll until the *spawned shell* owns the PTY's foreground group,
    // identified by its cwd. Before it does, `process_group_leader`
    // transiently reports this test binary's own group — and reading
    // that process's `/proc/<pid>/environ` still yields the
    // `UZE_SHIM_NAME` of the session running the suite, because
    // `/proc/environ` exposes the environment block captured at `exec`
    // and is unaffected by a later `unsetenv`. Accepting the first
    // `Some` therefore made this assert against the developer's own
    // session at random.
    // A deadline far past any scheduler: what is being waited on is
    // another process being scheduled and reaching `exec`, and the
    // assertion below is about *what* it reports, never about how fast.
    // Five seconds ran out under instrumented coverage on a small
    // machine, which is why coverage used to skip this test by name.
    //
    // Waited on by *identity*, not by directory. The pane's child already
    // stands in `pane_cwd` between `fork` and `exec` — that is when the
    // cwd is set — while still carrying the name it forked from. A loop
    // that stopped at the first matching directory therefore accepted a
    // process mid-spawn and read this test binary's own name back out of
    // it, which is exactly what a macOS runner caught. Waiting for the
    // shell to have `exec`ed also promotes the directory from a filter to
    // an assertion, which is what it should have been.
    let mut status = None;
    let mut last_seen = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        let reading = pane.reading().and_then(|reading| reading.status);
        if let Some((_, process)) = &reading
            && *process == expected_name
        {
            status = reading;
            break;
        }
        last_seen = reading.or(last_seen);
        thread::sleep(Duration::from_millis(10));
    }
    pane.stop();

    let (cwd, process) = status.unwrap_or_else(|| {
        panic!(
            "the spawned shell must own the PTY foreground group; \
                 waited for {expected_name:?} and last saw {last_seen:?}"
        )
    });
    assert_eq!(cwd, pane_cwd);
    assert_eq!(process, expected_name);
}

/// The kernel derives `comm` from the executed *file's own basename*,
/// not from anything a person typed — which is exactly why a real
/// Claude Code session reports its version number there instead of
/// `claude`: it runs from `~/.local/share/claude/versions/<version>`.
/// A copy of `sleep` under a version-number filename reproduces that
/// same shape without depending on Claude Code being installed.
/// `UZE_SHIM_NAME`, set by `src/shim.rs` right before it `exec`s into
/// the real binary, must survive that and still be what
/// `foreground_status` reports.
// Unix only: A POSIX shell `exec`s into a program named like a version, as a Unix harness binary is.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn foreground_status_prefers_the_shim_identity_over_a_version_named_comm() {
    let bin_dir = uze_testkit::temp::scratch("shim-identity-test");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let versioned_binary = bin_dir.join("2.1.251");
    uze_testkit::process::install_executable(
        &versioned_binary,
        &std::fs::read("/bin/sleep").unwrap(),
    );

    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(13),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Program {
            argv: vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                // `$$` is the shell's own pid, and `exec` keeps it — the
                // same relationship `src/shim.rs` has to the harness it
                // replaces itself with, which is what makes the stamp
                // belong to the process that carries it.
                format!(
                    "export UZE_SHIM_NAME=claude UZE_SHIM_PID=$$; exec {} 5",
                    versioned_binary.display()
                ),
            ],
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();

    // Five seconds, and only a matching reading is kept — the same two
    // properties as the sibling test above, and for the same two
    // reasons: what is being waited on is another process reaching
    // `exec`, and a reading taken before it does names the process this
    // one forked from.
    let mut status = None;
    let mut last_seen = None;
    for _ in 0..500 {
        let reading = pane.reading().and_then(|reading| reading.status);
        if let Some((_, process)) = &reading
            && process == "claude"
        {
            status = reading;
            break;
        }
        last_seen = reading.or(last_seen);
        thread::sleep(Duration::from_millis(10));
    }
    pane.stop();
    let _ = std::fs::remove_dir_all(&bin_dir);

    let (_, process) = status.unwrap_or_else(|| {
        panic!("the shim identity must reach the foreground; last saw {last_seen:?}")
    });
    assert_eq!(process, "claude");
}

/// The shim before its `exec`: `uze` run through a symlink named after
/// the harness, and no stamp, because the stamp is only in the
/// environment it hands on. That is the launcher at work, and must
/// never read as a harness that went around it. Linux already calls it
/// `claude` (the link's name); macOS calls it `uze` (the file the link
/// resolves to), so either name is the shim in the foreground.
// Unix only: Catches the shim between its start and its `exec`, which Windows, with no `exec`, has no moment for.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_shim_caught_before_its_exec_counts_as_the_launcher() {
    let bin_dir = uze_testkit::temp::scratch("shim-in-flight-test");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let uze = bin_dir.join("uze");
    uze_testkit::process::install_executable(&uze, &std::fs::read("/bin/sleep").unwrap());
    let shim = bin_dir.join("claude");
    std::os::unix::fs::symlink(&uze, &shim).unwrap();

    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(31),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Program {
            argv: vec![shim.display().to_string(), "5".to_owned()],
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();

    let mut through = None;
    let mut last_seen = None;
    for _ in 0..500 {
        let reading = pane.reading().and_then(|reading| reading.status);
        if let Some((_, process)) = &reading
            && (process == "claude" || process == "uze")
        {
            through = pane.reading().map(|reading| reading.through_launcher);
            break;
        }
        last_seen = reading.or(last_seen);
        thread::sleep(Duration::from_millis(10));
    }
    pane.stop();
    let _ = std::fs::remove_dir_all(&bin_dir);

    assert_eq!(
        through,
        Some(true),
        "the shim in flight is the launcher; last saw {last_seen:?}"
    );
}

/// What a client says about where it is deciding *where it lands*, and
/// only [`Seating::Open`] may bring a space into being.
///
/// This is the server half of what makes closing a space stick.
/// Naming a seat on every attach reopened the space closed just
/// before it — within a run, when the runtime went away and the
/// client attached again, and across runs, where the launch directory
/// remade a space the operator had removed and quit. Both are the
/// same failure: a close that does not stay closed is
/// indistinguishable from a close that did not work.
#[test]
fn only_asking_to_open_a_space_may_create_one() {
    let scratch = uze_testkit::temp::socket_scratch("attach-seating");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let elsewhere = scratch.join("elsewhere");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &elsewhere, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);
    let (server, _damage) = Server::new(seat_at(&project), socket_path().unwrap()).unwrap();
    let server = Arc::new(server);

    let attach = |seating: crate::Seating| {
        let (client, driver) = super::transport::pair().unwrap();
        let serving = {
            let server = Arc::clone(&server);
            std::thread::spawn(move || server.handle_client(client))
        };
        let mut writer = driver.try_clone().unwrap();
        let mut reader = std::io::BufReader::new(driver);
        send_request(
            &mut writer,
            &crate::ClientRequest::Attach {
                version: crate::PROTOCOL_VERSION,
                columns: 80,
                rows: 24,
                seating,
            },
        )
        .unwrap();
        let landed = std::iter::from_fn(|| read_event(&mut reader).unwrap())
            .find_map(|event| match event {
                crate::ClientEvent::Snapshot { session } => {
                    Some(session.selected_space().root.clone())
                }
                _ => None,
            })
            .expect("the client attached");
        let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
        let _ = serving.join();
        landed
    };
    let spaces = || {
        server
            .session
            .lock()
            .expect("session poisoned")
            .workspace
            .spaces
            .len()
    };

    assert_eq!(spaces(), 1, "the bootstrap space, and nothing else");

    let landed = attach(crate::Seating::At(seat_at(&elsewhere)));
    assert_eq!(spaces(), 1, "landing somewhere unopened created a space");
    assert_eq!(
        landed, project,
        "and the client lands where the session already was"
    );

    let landed = attach(crate::Seating::WhereItLeftOff);
    assert_eq!(spaces(), 1, "saying nothing created a space");
    assert_eq!(landed, project);

    let landed = attach(crate::Seating::At(seat_at(&project)));
    assert_eq!(spaces(), 1, "a seat that is already open opens nothing");
    assert_eq!(landed, project, "and is what the client lands on");

    let landed = attach(crate::Seating::Open(seat_at(&elsewhere)));
    assert_eq!(spaces(), 2, "asking to open a space did not open one");
    assert_eq!(landed, elsewhere, "and the client lands in it");
}

/// A tab is opened in the space the client selected last, so selecting
/// a space and then asking for a tab of its own lands the tab there —
/// the pair a click on a space whose shells all became agents sends.
#[test]
fn a_tab_asked_for_after_selecting_a_space_opens_in_that_space() {
    let scratch = uze_testkit::temp::socket_scratch("select-then-create");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let elsewhere = scratch.join("elsewhere");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &elsewhere, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);
    let (server, _damage) = Server::new(seat_at(&project), socket_path().unwrap()).unwrap();
    let server = Arc::new(server);
    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    let mut reader = std::io::BufReader::new(driver);
    send_request(
        &mut writer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 80,
            rows: 24,
            seating: crate::Seating::Open(seat_at(&elsewhere)),
        },
    )
    .unwrap();
    let first = std::iter::from_fn(|| read_event(&mut reader).unwrap())
        .find_map(|event| match event {
            crate::ClientEvent::Snapshot { session } => session
                .workspace
                .spaces
                .iter()
                .find(|space| space.root == project)
                .map(|space| space.id),
            _ => None,
        })
        .expect("the client attached beside the project's space");

    for request in [
        crate::ClientRequest::SelectSpace { space: first },
        crate::ClientRequest::CreateTab {
            label: "shell 1".into(),
            agent: None,
            columns: 80,
            rows: 24,
            cwd: Some(project.clone()),
            command: None,
            env: Vec::new(),
        },
    ] {
        send_request(&mut writer, &request).unwrap();
    }
    let opened = std::iter::from_fn(|| read_event(&mut reader).unwrap())
        .find_map(|event| match event {
            crate::ClientEvent::SessionUpdated { session } => {
                let tabs = |root: &Path| {
                    session
                        .workspace
                        .spaces
                        .iter()
                        .find(|space| space.root == root)
                        .map_or(0, |space| space.tabs.len())
                };
                (tabs(&project) + tabs(&elsewhere) == 3).then(|| (tabs(&project), tabs(&elsewhere)))
            }
            _ => None,
        })
        .expect("the tab was opened");
    let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
    let _ = serving.join();

    assert_eq!(
        opened,
        (2, 1),
        "the tab opened in the space selected, not the one attached to"
    );
}

// Unix only: Proves a process group and a FIFO end with the pane; Windows ends a Job Object, proven in uze-platform.
#[cfg(any(target_os = "linux", target_os = "macos"))]
/// Stopping a pane ends what its program left behind in its process
/// group, not only the program: a worker deaf to the hangup would
/// otherwise outlive the pane. The worker holds the only writer of a
/// FIFO, so its death is the FIFO hanging up — no polling for it.
#[test]
fn a_stopped_pane_takes_its_process_group_with_it() {
    let scratch = uze_testkit::temp::scratch("pane-group");
    std::fs::create_dir_all(&scratch).unwrap();
    let fifo = scratch.join("worker");
    let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);

    let (damage, damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(8),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Program {
            argv: vec![
                "sh".into(),
                "-c".into(),
                format!(
                    "trap '' HUP; sleep 300 > '{}' & echo worker-started; wait",
                    fifo.display()
                ),
            ],
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    // Opening for reading waits for the worker to open its end.
    let reader = std::fs::File::open(&fifo).unwrap();
    let started = damage_events.iter().any(|_| {
        pane.snapshot()
            .cells
            .iter()
            .map(|cell| cell.character)
            .collect::<String>()
            .contains("worker-started")
    });
    assert!(started, "the worker never started");

    pane.stop().join().expect("the reaper finished");

    // A read that ends is the worker's end of the FIFO closing, which
    // only its death does. On a thread, so a survivor fails the test
    // instead of hanging it; not `poll`, which macOS does not answer
    // for a FIFO.
    let (ended, hung_up) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = reader;
        let _ = std::io::Read::read_to_end(&mut reader, &mut Vec::new());
        let _ = ended.send(());
    });
    assert!(
        hung_up.recv_timeout(Duration::from_secs(10)).is_ok(),
        "the worker outlived its pane"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A client that stops reading is not buffered for without limit: once
/// its queue is full it is marked stale and sent nothing more, and once
/// it has caught up it is sent the whole workspace again.
#[test]
fn a_client_that_stops_reading_is_bounded_and_resynchronized() {
    let scratch = uze_testkit::temp::socket_scratch("stalled-client");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);
    let (server, _damage) = Server::new(seat_at(&project), socket_path().unwrap()).unwrap();
    let server = Arc::new(server);

    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    let mut reader = std::io::BufReader::new(driver);
    send_request(
        &mut writer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 80,
            rows: 24,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();
    let outbox = || {
        let clients = server.clients.lock().expect("clients poisoned");
        Arc::clone(&clients.first().expect("the client attached").events)
    };
    let attached = std::iter::from_fn(|| read_event(&mut reader).unwrap())
        .any(|event| matches!(event, crate::ClientEvent::Snapshot { .. }));
    assert!(attached, "the client never attached");

    // Far more than its queue and its socket's buffer can hold. Session
    // updates rather than damage: damage that changes nothing is not sent,
    // and a session broadcast persists nothing it already wrote.
    for _ in 0..super::OUTBOX_CAPACITY * 64 {
        server.broadcast_session();
    }
    let waiting = outbox()
        .backlog
        .pending
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        waiting <= super::OUTBOX_CAPACITY,
        "{waiting} events queued for a client that reads nothing"
    );
    assert!(
        outbox()
            .backlog
            .stale
            .load(std::sync::atomic::Ordering::Relaxed)
    );

    let caught_up =
        std::iter::from_fn(|| read_event(&mut reader).unwrap()).any(|_| outbox().awaits_resync());
    assert!(caught_up, "the client never drained its queue");
    server.resync_stale_clients();
    let resynchronized = std::iter::from_fn(|| read_event(&mut reader).unwrap())
        .any(|event| matches!(event, crate::ClientEvent::Snapshot { .. }));
    assert!(
        resynchronized,
        "the stale client was never sent the workspace again"
    );
    assert!(
        !outbox()
            .backlog
            .stale
            .load(std::sync::atomic::Ordering::Relaxed)
    );

    let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
    drop(writer);
    drop(reader);
    let _ = serving.join();
    server.stop_panes();
}

/// A stopped pane's process is reaped, not left a zombie for the life of
/// the server: once the reaper is done, its pid names no process at all
/// — a zombie would still answer `kill(pid, 0)`. Zombies and the hangup
/// the program ignores are POSIX's.
// Unix only: A zombie and a SIGHUP are Unix things.
#[cfg(unix)]
#[test]
fn a_stopped_pane_leaves_no_zombie() {
    let (damage, damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(7),
        std::env::temp_dir(),
        80,
        24,
        damage,
        // Deaf to the SIGHUP `kill` tries first, so it takes the SIGKILL
        // that nothing used to wait on.
        Launch::Program {
            argv: vec![
                "sh".into(),
                "-c".into(),
                "trap '' HUP; echo deaf-to-hangup; exec sleep 30".into(),
            ],
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    // Only once the trap is in place does the hangup go unheard; a
    // signal that beat it would kill the shell and prove nothing.
    let deaf = damage_events.iter().any(|_| {
        pane.snapshot()
            .cells
            .iter()
            .map(|cell| cell.character)
            .collect::<String>()
            .contains("deaf-to-hangup")
    });
    assert!(deaf, "the program never reported its trap");
    let pid = pane
        .child
        .lock()
        .expect("child poisoned")
        .process_id()
        .expect("a spawned child has a pid");

    pane.stop().join().expect("the reaper finished");

    assert_ne!(
        uze_platform::process::alive(pid),
        Some(true),
        "pid {pid} survived as a zombie"
    );
}

/// A program's first output can land before its pane is registered,
/// and damage for a pane nobody can find is dropped. Registration is
/// what flushes it, so a pane is delivered even when its program
/// prints nothing more — here, nothing at all.
#[test]
fn a_spawned_pane_is_flushed_once_it_is_registered() {
    let scratch = uze_testkit::temp::socket_scratch("flushed-on-spawn");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let (server, damage) = Server::new(seat_at(&project), socket_path().unwrap()).unwrap();
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .create_space(None, seat_at(&project), 80, 24)
        .pane;
    let silent = Launch::Program {
        argv: shell_argv("sleep 30", "Start-Sleep 30"),
        env: Vec::new(),
    };
    server.spawn_pane(pane, silent).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let flushed = std::iter::from_fn(|| {
        damage
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .ok()
    })
    .any(|notified| notified == pane);
    server.stop_panes();
    assert!(
        flushed,
        "the silent pane was never offered to the broadcaster"
    );
}

#[test]
fn the_server_works_in_no_checkout() {
    let project = Path::new("/project");
    let command = super::server_command(Path::new("/usr/bin/uze"), &seat_at(project));
    assert_ne!(command.get_current_dir(), Some(project));
    assert_eq!(
        command.get_current_dir(),
        Some(super::host::server_directory().as_path())
    );
}

/// A program that does not read its input fills the terminal's buffer,
/// and the write into it blocks for as long as the program runs. That
/// wait belongs to the one pane: the map every other pane's input,
/// output and resize go through stays free. Told with a POSIX line
/// discipline in raw mode; a pseudoconsole buffers input on its own terms.
// Unix only: Drives `/bin/sh` panes that `exec` into `cat`-like writers.
#[cfg(unix)]
#[test]
fn a_pane_that_stops_reading_does_not_hold_up_the_others() {
    let scratch = uze_testkit::temp::socket_scratch("blocked-write");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let (server, _damage) = Server::new(seat_at(&project), socket_path().unwrap()).unwrap();
    let server = Arc::new(server);
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .create_space(None, seat_at(&project), 80, 24)
        .pane;
    // Raw, because a canonical-mode terminal discards what overflows
    // its line buffer instead of blocking the writer.
    let deaf = Launch::Program {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            "stty raw -echo; exec sleep 30".into(),
        ],
        env: Vec::new(),
    };
    server.spawn_pane(pane, deaf).unwrap();
    // `sleep` in front means `stty raw` already ran: written any earlier,
    // a canonical-mode line discipline drops what overflows and the write
    // simply finishes.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !server
        .runtime(pane)
        .and_then(|runtime| runtime.reading().and_then(|reading| reading.status))
        .is_some_and(|(_, process)| process == "sleep")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the pane never reached `sleep`"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let writing = Arc::clone(&server);
    let writer = thread::spawn(move || writing.write_input(pane, &vec![b'x'; 1 << 20]));
    // Inside the write, with the pane's writer held, before the map is
    // asked about. A platform whose PTY takes the whole megabyte without
    // blocking never reaches the case this is about, and says so rather
    // than passing as though it had.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let held = server
            .panes
            .lock()
            .expect("panes poisoned")
            .get(&pane)
            .is_some_and(|runtime| runtime.writer.try_lock().is_err());
        if held {
            break;
        }
        if writer.is_finished() {
            eprintln!("the write never blocked on this platform; nothing to hold up");
            server.stop_panes();
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the write neither blocked nor finished"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let free = loop {
        if server.panes.try_lock().is_ok() {
            break true;
        }
        if std::time::Instant::now() >= deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Stopping takes the same map, so it would hang where this fails.
    assert!(free, "a blocked write held the pane map");
    server.stop_panes();
}

#[test]
fn attaching_without_a_root_neither_creates_nor_reopens_a_space() {
    let scratch = uze_testkit::temp::socket_scratch("rootless");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let other = scratch.join("other");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &other, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).unwrap();
    let server = Arc::new(server);
    // A second space, so closing the launch space leaves a survivor
    // rather than opening a replacement.
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .create_space(
            Some("other".into()),
            crate::SpaceSeat {
                root: other.clone(),
            },
            80,
            24,
        )
        .pane;
    server.spawn_pane(pane, Launch::Shell).unwrap();
    let launch = {
        let mut session = server.session.lock().expect("session poisoned");
        let launch = session
            .space_for(&seat_at(&project))
            .expect("the bootstrap space is rooted at the launch directory");
        let seat = crate::SpaceSeat {
            root: other.clone(),
        };
        assert!(
            session.remove_space(launch, seat, 80, 24).is_some(),
            "space closed"
        );
        launch
    };

    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    let mut reader = std::io::BufReader::new(driver);
    send_request(
        &mut writer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 80,
            rows: 24,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();
    let attached = loop {
        match read_event(&mut reader).unwrap() {
            Some(crate::ClientEvent::Snapshot { session }) => break session,
            Some(_) => {}
            None => panic!("the server hung up before attaching"),
        }
    };
    assert_eq!(
        attached.space_for(&seat_at(&project)),
        None,
        "a rootless attach left the closed space closed"
    );
    assert_eq!(attached.workspace.spaces.len(), 1);
    assert_ne!(attached.workspace.selected_space, launch);

    let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
    drop(writer);
    drop(reader);
    let _ = serving.join();
    server.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The whole point of persistence: a server that starts with nothing
/// running (simulating a reboot, a crash, `kill -9` — anything that
/// left no chance for a clean stop) still comes back with the same
/// spaces and tabs a previous instance for this same `root` had, each
/// tab's pane relaunched with whatever it was last spawned with —
/// `None` for a plain shell, the recorded `argv` for an agent.
// Unix only: Relaunches `sleep`, a program Windows does not ship.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_restarted_server_relaunches_the_same_spaces_tabs_and_agent_commands() {
    let scratch = uze_testkit::temp::socket_scratch("persist");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    std::fs::create_dir_all(&runtime_dir).unwrap();

    // See `uze_testkit::env::scope`: held for the rest of this test so no
    // other test's own `UZE_HOME` scoping can interleave with this
    // one's. Restored exactly, not just cleared, on the way out.
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (first, _damage) = Server::new(seat_at(&project), socket.clone()).unwrap();
    let agent_pane = first
        .session
        .lock()
        .expect("session poisoned")
        .create_space(
            Some("frontend".into()),
            crate::SpaceSeat {
                root: project.clone(),
            },
            80,
            24,
        )
        .pane;
    first.spawn_pane(agent_pane, sleep_five()).unwrap();
    // `CreateSpace`'s real dispatch (`runtime.rs`'s `handle_client`)
    // calls `broadcast_session`, which persists — replicated here
    // directly since this test drives `Server` without a socket.
    first.persist();
    first.stop_panes();
    // Restarting means the first server is *gone*: it holds the
    // workspace lock while it exists, and a second one restoring the
    // same spaces behind its back is the duplicate-agent failure that
    // lock is there to refuse.
    drop(first);

    let (second, _damage2) = Server::new(seat_at(&project), socket).unwrap();
    {
        let session = second.session.lock().expect("session poisoned");
        assert_eq!(session.workspace.spaces.len(), 2, "both spaces restored");
        let frontend = session
            .workspace
            .spaces
            .iter()
            .find(|space| space.label == "frontend")
            .expect("the second space's own label survived restore");
        let tab = &frontend.tabs[0];
        let panes = second.panes.lock().expect("panes poisoned");
        let runtime = panes
            .get(&tab.pane.id)
            .expect("restored tab's pane was actually spawned");
        assert_eq!(
            runtime.launch,
            sleep_five(),
            "restored tab relaunched with its original agent command"
        );
    }
    second.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_finished_direct_agent_is_replaced_by_a_shell_in_its_pane() {
    let scratch = uze_testkit::temp::socket_scratch("agentexit");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).unwrap();
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .create_space(
            Some("agent".into()),
            crate::SpaceSeat {
                root: project.clone(),
            },
            80,
            24,
        )
        .pane;
    server.spawn_pane(pane, exits_at_once(Vec::new())).unwrap();

    wait_for_shell_respawn(&server, pane);
    assert!(
        server
            .panes
            .lock()
            .expect("panes poisoned")
            .get(&pane)
            .is_some_and(|runtime| runtime.launch == Launch::Shell),
        "a completed direct agent must leave an interactive shell in its existing pane"
    );
    server.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn relaunch_command_for_process_recognizes_a_named_process_but_not_a_plain_shell() {
    assert_eq!(relaunch_command_for_process("zsh"), None);
    assert_eq!(relaunch_command_for_process("shell"), None);
    assert_eq!(relaunch_command_for_process(""), None);
    assert_eq!(relaunch_command_for_process("  "), None);
    // Whatever this reads is persisted and then spawned by the server
    // on the next restart, and the name it reads is one a process can
    // choose for itself (`UZE_SHIM_NAME` is an ordinary variable) — so
    // a candidate naming a file rather than a command is refused.
    assert_eq!(relaunch_command_for_process("/tmp/payload"), None);
    assert_eq!(relaunch_command_for_process("./payload"), None);
    assert_eq!(relaunch_command_for_process(r"C:\Temp\payload"), None);
    assert_eq!(relaunch_command_for_process(r".\payload"), None);
    assert_eq!(relaunch_command_for_process("C:payload"), None);
    assert_eq!(relaunch_command_for_process("PowerShell"), None);
    assert_eq!(
        relaunch_command_for_process("claude"),
        Some(vec!["claude".to_owned()])
    );
}

/// The exact case that motivated `relaunch_command_for_process`: a tab
/// opened as a plain "$ shell" (never through "+ agent", so it has no
/// launch of its own), where someone then typed an agent
/// straight into it — `update_pane_status` here stands in for the
/// status ticker's own probe reporting that live.
// Unix only: Relaunches `sleep`, a program Windows does not ship.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_plain_shell_tab_running_a_recognized_process_relaunches_as_that_process() {
    let scratch = uze_testkit::temp::socket_scratch("typed");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let socket = socket_path().unwrap();
    let (first, _damage) = Server::new(seat_at(&project), socket.clone()).unwrap();
    let pane_id = first
        .session
        .lock()
        .expect("session poisoned")
        .selected_tab()
        .pane
        .id;
    first
        .session
        .lock()
        .expect("session poisoned")
        .update_pane_status(pane_id, project.clone(), "sleep".to_owned());
    first.persist();
    first.stop_panes();
    drop(first);

    let (second, _damage2) = Server::new(seat_at(&project), socket).unwrap();
    {
        let session = second.session.lock().expect("session poisoned");
        let tab = session.selected_tab();
        let panes = second.panes.lock().expect("panes poisoned");
        let runtime = panes
            .get(&tab.pane.id)
            .expect("restored tab's pane was actually spawned");
        assert_eq!(
            runtime.launch,
            Launch::Program {
                argv: vec!["sleep".to_owned()],
                env: Vec::new(),
            },
            "a process typed straight into a plain shell tab still relaunches on restore"
        );
    }
    second.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The previous release wrote a kind per space. This build has no
/// field for one, and `serde` would have dropped it without a word —
/// which is why the version is read before the document.
///
/// What that guard used to do about it was set the whole file aside,
/// and on 2026-09-19 that cost an operator every space they had open
/// over a difference of one field. The guard now climbs the rung
/// instead: the kind is dropped deliberately, every other field is
/// carried, and the spaces open.
#[test]
fn a_workspace_from_the_previous_release_is_carried_across_rather_than_set_aside() {
    let scratch = uze_testkit::temp::socket_scratch("persprev");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let path = persisted_state_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Version 1 as the previous release wrote it: a kind per space.
    // The space carries a tab, because a space with none is not a
    // workspace anybody lost — `Session::restore` drops those, and the
    // claim being proven here is that a space with work in it survives.
    let kept = project.join("kept");
    std::fs::create_dir_all(&kept).unwrap();
    std::fs::write(
        &path,
        format!(
            r#"{{"spaces":[{{"label":"demo","root":{root},"kind":"worktree","tabs":[
                     {{"label":"shell","cwd":{root},"agent":null,"launch":"Shell"}}]}}]}}"#,
            root = serde_json::to_string(&kept).unwrap()
        )
        .as_bytes(),
    )
    .unwrap();

    let (restored, _) = load_persisted_workspace_at(&path);
    let restored = restored.expect("the previous release's spaces survive");
    assert_eq!(
        restored.spaces.len(),
        1,
        "the space is carried across, not thrown away"
    );
    assert_eq!(restored.spaces[0].label, "demo");
    assert!(
        path.exists(),
        "and the workspace keeps its own name: nothing was set aside"
    );
    let beside: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("unreadable"))
        .collect();
    assert!(beside.is_empty(), "nothing was set aside: {beside:?}");

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).expect("server");
    {
        let session = server.session.lock().expect("session poisoned");
        assert_eq!(
            session.workspace.spaces[0].root, kept,
            "the server opens on the workspace it was left, not on the seat"
        );
    }
    server.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A persisted command is a guess — the agent binary it names may have
/// been uninstalled or renamed since. That must degrade to a plain
/// shell in that one tab, never take the whole restored workspace down
/// with it.
/// Which tab belongs with which has to survive the process, and a
/// `TabId` does not — the snapshot names the agent by its position in
/// the very list `Session::restore` rebuilds.
#[test]
fn the_snapshot_names_a_tabs_agent_by_position() {
    let scratch = uze_testkit::temp::socket_scratch("persagent");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).expect("server");
    {
        let mut session = server.session.lock().expect("session poisoned");
        let space = session.workspace.selected_space;
        session.add_tab(space, "agent".into(), None, 80, 24, project.clone());
        let agent = session.selected_tab().id;
        session.add_tab(space, "shell".into(), Some(agent), 80, 24, project.clone());
    }
    server.persist();

    let written: PersistedWorkspace =
        serde_json::from_slice(&std::fs::read(persisted_state_path()).unwrap()).unwrap();
    let tabs = &written.spaces[0].tabs;
    assert_eq!(tabs.len(), 3, "the bootstrap shell, the agent, its shell");
    assert_eq!(tabs[2].agent, Some(1), "the shell belongs with the agent");
    assert_eq!(tabs[1].agent, None);

    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Selecting a tab broadcasts the session and persists it, but nothing
/// selection-shaped is persisted: the same bytes are not written (and
/// fsynced) again, and a real change still is.
#[test]
fn persisting_an_unchanged_workspace_writes_nothing() {
    let scratch = uze_testkit::temp::socket_scratch("persame");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).expect("server");
    server.persist();
    let path = persisted_state_path();
    assert!(path.exists());
    std::fs::remove_file(&path).unwrap();

    let first = server
        .session
        .lock()
        .expect("session poisoned")
        .selected_tab()
        .id;
    server
        .session
        .lock()
        .expect("session poisoned")
        .select_tab(first);
    server.persist();
    assert!(!path.exists(), "the same workspace was written again");

    {
        let mut session = server.session.lock().expect("session poisoned");
        let space = session.workspace.selected_space;
        session.add_tab(space, "agent".into(), None, 80, 24, project.clone());
    }
    server.persist();
    assert!(path.exists(), "a changed workspace was not written");

    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_persisted_command_that_no_longer_resolves_falls_back_to_a_plain_shell() {
    let scratch = uze_testkit::temp::socket_scratch("perstale");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let path = persisted_state_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let stale = PersistedWorkspace {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        spaces: vec![SpaceSeed {
            label: "space 1".into(),
            root: project.clone(),
            tabs: vec![TabSeed {
                label: "shell".into(),
                cwd: project.clone(),
                agent: None,
                launch: Launch::Program {
                    argv: vec!["definitely-not-a-real-binary-xyz".to_owned()],
                    env: Vec::new(),
                },
            }],
        }],
    };
    std::fs::write(&path, serde_json::to_vec(&stale).unwrap()).unwrap();

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket)
        .expect("a stale persisted command must not fail server startup");
    let session = server.session.lock().expect("session poisoned");
    let tab = session.selected_tab();
    let panes = server.panes.lock().expect("panes poisoned");
    assert!(
        panes.contains_key(&tab.pane.id),
        "the tab still got a pane, spawned as a plain shell instead"
    );
    drop(panes);
    drop(session);
    server.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The four bytes a peer sends first become an allocation before
/// anything inside the frame — the protocol version included — can be
/// read, so the prefix is the one number that has to be distrusted on
/// its own. `0xffffffff` asks for 4 GiB.
#[test]
fn a_length_prefix_past_the_frame_limit_is_refused_before_it_is_allocated() {
    for refused in [u32::MAX, MAX_FRAME + 1] {
        let mut wire: &[u8] = &refused.to_le_bytes();
        assert!(
            matches!(
                read_message::<_, crate::ClientRequest>(&mut wire),
                Err(RuntimeError::Protocol(_))
            ),
            "a {refused}-byte frame must be refused, not allocated"
        );
    }
    // And the limit itself is a size the wire accepts, not one it
    // refuses: a cap that fired one byte early would disconnect a
    // client at the moment it resized. Truncated after the prefix, so
    // what this proves is that the read got past the bound and went
    // looking for the bytes.
    let mut wire: &[u8] = &MAX_FRAME.to_le_bytes();
    assert!(
        matches!(
            read_message::<_, crate::ClientRequest>(&mut wire),
            Err(RuntimeError::Io(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof
        ),
        "a frame of exactly the limit is one the wire allows"
    );
}

/// The same bound on the way out, so the two sides cannot disagree
/// about what is sendable — and nothing half-written reaches the wire.
#[test]
fn a_frame_past_the_limit_is_never_written_either() {
    let framed = |payload: usize| crate::ClientRequest::Input {
        pane: PaneId(1),
        bytes: vec![0u8; payload],
    };
    let overhead = bincode::serialized_size(&framed(0)).unwrap() as usize;

    let mut wire = Vec::new();
    assert!(matches!(
        write_message(&mut wire, &framed(MAX_FRAME as usize + 1 - overhead)),
        Err(RuntimeError::Protocol(_))
    ));
    assert!(
        wire.is_empty(),
        "nothing may reach the wire that the other side would refuse"
    );

    // Exactly the limit is sendable, and the reader accepts it: the two
    // sides agree on the boundary itself, not merely on numbers well
    // past it.
    write_message(&mut wire, &framed(MAX_FRAME as usize - overhead)).unwrap();
    assert_eq!(wire.len(), MAX_FRAME as usize + 4, "prefix plus the frame");
    let mut sent: &[u8] = &wire;
    assert!(
        read_message::<_, crate::ClientRequest>(&mut sent)
            .unwrap()
            .is_some(),
        "a frame of exactly the limit round-trips"
    );
}

/// [`MAX_FRAME`] and [`MAX_PANE_DIMENSION`] are one decision in two
/// constants: a repaint of the largest pane a client may ask for has to
/// fit, or the cap would disconnect a client at the moment it resized.
/// Measured from one worst-case cell rather than by building the grid —
/// every field in it is fixed-width, so the arithmetic is exact.
#[test]
fn a_full_repaint_of_the_largest_pane_fits_in_one_frame() {
    let widest_cell = (
        u16::MAX,
        u16::MAX,
        crate::RenderCell {
            character: '\u{10ffff}',
            foreground: TerminalColor::Rgb {
                red: 1,
                green: 2,
                blue: 3,
            },
            background: TerminalColor::Rgb {
                red: 4,
                green: 5,
                blue: 6,
            },
            attributes: crate::CellAttributes {
                bold: true,
                dim: true,
                italic: true,
                underline: true,
                inverse: true,
                hidden: true,
                strikeout: true,
                selected: true,
            },
        },
    );
    let per_cell = bincode::serialized_size(&widest_cell).expect("a cell has a size");
    let cells = u64::from(MAX_PANE_DIMENSION) * u64::from(MAX_PANE_DIMENSION);
    assert!(
        per_cell * cells < u64::from(MAX_FRAME),
        "a {MAX_PANE_DIMENSION}x{MAX_PANE_DIMENSION} repaint is {} bytes, past the \
             {MAX_FRAME}-byte frame limit",
        per_cell * cells
    );
}

/// `columns`/`rows` arrive from a peer and go into `Term::resize`,
/// which allocates a cell per position and clamps nothing: 65535×65535
/// is ~137 GB, and a failed allocation aborts the process that owns
/// every live agent pane. One malformed frame must not be able to do
/// that, from a buggy client as easily as a hostile one.
#[test]
fn a_resize_to_the_largest_number_on_the_wire_leaves_the_server_answering() {
    let scratch = uze_testkit::temp::socket_scratch("resizemax");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).unwrap();
    let server = Arc::new(server);
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .selected_tab()
        .pane
        .id;

    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    // So a server that stops answering fails this test instead of
    // hanging it.
    driver
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut reader = std::io::BufReader::new(driver);
    send_request(
        &mut writer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 0,
            rows: 0,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();
    send_request(
        &mut writer,
        &crate::ClientRequest::Resize {
            pane,
            columns: u16::MAX,
            rows: u16::MAX,
        },
    )
    .unwrap();

    // Attaching repaints every pane first (see
    // [`Server::broadcast_snapshot`]), so the event this test is about
    // is the one that reports a size the pane did not start at.
    let resized = loop {
        match read_event(&mut reader).expect("the server must still be speaking") {
            Some(crate::ClientEvent::Damage(damage))
                if damage.pane == pane && (damage.columns, damage.rows) != (80, 24) =>
            {
                break damage;
            }
            Some(_) => {}
            None => panic!("the server hung up rather than bounding the resize"),
        }
    };
    assert_eq!(
        (resized.columns, resized.rows),
        (MAX_PANE_DIMENSION, MAX_PANE_DIMENSION),
        "the resize is bounded and still honoured, not refused"
    );

    let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
    drop(writer);
    drop(reader);
    let _ = serving.join();
    server.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A selection is drawn for every client, so the server puts it away
/// itself when nobody will: when the drag covered only blanks, and when
/// the client that made it leaves without saying so.
#[test]
fn a_selection_is_put_away_when_it_covers_nothing_or_its_client_leaves() {
    let scratch = uze_testkit::temp::socket_scratch("selectleave");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).unwrap();
    let server = Arc::new(server);
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .selected_tab()
        .pane
        .id;
    let selected_cells = || {
        server
            .runtime(pane)
            .expect("the pane is running")
            .snapshot()
            .cells
            .iter()
            .filter(|cell| cell.attributes.selected)
            .count()
    };
    let blank_rows = crate::SelectionGesture::Begin {
        anchor: (0, 20),
        head: (10, 22),
    };

    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    driver
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut reader = std::io::BufReader::new(driver);
    for request in [
        crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 0,
            rows: 0,
            seating: crate::Seating::WhereItLeftOff,
        },
        crate::ClientRequest::Select {
            pane,
            gesture: blank_rows,
        },
        crate::ClientRequest::CopySelection { pane },
    ] {
        send_request(&mut writer, &request).unwrap();
    }
    let copied = loop {
        match read_event(&mut reader).expect("the server must still be speaking") {
            Some(crate::ClientEvent::SelectionText { text, .. }) => break text,
            Some(_) => {}
            None => panic!("the server hung up before answering the copy"),
        }
    };
    assert_eq!(copied, "");
    assert_eq!(
        selected_cells(),
        0,
        "a selection of blanks is not kept drawn"
    );

    send_request(
        &mut writer,
        &crate::ClientRequest::Select {
            pane,
            gesture: blank_rows,
        },
    )
    .unwrap();
    drop(writer);
    drop(reader);
    let _ = serving.join();
    assert_eq!(
        selected_cells(),
        0,
        "a client that left took its selection with it"
    );

    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// `CommandBuilder` seeds a pane from the *server's* environment, and
/// the server is started by whatever `uze` first needed one — in this
/// project, routinely a `uze` run from inside a shimmed agent. A plain
/// shell that inherited that stamp reports as the agent, persists as
/// one, and is relaunched as one on the next restart.
// Unix only: Reads a `/bin/sh` pane's environment through its process group leader.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_pane_does_not_inherit_the_servers_shim_identity() {
    let mut env = uze_testkit::env::scope();
    env.set("UZE_SHIM_NAME", "claude")
        .set("UZE_SHIM_PID", std::process::id().to_string());

    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(21),
        uze_platform::path::canonical(&std::env::temp_dir()).unwrap(),
        80,
        24,
        damage,
        Launch::Shell,
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let expected_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sh")
        .to_owned();

    // Waited on by identity, for the reason the sibling tests spell
    // out: a reading taken before the shell has `exec`ed names the
    // process it forked from, which here is this test binary.
    let mut reported = None;
    for _ in 0..500 {
        let reading = pane.reading().and_then(|reading| reading.status);
        if let Some((_, process)) = &reading
            && *process == expected_name
        {
            reported = reading;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let leader = pane
        .master
        .lock()
        .expect("master poisoned")
        .as_ref()
        .and_then(|master| master.process_group_leader());
    let through_launcher = pane.reading().map(|reading| reading.through_launcher);
    pane.stop();

    assert!(
        reported.is_some(),
        "the pane must report the shell it actually spawned, not the identity of the \
             session that happened to start the server"
    );
    assert_eq!(
        through_launcher,
        Some(false),
        "a shell nobody launched through a shim is reported as not coming through one"
    );
    let leader = leader.expect("the spawned shell owns the PTY foreground group");
    assert_eq!(
        uze_platform::probe::environment_value_of(leader as u32, "UZE_SHIM_NAME"),
        None,
        "a pane's environment may only carry what that pane's own launch put there"
    );
}

fn read_when_written(path: &Path) -> String {
    for _ in 0..500 {
        if let Ok(content) = std::fs::read_to_string(path) {
            return content;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("{} was never written", path.display())
}

/// The command a launch runs to report what its environment carries:
/// one file, one value, then exit. The value lands on a name of its own
/// and is renamed onto the reported path: a redirection creates the file
/// before the shell writes into it, so a reader watching for the path to
/// appear would otherwise be free to read the empty half of that window.
fn report_variable(variable: &str, into: &Path) -> Vec<String> {
    let partial = into.with_extension("partial");
    let (partial, reported) = (partial.display(), into.display());
    shell_argv(
        &format!(
            "printf %s \"${{{variable}-unset}}\" > \"{partial}\" && mv \"{partial}\" \"{reported}\""
        ),
        &format!(
            "$value = [Environment]::GetEnvironmentVariable('{variable}'); \
             if ($null -eq $value) {{ $value = 'unset' }}; \
             [IO.File]::WriteAllText('{partial}', $value); \
             Move-Item -LiteralPath '{partial}' -Destination '{reported}'"
        ),
    )
}

fn seat_at(root: &Path) -> crate::SpaceSeat {
    crate::SpaceSeat {
        root: root.to_path_buf(),
    }
}

// Unix only: `sleep`, a program Windows does not ship.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn sleep_five() -> Launch {
    Launch::Program {
        argv: vec!["sleep".to_owned(), "5".to_owned()],
        env: Vec::new(),
    }
}

/// The shell exiting at once, not `true`: macOS keeps `true` in
/// `/usr/bin`, Windows has none, and what this needs is any process that
/// exits at once.
fn exits_at_once(env: Vec<(String, String)>) -> Launch {
    Launch::Program {
        argv: shell_argv("exit 0", "exit 0"),
        env,
    }
}

/// Waits for `pane`, whose program ends at once, to be given the person's
/// shell in its place. That program is a shell line, and a PowerShell
/// started cold takes seconds to begin and end.
fn wait_for_shell_respawn(server: &Server, pane: PaneId) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        server.restore_finished_agent_panes();
        let restored = server
            .panes
            .lock()
            .expect("panes poisoned")
            .get(&pane)
            .is_some_and(|runtime| runtime.launch == Launch::Shell);
        if restored {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn stamp(id: &str) -> Vec<(String, String)> {
    vec![(
        crate::launch::AGENT_IDENTITY_VARIABLE.to_owned(),
        id.to_owned(),
    )]
}

#[test]
fn a_launch_environment_reaches_the_first_process() {
    let scratch = uze_testkit::temp::scratch("launchenv");
    let report = scratch.join("report");
    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(31),
        scratch.clone(),
        80,
        24,
        damage,
        Launch::Program {
            argv: report_variable(crate::launch::AGENT_IDENTITY_VARIABLE, &report),
            env: stamp("agent-31"),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    assert_eq!(read_when_written(&report), "agent-31");
    assert_eq!(pane.launch.env(), stamp("agent-31"));
    pane.stop();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A pane carries only what its own launch put there: a server that was
/// itself started inside an agent's pane does not hand that agent's
/// identity to the panes it opens, whatever they run.
#[test]
fn a_pane_does_not_inherit_the_servers_agent_identity() {
    let scratch = uze_testkit::temp::scratch("inheritagent");
    let report = scratch.join("report");
    let mut env = uze_testkit::env::scope();
    env.set(crate::launch::AGENT_IDENTITY_VARIABLE, "the-servers-own");
    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(32),
        scratch.clone(),
        80,
        24,
        damage,
        Launch::Program {
            argv: report_variable(crate::launch::AGENT_IDENTITY_VARIABLE, &report),
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();
    assert_eq!(read_when_written(&report), "unset");
    pane.stop();
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_launch_environment_survives_a_restart() {
    let scratch = uze_testkit::temp::socket_scratch("envrestart");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let report = scratch.join("report");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let path = persisted_state_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let persisted = PersistedWorkspace {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        spaces: vec![SpaceSeed {
            label: "space 1".into(),
            root: project.clone(),
            tabs: vec![TabSeed {
                label: "agent 1".into(),
                cwd: project.clone(),
                agent: None,
                launch: Launch::Program {
                    argv: report_variable(crate::launch::AGENT_IDENTITY_VARIABLE, &report),
                    env: stamp("agent-restarted"),
                },
            }],
        }],
    };
    std::fs::write(&path, serde_json::to_vec(&persisted).unwrap()).unwrap();

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).unwrap();
    assert_eq!(read_when_written(&report), "agent-restarted");
    let session = server.session.lock().expect("session poisoned");
    let tab = session.selected_tab();
    assert_eq!(
        tab.env,
        stamp("agent-restarted"),
        "the restored tab reports the launch it was respawned with"
    );
    drop(session);
    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_shell_respawn_carries_no_launch_environment() {
    let scratch = uze_testkit::temp::socket_scratch("envshell");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).unwrap();
    let pane = server
        .session
        .lock()
        .expect("session poisoned")
        .create_space(
            Some("agent".into()),
            crate::SpaceSeat {
                root: project.clone(),
            },
            80,
            24,
        )
        .pane;
    server
        .spawn_pane(pane, exits_at_once(stamp("agent-done")))
        .unwrap();
    let launched = server
        .session
        .lock()
        .expect("session poisoned")
        .selected_tab()
        .env
        .clone();
    assert_eq!(launched, stamp("agent-done"), "the tab reports the launch");

    wait_for_shell_respawn(&server, pane);
    let panes = server.panes.lock().expect("panes poisoned");
    let runtime = panes.get(&pane).expect("the pane was respawned");
    assert_eq!(runtime.launch, Launch::Shell);
    drop(panes);
    let session = server.session.lock().expect("session poisoned");
    assert!(
        session.selected_tab().env.is_empty(),
        "a tab respawned as a plain shell reports no launch"
    );
    drop(session);
    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The other half of the identity rule: an *inherited* stamp names an
/// ancestor, not the process it is read from, so it must be ignored.
/// Every child of a shimmed agent carries `UZE_SHIM_NAME`.
// Unix only: A POSIX shell `exec`s into the stamped program.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn foreground_status_ignores_a_shim_identity_stamped_for_another_process() {
    let bin_dir = uze_testkit::temp::scratch("shim-inherited-test");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let versioned_binary = bin_dir.join("2.1.251");
    uze_testkit::process::install_executable(
        &versioned_binary,
        &std::fs::read("/bin/sleep").unwrap(),
    );

    let (damage, _damage_events) = std::sync::mpsc::channel();
    let pane = PaneRuntime::spawn(
        PaneId(23),
        std::env::temp_dir(),
        80,
        24,
        damage,
        Launch::Program {
            argv: vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                format!(
                    "export UZE_SHIM_NAME=claude UZE_SHIM_PID=1; exec {} 5",
                    versioned_binary.display()
                ),
            ],
            env: Vec::new(),
        },
        Arc::new(Mutex::new(Palette::default())),
    )
    .unwrap();

    let mut reported = None;
    let mut last_seen = None;
    for _ in 0..500 {
        let reading = pane.reading().and_then(|reading| reading.status);
        if let Some((_, process)) = &reading
            && process == "2.1.251"
        {
            reported = reading;
            break;
        }
        assert!(
            !matches!(&reading, Some((_, process)) if process == "claude"),
            "a stamp made for another process must never be read as this one's identity"
        );
        last_seen = reading.or(last_seen);
        thread::sleep(Duration::from_millis(10));
    }
    pane.stop();
    let _ = std::fs::remove_dir_all(&bin_dir);

    assert!(
        reported.is_some(),
        "the kernel's own name for the process is what is left; last saw {last_seen:?}"
    );
}

/// The pid behind a socket is the kernel's answer, but a moment old by
/// the time it would be signalled, and pids are recycled: a process that
/// is not running `uze` is never signalled — an editor, a build, another
/// agent of the person's own.
// Unix only: A process that is not `uze`, run as a POSIX shell (`ReadyProcess`).
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_process_that_is_not_uze_is_never_signalled() {
    let scratch = uze_testkit::temp::socket_scratch("bystander");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &scratch);

    let bystander = ReadyProcess::spawn(Path::new("/bin/sh"));
    retire(
        bystander.pid(),
        &super::transport::scratch_endpoint(&scratch, "test.sock"),
    );

    assert!(
        bystander.finish(),
        "a process that is not uze must be left running"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A server the client started and never reaped is a zombie: still
/// addressable by `kill(pid, 0)`, which once left the endpoint held
/// hostage for the whole remaining life of that client. A zombie holds
/// no descriptor, so it holds no claim.
// Unix only: A zombie, left unreaped with `waitid`, is a Linux thing.
#[cfg(target_os = "linux")]
#[test]
fn a_crashed_server_nobody_reaped_holds_no_claim() {
    let scratch = uze_testkit::temp::scratch("terminal-zombie");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &scratch);

    let holder = ClaimHolder::spawn(&scratch);
    assert!(workspace_is_claimed(), "a running server holds its claim");

    let zombie = holder.crash();
    assert!(
        !workspace_is_claimed(),
        "an unreaped dead server must not hold its workspace hostage"
    );

    zombie.reap();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The recorded WSL case: `/tmp` wiped under a live server takes the
/// socket with it, and a second server started then would restore the
/// same `workspace.json` — every agent twice in the same checkout, both
/// servers persisting over each other.
/// The claim lives beside the workspace, under `$UZE_HOME`, so a
/// cleaner that can reach it has taken the workspace too.
#[test]
fn a_second_server_refuses_to_restore_a_workspace_another_one_holds() {
    let scratch = uze_testkit::temp::socket_scratch("wslock");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&uze_home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let socket = socket_path().unwrap();
    assert!(
        workspace_lock_path().starts_with(&uze_home),
        "the claim must live beside the workspace, never in a wipeable temp directory"
    );

    let first = ClaimHolder::spawn(&uze_home);
    let second = Server::new(seat_at(&project), socket.clone());
    assert!(
        matches!(second, Err(RuntimeError::Protocol(_))),
        "a workspace a live server holds must not be restored a second time"
    );

    first.release();
    let (third, _damage3) =
        Server::new(seat_at(&project), socket).expect("the claim is released with its holder");
    third.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// Held by the kernel, so a crash releases it: nothing to clean up, and
/// a stale claim is impossible by construction.
#[test]
fn a_workspace_claim_is_exclusive_and_released_with_its_holder() {
    let scratch = uze_testkit::temp::scratch("terminal-workspace-lock");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &scratch);

    let holder = ClaimHolder::spawn(&scratch);
    match WorkspaceLock::acquire() {
        Err(RuntimeError::Protocol(refusal)) => assert!(
            refusal.contains("already serving this workspace"),
            "contention has to name the server that holds it, not an errno: {refusal}"
        ),
        Err(other) => panic!("a held claim must read as contention, not as {other}"),
        Ok(_) => panic!("a held claim must not be granted twice"),
    }
    holder.release();
    WorkspaceLock::acquire().expect("released with its holder");

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A client asks for the claim shared and a server takes it exclusively,
/// so a server starting while a client asks can tell the asker from a
/// server — and gives up only for a server.
#[test]
fn an_asker_is_never_mistaken_for_a_server() {
    let scratch = uze_testkit::temp::scratch("terminal-lock-asker");
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("workspace.lock");
    let open = || {
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .unwrap()
    };
    let starting = open();

    let asker = open();
    super::try_lock(&asker, super::LockMode::Shared).expect("an asker takes it shared");
    assert!(!held_by_a_server(&starting).unwrap());
    drop(asker);

    let server = open();
    super::try_lock(&server, super::LockMode::Exclusive).expect("a server takes it exclusively");
    assert!(held_by_a_server(&starting).unwrap());
    drop(server);

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A server whose `$UZE_HOME` was deleted while it ran still holds its
/// lock — on a file that no longer exists. What attach reads is the
/// workspace as it is now: nobody holds it, so the listener is serving
/// a world that is gone and is replaced rather than attached to.
#[test]
fn a_workspace_deleted_under_its_server_reads_as_unclaimed() {
    let scratch = uze_testkit::temp::scratch("terminal-workspace-orphaned");
    std::fs::create_dir_all(&scratch).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &scratch);

    let holder = ClaimHolder::spawn(&scratch);
    assert!(workspace_is_claimed(), "a live claim is a claim");

    std::fs::remove_dir_all(scratch.join("state")).unwrap();
    assert!(
        !workspace_is_claimed(),
        "a claim on a deleted file holds nothing anybody can reach"
    );

    holder.release();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Set on the process that plays the other server in the claim tests.
const CLAIM_HOLDER: &str = "UZE_TERMINAL_TEST_HOLDS_CLAIM";
/// What that process says once the claim is its.
const CLAIM_HELD: &str = "workspace claim held";
/// Set, to a project root, on the holder that is to be a whole server —
/// endpoint bound and handshakes answered — rather than a claim and
/// nothing else.
const SERVING_HOLDER: &str = "UZE_TERMINAL_TEST_SERVES";

/// The other server in the two claim tests: a process of its own that
/// takes the workspace claim under the `UZE_HOME` it is given, says so,
/// and keeps it for as long as its stdin stays open.
///
/// A claim is made against a *process* — `flock` lives on the open file
/// description, which a `fork` shares with the child until the child's
/// own `exec` closes it. A test that held the claim itself and dropped
/// it could therefore find it still held, for an instant, by a process
/// a sibling test had just forked; a claim this process never took is
/// one nothing it forked can be keeping. Ignored so the suite never
/// runs it on its own — [`ClaimHolder`] runs it, by name, in a process
/// of its own — and guarded by [`CLAIM_HOLDER`] so `--include-ignored`
/// cannot sit it on a terminal's stdin.
#[test]
#[ignore = "the holder side of the workspace-claim tests, run by them in a process of their own"]
fn holds_the_workspace_claim_while_its_stdin_is_open() {
    if std::env::var_os(CLAIM_HOLDER).is_none() {
        return;
    }
    // Asked to be a whole server: `Server::new` takes the claim, the
    // endpoint is bound, and clients are answered — everything a
    // running server of another build is, since what an attach does
    // about one is decided by what it answers.
    let _held = match std::env::var_os(SERVING_HOLDER) {
        Some(root) => {
            let socket = socket_path().expect("the holder's endpoint");
            let (server, _damage) =
                Server::new(seat_at(Path::new(&root)), socket.clone()).expect("the holder serves");
            let server = Arc::new(server);
            let listener = bind_endpoint(&socket).expect("the holder binds its endpoint");
            std::thread::spawn(move || {
                for stream in
                    std::iter::repeat_with(|| super::transport::accept(&listener)).flatten()
                {
                    let server = Arc::clone(&server);
                    std::thread::spawn(move || server.handle_client(stream));
                }
            });
            None
        }
        None => Some(WorkspaceLock::acquire().expect("the holder's claim is granted")),
    };
    println!("{CLAIM_HELD}");
    let mut until_eof = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut until_eof);
}

/// A separate process holding the workspace claim under one `UZE_HOME`
/// — this test binary, re-run on the one ignored test above.
struct ClaimHolder {
    process: std::process::Child,
    /// Kept open until the holder has exited, so its test harness has
    /// somewhere to write its own closing lines.
    _output: std::io::BufReader<std::process::ChildStdout>,
}

impl ClaimHolder {
    /// Returns once the holder says the claim is its.
    fn spawn(uze_home: &Path) -> Self {
        Self::spawn_as(&std::env::current_exe().unwrap(), uze_home)
    }

    /// The same holder, run from `executable` — a copy of this test
    /// binary, for a server of another build.
    fn spawn_as(executable: &Path, uze_home: &Path) -> Self {
        Self::spawn_with(executable, uze_home, None)
    }

    /// A holder that is a whole server: it binds this `UZE_HOME`'s
    /// endpoint over `project` and answers handshakes, which is what an
    /// attach asks of a server before it decides anything about it.
    fn spawn_serving(executable: &Path, uze_home: &Path, project: &Path) -> Self {
        Self::spawn_with(executable, uze_home, Some(project))
    }

    fn spawn_with(executable: &Path, uze_home: &Path, serving: Option<&Path>) -> Self {
        let mut command = std::process::Command::new(executable);
        if let Some(project) = serving {
            command.env(SERVING_HOLDER, project);
        }
        let mut process = command
            .args([
                "--ignored",
                "--exact",
                "--nocapture",
                "runtime::tests::holds_the_workspace_claim_while_its_stdin_is_open",
            ])
            .env("UZE_HOME", uze_home)
            .env(CLAIM_HOLDER, "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("this test binary runs itself");
        let mut output = std::io::BufReader::new(process.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            match std::io::BufRead::read_line(&mut output, &mut line) {
                Ok(0) => panic!("the holder exited without taking the claim"),
                Ok(_) if line.trim_end() == CLAIM_HELD => break,
                Ok(_) => continue,
                Err(error) => panic!("reading the holder: {error}"),
            }
        }
        Self {
            process,
            _output: output,
        }
    }

    fn pid(&self) -> u32 {
        self.process.id()
    }

    /// Lets the holder exit and waits for it: the kernel releases the
    /// claim with the process, so once this returns nothing holds it.
    fn release(mut self) {
        drop(self.process.stdin.take());
        let status = self.process.wait().expect("the holder is waited on");
        assert!(status.success(), "the holder's own run failed: {status}");
    }

    /// Kills the holder and leaves it unreaped — what a client that
    /// started a server and never waited on it is left with.
    // Unix only: A zombie, left unreaped with `waitid`, is a Linux thing.
    #[cfg(target_os = "linux")]
    fn crash(mut self) -> Zombie {
        self.process.kill().expect("the holder is killed");
        let mut exited: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // `WNOWAIT` blocks until the holder has exited and leaves it a
        // zombie rather than reaping it.
        let waited = unsafe {
            libc::waitid(
                libc::P_PID,
                self.pid(),
                &mut exited,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        assert_eq!(waited, 0, "{}", std::io::Error::last_os_error());
        Zombie(self)
    }
}

// Unix only: A zombie, left unreaped with `waitid`, is a Linux thing.
#[cfg(target_os = "linux")]
struct Zombie(ClaimHolder);

// Unix only: A zombie, left unreaped with `waitid`, is a Linux thing.
#[cfg(target_os = "linux")]
impl Zombie {
    fn reap(mut self) {
        let _ = self.0.process.wait();
    }
}

/// This test binary under the name `uze` at another path: to the
/// process table, a `uze` that is not this build.
fn another_build_of_this_binary(scratch: &Path) -> PathBuf {
    let directory = scratch.join("another-build");
    std::fs::create_dir_all(&directory).unwrap();
    let copy = directory.join(uze_platform::executable::file_name("uze"));
    let image = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    uze_platform::executable::install(&copy, &image).unwrap();
    copy
}

/// A shell that has certainly `exec`ed — it said so — and waits to be
/// told to finish.
// A POSIX shell's `-c`, for the Unix tests that need a process that is not
// `uze` and is certainly running.
#[cfg(any(target_os = "linux", target_os = "macos"))]
struct ReadyProcess {
    process: std::process::Child,
    output: std::io::BufReader<std::process::ChildStdout>,
}

// See `ReadyProcess`: a POSIX shell, for the Unix tests.
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl ReadyProcess {
    fn spawn(shell: &Path) -> Self {
        let mut process = std::process::Command::new(shell)
            .args(["-c", "echo ready; read line; echo alive"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the shell runs");
        let mut output = std::io::BufReader::new(process.stdout.take().unwrap());
        let mut line = String::new();
        std::io::BufRead::read_line(&mut output, &mut line).unwrap();
        assert_eq!(line.trim_end(), "ready");
        Self { process, output }
    }

    fn pid(&self) -> u32 {
        self.process.id()
    }

    /// Tells it to finish, and says whether it was still there to.
    fn finish(mut self) -> bool {
        let told = self
            .process
            .stdin
            .take()
            .is_some_and(|mut stdin| std::io::Write::write_all(&mut stdin, b"\n").is_ok());
        let mut line = String::new();
        let answered = told
            && std::io::BufRead::read_line(&mut self.output, &mut line).is_ok()
            && line.trim_end() == "alive";
        let _ = self.process.wait();
        answered
    }
}

/// The incident of 2026-09-19: a release moved the workspace's shape,
/// the spaces were set aside, and the operator started from nothing.
/// The difference was one field — `kind` per space, which moved onto
/// the agent — and every other field mapped one to one.
#[test]
fn a_workspace_that_gave_every_space_a_kind_opens_on_this_build() {
    let scratch = uze_testkit::temp::scratch("terminal-workspace-shape-1");
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("workspace.json");
    std::fs::write(
        &path,
        br#"{"spaces":[
                 {"label":"uze","root":"/tmp/uze","kind":"worktree","tabs":[
                   {"label":"shell","cwd":"/tmp/uze","agent":null,"launch":"Shell"}]},
                 {"label":"home","root":"/tmp/home","kind":"plain","tabs":[]}]}"#,
    )
    .unwrap();

    let (workspace, set_aside) = load_persisted_workspace_at(&path);
    assert!(
        set_aside.is_none(),
        "a shape this build knows is carried across, not set aside"
    );
    let workspace = workspace.expect("the spaces survive the upgrade");
    assert_eq!(workspace.schema_version, WORKSPACE_SCHEMA_VERSION);
    let roots: Vec<_> = workspace
        .spaces
        .iter()
        .map(|space| space.root.display().to_string())
        .collect();
    assert_eq!(roots, ["/tmp/uze", "/tmp/home"], "every space, in order");
    assert_eq!(
        workspace.spaces[0].tabs.len(),
        1,
        "and every tab the space carried"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// Nothing persisted at all is a first run, not a loss.
#[test]
fn a_first_run_reports_nothing() {
    let scratch = uze_testkit::temp::socket_scratch("setaside-first");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).expect("server");
    assert!(
        server
            .set_aside
            .lock()
            .expect("set-aside poisoned")
            .is_none(),
        "there was nothing to lose, so there is nothing to say"
    );
    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The runtime starts before anyone is watching, and it is a different
/// process from the screen. So what it could not carry waits for the
/// first client and is said there — not in a log that is off unless
/// `UZE_LOG` is set, which is where the one that mattered went.
#[test]
fn a_client_is_told_what_the_runtime_could_not_carry() {
    let scratch = uze_testkit::temp::socket_scratch("setaside-told");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&uze_home).unwrap();
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);

    let path = persisted_state_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not a workspace at all").unwrap();

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&project), socket).expect("server");
    let server = std::sync::Arc::new(server);

    // A socket pair stands in for the endpoint: what is being proven
    // is what a client is told once it attaches, not how it got there.
    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    let mut reader = std::io::BufReader::new(driver);
    send_request(
        &mut writer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 80,
            rows: 24,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();

    let mut told = None;
    for _ in 0..8 {
        match read_event(&mut reader) {
            Ok(Some(crate::ClientEvent::WorkspaceSetAside { kept_at, .. })) => {
                told = Some(kept_at);
                break;
            }
            Ok(Some(_)) => {}
            _ => break,
        }
    }
    let kept_at = told.expect("the client is told, rather than a log nobody turned on");
    assert!(kept_at.exists(), "and told where the bytes were kept");
    assert!(
        !path.exists(),
        "the workspace itself is out of the way, under a name nothing reads as one"
    );

    let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
    drop(writer);
    let _ = serving.join();
    server.stop_panes();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Two builds on one machine is the ordinary state of this repository.
/// A workspace a newer build wrote is not this one's to move.
#[test]
fn a_workspace_from_a_newer_build_is_left_exactly_as_it_is() {
    let scratch = uze_testkit::temp::scratch("terminal-workspace-newer");
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("workspace.json");
    let bytes = br#"{"schema_version":99,"spaces":[]}"#;
    std::fs::write(&path, bytes).unwrap();

    let (workspace, set_aside) = load_persisted_workspace_at(&path);
    assert!(
        workspace.is_none(),
        "this build starts from the seat it was given"
    );
    assert!(
        set_aside.is_none(),
        "and takes nothing away from the newer one"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "the bytes stay exactly where the newer build put them"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// Bytes that are not a workspace at all are the one case that still
/// sets aside — and the bytes are kept, never deleted.
#[test]
fn a_workspace_that_cannot_be_read_is_kept_and_reported() {
    let scratch = uze_testkit::temp::scratch("terminal-workspace-garbage");
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("workspace.json");
    std::fs::write(&path, b"not a workspace").unwrap();

    let (workspace, set_aside) = load_persisted_workspace_at(&path);
    assert!(workspace.is_none());
    let set_aside = set_aside.expect("the runtime can say what it could not carry");
    assert!(!path.exists(), "the workspace is out of the way");
    assert!(set_aside.path.exists(), "and its bytes are kept");

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The whole workspace is rewritten on every structural change, and a
/// plain write truncates before it fills. A reader must see the old
/// file or the new one, never half of either.
#[test]
fn the_persisted_workspace_is_replaced_in_one_step() {
    let scratch = uze_testkit::temp::scratch("terminal-atomic-write");
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("workspace.json");
    std::fs::write(&path, b"{\"spaces\":[]}").unwrap();

    write_atomically(&path, b"{\"spaces\":[{}]}").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"spaces\":[{}]}");
    assert!(
        !scratch.join("workspace.json.tmp").exists(),
        "the temporary is renamed over the target, not left beside it"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// [`MAX_FRAME`] bounds a repaint of *one* pane at
/// [`MAX_PANE_DIMENSION`]. A snapshot carrying every pane in a single
/// frame is therefore bounded by nothing a client cannot exceed: three
/// panes at a size `within_pane_bounds` permits — and that a restart
/// restores — made the frame unsendable, and every attached client sat
/// frozen on chrome that still looked live.
#[test]
fn every_pane_reaches_a_client_when_one_frame_could_not_have_carried_them_all() {
    let scratch = uze_testkit::temp::socket_scratch("bigsnap");
    let uze_home = scratch.join("home");
    let runtime_dir = scratch.join("runtime");
    let roots: Vec<PathBuf> = (0..3)
        .map(|index| scratch.join(format!("p{index}")))
        .collect();
    for directory in [&uze_home, &runtime_dir].into_iter().chain(roots.iter()) {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (server, _damage) = Server::new(seat_at(&roots[0]), socket).unwrap();
    let server = Arc::new(server);
    for root in &roots[1..] {
        server
            .ensure_space(&seat_at(root), PLACEHOLDER_PANE_SIZE)
            .expect("a space per root");
    }

    // Filled through the pane's own parser, so what the client is sent
    // is a real grid and not a hand-built one. The character is
    // four bytes of UTF-8 — the widest a cell can carry, and what makes
    // three of these panes exceed one frame rather than merely approach
    // it.
    let widest = '\u{1d54f}';
    let mut painted = Vec::new();
    for row in 0..MAX_PANE_DIMENSION {
        if row > 0 {
            painted.extend_from_slice(b"\r\n");
        }
        for _ in 0..MAX_PANE_DIMENSION {
            let mut encoded = [0u8; 4];
            painted.extend_from_slice(widest.encode_utf8(&mut encoded).as_bytes());
        }
    }
    let pane_ids: Vec<PaneId> = server
        .panes
        .lock()
        .expect("panes poisoned")
        .keys()
        .copied()
        .collect();
    assert_eq!(pane_ids.len(), 3, "one pane per space");
    for pane in &pane_ids {
        server.resize_pane(*pane, MAX_PANE_DIMENSION, MAX_PANE_DIMENSION);
    }
    for runtime in server.panes.lock().expect("panes poisoned").values() {
        let mut parser: Processor = Processor::new();
        parser.advance(
            &mut *runtime.terminal.lock().expect("terminal poisoned"),
            &painted,
        );
    }

    let (client, driver) = super::transport::pair().unwrap();
    let serving = {
        let server = Arc::clone(&server);
        std::thread::spawn(move || server.handle_client(client))
    };
    let mut writer = driver.try_clone().unwrap();
    driver
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    let mut reader = std::io::BufReader::new(driver);
    send_request(
        &mut writer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 0,
            rows: 0,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();

    let mut repainted = std::collections::BTreeSet::new();
    while repainted.len() < pane_ids.len() {
        match read_event(&mut reader)
            .expect("every pane has to reach the client, one frame at a time")
        {
            Some(crate::ClientEvent::Damage(damage)) => {
                assert_eq!(
                    (damage.columns, damage.rows),
                    (MAX_PANE_DIMENSION, MAX_PANE_DIMENSION)
                );
                assert_eq!(
                    damage.changed.len(),
                    usize::from(MAX_PANE_DIMENSION) * usize::from(MAX_PANE_DIMENSION),
                    "a repaint names every cell"
                );
                assert_eq!(
                    damage.changed[0].2.character, widest,
                    "the cells arrive as the pane actually holds them"
                );
                repainted.insert(damage.pane);
            }
            Some(_) => {}
            None => panic!("the server hung up instead of repainting every pane"),
        }
    }
    assert_eq!(
        repainted,
        pane_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    );

    let _ = send_request(&mut writer, &crate::ClientRequest::Detach);
    drop(writer);
    drop(reader);
    let _ = serving.join();
    server.stop_panes();

    let _ = std::fs::remove_dir_all(&scratch);
}

/// A frame that will not go out has to end the connection, not just
/// the thread that tried to write it. Dropping only the writer's dup
/// leaves the peer's read half open: no EOF, no error, and a client
/// sitting on chrome that still looks live while events it will never
/// see pile up behind it.
#[test]
fn a_client_an_event_cannot_reach_is_disconnected_rather_than_frozen() {
    let (peer, socket) = super::transport::pair().unwrap();
    let (outbox, receiver) = super::Outbox::new();
    let events = Arc::new(outbox);
    let writing = {
        let backlog = events.backlog();
        std::thread::spawn(move || super::forward_events(socket, &receiver, &backlog))
    };

    events.reply(crate::ClientEvent::Error {
        message: "x".repeat(MAX_FRAME as usize + 1),
    });

    let mut read = peer;
    read.set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut byte = [0u8; 1];
    assert_eq!(
        std::io::Read::read(&mut read, &mut byte).unwrap(),
        0,
        "the peer must see EOF, which is what runs its disconnected path"
    );
    drop(events);
    writing.join().unwrap();
}

/// `uze workspace stop` is the documented way out of a server that has
/// to go — and, since the workspace lock makes a survivor refuse every
/// replacement, the only one short of a manual `kill`. It has to be
/// heard by a server no client has ever attached to, which is where it
/// was being dropped: `Stop` as a first frame fell through to "not an
/// `Attach`" and the connection was closed without an answer.
// Unix only: Runs `sleep`, a program Windows does not ship.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn stop_is_heard_as_a_first_frame_by_a_server_nobody_attached_to() {
    let scratch = uze_testkit::temp::socket_scratch("stopfirst");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    let runtime_dir = scratch.join("runtime");
    for directory in [&uze_home, &project, &runtime_dir] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home)
        .set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket = socket_path().unwrap();
    let (served, serving) = std::sync::mpsc::channel();
    let serve_root = project.clone();
    std::thread::spawn(move || {
        let _ = served.send(super::serve(seat_at(&serve_root)));
    });

    let mut ready = false;
    for _ in 0..200 {
        if super::transport::connect(&socket).is_ok() {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(ready, "the server must be listening before it is stopped");

    super::stop().expect("a running server must acknowledge stop");
    serving
        .recv_timeout(Duration::from_secs(30))
        .expect("the stopped server must leave its accept loop")
        .expect("and leave it cleanly");
    assert!(
        !socket.exists(),
        "a stopped server clears the endpoint it was reached at"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// `flock` says no for reasons that are not contention, and reading
/// them all as contention told the person to go and stop a server that
/// does not exist — permanently, on an `$UZE_HOME` that happens to sit
/// on NFS, FUSE or a 9p mount, with no command that could clear it.
#[test]
fn only_a_held_lock_reads_as_another_server() {
    let refusal = |kind| super::classify_lock_refusal(std::io::Error::from(kind));
    assert!(matches!(
        refusal(std::io::ErrorKind::WouldBlock),
        super::LockRefusal::Contended
    ));
    assert!(matches!(
        refusal(std::io::ErrorKind::Interrupted),
        super::LockRefusal::Interrupted
    ));
    for unsupported in [
        std::io::ErrorKind::Unsupported,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::InvalidInput,
        std::io::ErrorKind::Other,
    ] {
        assert!(
            matches!(refusal(unsupported), super::LockRefusal::Unsupported(_)),
            "{unsupported:?} is a filesystem that cannot lock, not a server that holds one"
        );
    }
}

/// Only two facts decide what an attach does to the endpoint: who holds
/// the workspace claim, and who the kernel and the process table say is
/// listening. A claimed workspace is never taken from a listener
/// nobody can vouch for — without a readable process table that is
/// every listener, and the one serving is alive.
#[test]
fn an_attach_replaces_only_a_server_it_can_name() {
    let pid = 4242;
    for (claimed, listener, expected) in [
        (true, Listener::ThisBuild(pid), Arrival::Connect),
        (true, Listener::Unrecognized, Arrival::Connect),
        (true, Listener::Nobody, Arrival::Connect),
        // Alive and serving this workspace: asked, never ended on the
        // strength of the image it was started from.
        (true, Listener::AnotherBuild(pid), Arrival::Ask(pid)),
        (false, Listener::ThisBuild(pid), Arrival::Replace(pid)),
        (false, Listener::AnotherBuild(pid), Arrival::Replace(pid)),
        (false, Listener::Unrecognized, Arrival::Start),
        (false, Listener::Nobody, Arrival::Start),
    ] {
        assert_eq!(
            arrival(claimed, listener),
            expected,
            "claimed: {claimed}, listener: {listener:?}"
        );
    }
}

/// The writer thread serving a client must end when the client does.
///
/// It once could not: it was handed the client's whole `Outbox`, so it
/// held a sender to the channel it was blocked on and `recv` never
/// reported the hang-up. Every connection the endpoint ever accepted —
/// each `uze` attaching, and each probe asking who listens here — then
/// cost one thread and one descriptor for the life of the server, which
/// is how two servers reached fourteen thousand threads apiece and left
/// a machine unable to `fork`. The socket is deliberately still open on
/// the peer's side, so what ends the thread can only be the channel.
#[test]
fn a_clients_writer_thread_ends_with_the_client() {
    let (_peer, socket) = super::transport::pair().unwrap();
    let (outbox, receiver) = Outbox::new();
    let outbox = Arc::new(outbox);
    let backlog = outbox.backlog();
    let writer = thread::spawn(move || forward_events(socket, &receiver, &backlog));

    drop(outbox);

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !writer.is_finished() && std::time::Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        writer.is_finished(),
        "the last outbox is gone, so nothing can reach this client again"
    );
    writer.join().unwrap();
}

/// And the same seen from the wire: a peer the server turns away is
/// hung up on, not held. Reading to end-of-file is the proof that no
/// thread inside the server still owns a copy of this connection.
#[test]
fn a_peer_the_server_refuses_is_hung_up_on() {
    let scratch = uze_testkit::temp::socket_scratch("refused-peer-hangs-up");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    for directory in [&uze_home, &project] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);
    let socket = super::transport::scratch_endpoint(&scratch, "test.sock");
    let (server, _damage) = Server::new(seat_at(&project), socket.clone()).unwrap();
    let server = Arc::new(server);
    let listener = super::transport::bind(&socket).unwrap();
    let serving = thread::spawn(move || {
        let stream = super::transport::accept(&listener).unwrap();
        server.handle_client(stream);
    });

    let mut peer = super::transport::connect(&socket).unwrap();
    peer.set_read_timeout(Some(ANSWERS_WITHIN)).unwrap();
    send_request(
        &mut peer,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION + 1,
            columns: 80,
            rows: 24,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();
    assert!(
        matches!(
            read_event(&mut peer.try_clone().unwrap()),
            Ok(Some(crate::ClientEvent::Error { .. }))
        ),
        "a peer speaking another protocol is told so"
    );

    let mut rest = Vec::new();
    std::io::Read::read_to_end(&mut peer, &mut rest).expect("the server hangs up");
    assert!(rest.is_empty(), "nothing follows the refusal");

    let _ = serving.join();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A `make install` over a running server leaves every later `uze`
/// looking at a server built from another image — and ending one costs
/// every agent it runs its process, mid-conversation, whether or not
/// the two builds could have talked. They usually could: the image
/// says which binary a server came from, and `PROTOCOL_VERSION` says
/// what it speaks. A server that answers this build's handshake is
/// attached to, and the panes it is running go on running.
#[test]
fn a_server_that_answers_this_builds_handshake_serves_it() {
    let scratch = uze_testkit::temp::socket_scratch("serves-answered");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    for directory in [&uze_home, &project] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);
    let socket = super::transport::scratch_endpoint(&scratch, "test.sock");
    let (server, _damage) = Server::new(seat_at(&project), socket.clone()).unwrap();
    let server = Arc::new(server);
    let listener = super::transport::bind(&socket).unwrap();
    let serving = std::thread::spawn(move || {
        let stream = super::transport::accept(&listener).unwrap();
        server.handle_client(stream);
    });

    assert!(
        serves_this_build(&socket),
        "a server that reads this build's attach and describes the workspace can serve it"
    );

    let _ = serving.join();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The state the question exists to find: something is listening at the
/// endpoint of a claimed workspace and cannot be talked to — a server
/// built to another framing, one that refuses this build's version, one
/// that hung up, one that has stopped answering at all. Each is a "no",
/// and the silent one is a "no" within a bound rather than an attach
/// that waits on it forever.
#[test]
fn a_server_that_cannot_answer_is_never_taken_for_one_that_can() {
    let scratch = uze_testkit::temp::socket_scratch("serves-refused");
    std::fs::create_dir_all(&scratch).unwrap();

    let refusing = super::transport::scratch_endpoint(&scratch, "refusing.sock");
    let listener = super::transport::bind(&refusing).unwrap();
    let answering = std::thread::spawn(move || {
        let mut stream = super::transport::accept(&listener).unwrap();
        let _ = write_message(
            &mut stream,
            &crate::ClientEvent::Error {
                message: "incompatible terminal runtime protocol".into(),
            },
        );
    });
    assert!(
        !serves_this_build(&refusing),
        "a server that refuses this build's version cannot serve it"
    );
    let _ = answering.join();

    let hanging_up = super::transport::scratch_endpoint(&scratch, "hanging-up.sock");
    let listener = super::transport::bind(&hanging_up).unwrap();
    let dropping = std::thread::spawn(move || drop(super::transport::accept(&listener).unwrap()));
    assert!(
        !serves_this_build(&hanging_up),
        "and neither can one that hangs up on the handshake"
    );
    let _ = dropping.join();

    let silent = super::transport::scratch_endpoint(&scratch, "silent.sock");
    let listener = super::transport::bind(&silent).unwrap();
    let (answered, asked) = std::sync::mpsc::channel::<()>();
    let holding = std::thread::spawn(move || {
        let held = super::transport::accept(&listener).unwrap();
        let _ = asked.recv();
        drop(held);
    });
    let began = std::time::Instant::now();
    assert!(!serves_this_build(&silent), "nor one that says nothing");
    assert!(
        began.elapsed() < ANSWERS_WITHIN * 2,
        "and the silence is bounded: an attach cannot wait on it"
    );
    drop(answered);
    let _ = holding.join();

    assert!(
        !serves_this_build(&super::transport::scratch_endpoint(&scratch, "nobody.sock")),
        "an endpoint nothing is behind answers nothing either"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// What all of this is for, end to end: a second terminal opened while
/// agents are running in the first attaches to the server already
/// serving them, even though a `make install` has made that server
/// "another build" in the meantime. It used to be retired on sight —
/// every pane it held killed with it, mid-conversation, because a
/// binary had been replaced on disk.
#[test]
fn a_second_client_attaches_to_a_live_server_of_another_build() {
    let scratch = uze_testkit::temp::socket_scratch("attach-another-build");
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    for directory in [&uze_home, &project] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home);
    let serving =
        ClaimHolder::spawn_serving(&another_build_of_this_binary(&scratch), &uze_home, &project);
    let socket = socket_path().unwrap();
    assert_eq!(
        listener_at(&socket),
        Listener::AnotherBuild(serving.pid()),
        "the server at the endpoint was started from another image"
    );

    let mut stream = super::attach(&seat_at(&project)).expect("the client attaches");

    assert_eq!(
        super::claim_holder(),
        Some(serving.pid()),
        "to the server that was already there, which still holds the workspace"
    );
    send_request(
        &mut stream,
        &crate::ClientRequest::Attach {
            version: crate::PROTOCOL_VERSION,
            columns: 80,
            rows: 24,
            seating: crate::Seating::WhereItLeftOff,
        },
    )
    .unwrap();
    let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
    let described = std::iter::from_fn(|| read_event(&mut reader).unwrap())
        .find_map(|event| match event {
            crate::ClientEvent::Snapshot { session } => Some(session),
            _ => None,
        })
        .expect("and it serves this client");
    assert!(!described.workspace.spaces.is_empty());

    let _ = send_request(&mut stream, &crate::ClientRequest::Detach);
    serving.release();
    let _ = std::fs::remove_dir_all(&scratch);
}

/// `SO_RCVTIMEO` restarts on every successful read, so a deadline
/// spelled with it alone is no deadline at all: a peer dribbling a byte
/// just inside it holds a reader thread, a writer thread and whatever
/// it has allocated for as long as it likes — the very thing
/// [`HANDSHAKE_DEADLINE`] says it prevents.
#[test]
fn a_dribbling_peer_runs_out_of_handshake_rather_than_restarting_it() {
    let (peer, socket) = super::transport::pair().unwrap();
    let dribbling = std::thread::spawn(move || {
        let mut peer = peer;
        for _ in 0..40 {
            if std::io::Write::write_all(&mut peer, &[0u8]).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(60));
        }
    });

    let began = std::time::Instant::now();
    let mut reader =
        std::io::BufReader::new(super::Handshake::new(socket, Duration::from_millis(150)));
    let refused = super::read_message_within::<_, crate::ClientRequest>(
        &mut reader,
        super::MAX_HANDSHAKE_FRAME,
    );
    let waited = began.elapsed();

    assert!(
        refused.is_err(),
        "a peer that never finishes saying who it is has to be let go"
    );
    assert!(
        waited < Duration::from_secs(2),
        "the deadline bounds the whole handshake, not each read of it (waited {waited:?})"
    );
    drop(reader);
    let _ = dribbling.join();
}

/// The first frame is the one nothing has vouched for, and the only
/// two things it may say are hundreds of bytes. Sizing it by the
/// largest repaint this wire ever carries let a stranger reserve
/// 64 MiB by writing four bytes.
#[test]
fn a_first_frame_is_bounded_by_what_a_handshake_says_not_by_a_repaint() {
    const { assert!(super::MAX_HANDSHAKE_FRAME < MAX_FRAME) };
    let mut wire: &[u8] = &(super::MAX_HANDSHAKE_FRAME + 1).to_le_bytes();
    assert!(
        matches!(
            super::read_message_within::<_, crate::ClientRequest>(
                &mut wire,
                super::MAX_HANDSHAKE_FRAME
            ),
            Err(RuntimeError::Protocol(_))
        ),
        "a handshake frame past the handshake's own bound is refused"
    );

    let attach = bincode::serialize(&crate::ClientRequest::Attach {
        version: crate::PROTOCOL_VERSION,
        columns: 200,
        rows: 50,
        seating: crate::Seating::Open(seat_at(Path::new("/some/ordinary/project/path"))),
    })
    .unwrap();
    assert!(
        attach.len() < super::MAX_HANDSHAKE_FRAME as usize,
        "and the bound still has to fit what a handshake actually says"
    );
}

/// The endpoint as a file in a directory: what a Unix-domain socket is and a
/// named pipe is not.
// Unix only: Unix socket files and their directory; the named pipe has neither.
#[cfg(unix)]
mod socket_files;

/// A Windows pane, driven as a person at the keyboard drives it: its
/// text as one string, and a wait for some to appear.
// Windows only: these drive Windows PowerShell and ConPTY, the console a
// Windows pane is; the Unix panes are driven by the tests above.
#[cfg(windows)]
mod windows_panes {
    use super::*;

    fn shell_pane(columns: u16, rows: u16) -> (PaneRuntime, std::sync::mpsc::Receiver<PaneId>) {
        // What `terminal host-pane` does for a pane in production: however
        // this test binary was started, its programs can be interrupted.
        uze_platform::interrupt::restore_default();
        let (damage, damage_events) = std::sync::mpsc::channel();
        let pane = PaneRuntime::spawn(
            PaneId(91),
            std::env::temp_dir(),
            columns,
            rows,
            damage,
            Launch::Shell,
            Arc::new(Mutex::new(Palette::default())),
        )
        .unwrap();
        (pane, damage_events)
    }

    fn text(pane: &PaneRuntime) -> String {
        pane.snapshot()
            .cells
            .into_iter()
            .map(|cell| cell.character)
            .collect()
    }

    fn shows(
        pane: &PaneRuntime,
        damage: &std::sync::mpsc::Receiver<PaneId>,
        wanted: impl Fn(&str) -> bool,
    ) -> bool {
        wanted(&text(pane))
            || std::iter::from_fn(|| damage.recv_timeout(Duration::from_secs(20)).ok())
                .any(|_| wanted(&text(pane)))
    }

    /// The console's startup question (the cursor position, `ESC[6n`) is
    /// answered, or PowerShell would sit waiting for it and never prompt.
    #[test]
    fn the_shell_prompts_because_its_startup_question_is_answered() {
        let (pane, damage) = shell_pane(80, 24);
        let prompted = shows(&pane, &damage, |screen| screen.contains("PS "));
        pane.stop();
        assert!(prompted, "the prompt never came: {}", text(&pane).trim());
    }

    /// Ctrl+C reaches a program a pane runs, as at a console of one's own:
    /// what is typed after it runs.
    #[test]
    fn ctrl_c_stops_a_program_running_in_a_pane() {
        let (pane, damage) = shell_pane(100, 30);
        assert!(shows(&pane, &damage, |screen| screen.contains("PS ")));
        pane.write(b"ping -t 127.0.0.1\r");
        assert!(
            shows(&pane, &damage, |screen| screen.matches("127.0.0.1").count()
                >= 4),
            "ping answered"
        );
        pane.write(b"\x03");
        // The interrupt also empties the console's input, so what comes
        // next is typed once the prompt is back, as a person would.
        assert!(
            shows(&pane, &damage, |screen| screen.matches("PS ").count() >= 2),
            "the prompt came back after Ctrl+C"
        );
        pane.write(b"Write-Output ('after' + '-interrupt')\r");
        let resumed = shows(&pane, &damage, |screen| screen.contains("after-interrupt"));
        let screen = text(&pane);
        pane.stop();
        assert!(
            resumed,
            "the shell never ran what came after Ctrl+C:\n{}",
            screen
                .as_bytes()
                .chunks(100)
                .map(String::from_utf8_lossy)
                .map(|line| line.trim_end().to_owned())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// A resized pane is the new size, and what it held is still on it.
    #[test]
    fn a_resized_pane_takes_the_new_size_and_keeps_its_text() {
        let (pane, damage) = shell_pane(100, 30);
        assert!(shows(&pane, &damage, |screen| screen.contains("PS ")));
        pane.write(b"Write-Output ('before' + '-resize')\r");
        assert!(shows(&pane, &damage, |screen| screen.contains("before-resize")));
        pane.resize(60, 20);
        let resized = shows(&pane, &damage, |_| {
            let snapshot = pane.snapshot();
            snapshot.columns == 60 && snapshot.rows == 20
        });
        let kept = text(&pane).contains("before-resize");
        pane.stop();
        assert!(resized, "the pane never took its new size");
        assert!(kept, "the resize lost what the pane held");
    }

    /// Once a pane is closed its reader ends: nothing is left waiting on a
    /// console nobody holds.
    #[test]
    fn a_closed_pane_s_reader_ends() {
        let (pane, damage) = shell_pane(80, 24);
        assert!(shows(&pane, &damage, |screen| screen.contains("PS ")));
        pane.stop().join().unwrap();
        drop(pane);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let ended = loop {
            match damage.recv_timeout(Duration::from_millis(250)) {
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break true,
                _ if std::time::Instant::now() > deadline => break false,
                _ => {}
            }
        };
        assert!(ended, "the reader outlived its pane");
    }
}
