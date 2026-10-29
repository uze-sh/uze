"""The flags a harness's release defines, against the snapshot UZE keeps.

Each integration declares the flags an operator may add to an agent's
launch, and which of them UZE composes itself (`IntegrationPort::flag_catalog`).
That catalog is proven against a committed `--help` snapshot by the Rust
suite; this contract proves the snapshot against the release, so a flag a
vendor removed or added cannot sit unseen behind a green unit test. It
compares the flags each help text defines, never its wording: a reworded
description is not drift, a flag that appeared or vanished is.

`harness-watch` refreshes the snapshot and the catalog together.
"""

import os
import subprocess

from shared.common import HARNESS_IMAGE, VERSION_PROBES, ansi_strip, check, describe

#: An option's own line is indented this far at most; a description that
#: wraps onto the next line sits deeper, and a flag it mentions is not one
#: it defines.
DEFINITION_INDENT = 6


def assert_contract(cfg, prov_ip, bindings):
    with describe("flags"):
        check(
            "flags-snapshot-declared",
            bool(bindings.help_snapshots),
            f"{bindings.harness} names the --help snapshots its catalog is proven by",
        )
        for args, snapshot in bindings.help_snapshots:
            _assert_unchanged(cfg, args, snapshot)


def _assert_unchanged(cfg, args, snapshot):
    command = [VERSION_PROBES[cfg.harness][0], *args]
    label = " ".join(command)
    name = "-".join(arg.lstrip("-") for arg in args)
    live = _live_help(command)
    check(
        f"flags-{name}-runs",
        live is not None,
        f"`{label}` printed its help" if live is not None else f"`{label}` printed nothing",
    )
    if live is None:
        return
    with open(os.path.join(os.path.dirname(cfg.repo), snapshot)) as file:
        kept = file.read()
    appeared, vanished = drift(kept, live)
    check(
        f"flags-{name}-matches-snapshot",
        not appeared and not vanished,
        describe_drift(snapshot, appeared, vanished),
    )


def _live_help(command):
    """What the release prints for `command`, without colour, or `None`
    when it could not be run."""
    try:
        out = subprocess.run(
            [
                "docker",
                "run",
                "--rm",
                "--network",
                "none",
                "-e",
                "HOME=/home/node",
                "-e",
                "NO_COLOR=1",
                HARNESS_IMAGE,
                *command,
            ],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except Exception:
        return None
    text = ansi_strip(out.stdout + out.stderr)
    return text if defined_flags(text) else None


def defined_flags(help_text):
    """Every flag `help_text` defines, in each spelling it shows: the
    dash-led words opening an option's own line (`-c, --continue`,
    `--session, -s string`), up to its value. The same reading as the Rust
    suite's `flags_in_help`, so both sides agree on what a snapshot holds."""
    flags = set()
    for line in help_text.splitlines():
        if len(line) - len(line.lstrip()) > DEFINITION_INDENT:
            continue
        for word in line.split():
            word = word.rstrip(",")
            if not _is_flag(word):
                break
            flags.add(word)
    return flags


def _is_flag(word):
    name = word.lstrip("-")
    dashes = len(word) - len(name)
    return (
        dashes in (1, 2)
        and name[:1].isascii()
        and name[:1].isalpha()
        and all(c.isascii() and (c.isalnum() or c in "-_") for c in name)
    )


def drift(kept, live):
    """The flags the release defines that the snapshot does not, and the
    ones the snapshot defines that the release no longer does."""
    kept_flags, live_flags = defined_flags(kept), defined_flags(live)
    return sorted(live_flags - kept_flags), sorted(kept_flags - live_flags)


def describe_drift(snapshot, appeared, vanished):
    if not appeared and not vanished:
        return f"the release defines exactly the flags {snapshot} holds"
    parts = []
    if appeared:
        parts.append(f"appeared: {', '.join(appeared)}")
    if vanished:
        parts.append(f"vanished: {', '.join(vanished)}")
    return f"{snapshot} drifted from the release ({'; '.join(parts)})"
