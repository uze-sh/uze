"""Observation experiment: how long does the contract's own Codex launch
take to hand back a ready prompt, and is the harness doing anything in it?

Runs the binding's `session` and `prepare` exactly as the contract does,
times `prepare`, and keeps the recording (`codex-contract.typescript` /
`.timing`) and the provider's request log. The recording answers the
question: a gap in it that ends with the Lab's own first keystroke is the
driver waiting on an idle harness, not the harness waiting on the network.

Measured 2026-10-09 in `/work` (no trust dialog), codex-cli 0.161.0 and
0.162.0: the prompt and "? for shortcuts" in the first frame (~3 s), the
model in the status line (~11 s), then a ~9 s pause and the welcome "To get
started, describe a task" (~20 s), the frame that says the session exists.
The provider was asked `GET /v1/responses` and one WebSocket and answered
both. `prepare` returned at 135.6 s while `drive_onboarding` waited for the
prompt in a third read that never came, and returns on the welcome since.

Run:
  python3 conformance/lab.py --harness codex --experiment codex/boot-stall
"""

import subprocess
import time

from harnesses.codex.bindings import CodexBindings
from shared import common


def run(cfg, prov_ip):
    prov_ip = common.start_provider(cfg, "static")
    time.sleep(1)
    bindings = CodexBindings()
    start = time.time()
    with bindings.session(cfg, prov_ip) as tui:
        _, matched = bindings.prepare(tui)
        took = time.time() - start
        print(f"[boot-stall] prepare returned at {took:.1f}s")
        subprocess.run(
            f"docker logs {cfg.prov_name} > {cfg.outdir}/provider.log 2>&1",
            shell=True,
        )
        common.check("boot-stall-ready", matched, f"prepare took {took:.1f}s")
