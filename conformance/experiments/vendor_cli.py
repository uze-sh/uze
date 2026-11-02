"""Experiments that measure a vendor CLI's own mechanics, with no model
turn and no UZE in the loop: one container runs a list of named steps, and
each step's combined output and exit code come back for the checks.

A step's command runs as written, so it must not end in a pipe: the exit
code recorded is the command's own, never `head`'s.
"""

import json
import os
import subprocess

from shared import common


def script(setup, steps):
    """The container's shell: `setup` silently, then each `(name, command)`
    framed by `=== name` and `rc=<exit>`."""
    framed = "\n".join(
        f"echo '=== {name}'\n{command} 2>&1\necho \"rc=$?\"" for name, command in steps
    )
    return f"""
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
set -e
{setup}
set +e
{framed}
"""


def run_steps(cfg, prov_ip, tag, setup, steps, timeout=600):
    """Runs the steps in one fresh container and returns `{name: (rc, out)}`.
    The full transcript is kept as `<tag>.out` in the run outdir."""
    proc = subprocess.run(
        common.docker_base(cfg, prov_ip, script(setup, steps), tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=timeout,
    )
    transcript = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, f"{tag}.out"), "w") as f:
        f.write(transcript)
    results, name, lines = {}, None, []
    for line in proc.stdout.splitlines():
        if line.startswith("=== "):
            name, lines = line[4:].strip(), []
        elif name and line.startswith("rc=") and line[3:].isdigit():
            results[name] = (int(line[3:]), "\n".join(lines))
        elif name:
            lines.append(line)
    return results


def json_in(text):
    """The first JSON array or object in a step's output, which may carry a
    line of the vendor's own chatter before it; `None` when there is none."""
    starts = [i for i in (text.find("["), text.find("{")) if i >= 0]
    if not starts:
        return None
    try:
        return json.JSONDecoder().raw_decode(text[min(starts) :])[0]
    except ValueError:
        return None


def check_version(name, results, step="version"):
    """Records the vendor version the run measured, which every fact this
    experiment proves is stated against."""
    rc, out = results.get(step, (1, ""))
    version = out.strip().splitlines()[0] if out.strip() else ""
    common.check(name, rc == 0 and bool(version), version or "no version printed")
    return version
