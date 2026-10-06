"""What a person meets the first time a harness opens after `uze install`.

Every other contract starts its harness already past whatever it asks a
person; this one is the person. A plugin with hooks is installed, UZE is
asked what it delivered — before the harness ever opens — and the harness
is then opened and answered on screen the way a person answers it: folder
trust, and on a harness that reviews hooks, that review.

What UZE says is the subject, so each answer is held against the harness's
own record of the same fact, never against another UZE report: on a
harness that reviews hooks, UZE must name them as held back while the
harness has recorded no trust, and stop once it has.
"""

import json
import subprocess
import time

from shared.common import check, describe, start_provider

#: The plugin a person installs: one hook group per alias, so a harness
#: that reviews hooks has something to hold back.
PLUGIN = "hook-rows"

STATUS_BEFORE = "/work/uze-status-before.json"

#: Left by the shell once `uze status -m` has answered and before the harness
#: starts. Polled in the container, never read from the terminal: a read
#: there consumes what it reads, and on a slow runner the harness's first
#: onboarding frame arrived in the same read as a printed marker, so the
#: onboarding was taken off the screen before anything could answer it.
BEFORE_DONE = "/work/uze-status-before.done"


def assert_contract(cfg, prov_ip, bindings):
    with describe("first-session"):
        _assert_first_session(cfg, bindings)
    start_provider(cfg, "static")


def _status(cfg):
    """What `uze status -m` says now, inside the running harness container."""
    out = subprocess.run(
        [
            "docker",
            "exec",
            cfg.harness_container,
            "sh",
            "-c",
            "HOME=/work/home UZE_HOME=/work/home/.uze PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin "
            "uze status -m --format json",
        ],
        capture_output=True,
        text=True,
        errors="replace",
    )
    try:
        return json.loads(out.stdout)
    except ValueError:
        return None


def _stderr(cfg):
    return subprocess.run(
        ["docker", "exec", cfg.harness_container, "cat", "/work/uze-status-before.err"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout[-200:]


def _status_taken(cfg, seconds=60):
    """Waits for the shell to say `uze status -m` has answered, without
    touching the harness's terminal."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        taken = subprocess.run(
            ["docker", "exec", cfg.harness_container, "test", "-e", BEFORE_DONE],
            capture_output=True,
        )
        if taken.returncode == 0:
            return True
        time.sleep(0.5)
    return False


def _held(report, harness):
    """What UZE says this harness holds back of the plugin; the image
    carries every harness, so the machine's report names the others too."""
    return [
        note
        for note in (report or {}).get("held_back", [])
        if note.get("plugin", "").startswith(PLUGIN) and note.get("harness") == harness
    ]


def _assert_first_session(cfg, bindings):
    prov_ip = start_provider(cfg, "static")
    before_cmd = (
        f"uze status -m --format json > {STATUS_BEFORE} 2>/work/uze-status-before.err; "
        f"touch {BEFORE_DONE}"
    )
    with bindings.hook_session(cfg, prov_ip, PLUGIN, "first", before=before_cmd) as tui:
        # The status is taken by the shell before the harness starts; the
        # harness cannot have recorded anything a person answers yet.
        _status_taken(cfg)
        read = subprocess.run(
            ["docker", "exec", cfg.harness_container, "cat", STATUS_BEFORE],
            capture_output=True,
            text=True,
            errors="replace",
        )
        try:
            before = json.loads(read.stdout)
        except ValueError:
            before = None
        recorded_before = bindings.hook_review_recorded(cfg)
        plain, ready = bindings.prepare(tui)
        check(
            "first-session-ready",
            bool(ready),
            "every prompt was answered on screen"
            if ready
            else plain[-160:].replace("\n", " "),
        )
        after = _status(cfg)
        recorded_after = bindings.hook_review_recorded(cfg)

    check(
        "first-session-status-read",
        before is not None and after is not None,
        "`uze status -m` answered before the harness opened and after"
        if before is not None and after is not None
        else f"before: {read.stdout[-160:]!r} {_stderr(cfg)}",
    )
    if recorded_before is None:
        # A harness that runs delivered hooks without asking: UZE must
        # not claim anything waits.
        check(
            "first-session-nothing-held-back",
            before is not None and not _held(before, bindings.display_name),
            f"held back: {_held(before, bindings.display_name)}",
        )
        return
    held_before = _held(before, bindings.display_name)
    check(
        "first-session-held-back-reported",
        bool(held_before) and not recorded_before,
        f"UZE named {len(held_before)} hook(s) waiting while the harness had recorded no trust"
        if held_before and not recorded_before
        else f"held back: {held_before}; trust recorded before: {recorded_before}",
    )
    check(
        "first-session-held-back-cleared",
        recorded_after and not _held(after, bindings.display_name),
        "once the review was answered on screen, the harness recorded trust and UZE named nothing waiting"
        if recorded_after and not _held(after, bindings.display_name)
        else f"trust recorded after: {recorded_after}; still held: {_held(after, bindings.display_name)}",
    )
