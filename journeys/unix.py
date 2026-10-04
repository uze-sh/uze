"""The journey runner's machine and screen on Linux and macOS.

`journey.py` selects this module or `windows.py` once, at import, and asks
every platform question through it: what a process inherited and where it
stands, how a command gets a terminal nobody answers, and how a screen is
held and read. Nothing outside the two modules branches on the platform.

The screen is a tmux session holding the app's pty; the process table is
`/proc` on Linux and `ps`/`lsof` on macOS.
"""

from __future__ import annotations

import fcntl
import os
import shlex
import signal
import subprocess
import sys
import termios
from dataclasses import dataclass
from pathlib import Path

PLATFORM = "macos" if sys.platform == "darwin" else "linux"
# What the shell a pane opens is called in the process table.
SHELL = "bash"
EXECUTABLE_SUFFIX = ""
REQUIRED_TOOLS = ("tmux", "git")
DEFAULT_WORLDS = "/tmp/uze-journeys"


def prepare_interpreter() -> None:
    """Nothing to prepare: this interpreter already reads and writes UTF-8."""


def missing_tools() -> list[str]:
    return []


def spell(path: Path | str) -> str:
    """A path as a journey's placeholders spell it."""
    return str(path)


def host_path(path: str) -> str:
    """`path` as this filesystem holds it: as written, here."""
    return path


def shell_rc_name() -> str:
    """The startup file the world's shell actually reads on this platform.

    The world runs bash, and bash reads `.bashrc` for an interactive
    non-login shell and `.bash_profile` for a login one. On Linux a terminal
    opens the former; on macOS every terminal window is a login shell, so
    that is the file UZE writes its `PATH` line into there — and a journey
    asserting `.bashrc` on a Mac would be asserting the wrong file, not
    finding a bug.
    """
    return ".bash_profile" if sys.platform == "darwin" else ".bashrc"


def world_environment(root: Path, binary: Path) -> dict:
    """What a world's environment carries on this platform, beside the
    identity and homes every platform's world shares."""
    return {
        "XDG_RUNTIME_DIR": str(root / "run"),
        "PATH": f"{root / 'bin'}:{binary.parent}:/usr/local/bin:/usr/bin:/bin",
        "TERM": "xterm-256color",
        "SHELL": "/bin/bash",
        "LANG": "C.UTF-8",
        "PS1": "journey $ ",
    }


# ── the process table ────────────────────────────────────────────────────
#
# A `then` check may ask whether something is still running, and scope the
# question to this world — "is an agent still standing in that checkout".
# Answering it means reading two facts about a process this script did not
# start: what it inherited, and where it is standing. Linux keeps both in
# `/proc`; macOS has neither and answers through `ps -E` and `lsof`.
#
# The rule these functions exist to enforce: **`None` is not "no"**. Reading
# `/proc` on a machine that has none used to raise `OSError`, get caught, and
# `continue` — so every process was skipped, nothing was ever found, and a
# check asserting `alive: false` passed while observing exactly nothing. That
# is the one failure this tier is built to prevent, reproduced by the runner
# itself. A platform that cannot answer now says so and the run stops.


def process_environ(pid: int | str) -> bytes | None:
    """The environment `pid` was started with. `None` means *this platform
    could not say* — never that the variable is absent."""
    if sys.platform == "linux":
        try:
            return Path(f"/proc/{pid}/environ").read_bytes()
        except OSError:
            return b""
    if sys.platform == "darwin":
        # `ps -E` appends the environment to the command line. It answers for
        # processes this user owns, which is every process a journey starts.
        result = subprocess.run(
            ["ps", "-Ewwo", "command=", "-p", str(pid)],
            capture_output=True,
        )
        return result.stdout if result.returncode == 0 else b""
    return None


def process_cwd(pid: int | str) -> str | None:
    """The directory `pid` is standing in, or `None` when unobservable."""
    if sys.platform == "linux":
        try:
            return str(Path(f"/proc/{pid}/cwd").resolve())
        except OSError:
            return None
    if sys.platform == "darwin":
        result = subprocess.run(
            ["lsof", "-a", "-d", "cwd", "-Fn", "-p", str(pid)],
            capture_output=True,
            text=True,
        )
        for line in result.stdout.splitlines():
            if line.startswith("n"):
                return line[1:]
        return None
    return None


def process_alive(pid: int) -> bool:
    """Signal 0: the portable "does this pid exist" — `/proc/<pid>` is not."""
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        # Alive, and owned by somebody else.
        return True
    return True


def process_table_problem() -> str | None:
    """Why a `process:` check could only ever answer "no" here, if it could."""
    if sys.platform not in ("linux", "darwin"):
        return (
            f"the process table cannot be read on {sys.platform}: a `process:` "
            "check here would observe nothing and report 'not running'. Teach "
            "`process_environ`/`process_cwd` this platform before running "
            "journeys on it."
        )
    return None


def pids_matching(pattern: str) -> list[str]:
    """Every process whose full command line matches `pattern`."""
    return subprocess.run(
        ["pgrep", "-f", pattern], capture_output=True, text=True
    ).stdout.split()


def process_listing() -> list[str]:
    """Every process, as `pid command line`."""
    return subprocess.run(
        ["pgrep", "-a", "."], capture_output=True, text=True
    ).stdout.splitlines()


def terminate(pid: int) -> bool:
    try:
        os.kill(pid, signal.SIGTERM)
    except OSError:
        return False
    return True


def kill(pid: int) -> None:
    try:
        os.kill(pid, signal.SIGKILL)
    except OSError:
        pass


def kill_session(session: int) -> None:
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
        except (OSError, IndexError):
            continue
        if int(fields[3]) == session:
            try:
                os.kill(int(entry.name), signal.SIGKILL)
            except ProcessLookupError:
                pass


def list_tree(roots: list[Path], depth: int) -> str:
    """Every path under `roots`, `depth` levels down, one per line."""
    return subprocess.run(
        ["find", *map(str, roots), "-maxdepth", str(depth)],
        capture_output=True,
        text=True,
    ).stdout


# ── commands ─────────────────────────────────────────────────────────────


def shell_argv(command: str) -> list[str]:
    """The world's shell running one line."""
    return ["/bin/sh", "-c", command]


def run_with_a_terminal(
    command, timeout: float = 120, **options
) -> subprocess.CompletedProcess:
    """`subprocess.run`, with a controlling terminal nobody answers.

    stdin and the captured streams stay off the terminal, so UZE behaves as
    it does under any script. What changes is `/dev/tty`: a person's machine
    has one, and a vendor installer that asks on it hangs a provisioning step
    that let it through — which no world without one could ever show. The
    deadline turns that hang into a failure that says so.

    A string runs under `sh -m`, as an interactive shell would: a job sent to
    the background gets a process group of its own, so the hangup the
    terminal's session sends when the shell exits does not take it along.
    """
    if isinstance(command, str):
        command = ["/bin/sh", "-m", "-c", command]
    controller, terminal = os.openpty()
    path = os.ttyname(terminal)
    os.close(terminal)

    def take_the_terminal() -> None:
        descriptor = os.open(path, os.O_RDWR)
        fcntl.ioctl(descriptor, termios.TIOCSCTTY, 0)
        os.close(descriptor)

    try:
        process = subprocess.Popen(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
            preexec_fn=take_the_terminal,
            **options,
        )
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            kill_session(process.pid)
            stdout, stderr = process.communicate()
            stderr += f"\n[journey] still running after {timeout:.0f}s, killed\n"
        return subprocess.CompletedProcess(
            process.args, process.returncode, stdout, stderr
        )
    finally:
        os.close(controller)


def take_lease(path: str, on_busy) -> None:
    """Holds `path` exclusively for as long as this process lives, calling
    `on_busy(holder)` first when another process holds it."""
    lease = os.open(path, os.O_RDWR | os.O_CREAT, 0o644)
    try:
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        on_busy(os.pread(lease, 32, 0).decode(errors="replace").strip() or "?")
        fcntl.flock(lease, fcntl.LOCK_EX)
    os.ftruncate(lease, 0)
    os.pwrite(lease, str(os.getpid()).encode(), 0)


# ── the screen ───────────────────────────────────────────────────────────


@dataclass
class Terminal:
    """One tmux session holding the app's pty."""

    session: str

    # tmux's `pipe-pane` hands over every byte the app writes, verbatim.
    TAP_REFUSAL = None

    @classmethod
    def open(
        cls,
        command,
        cwd: str,
        env: dict,
        cols: int,
        rows: int,
        cast: Path | None,
        title: str,
        tap: str | None,
    ) -> Terminal:
        session = f"journey-{os.getpid()}"
        subprocess.run(["tmux", "kill-session", "-t", session], capture_output=True)
        # `env -i` rather than tmux's own `-e`: tmux sessions inherit the
        # tmux server's environment, and one inherited `UZE_PANE` makes the
        # app believe it is nested inside a pane of the developer's own
        # running workspace.
        launch = " ".join(
            ["env", "-i"]
            + [shlex.quote(f"{key}={value}") for key, value in env.items()]
            + [shlex.quote(command) if isinstance(command, str) else " ".join(command)]
        )
        # A cast is for a person to watch; it is never what proves a check.
        # Recorded inside the tmux pane, so reading the screen is unaffected.
        if cast:
            launch = (
                f"asciinema rec -q --overwrite -e TERM "
                f"-t {shlex.quote(title)} -c {shlex.quote(launch)} {shlex.quote(str(cast))}"
            )
        # The pane outlives the app on purpose. tmux tears a session down the
        # moment its command exits, and an app that refused to start would
        # then leave an empty capture — the one frame worth having.
        epilogue = (
            "; status=$?"
            "; printf '\\n[journey] the app exited with %s\\n' \"$status\""
            "; sleep 3600"
        )
        launch = "sh -c " + shlex.quote(launch + epilogue)
        subprocess.run(
            [
                "tmux",
                "new-session",
                "-d",
                "-s",
                session,
                "-x",
                str(cols),
                "-y",
                str(rows),
                "-c",
                cwd,
                launch,
            ],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["tmux", "set-option", "-t", session, "status", "off"], capture_output=True
        )
        # What the app writes to its terminal, byte for byte — the only
        # witness of a request the app makes of the terminal itself (a
        # clipboard write) rather than of a cell. Per pane, so the tmux
        # server's own options are left alone.
        if tap:
            subprocess.run(
                [
                    "tmux",
                    "pipe-pane",
                    "-t",
                    session,
                    "-o",
                    f"cat >> {shlex.quote(tap)}",
                ],
                check=True,
                capture_output=True,
            )
        return cls(session)

    def attach_hints(self) -> list[str]:
        return [
            f"attach with:  tmux attach -t {self.session}",
            f"read it with: tmux capture-pane -t {self.session} -p",
        ]

    def pane(self) -> str:
        return subprocess.run(
            ["tmux", "capture-pane", "-t", self.session, "-p"],
            capture_output=True,
            text=True,
        ).stdout

    def alive(self) -> bool:
        return (
            subprocess.run(
                ["tmux", "has-session", "-t", self.session], capture_output=True
            ).returncode
            == 0
        )

    def key(self, name: str) -> None:
        self._send(name)

    def literal(self, text: str) -> None:
        self._send("-l", text)

    def kill(self) -> None:
        subprocess.run(
            ["tmux", "kill-session", "-t", self.session], capture_output=True
        )

    def _send(self, *args: str) -> None:
        subprocess.run(
            ["tmux", "send-keys", "-t", self.session, *args], capture_output=True
        )
