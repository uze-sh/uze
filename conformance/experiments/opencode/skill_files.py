"""Does OpenCode show the model a delivered Skill's supporting files?

OpenCode answers a `skill` tool call with the Skill's body and a
`<skill_files>` block listing the files beside its `SKILL.md`. Its walker
does not descend a skill root that is a symbolic link: measured on 2.0.18
(the delivery-mechanics study of 2026-09-28), a linked root delivers the body
and an empty `<skill_files>`. UZE delivered every loose Skill that way until
`deliver-the-whole-plugin`; it now writes a real directory.

This installs the Lab's `flow` plugin with the UZE under test, scripts one
`skill` call for `flow:locate` (whose directory carries
`references/where.md`), and reads the request that carries the tool result
off the wire:

  skill-files-body-delivered   the Skill's body reached the model
  skill-files-listed           `<skill_files>` names references/where.md

Run: python3 conformance/lab.py --harness opencode --experiment opencode/skill_files
"""

import json
import os
import re
import subprocess

from harnesses.opencode.scenarios import opencode_container
from shared import common

TRIGGER = "uze-skill-files-probe"
BODY = "UZE_SKILL_BODY_LOCATE"
FILE = "references/where.md"


def run(cfg, prov_ip):
    prov_ip = (
        common.start_provider(
            cfg,
            "toolcall",
            {
                "DISCOVERY": "1",
                "TOOL_TRIGGER": TRIGGER,
                "TOOL_NAME": "skill",
                "TOOL_ARGS": json.dumps({"id": "flow:locate"}),
            },
        )
        or prov_ip
    )
    turn = f"cd /work && timeout 150 opencode run --standalone --auto '{TRIGGER}' 2>&1 | tail -20"
    cmd = opencode_container(cfg, prov_ip, turn, plugins="flow", tty=False)
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=900
    )
    with open(os.path.join(cfg.outdir, "opencode-skill-files.out"), "w") as f:
        f.write(proc.stdout + "\n=== stderr\n" + proc.stderr)
    log = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(os.path.join(cfg.outdir, "opencode-skill-files.requests.log"), "w") as f:
        f.write(log)

    carrying = [block for block in log.split("### ") if BODY in block]
    common.check(
        "skill-files-body-delivered",
        bool(carrying),
        "the locate Skill's body reached the model"
        if carrying
        else "no request carried the locate Skill's body",
    )
    if not carrying:
        return
    listed = [
        match
        for block in carrying
        for match in re.findall(r"<skill_files>(.*?)</skill_files>", block, re.S)
    ]
    names = any(FILE in entry for entry in listed)
    common.check(
        "skill-files-listed",
        names,
        f"<skill_files> names {FILE}"
        if names
        else f"<skill_files> did not name {FILE}: {[entry[:200] for entry in listed] or 'no block'}",
    )
