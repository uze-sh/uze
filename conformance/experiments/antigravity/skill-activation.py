"""Exploration: how does Antigravity offer a skill to the model, and does
using one leave a trace a usage count can read?

A plain skill sits in the user's global skills directory, its description
and its body each carrying a nonce. One headless turn asks for what the
skill is for; the provider answers that turn by reading the skill's
`SKILL.md` with the file tool the request declares, the way the harness's
own instructions tell a model to activate a skill. No UZE in the loop: plain
`agy` with a global `PostToolUse` logger hook, so what is measured is the
harness.

What is measured, from the requests the harness sent (`--discovery`), the
hook's log and the conversation's own transcript:

- whether the skill's description reaches the model before any tool runs,
  and whether its body does (if it does, activation needs no tool and a
  count cannot see it);
- what the request's instructions say about using a skill;
- whether the read of `SKILL.md` reaches a `PostToolUse` hook, with the
  path in `toolCall.args`;
- whether the read is in `brain/<id>/.system_generated/logs/transcript.jsonl`
  as a `tool_calls` entry naming the path.

Finding (2026-10-31, 1.4.12): the system instruction lists every skill as
`- <name> (<path to SKILL.md>): <description>` under "Available skills",
with "If a skill seems relevant to your current task, you MUST read its
`SKILL.md` instructions before proceeding" and "always use the exact path
provided". The description reaches the model from the first request; the
body only in the request after the `read_file` of that exact path. The read
reaches a global `PostToolUse` hook (`toolCall.name` = `read_file`,
`toolCall.args.Path` = the path) and is written to
`brain/<id>/.system_generated/logs/transcript.jsonl` as a `tool_calls`
entry. So a skill used is a read of its listed path, visible to both a hook
and the transcript; one handed to a subagent is read by that subagent.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/skill-activation --discovery
"""

import json
import subprocess

from harnesses.antigravity.scenarios import agy_setup
from shared import common

SKILL = "probe-skill"
SKILL_DIR = f"/work/home/.gemini/antigravity-cli/skills/{SKILL}"
SKILL_FILE = f"{SKILL_DIR}/SKILL.md"
DESCRIPTION_MARK = "UZE_SKA_DESCRIPTION_9f2c"
BODY_MARK = "UZE_SKA_BODY_7a41"
HOOK_LOG = "/work/hooklog"
GLOBAL_HOOKS = "/work/home/.gemini/config/hooks.json"
#: The file tool and its declared argument names on 1.4.12
#: (evidence/tools/antigravity.json); `view_file` is the older name.
READ = "read_file"
ARGS = json.dumps({"Path": SKILL_FILE})

PRELUDE = f"""
mkdir -p {SKILL_DIR} /work/home/.gemini/config
cat > {SKILL_FILE} <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe report. {DESCRIPTION_MARK}
---

Write the probe report. {BODY_MARK}
EOF
cat > {GLOBAL_HOOKS} <<'EOF'
{{
  "usage-probe": {{
    "PostToolUse": [
      {{
        "matcher": "*",
        "hooks": [
          {{ "type": "command", "command": "cat >> {HOOK_LOG}; echo >> {HOOK_LOG}; printf '{{}}'" }}
        ]
      }}
    ]
  }}
}}
EOF
"""


def final_script():
    # decision: experiment-isolation
    return f"""
cd /work && mkdir -p project && cd project
agy --print "write the probe report" --output-format stream-json \\
  --dangerously-skip-permissions --print-timeout 90s --log-file /work/agy.log >/dev/null 2>&1 || true
echo '===== hook log ====='
cat {HOOK_LOG} 2>/dev/null | head -c 4000
echo
echo '===== transcripts ====='
for t in /work/home/.gemini/antigravity-cli/brain/*/.system_generated/logs/transcript.jsonl; do
  echo "== $t"; cat "$t" | head -c 4000; echo
done
echo '===== hooks loaded ====='
grep -n -i "hook" /work/agy.log | grep -v Migration | head -20
"""


def run(cfg, prov_ip):
    common.start_provider(
        cfg,
        "toolcall",
        {
            "TOOL_NAME": READ,
            "FC_ARGS": ARGS,
            "FINAL_TEXT": "UZE_SKA_DONE",
            "DISCOVERY": "1",
        },
    )
    setup = agy_setup(
        cfg,
        prov_ip,
        include_mcp=False,
        final_cmd=final_script(),
        plugins="",
        prelude=PRELUDE,
    )
    r = subprocess.run(
        common.docker_base(cfg, prov_ip, setup, tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=300,
    )
    out = r.stdout
    with open(f"{cfg.outdir}/skill_activation.out", "w") as f:
        f.write(out)
    print(out[-8000:], flush=True)
    if r.stderr:
        print(r.stderr[-1500:], flush=True)

    hook_log = out.split("===== hook log =====", 1)[-1].split(
        "===== transcripts =====", 1
    )[0]
    transcripts = out.split("===== transcripts =====", 1)[-1].split(
        "===== hooks loaded =====", 1
    )[0]
    common.check(
        "skill-read-reaches-post-tool-use-hook",
        READ in hook_log and SKILL_FILE in hook_log,
        "the hook was handed the read of SKILL.md with its path",
    )
    common.check(
        "skill-read-is-in-the-transcript",
        READ in transcripts and SKILL_FILE in transcripts,
        "transcript.jsonl names the read and its path",
    )
    common.check(
        "a-transcript-is-written",
        "transcript.jsonl" in transcripts,
        "a transcript was written for the conversation",
    )
