"""Where OpenCode puts a worktree the model makes with its own
`worktree_create` tool (≈2.0.27), which UZE accounts for as the harness's
isolation. No UZE in the loop: the model calls the tool once in a Git
repository, and the experiment reads `git worktree list` afterwards.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/worktree-tool --discovery
"""

import json
import os
import re
import subprocess

from experiments.relaunch_probe import bindings_for
from shared.common import check, pull_captures, start_provider

CWD = "/work/wt-project"
NAME = "uzeprobetree"
BEGIN = "UZE_WT_BEGIN"
PRELUDE = (
    f"mkdir -p {CWD} && cd {CWD} && git init -q -b main . && "
    "git -c user.name=lab -c user.email=lab@uze.invalid commit -q --allow-empty -m init\n"
)
# `worktree_create` is a Code Mode tool: the model reaches it through
# `execute`, in the `opencode` namespace of the catalog.
TOOL = "execute"
CODE = f'return await tools["opencode"].worktree_create({{name: "{NAME}"}});'


def run(cfg, prov_ip):
    bindings = bindings_for(cfg.harness)
    prompt = "make a worktree for the probe"
    prov_ip = start_provider(
        cfg,
        "toolcall",
        {
            "TOOL_NAME": TOOL,
            "TOOL_ARGS": json.dumps({"code": CODE}),
            "TOOL_TRIGGER": prompt,
        },
    )
    cmd = bindings.headless(cfg, prov_ip, PRELUDE, prompt, CWD)
    listing_cmd = f"echo {BEGIN}; git -C {CWD} worktree list --porcelain\n"
    cmd = [
        part[: part.rfind("2>&1\n") + 5]
        + listing_cmd
        + part[part.rfind("2>&1\n") + 5 :]
        if "opencode run" in part
        else part
        for part in cmd
    ]
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=600
    )
    out = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, "worktree-tool.out"), "w") as f:
        f.write(out)
    listing = out[out.find(BEGIN) :] if BEGIN in out else ""
    print(listing[:2000], flush=True)
    pull_captures(cfg)
    path = os.path.join(cfg.outdir, "raw-requests.log")
    requests = open(path, errors="replace").read() if os.path.exists(path) else ""
    print(
        "declared worktree tools:",
        sorted(set(re.findall(r'"name":"([a-z_]*worktree[a-z_]*)"', requests))),
        flush=True,
    )
    created = [
        line.split(" ", 1)[1]
        for line in listing.splitlines()
        if line.startswith("worktree ") and NAME in line
    ]
    check("worktree-tool-created", bool(created), f"created: {created}")
    check(
        "worktree-tool-in-data-dir",
        any(
            p.startswith("/work/home/.local/share/opencode/worktree/") for p in created
        ),
        f"{created}",
    )
