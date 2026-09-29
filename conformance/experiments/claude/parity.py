"""Parity experiment: a Claude Code plugin written to the vendor's own
documentation, against the same plugin written in UZE's format and
delivered by UZE.

The native twin is the answer key. Both twins (`_fixtures/parity/`) carry
the same behaviour: skills with scripts, references and a file outside
`skills/` reached through the plugin root, user-only and model-only
policies, a flat and a nested agent with vendor fields, a stdio MCP server
named through the root placeholder, deny/observe hooks around the shell,
and a stdio MCP server. Only what is common to the harnesses is compared:
Claude-only components (`bin/`, `commands/`) are out of the product's scope
(ADR-030) and the twins do not ship them. Both twins declare a SessionStart hook.

Each probe is one headless `claude -p` turn in a fresh container, so no
state crosses probes; the provider answers the turn with one scripted tool
call and records the raw requests. What Claude exposed to the model — tool
names, the agent types the Agent tool offers, the skills listed, the body a
skill or agent delivers, the model a subagent ran on — is read from those
requests, never from Claude's own UI and never from UZE's report.

Run: python3 conformance/lab.py --harness claude --experiment claude/parity
Env: PARITY_FORMS=native,uze  PARITY_PROBES=listing,skill,...
"""

import concurrent.futures
import json
import os
import re
import subprocess
import time

from harnesses.claude.scenarios import generate_certs
from shared import common

FORMS = os.environ.get("PARITY_FORMS", "native,uze,dual").split(",")
PROBES = {
    "listing": ("static", None, None),
    "skill": ("toolcall", "Skill", {"skill": "kit:probe"}),
    "manual": ("toolcall", "Skill", {"skill": "kit:manual"}),
    "hidden": ("toolcall", "Skill", {"skill": "kit:hidden"}),
    "agent": (
        "toolcall",
        "Agent",
        {
            "subagent_type": "kit:reviewer",
            "description": "parity",
            "prompt": "parity agent probe",
        },
    ),
    "agent-bare": (
        "toolcall",
        "Agent",
        {
            "subagent_type": "reviewer",
            "description": "parity",
            "prompt": "parity agent probe",
        },
    ),
    "agent-nested": (
        "toolcall",
        "Agent",
        {
            "subagent_type": "kit:review:security",
            "description": "parity",
            "prompt": "parity nested probe",
        },
    ),
    "agent-renamed": (
        "toolcall",
        "Agent",
        {
            "subagent_type": "kit:review:audit",
            "description": "parity",
            "prompt": "parity renamed probe",
        },
    ),
    "mcp": ("toolcall", "@mcp", {}),
    "plain": (
        "toolcall",
        "Bash",
        {"command": "echo PARITY_PLAIN_OK", "description": "parity plain"},
    ),
    "deny": (
        "toolcall",
        "Bash",
        {"command": "echo PARITY_DENY_ME", "description": "parity deny"},
    ),
}
SELECTED = os.environ.get("PARITY_PROBES", ",".join(PROBES)).split(",")
MARKERS = [
    "PARITY_SKILL_BODY_PROBE",
    "PARITY_SKILL_BODY_MANUAL",
    "PARITY_SKILL_BODY_HIDDEN",
    "PARITY_PHASE_FILE",
    "PARITY_SKILL_SCRIPT_OK",
    "PARITY_AGENT_BODY_REVIEWER",
    "PARITY_AGENT_BODY_SECURITY",
    "PARITY_AGENT_BODY_AUDIT",
    "PARITY_MCP_PROOF",
    "PARITY_HOOK_DENIED",
    "${CLAUDE_PLUGIN_ROOT}",
    "${PLUGIN_ROOT}",
]

GIT = (
    "git -c init.defaultBranch=main -c user.name='UZE Lab' "
    "-c user.email=lab@uze.invalid -c commit.gpgsign=false"
)


def install(form):
    if form == "native":
        return """
cp -r /opt/parity/native /work/native
echo '=== install'
claude plugin marketplace add /work/native 2>&1
claude plugin install kit@parity-native 2>&1
"""
    if form == "dual":
        # The migration report's first round: the author's own Claude plugin,
        # with UZE's manifests beside it, installed through UZE.
        return f"""
cp -r /opt/parity/native /work/pm
echo '{{"name": "parity-dual", "plugins": [{{"name": "kit", "source": "./plugins/kit"}}]}}' > /work/pm/marketplace.json
echo '{{"name": "kit", "version": "1.0.0", "description": "Parity reference plugin"}}' > /work/pm/plugins/kit/plugin.json
{GIT} -C /work/pm init -q && {GIT} -C /work/pm add -A && {GIT} -C /work/pm commit -q -m parity
echo '=== install'
uze market add /work/pm 2>&1
uze install kit@parity-dual -m 2>&1
echo '=== uze-inspect'
uze inspect kit 2>&1
"""
    return f"""
cp -r /opt/parity/uze /work/pm
{GIT} -C /work/pm init -q && {GIT} -C /work/pm add -A && {GIT} -C /work/pm commit -q -m parity
echo '=== install'
uze market add /work/pm 2>&1
uze install kit@parity-uze -m 2>&1
echo '=== uze-inspect'
uze inspect kit 2>&1
"""


INVENTORY = """
echo '=== plugin-list'
claude plugin list --json 2>&1
echo '=== cache'
find /work/home/.claude/plugins/cache -printf '%y %P -> %l\\n' 2>/dev/null | sort
echo '=== validate'
for d in /work/home/.claude/plugins/cache/*/kit/*; do claude plugin validate "$d" 2>&1; done
echo '=== loose'
find /work/home/.claude/agents /work/home/.claude/skills -maxdepth 1 -printf '%y %P -> %l\\n' 2>/dev/null | sort
echo '=== settings'
cat /work/home/.claude/settings.json 2>&1
echo '=== user-mcp'
grep -o '"mcpServers":{[^}]*}[^}]*}' /work/home/.claude.json 2>&1
"""


def container(cfg, prov_ip, form, prompt, inventory):
    script = f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CLAUDE_CONFIG_DIR=/work/home/.claude UZE_HOME=/work/home/.uze
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
mkdir -p /work/home/.claude
cp /app/fixtures/claude.json /work/home/.claude.json
cd /work && mkdir -p proj && cd proj
{install(form)}
set +e
{INVENTORY if inventory else ""}
echo '=== turn'
timeout 150 claude -p {json.dumps(prompt)} --permission-mode bypassPermissions --output-format json 2>&1
echo "=== turn-exit $?"
echo '=== markers'
ls /work/parity-*.marker 2>/dev/null
"""
    cmd = common.docker_base(cfg, prov_ip, script, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    parity = os.path.join(cfg.repo, "_fixtures", "parity")
    return (
        cmd[:i]
        + ["-v", f"{ca_crt}:/app/ca.crt:ro", "-v", f"{parity}:/opt/parity:ro"]
        + cmd[i:]
    )


def raw_requests(cfg):
    out = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
    ).stdout
    bodies = []
    for block in re.split(r"^### ", out, flags=re.M)[1:]:
        if not block.startswith("POST /v1/messages"):
            continue
        start = block.find("\n{")
        try:
            bodies.append(json.loads(block[start:].strip()))
        except ValueError:
            pass
    return bodies


def describe(bodies):
    """What the model was shown, per request."""
    rows = []
    for b in bodies:
        tools = {t.get("name"): t for t in b.get("tools", [])}
        agent_desc = (tools.get("Agent") or tools.get("Task") or {}).get(
            "description", ""
        )
        whole = json.dumps(b)
        rest = whole.replace(json.dumps(agent_desc)[1:-1], "")
        results = []
        for m in b.get("messages", []):
            for c in m.get("content", []) if isinstance(m.get("content"), list) else []:
                if c.get("type") == "tool_result":
                    content = c.get("content")
                    text = content if isinstance(content, str) else json.dumps(content)
                    results.append(text[:1500])
        rows.append(
            {
                "model": b.get("model"),
                "tools": sorted(t for t in tools if t),
                "mcp_tools": sorted(t for t in tools if t and t.startswith("mcp__")),
                "agent_types": sorted(
                    set(re.findall(r"^- ([\w:.-]+):", agent_desc, re.M))
                ),
                "kit_names": sorted(set(re.findall(r"kit:[a-z:-]+", rest))),
                "markers": [m for m in MARKERS if m in whole],
                "mcp_names": sorted(set(re.findall(r"mcp__[A-Za-z0-9_-]+", whole))),
                "body_values": sorted(
                    set(re.findall(r"(?:root|phase|script)=[^\\\"]{0,160}", whole))
                ),
                "tool_results": results,
                "system_head": json.dumps(b.get("system"))[:300],
            }
        )
    return rows


def sections(stdout):
    out, name = {}, "pre"
    for line in stdout.splitlines():
        if line.startswith("=== "):
            name = line[4:].strip()
            out[name] = []
        else:
            out.setdefault(name, []).append(line)
    return {k: "\n".join(v) for k, v in out.items()}


#: Probes running at once. Each has its own network and provider, so they
#: share nothing but the host; the listing probes go first because the MCP
#: probe calls the tool its form's listing found.
JOBS = int(os.environ.get("PARITY_JOBS", "4"))


def probe_once(cfg, form, probe, tool):
    """One probe in a world of its own: provider, headless turn, wire."""
    mode, _, args = PROBES[probe]
    env = {
        "DISCOVERY": "1",
        "RESPONSE_TEXT": "PARITY_DONE",
        "FINAL_TEXT": "PARITY_DONE",
    }
    if tool:
        env.update({"TOOL_NAME": tool, "TOOL_ARGS": json.dumps(args)})
    with common.isolated_world(cfg, f"{form}-{probe}") as world:
        prov_ip = common.start_provider(world, mode, env)
        time.sleep(1)
        cmd = container(
            world, prov_ip, form, f"parity {probe}", inventory=probe == "listing"
        )
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=400)
        rows = describe(raw_requests(world))
    result = {"container": sections(proc.stdout + proc.stderr), "requests": rows}
    with open(os.path.join(cfg.outdir, f"parity-{form}-{probe}.json"), "w") as f:
        json.dump(result, f, indent=1)
    print(f"[parity] {form}/{probe}: {len(rows)} requests", flush=True)
    return result


def run(cfg, prov_ip):
    common.generate_certs(cfg)
    report = {form: {} for form in FORMS}
    mcp_tool = {}
    first = [(form, "listing") for form in FORMS if "listing" in SELECTED]
    rest = [(form, probe) for form in FORMS for probe in SELECTED if probe != "listing"]
    with concurrent.futures.ThreadPoolExecutor(max_workers=JOBS) as pool:
        for (form, probe), result in zip(
            first,
            pool.map(lambda job: probe_once(cfg, *job, PROBES[job[1]][1]), first),
        ):
            report[form][probe] = result
            listed = [
                t
                for r in result["requests"]
                for t in r["mcp_names"]
                if t.endswith("uze_conformance")
            ]
            if listed:
                mcp_tool[form] = listed[0]
        runnable = []
        for form, probe in rest:
            tool = PROBES[probe][1]
            if tool == "@mcp":
                tool = mcp_tool.get(form)
                if not tool:
                    report[form][probe] = {"skipped": "no MCP tool listed"}
                    continue
            runnable.append((form, probe, tool))
        for (form, probe, _), result in zip(
            runnable, pool.map(lambda job: probe_once(cfg, *job), runnable)
        ):
            report[form][probe] = result
    with open(os.path.join(cfg.outdir, "parity.json"), "w") as f:
        json.dump(report, f, indent=1)
    for form in [form for form in FORMS if form != "native" and "native" in FORMS]:
        for name, native, delivered in compare(report, form):
            check = f"parity-{name}" if form == "uze" else f"parity-{form}-{name}"
            common.check(
                check, native == delivered, f"native: {native} | {form}: {delivered}"
            )


def observed(report):
    """Per form, the facts a session exposes, read from the wire."""

    def requests(form, probe):
        return report[form].get(probe, {}).get("requests", [])

    def seen(form, probe, marker):
        return any(marker in r["markers"] for r in requests(form, probe))

    def results(form, probe):
        return " ".join(t for r in requests(form, probe) for t in r["tool_results"])

    def subagent(form, probe):
        for r in requests(form, probe):
            if any(m.startswith("PARITY_AGENT_BODY") for m in r["markers"]):
                return f"ran on {r['model']}"
        return "not found" if "not found" in results(form, probe) else "not run"

    def container(form, probe, part):
        return report[form].get(probe, {}).get("container", {}).get(part, "")

    def root(form):
        values = [
            v
            for r in requests(form, "skill")
            for v in r["body_values"]
            if v.startswith("root=")
        ]
        if not values:
            return "no body"
        return "literal" if "PLUGIN_ROOT}" in values[0] else "resolved"

    facts = {}
    for form in report:
        listing = requests(form, "listing")
        names = set(listing[0]["kit_names"]) if listing else set()
        facts[form] = {
            "skill-listed-probe": "kit:probe" in " ".join(names),
            "skill-listed-hidden": "kit:hidden" in " ".join(names),
            "skill-body": seen(form, "skill", "PARITY_SKILL_BODY_PROBE"),
            "skill-root-placeholder": root(form),
            "skill-user-only-refused": "disable-model-invocation"
            in results(form, "manual"),
            "skill-model-only-body": seen(form, "hidden", "PARITY_SKILL_BODY_HIDDEN"),
            "agent-qualified": subagent(form, "agent"),
            "agent-bare-name": subagent(form, "agent-bare"),
            "agent-nested-qualified": subagent(form, "agent-nested"),
            "agent-frontmatter-name": subagent(form, "agent-renamed"),
            "mcp-tool-names": sorted({n for r in listing for n in r["mcp_names"]}),
            "mcp-call": seen(form, "mcp", "PARITY_MCP_PROOF"),
            "hook-pre-tool-deny": seen(form, "deny", "PARITY_HOOK_DENIED"),
            "hook-post-tool": "post-tool" in container(form, "plain", "markers"),
            "hook-session-start": "session-start"
            in container(form, "listing", "markers"),
            "cache-complete": "skills/probe/SKILL.md"
            in container(form, "listing", "cache"),
        }
    return facts


def compare(report, form):
    facts = observed(report)
    return [(k, facts["native"][k], facts[form][k]) for k in facts["native"]]
