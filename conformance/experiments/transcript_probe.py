"""Recording what a harness's own transcript looks like, for the briefing's reader.

When an agent continues on another harness, UZE reads the conversation the
harness left behind out of that harness's own transcript — a format no
vendor documents and every vendor changes. The reader is pinned in the
deterministic suite against a transcript this probe records from the real
binary: one typed turn carrying a marker, answered by the synthetic
provider, then the file the harness wrote, copied out of the container
verbatim. Nothing a person typed is in it; the provider is synthetic.

  python3 conformance/lab.py --harness claude --experiment claude/transcript
  python3 conformance/lab.py --harness codex --experiment codex/transcript

Writes `tests/_fixtures/foreign/transcripts/<harness>.jsonl`; the diff is
the review of what the vendor changed. Re-record on a version drift.
"""

import os
import subprocess
import time
from pathlib import Path

from experiments.relaunch_probe import bindings_for
from shared.common import HARNESS_IMAGE

#: The turn the fixture's reader must find as the operator's.
MARKER = "UZE_CONFORMANCE_PECAN"
#: Where each harness files a conversation inside the container.
TRANSCRIPTS = {
    "claude": "/work/home/.claude/projects/*/*.jsonl",
    "codex": "/work/home/.codex/sessions/*/*/*/rollout-*.jsonl",
}
FIXTURES = Path(__file__).resolve().parents[2] / "tests/_fixtures/foreign/transcripts"


def say(line):
    print(f"    {line}", flush=True)


def run(cfg, prov_ip):
    pattern = TRANSCRIPTS.get(cfg.harness)
    if pattern is None:
        say(f"{cfg.harness} has no transcript reader to pin")
        return
    bindings = bindings_for(cfg.harness)
    with bindings.session(cfg, prov_ip) as tui:
        plain, matched = bindings.prepare(tui)
        if not matched:
            say(f"never reached the prompt: {plain[-300:]!r}")
            return
        bindings.await_input(tui)
        tui.type(f"{MARKER}: make branch names short")
        tui.submit()
        tui.collect(reads=8)
        time.sleep(2)
        transcript = _copy_out(pattern)
    if not transcript:
        say("the harness wrote no transcript")
        return
    FIXTURES.mkdir(parents=True, exist_ok=True)
    target = FIXTURES / f"{cfg.harness}.jsonl"
    target.write_text(transcript)
    say(
        f"wrote {target} ({len(transcript)} bytes, marker present: {MARKER in transcript})"
    )


def _copy_out(pattern):
    """The newest transcript in the one harness container this run holds."""
    containers = subprocess.run(
        [
            "docker",
            "ps",
            "--filter",
            f"ancestor={HARNESS_IMAGE}",
            "--format",
            "{{.ID}}",
        ],
        capture_output=True,
        text=True,
    ).stdout.split()
    for container in containers:
        found = subprocess.run(
            [
                "docker",
                "exec",
                container,
                "sh",
                "-c",
                f"cat $(ls -t {pattern} | head -1)",
            ],
            capture_output=True,
            text=True,
            env={**os.environ},
        )
        if found.returncode == 0 and found.stdout.strip():
            return found.stdout
    return ""
