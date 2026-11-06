//! What a stopping server does with its panes, watched from the pane's
//! child: which signals reach it, and what has happened by the time `serve`
//! is released. `tests/cli/workspace_stop.rs` proves the outcome through the
//! real binary.

use super::*;
use portable_pty::{Child, ChildKiller, ExitStatus};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// A pane's child, counting every signal it is asked to send, and whose
/// `wait` can be held and reports when it has been reached.
#[derive(Debug)]
struct WatchedChild {
    real: Box<dyn Child + Send + Sync>,
    signals: Arc<AtomicUsize>,
    wait_reached: Option<mpsc::Sender<&'static str>>,
    wait_held: Option<Mutex<mpsc::Receiver<()>>>,
}

#[derive(Debug)]
struct WatchedKiller {
    real: Box<dyn ChildKiller + Send + Sync>,
    signals: Arc<AtomicUsize>,
}

impl ChildKiller for WatchedKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        self.signals.fetch_add(1, Ordering::SeqCst);
        self.real.kill()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self {
            real: self.real.clone_killer(),
            signals: Arc::clone(&self.signals),
        })
    }
}

impl ChildKiller for WatchedChild {
    fn kill(&mut self) -> std::io::Result<()> {
        self.signals.fetch_add(1, Ordering::SeqCst);
        self.real.kill()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(WatchedKiller {
            real: self.real.clone_killer(),
            signals: Arc::clone(&self.signals),
        })
    }
}

impl Child for WatchedChild {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.real.try_wait()
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        if let Some(reached) = &self.wait_reached {
            let _ = reached.send("the pane's child reached");
        }
        if let Some(held) = &self.wait_held {
            let _ = held.lock().unwrap().recv();
        }
        self.real.wait()
    }

    fn process_id(&self) -> Option<u32> {
        self.real.process_id()
    }
}

/// A child with nothing behind it, in `pane`'s place for the instant its real
/// one is moved into a [`WatchedChild`].
#[derive(Debug)]
struct NoChild;

impl ChildKiller for NoChild {
    fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self)
    }
}

impl Child for NoChild {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        Ok(None)
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        Ok(ExitStatus::with_exit_code(0))
    }

    fn process_id(&self) -> Option<u32> {
        None
    }
}

fn watch(
    pane: &PaneRuntime,
    wait_reached: Option<mpsc::Sender<&'static str>>,
    wait_held: Option<mpsc::Receiver<()>>,
) -> Arc<AtomicUsize> {
    let signals = Arc::new(AtomicUsize::new(0));
    let mut child = pane.child.lock().unwrap();
    let real = std::mem::replace(&mut *child, Box::new(NoChild));
    *child = Box::new(WatchedChild {
        real,
        signals: Arc::clone(&signals),
        wait_reached,
        wait_held: wait_held.map(Mutex::new),
    });
    signals
}

struct OneShell {
    server: Arc<Server>,
    scratch: PathBuf,
    _env: uze_testkit::env::ProcessEnvGuard<'static>,
}

impl OneShell {
    fn pane(&self) -> Arc<PaneRuntime> {
        let panes = self.server.panes.lock().unwrap();
        Arc::clone(panes.values().next().expect("a server opens one shell"))
    }
}

impl Drop for OneShell {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

fn a_server_with_one_shell(label: &str) -> OneShell {
    let scratch = uze_testkit::temp::socket_scratch(label);
    let uze_home = scratch.join("home");
    let project = scratch.join("project");
    for directory in [&uze_home, &project] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut env = uze_testkit::env::scope();
    env.set("UZE_HOME", &uze_home).set("UZE_SHELL", "/bin/sh");
    let (server, _damage) = Server::new(
        seat_at(&project),
        Palette::default(),
        socket_path().unwrap(),
    )
    .unwrap();
    OneShell {
        server: Arc::new(server),
        scratch,
        _env: env,
    }
}

/// The reason `serve` must wait: what a reaper has yet to do when the
/// process exits is never done. What it waits for is held here at the last
/// thing a reaper does, so whichever of the two reports first is the order
/// they really ran in.
#[test]
fn a_shut_down_releases_serve_only_once_every_pane_is_reaped() {
    let shell = a_server_with_one_shell("stop-order");
    let (said, heard) = mpsc::channel();
    let (hold, held) = mpsc::channel();
    watch(&shell.pane(), Some(said.clone()), Some(held));

    let releasing = Arc::clone(&shell.server);
    let released = thread::spawn(move || {
        drop(releasing.await_stop());
        let _ = said.send("serve released");
    });
    let stopping = Arc::clone(&shell.server);
    let stopped = thread::spawn(move || stopping.shut_down());

    let first = heard.recv_timeout(Duration::from_secs(30));
    hold.send(()).unwrap();
    stopped.join().unwrap();
    released.join().unwrap();
    assert_eq!(
        first,
        Ok("the pane's child reached"),
        "`serve` was released before the pane it was stopping was reaped"
    );
    assert!(
        *shell.server.stopped.lock().unwrap(),
        "and the flag that turns new work away is up"
    );
}

/// Two `Stop` requests, or a `Stop` and a retiring `SIGTERM`, are two calls.
#[test]
fn a_second_shut_down_sends_a_stopped_pane_no_signal() {
    let shell = a_server_with_one_shell("stop-twice");
    let signals = watch(&shell.pane(), None, None);

    shell.server.shut_down();
    let after_the_first = signals.load(Ordering::SeqCst);
    assert!(after_the_first > 0, "the first stop signals the pane");

    shell.server.shut_down();
    assert_eq!(signals.load(Ordering::SeqCst), after_the_first);
}

/// A reaped pid may be anybody's by now, whatever path stops the pane again.
#[test]
fn a_reaped_pane_is_never_signalled_again() {
    let shell = a_server_with_one_shell("stop-reaped");
    let pane = shell.pane();
    let signals = watch(&pane, None, None);

    pane.stop().join().unwrap();
    let after_the_first = signals.load(Ordering::SeqCst);
    assert!(after_the_first > 0, "the first stop signals the pane");

    pane.stop().join().unwrap();
    assert_eq!(signals.load(Ordering::SeqCst), after_the_first);
}

/// A pane started after the stop took its list of panes is one nothing would
/// ever end.
#[test]
fn a_stopping_server_starts_no_pane() {
    let shell = a_server_with_one_shell("stop-spawn");
    let pane = shell.pane().id;

    shell.server.shut_down();

    let respawned = shell.server.spawn_pane(pane, Launch::Shell);
    assert!(respawned.is_err(), "{respawned:?}");
    let leader = shell.pane().leader.expect("the shell has a pid");
    assert_eq!(
        uze_platform::process::alive(leader),
        Some(false),
        "and the pane it stopped is the one still on record, gone"
    );
}
