"""The journey runner's machine and screen on Windows.

The counterpart of `unix.py`, answering the same questions with what
Windows has: the screen is a ConPTY (`pywinpty`) feeding a `pyte` screen,
the process table is `psutil`, and a `shell:` step runs under Git Bash,
reached by absolute path so the world's own `PATH` never carries it.

What the packages are, and how they are installed, is
`journeys/requirements-windows.txt`.
"""

from __future__ import annotations

import filecmp
import glob
import msvcrt
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

try:
    import psutil
    import pyte
    from winpty import PtyProcess
except ImportError as missing:
    raise SystemExit(
        f"journey: {missing.name} is not installed. On Windows the runner needs "
        "`python -m pip install --only-binary :all: -r journeys/requirements-windows.txt`."
    ) from None

PLATFORM = "windows"
# What the shell a pane opens is called in the process table: Windows
# PowerShell, since the world's PATH reaches no PowerShell 7. Anchored to
# the program, because the process that hosts a pane names the shell it
# starts on its own command line (`uze terminal host-pane … -- powershell.exe`).
SHELL = r"^(\S*/)?powershell\.exe\b"
EXECUTABLE_SUFFIX = ".exe"
REQUIRED_TOOLS = ("git",)
# At the drive's root, not under %TEMP%: PowerShell's prompt is the whole
# working directory, and under a profile's temporary directory it fills a
# pane's width, so the command a journey types wraps and no pattern meets
# it whole. Any account may create a directory there.
DEFAULT_WORLDS = os.environ.get("SystemDrive", "C:") + "\\uze-journeys"

# Git for Windows' own bash, where every hosted runner has it. Runner tooling,
# not the world's: a `shell:` step is what an agent would type, and it is
# written in POSIX sh, but the `uze` under test must meet the machine a
# Windows user has, whose `PATH` reaches no `/usr/bin`. The MSYS bash itself
# rather than the `bin\bash.exe` launcher, which puts Git's own `curl` and
# `ssh` ahead of the world's `PATH` and so ahead of the stand-ins.
GIT_BASH = Path(
    os.environ.get("JOURNEY_BASH", r"C:\Program Files\Git\usr\bin\bash.exe")
)

# What a Windows process cannot start without, or reads to find the
# machine's own directories. Passed through by name, never the rest.
SYSTEM_VARIABLES = (
    "SystemRoot",
    "windir",
    "SystemDrive",
    "ComSpec",
    "PATHEXT",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "CommonProgramW6432",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "OS",
    "COMPUTERNAME",
    "USERNAME",
    "USERDOMAIN",
)


def prepare_interpreter() -> None:
    """Runs this script again in UTF-8 mode when it is not already in it.

    Windows Python reads files and child output in the ANSI code page unless
    told otherwise, and the suites are UTF-8: a journey whose `expect` is a
    box-drawing glyph would be compared against mojibake.
    """
    if not sys.flags.utf8_mode:
        raise SystemExit(
            subprocess.call([sys.executable, "-X", "utf8", *sys.argv], close_fds=True)
        )


def missing_tools() -> list[str]:
    if GIT_BASH.is_file():
        return []
    return [f"Git Bash at {GIT_BASH} (name another one in JOURNEY_BASH)"]


def spell(path: Path | str) -> str:
    """A path as a journey's placeholders spell it: forward slashes, which
    both Git Bash and every Windows program read, where a backslash would be
    an escape to the shell a `shell:` step runs in."""
    return Path(path).as_posix()


def host_path(path: str) -> str:
    """`path` as NTFS holds it: each colon after the drive is the `-` UZE
    names the file with (`uze_platform::fs_name::file_name_for`), since a
    colon there would address an alternate data stream instead."""
    drive, rest = os.path.splitdrive(path)
    return drive + rest.replace(":", "-")


def launchers(pattern: str) -> list[str]:
    """The launchers matching `pattern`: executables, since an ordinary
    account makes no link to one and UZE places a copy instead."""
    return sorted(path for path in glob.glob(pattern + ".exe") if os.path.isfile(path))


def launches(launcher: str, binary: Path) -> bool:
    """Whether running `launcher` runs `binary`: it is a copy of it."""
    return filecmp.cmp(launcher, binary, shallow=False)


def shell_rc_name() -> str:
    """The profile Windows PowerShell reads at startup, the one every
    supported Windows carries — what UZE would have to write to reach a
    shell started outside its panes."""
    return "Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"


def world_environment(root: Path, binary: Path) -> dict:
    """What a world's environment carries on Windows, beside the identity
    and homes every platform's world shares: the profile directories a
    program looks for before it looks at the home, a temporary directory of
    the world's own, and a `PATH` of the world's binaries, Git and the
    system's — the machine a person has, with Git for Windows installed."""
    home = root / "home"
    temporary = root / "tmp"
    for directory in (
        home / "AppData" / "Roaming",
        home / "AppData" / "Local",
        temporary,
    ):
        directory.mkdir(parents=True, exist_ok=True)
    env = _environment(root, binary, home, temporary)
    env["PSModuleAnalysisCachePath"] = str(_seasoned_powershell(root.parent, env))
    return env


def _system32() -> Path:
    return Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32"


def _environment(root: Path, binary: Path, home: Path, temporary: Path) -> dict:
    system32 = _system32()
    system_root = system32.parent
    git = shutil.which("git")
    path = [
        root / "bin",
        binary.parent,
        *([Path(git).parent] if git else []),
        system32,
        system_root,
        system32 / "Wbem",
        system32 / "WindowsPowerShell" / "v1.0",
    ]
    env = {name: os.environ[name] for name in SYSTEM_VARIABLES if name in os.environ}
    env.update(
        {
            "USERPROFILE": str(home),
            "APPDATA": str(home / "AppData" / "Roaming"),
            "LOCALAPPDATA": str(home / "AppData" / "Local"),
            "TEMP": str(temporary),
            "TMP": str(temporary),
            "PATH": os.pathsep.join(map(str, path)),
        }
    )
    return env


def _seasoned_powershell(worlds: Path, env: dict) -> Path:
    """The module cache every world's Windows PowerShell reads, built once
    per run.

    The first command a PowerShell runs walks every module on the machine
    to find the one that command lives in, and writes what it found to this
    cache. Measured on the hosted runners, that walk is 33-40 seconds, and
    it landed inside whichever journey first typed into a pane: on an Arm
    runner it took most of `09-text-copied-from-a-pane`'s 30-second gesture,
    and failed it whenever it took all of it. No person meets that walk on
    every shell they open, so it is paid here, outside any gesture."""
    cache = worlds / ".powershell" / "ModuleAnalysisCache"
    if cache.is_file():
        return cache
    cache.parent.mkdir(parents=True, exist_ok=True)
    # Built beside the cache and moved into place whole, so a run beside
    # this one reads either no cache or a finished one.
    staging = cache.with_name(f"{cache.name}.{os.getpid()}")
    started = time.monotonic()
    subprocess.run(
        [
            str(_system32() / "WindowsPowerShell" / "v1.0" / "powershell.exe"),
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "echo seasoned | Out-Null",
        ],
        env={**env, "PSModuleAnalysisCachePath": str(staging)},
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=600,
    )
    print(
        f"journey: Windows PowerShell's module cache took {time.monotonic() - started:.1f}s to build",
        file=sys.stderr,
    )
    if staging.is_file():
        os.replace(staging, cache)
    else:
        print(
            f"journey: Windows PowerShell wrote nothing to {staging}; the first pane to run a command builds it",
            file=sys.stderr,
        )
    return cache


# ── the process table ────────────────────────────────────────────────────
#
# psutil answers both facts a `process:` check needs — what a process
# inherited and where it is standing — for every process this user owns.
# A process it cannot read answers `b""`/`None` and is skipped, the same as
# a process that exited between the listing and the question.


def process_environ(pid: int | str) -> bytes | None:
    try:
        environ = psutil.Process(int(pid)).environ()
    except (psutil.Error, OSError):
        return b""
    return b"\0".join(f"{key}={value}".encode() for key, value in environ.items())


def process_cwd(pid: int | str) -> str | None:
    try:
        return spell(psutil.Process(int(pid)).cwd())
    except (psutil.Error, OSError):
        return None


def process_alive(pid: int) -> bool:
    # Never `os.kill(pid, 0)` here: on Windows that is TerminateProcess
    # with exit code 0, which answers "is it running" by ending it.
    return psutil.pid_exists(pid)


def process_table_problem() -> str | None:
    return None


def command_line(process: psutil.Process) -> str:
    """The command line as `pgrep -f` matches it, with forward slashes so a
    journey's `{world}/bin/claude` matches the path Windows reports."""
    return " ".join(process.info["cmdline"] or [process.info["name"] or ""]).replace(
        "\\", "/"
    )


def processes() -> list:
    return [
        process
        for process in psutil.process_iter(["pid", "name", "cmdline"])
        if process.info["pid"] != os.getpid()
    ]


def pids_matching(pattern: str) -> list[str]:
    expression = re.compile(pattern)
    return [
        str(process.info["pid"])
        for process in processes()
        if expression.search(command_line(process))
    ]


def process_listing() -> list[str]:
    return [f"{process.info['pid']} {command_line(process)}" for process in processes()]


def terminate(pid: int) -> bool:
    try:
        psutil.Process(pid).terminate()
    except (psutil.Error, OSError):
        return False
    return True


def kill(pid: int) -> None:
    try:
        psutil.Process(pid).kill()
    except (psutil.Error, OSError):
        pass


def kill_tree(pid: int) -> None:
    """`pid` and everything it started. Windows has no session to signal,
    and a child outlives its parent here unless it is ended by name."""
    try:
        root = psutil.Process(pid)
        family = [*root.children(recursive=True), root]
    except (psutil.Error, OSError):
        return
    for member in family:
        try:
            member.kill()
        except (psutil.Error, OSError):
            pass
    psutil.wait_procs(family, timeout=5)


def list_tree(roots: list[Path], depth: int) -> str:
    """Every path under `roots`, `depth` levels down, one per line — what
    `find -maxdepth` prints."""
    lines = []
    for root in roots:
        if not root.exists():
            continue
        lines.append(spell(root))
        base = len(root.parts)
        for directory, subdirectories, files in os.walk(root):
            level = len(Path(directory).parts) - base
            if level >= depth:
                subdirectories[:] = []
                continue
            for name in sorted(subdirectories) + sorted(files):
                lines.append(spell(Path(directory) / name))
    return "\n".join(lines) + "\n"


# ── commands ─────────────────────────────────────────────────────────────


def shell_argv(command: str) -> list[str]:
    """Git Bash running one line, with the world's `PATH` first and the
    POSIX tools a step is written with after it."""
    return [str(GIT_BASH), "-c", f'PATH="$PATH:/usr/bin"\n{command}']


def run_with_a_terminal(
    command, timeout: float = 120, **options
) -> subprocess.CompletedProcess:
    """`subprocess.run` with stdin off, a deadline, and the whole tree ended
    when the deadline passes.

    Windows has no controlling terminal to give the process, so the Unix
    runner's "a terminal nobody answers" has no counterpart: a vendor
    installer that asks has its console, and no console input arrives. The
    deadline still turns a hang into a failure that says so.
    """
    if isinstance(command, str):
        command = shell_argv(command)
    process = subprocess.Popen(
        command,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        **options,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        kill_tree(process.pid)
        stdout, stderr = process.communicate()
        stderr += f"\n[journey] still running after {timeout:.0f}s, killed\n"
    return subprocess.CompletedProcess(process.args, process.returncode, stdout, stderr)


# The byte the lease locks, far past the pid written at the start: a region
# `msvcrt.locking` holds is unreadable to every other process, and the run
# waiting for this one has to read who it is waiting for.
LEASE_BYTE = 1 << 20


def take_lease(path: str, on_busy) -> None:
    lease = os.open(path, os.O_RDWR | os.O_CREAT)

    def lock() -> bool:
        os.lseek(lease, LEASE_BYTE, os.SEEK_SET)
        try:
            msvcrt.locking(lease, msvcrt.LK_NBLCK, 1)
        except OSError:
            return False
        return True

    if not lock():
        with open(path, "rb") as held:
            on_busy(held.read(32).decode(errors="replace").strip() or "?")
        while not lock():
            time.sleep(0.5)
    os.ftruncate(lease, 0)
    os.lseek(lease, 0, os.SEEK_SET)
    os.write(lease, str(os.getpid()).encode())


# ── the screen ───────────────────────────────────────────────────────────

# tmux's key names, as the suites spell them, to what a terminal sends.
NAMED_KEYS = {
    "Enter": "\r",
    "Escape": "\x1b",
    "Tab": "\t",
    "BTab": "\x1b[Z",
    "BSpace": "\x7f",
    "Space": " ",
    "Up": "\x1b[A",
    "Down": "\x1b[B",
    "Right": "\x1b[C",
    "Left": "\x1b[D",
    "Home": "\x1b[H",
    "End": "\x1b[F",
    "PageUp": "\x1b[5~",
    "PPage": "\x1b[5~",
    "PageDown": "\x1b[6~",
    "NPage": "\x1b[6~",
    "Delete": "\x1b[3~",
    "DC": "\x1b[3~",
    "Insert": "\x1b[2~",
    "IC": "\x1b[2~",
    "F1": "\x1bOP",
    "F2": "\x1bOQ",
    "F3": "\x1bOR",
    "F4": "\x1bOS",
    "F5": "\x1b[15~",
    "F6": "\x1b[17~",
    "F7": "\x1b[18~",
    "F8": "\x1b[19~",
    "F9": "\x1b[20~",
    "F10": "\x1b[21~",
    "F11": "\x1b[23~",
    "F12": "\x1b[24~",
}

CONTROL_PUNCTUATION = {
    "Space": "\x00",
    "@": "\x00",
    "[": "\x1b",
    "\\": "\x1c",
    "]": "\x1d",
    "^": "\x1e",
    "_": "\x1f",
}


def key_sequence(name: str) -> str:
    """What a terminal sends for the key tmux calls `name`: a named key,
    `C-x` and `M-x` (combinable as `C-M-x`), or a single character."""
    if name in NAMED_KEYS:
        return NAMED_KEYS[name]
    if name.startswith("M-") and len(name) > 2:
        return "\x1b" + key_sequence(name[2:])
    if name.startswith("C-") and len(name) > 2:
        rest = name[2:]
        if len(rest) == 1 and rest.isalpha():
            return chr(ord(rest.lower()) & 0x1F)
        if rest in CONTROL_PUNCTUATION:
            return CONTROL_PUNCTUATION[rest]
        return key_sequence(rest)
    if len(name) == 1:
        return name
    raise ValueError(f"no key named {name!r}")


class Terminal:
    """A ConPTY holding the app, read into a `pyte` screen.

    ConPTY renders the console the app draws into as VT, which `pyte`
    replays onto a cell grid of the same size — so a click aimed at a cell
    of this grid is a click at the same cell of the app's console, written
    back as the SGR report ConPTY turns into the app's mouse event. A
    terminal also answers questions: ConPTY asks for the cursor position
    when it starts and waits for the answer, which `pyte` gives through
    `write_process_input` and this writes back into the pty.
    """

    # ConPTY passes a request of the terminal itself (an OSC 52 clipboard
    # write) through to the output it renders, so the tap reads what the app
    # asked for, as tmux's `pipe-pane` does.
    TAP_REFUSAL = None

    def __init__(self, process, cols: int, rows: int, tap: str | None):
        self.process = process
        self.tap = tap
        self.screen = pyte.Screen(cols, rows)
        self.screen.write_process_input = self._answer
        self.stream = pyte.Stream(self.screen)
        self.lock = threading.Lock()
        self.exited = threading.Event()
        threading.Thread(target=self._read, daemon=True).start()

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
        argv = [command] if isinstance(command, str) else list(command)
        process = PtyProcess.spawn(
            argv, cwd=os.path.normpath(cwd), env=env, dimensions=(rows, cols)
        )
        return cls(process, cols, rows, tap)

    def attach_hints(self) -> list[str]:
        return [
            f"the app is pid {self.process.pid}; a ConPTY session has no "
            "attach — read it from this runner"
        ]

    def _answer(self, data: str) -> None:
        try:
            self.process.write(data)
        except (EOFError, OSError):
            pass

    def _read(self) -> None:
        while True:
            try:
                data = self.process.read(65536)
            except (EOFError, OSError):
                break
            if data:
                with self.lock:
                    self.stream.feed(data)
                if self.tap:
                    with open(self.tap, "ab") as tap:
                        tap.write(data.encode())
            elif not self.process.isalive():
                break
        # The frame the app left stays readable, as the tmux pane outliving
        # its command keeps it on Unix — an app that refused to start is
        # exactly when that frame is worth having.
        deadline = time.monotonic() + 5
        while self.process.isalive() and time.monotonic() < deadline:
            time.sleep(0.1)
        with self.lock:
            self.stream.feed(
                f"\r\n[journey] the app exited with {self.process.exitstatus}\r\n"
            )
        self.exited.set()

    def pane(self) -> str:
        with self.lock:
            rows = [row.rstrip() for row in self.screen.display]
        return "\n".join(rows) + "\n"

    def alive(self) -> bool:
        return not self.exited.is_set() and self.process.isalive()

    def key(self, name: str) -> None:
        self.literal(key_sequence(name))

    def literal(self, text: str) -> None:
        try:
            self.process.write(text)
        except (EOFError, OSError):
            pass

    def kill(self) -> None:
        kill_tree(self.process.pid)
        try:
            self.process.close(force=True)
        except (EOFError, OSError):
            pass
