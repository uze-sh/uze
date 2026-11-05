//! Stopping a workspace through the real binary: once the server has ended,
//! nothing that ignored its hangup is left of its panes. A unit test cannot
//! say this, since what it is about is the process exiting.

use std::{
    ffi::CString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use uze_testkit::{process::install_executable, temp::TestEnvironment};

/// Far past any scheduler: each wait below ends on an event, and this only
/// turns one that never comes into a failure instead of a hung suite.
const WITHIN: Duration = Duration::from_secs(60);

/// A server whose one pane left a worker behind that ignores its hangup.
///
/// The worker can end in only two ways, and both close `lifeline`: killed,
/// or told that `release` closed, when it writes `survived` first. So reading
/// `lifeline` to its end settles which it was, with no time to guess at, and
/// a test that dies leaves nothing running.
struct Served {
    env: TestEnvironment,
    server: Child,
    lifeline: File,
    release: Option<File>,
    survived: PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        // `Child` will not signal what it has already reaped.
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

fn fifo(path: &Path) {
    let spelled = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: `spelled` is a NUL-terminated string that outlives the call.
    assert_eq!(unsafe { libc::mkfifo(spelled.as_ptr(), 0o600) }, 0);
}

fn within<T: Send + 'static>(what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(work());
    });
    finished
        .recv_timeout(WITHIN)
        .unwrap_or_else(|_| panic!("{what} did not happen within {WITHIN:?}"))
}

fn serve_a_pane_that_ignores_its_hangup() -> Served {
    let env = TestEnvironment::isolated();
    let lifeline = env.root().join("lifeline");
    let release = env.root().join("release");
    let survived = env.root().join("survived");
    fifo(&lifeline);
    fifo(&release);
    let shell = env.fake_bin.join("deaf-shell");
    install_executable(
        &shell,
        format!(
            "#!/bin/sh\n\
             trap '' HUP\n\
             (\n\
             exec 3> '{lifeline}' 4< '{release}'\n\
             read -r line <&4\n\
             : > '{survived}'\n\
             ) &\n\
             wait\n",
            lifeline = lifeline.display(),
            release = release.display(),
            survived = survived.display(),
        )
        .as_bytes(),
    );

    let server = env
        .command(env!("CARGO_BIN_EXE_uze"))
        .args(["terminal", "serve", "--project"])
        .arg(&env.project)
        .env("UZE_SHELL", &shell)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the real uze binary serves a terminal");
    let (lifeline, release) = within("the worker opening its two ends", move || {
        (
            File::open(lifeline).unwrap(),
            File::create(release).unwrap(),
        )
    });
    let served = Served {
        env,
        server,
        lifeline,
        release: Some(release),
        survived,
    };
    served.answers();
    served
}

impl Served {
    /// The server answers only once it listens for a stop too: earlier, a
    /// `SIGTERM` would end it the default way, which is not what is tested.
    fn answers(&self) {
        let deadline = Instant::now() + WITHIN;
        loop {
            let mut scope = uze_testkit::env::scope();
            scope.set("UZE_HOME", &self.env.uze_home);
            if matches!(uze_terminal::usage(), Ok(uze_terminal::Usage::Running(_))) {
                return;
            }
            assert!(Instant::now() < deadline, "the server never answered");
        }
    }

    fn listening(&self) -> bool {
        let mut scope = uze_testkit::env::scope();
        scope.set("UZE_HOME", &self.env.uze_home);
        uze_terminal::socket_path().unwrap().exists()
    }

    fn has_ended(&mut self) {
        let deadline = Instant::now() + WITHIN;
        while self.server.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the server never ended");
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Whether the worker was still alive to hear `release` close. Called
    /// once the server has ended: whatever signal it sent has landed by
    /// then, and a killed process runs nothing more.
    fn worker_survived(&mut self) -> bool {
        drop(self.release.take());
        let mut lifeline = self.lifeline.try_clone().unwrap();
        within("the worker ending", move || {
            let _ = lifeline.read_to_end(&mut Vec::new());
        });
        self.survived.exists()
    }
}

#[test]
fn a_stopped_workspace_leaves_nothing_that_ignored_its_hangup() {
    let mut served = serve_a_pane_that_ignores_its_hangup();

    let stop = served
        .env
        .command(env!("CARGO_BIN_EXE_uze"))
        .args(["workspace", "stop"])
        .output()
        .unwrap();
    assert!(stop.status.success(), "{stop:?}");
    assert!(
        !served.listening(),
        "whoever opens the workspace next meets a server on its way out"
    );
    served.has_ended();

    assert!(
        !served.worker_survived(),
        "a process that ignored its hangup outlived `uze workspace stop`"
    );
}

/// What `retire` asks a server it is replacing with.
#[test]
fn a_server_told_to_stop_by_signal_leaves_nothing_that_ignored_its_hangup() {
    let mut served = serve_a_pane_that_ignores_its_hangup();

    let server = libc::pid_t::try_from(served.server.id()).unwrap();
    // SAFETY: a positive pid: this test's own child, not yet reaped.
    assert_eq!(unsafe { libc::kill(server, libc::SIGTERM) }, 0);
    served.has_ended();

    assert!(
        !served.worker_survived(),
        "a process that ignored its hangup outlived the server's SIGTERM"
    );
}
