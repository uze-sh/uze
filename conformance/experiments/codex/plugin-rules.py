"""Can a Codex plugin carry rules that reach a session?

`project-rules` keeps a package's `rules/` out of every machine-wide
delivery, because a rule installed for the whole machine fires in every
project. Antigravity's plugins have a `rules/` directory; this asks whether
Codex's do (`plugin-rules` task 1.3), with plain `codex`, no UZE in the
loop.

A local Codex marketplace offers one plugin, `lab-rules`, installed with
`codex plugin marketplace add` and `codex plugin add`. Its
`.codex-plugin/plugin.json` declares `skills` (the control: the Skill's
description reaching the model proves the plugin is loaded) and an
undeclared `"rules": "./rules/"`. The plugin carries every shape a rule
could take:

- `rules/always.md`, Antigravity's `trigger: always_on` format;
- `rules/AGENTS.md` and a root `AGENTS.md`, instruction files;
- `rules/lab.rules`, Codex's own execpolicy format, forbidding
  `printf UZE_PR_EXEC_PLUGIN`.

The person's own `~/.codex/rules/default.rules` forbids
`printf UZE_PR_EXEC_USER`, the control showing execpolicy is enforced in
the Lab. One headless `codex exec` turn runs both commands through code
mode's `exec_command`; the judge reads the model requests for each marker
and each command's output for a refusal.

Run: python3 conformance/lab.py --harness codex --experiment codex/plugin-rules --discovery
"""

import json
import os
import shlex
import subprocess

from experiments import agent_context_probe as probe
from harnesses.codex.bindings import CodexBindings
from harnesses.codex.scenarios import codex_container
from shared import common

MARKET = "/work/lab-mk"
PLUGIN = f"{MARKET}/plugins/lab-rules"
PROJECT = "/work/pr-project"
PROMPT = "UZE_PR_TURN run the two commands"
MARKERS = {
    "skill": "UZE_PR_SKILL_DESC",
    "rule-md": "UZE_PR_RULE_MD",
    "rules-agents-md": "UZE_PR_RULES_AGENTS",
    "root-agents-md": "UZE_PR_ROOT_AGENTS",
}
PLUGIN_CMD = "printf UZE_PR_EXEC_PLUGIN"
USER_CMD = "printf UZE_PR_EXEC_USER"


def _write(path, text):
    return (
        f"mkdir -p {shlex.quote(os.path.dirname(path))} && "
        f"printf '%s' {shlex.quote(text)} > {shlex.quote(path)}"
    )


def _forbid(command):
    words = ", ".join(json.dumps(w) for w in command.split())
    return f'prefix_rule(pattern=[{words}], decision="forbidden")\n'


def prelude():
    market = {
        "name": "lab-mk",
        "interface": {"displayName": "Lab"},
        "plugins": [
            {
                "name": "lab-rules",
                "source": {"source": "local", "path": "./plugins/lab-rules"},
                "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
                "category": "Developer tools",
            }
        ],
    }
    manifest = {
        "name": "lab-rules",
        "version": "0.1.0",
        "description": "lab",
        "skills": "./skills/",
        "rules": "./rules/",
    }
    files = {
        f"{MARKET}/.agents/plugins/marketplace.json": json.dumps(market),
        f"{PLUGIN}/.codex-plugin/plugin.json": json.dumps(manifest),
        f"{PLUGIN}/skills/probe/SKILL.md": (
            f"---\nname: probe\ndescription: {MARKERS['skill']} lab probe\n---\n\nLab.\n"
        ),
        f"{PLUGIN}/rules/always.md": (
            f"---\ntrigger: always_on\n---\n\nLab convention {MARKERS['rule-md']}.\n"
        ),
        f"{PLUGIN}/rules/AGENTS.md": f"Lab convention {MARKERS['rules-agents-md']}.\n",
        f"{PLUGIN}/AGENTS.md": f"Lab convention {MARKERS['root-agents-md']}.\n",
        f"{PLUGIN}/rules/lab.rules": _forbid(PLUGIN_CMD),
        "/work/home/.codex/rules/default.rules": _forbid(USER_CMD),
    }
    lines = [_write(path, text) for path, text in files.items()]
    lines += [
        f"cd {MARKET} && git init -q -b main . && git add -A && "
        "git -c user.name=lab -c user.email=lab@uze.invalid commit -q -m init",
        f"codex plugin marketplace add {MARKET} 2>&1 | tail -n 3",
        "codex plugin add lab-rules@lab-mk 2>&1 | tail -n 3",
        "echo '=== plugin list'; codex plugin list --json 2>&1 | head -c 2000; echo",
        "echo '=== cache'; find /work/home/.codex/plugins -path '*lab-rules*' -type f 2>/dev/null",
        f"mkdir -p {PROJECT} && cd {PROJECT} && git init -q -b main .",
    ]
    return "\n".join(lines)


def _exec(command):
    return {
        "tool": "functions.exec",
        "args": (
            f"const r = await tools.exec_command({{cmd: {json.dumps(command)}}});\n"
            "text(JSON.stringify(r));"
        ),
    }


def job(world):
    mode, env = CodexBindings().sequence(
        [_exec(PLUGIN_CMD), _exec(USER_CMD)], "UZE_PR_TURN"
    )
    env = dict(env, DISCOVERY="1")
    prov_ip = common.start_provider(world, mode, env)
    final = f"""{prelude()}
set +e
echo '=== turn'
timeout 240 codex exec {shlex.quote(PROMPT)} 2>&1
"""
    proc = subprocess.run(
        codex_container(world, prov_ip, final, plugins="", tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=480,
    )
    return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}


def _outputs(reqs):
    """Every tool output the harness sent back, by call id."""
    found = {}
    for _, body, _ in reqs:
        for item in (body or {}).get("input", []) if isinstance(body, dict) else []:
            if isinstance(item, dict) and item.get("type", "").endswith("call_output"):
                found[item.get("call_id")] = json.dumps(item.get("output"))[:600]
    return found


def run(cfg, prov_ip):
    out = common.concurrently(cfg, {"plugin": job})["plugin"]
    with open(os.path.join(cfg.outdir, "plugin-rules.out"), "w") as f:
        f.write(out["out"])
    with open(os.path.join(cfg.outdir, "plugin-rules-raw-requests.log"), "w") as f:
        f.write(out["log"])
    reqs = probe.requests(out["log"])
    seen = {
        name: sorted(
            {
                f"#{i} {h['path']} (role={h['role']})"
                for i, (_, b, _) in enumerate(reqs)
                if b is not None
                for h in probe.locate(b, mark)
            }
        )
        for name, mark in MARKERS.items()
    }
    outputs = _outputs(reqs)
    result = {"markers": seen, "outputs": outputs}
    with open(os.path.join(cfg.outdir, "plugin-rules-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    probe.say(json.dumps(result)[:3000])
    common.check(
        "codex-plugin-loaded",
        bool(seen["skill"]),
        f"the plugin's Skill reached the model at {seen['skill'][:3]}",
    )
    for name in ("rule-md", "rules-agents-md", "root-agents-md"):
        common.check(
            f"codex-plugin-{name}-reaches-model",
            bool(seen[name]),
            f"{MARKERS[name]} at {seen[name][:3]}",
        )

    def refused(call):
        text = outputs.get(call, "").lower()
        return "forbidden" in text or "rejected" in text or "blocked" in text

    common.check(
        "codex-user-execpolicy-applies",
        refused("fc_uze_2"),
        f"{USER_CMD} (the person's rules, the control): {outputs.get('fc_uze_2')}",
    )
    common.check(
        "codex-plugin-execpolicy-applies",
        refused("fc_uze_1"),
        f"{PLUGIN_CMD} (the plugin's rules/lab.rules): {outputs.get('fc_uze_1')}",
    )
