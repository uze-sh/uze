"""Proof experiment: a real marketplace, delivered by UZE, as Claude Code sees it.

Points at a marketplace outside the repository (`PROOF_MARKET`, a directory
holding `marketplace.json`), installs every plugin it lists through `uze`
into the real Claude Code (synthetic provider, no tokens, isolated), and
compares what the session exposes to the model with what the marketplace's
own files declare:

- every skill offered as `<plugin>:<skill>` (a user-only skill excepted);
- every agent offered as `<plugin>:<subdirs>:<name>`, read from the list
  Claude itself answers with when asked for an agent it does not know;
- every MCP server's tools under `mcp__plugin_<plugin>_<server>__`;
- `PROOF_SKILL` (a skill whose body names `${PLUGIN_ROOT}/...`) delivered
  with the placeholder resolved to a path that exists;
- `PROOF_AGENT` dispatched by its qualified label, its body reaching the
  model on the model its frontmatter declares.

Nothing is read from UZE's own report: the expectation comes from the
marketplace files, the observation from the wire.

Run: PROOF_MARKET=/path/to/market PROOF_SKILL=forge:run \\
     PROOF_AGENT=flow:review-orchestrator \\
     python3 conformance/lab.py --harness claude --experiment claude/marketplace_proof
"""

import json
import os
import re
import subprocess
import time

from experiments.claude.parity import raw_requests, sections
from harnesses.claude.scenarios import generate_certs
from shared import common

MARKET = os.environ.get("PROOF_MARKET", "")
PROOF_SKILL = os.environ.get("PROOF_SKILL", "")
PROOF_AGENT = os.environ.get("PROOF_AGENT", "")
MODEL_IDS = {"haiku": "haiku", "sonnet": "sonnet", "opus": "opus"}

GIT = (
    "git -c init.defaultBranch=main -c user.name='UZE Lab' "
    "-c user.email=lab@uze.invalid -c commit.gpgsign=false"
)


def frontmatter(path):
    text = open(path, encoding="utf-8").read()
    match = re.match(r"^---\n(.*?)\n---\n", text, re.S)
    fields = {}
    for line in (match.group(1) if match else "").splitlines():
        key, _, value = line.partition(":")
        if value and not line.startswith(" "):
            fields[key.strip()] = value.strip().strip('"')
    return fields, text


def expected():
    """What the marketplace's own files declare, per capability."""
    manifest = json.load(open(os.path.join(MARKET, "marketplace.json")))
    skills, agents, servers = set(), {}, set()
    for plugin in manifest["plugins"]:
        name = plugin["name"]
        root = os.path.normpath(os.path.join(MARKET, plugin["source"]))
        for skill_dir in (
            sorted(os.listdir(os.path.join(root, "skills")))
            if os.path.isdir(os.path.join(root, "skills"))
            else []
        ):
            fields, text = frontmatter(
                os.path.join(root, "skills", skill_dir, "SKILL.md")
            )
            if "model: false" not in text:
                skills.add(f"{name}:{fields.get('name', skill_dir)}")
        agents_root = os.path.join(root, "agents")
        for dirpath, _, files in os.walk(agents_root):
            for file in files:
                if not file.endswith(".md"):
                    continue
                fields, _ = frontmatter(os.path.join(dirpath, file))
                subdirs = os.path.relpath(dirpath, agents_root)
                parts = [] if subdirs == "." else subdirs.split(os.sep)
                label = ":".join([name, *parts, fields.get("name", file[:-3])])
                agents[label] = fields.get("model")
        mcp = os.path.join(root, "mcp.json")
        if os.path.isfile(mcp):
            for server in json.load(open(mcp)).get("mcpServers", {}):
                servers.add(f"mcp__plugin_{name}_{server}__")
    return manifest, skills, agents, servers


def container(cfg, prov_ip, manifest, prompt):
    installs = "\n".join(
        f'uze install {plugin["name"]}@{manifest["name"]} -m 2>&1; echo "install-exit {plugin["name"]} $?"'
        for plugin in manifest["plugins"]
    )
    script = f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CLAUDE_CONFIG_DIR=/work/home/.claude UZE_HOME=/work/home/.uze
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
mkdir -p /work/home/.claude /work/proj
cp /app/fixtures/claude.json /work/home/.claude.json
cp -r /opt/proof /work/pm
{GIT} -C /work/pm init -q && {GIT} -C /work/pm add -A && {GIT} -C /work/pm commit -q -m proof
set +e
echo '=== install'
uze market add /work/pm 2>&1
{installs}
echo '=== agents-dir'
ls -la /work/home/.claude/agents 2>&1
echo '=== store'
find /work/home/.uze/store/plugins -maxdepth 3 2>&1 | sort
cd /work/proj
echo '=== turn'
# decision: experiment-isolation
timeout 150 claude -p {json.dumps(prompt)} --permission-mode bypassPermissions --output-format json 2>&1
echo "=== turn-exit $?"
"""
    cmd = common.docker_base(cfg, prov_ip, script, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    return (
        cmd[:i]
        + ["-v", f"{ca_crt}:/app/ca.crt:ro", "-v", f"{MARKET}:/opt/proof:ro"]
        + cmd[i:]
    )


def turn(cfg, prov_ip, manifest, name, mode, tool=None, args=None):
    env = {"DISCOVERY": "1", "RESPONSE_TEXT": "PROOF_DONE", "FINAL_TEXT": "PROOF_DONE"}
    if tool:
        env.update({"TOOL_NAME": tool, "TOOL_ARGS": json.dumps(args)})
    prov_ip = common.start_provider(cfg, mode, env) or prov_ip
    time.sleep(1)
    proc = subprocess.run(
        container(cfg, prov_ip, manifest, f"proof {name}"),
        capture_output=True,
        text=True,
        timeout=600,
    )
    bodies = raw_requests(cfg)
    parts = sections(proc.stdout + proc.stderr)
    with open(os.path.join(cfg.outdir, f"proof-{name}.json"), "w") as f:
        json.dump({"container": parts, "requests": bodies}, f, indent=1)
    return parts, bodies


def results(bodies):
    out = []
    for body in bodies:
        for message in body.get("messages", []):
            content = message.get("content")
            for block in content if isinstance(content, list) else []:
                if block.get("type") == "tool_result":
                    value = block.get("content")
                    out.append(value if isinstance(value, str) else json.dumps(value))
    return "\n".join(out)


def run(cfg, prov_ip):
    given = bool(MARKET) and os.path.isfile(os.path.join(MARKET, "marketplace.json"))
    common.check(
        "proof-market-given",
        given,
        f"PROOF_MARKET={MARKET!r}" + ("" if given else " has no marketplace.json"),
    )
    if not given:
        return
    manifest, skills, agents, servers = expected()

    parts, bodies = turn(cfg, prov_ip, manifest, "listing", "static")
    install = parts.get("install", "")
    failed = [
        line
        for line in install.splitlines()
        if line.startswith("install-exit") and not line.endswith(" 0")
    ]
    common.check(
        "proof-every-plugin-installs",
        not failed,
        "; ".join(failed) or f"{len(manifest['plugins'])} installed",
    )
    listing = json.dumps(bodies[0]) if bodies else ""
    offered_skills = {name for name in skills if name in listing}
    common.check(
        "proof-skills-offered-by-label",
        offered_skills == skills,
        f"missing: {sorted(skills - offered_skills)}"
        if offered_skills != skills
        else f"{len(skills)} skills",
    )
    offered_tools = {prefix for prefix in servers if prefix in listing}
    common.check(
        "proof-mcp-tools-offered",
        offered_tools == servers,
        f"missing: {sorted(servers - offered_tools)}"
        if offered_tools != servers
        else f"{sorted(servers)}",
    )

    _, bodies = turn(
        cfg,
        prov_ip,
        manifest,
        "roster",
        "toolcall",
        "Agent",
        {"subagent_type": "proof:none", "description": "proof", "prompt": "roster"},
    )
    answer = results(bodies)
    listed = answer.split("Available agents:")[-1].split("</tool_use_error>")[0]
    names = {name.strip().rstrip(".") for name in listed.split(",") if name.strip()}
    missing = set(agents) - names
    bare = {label.rsplit(":", 1)[-1] for label in agents} & names
    common.check(
        "proof-agents-offered-by-label",
        not missing,
        f"missing: {sorted(missing)}" if missing else f"{len(agents)} agents",
    )
    common.check(
        "proof-no-agent-offered-by-bare-name",
        not bare,
        f"bare: {sorted(bare)}" if bare else "none",
    )

    if PROOF_SKILL:
        _, bodies = turn(
            cfg, prov_ip, manifest, "skill", "toolcall", "Skill", {"skill": PROOF_SKILL}
        )
        delivered = [json.dumps(body) for body in bodies[1:]]
        text = "\n".join(delivered)
        common.check(
            "proof-skill-body-delivered",
            bool(delivered) and PROOF_SKILL.split(":")[-1] in text,
            PROOF_SKILL,
        )
        common.check(
            "proof-skill-root-resolved",
            "${PLUGIN_ROOT}" not in text and "${CLAUDE_PLUGIN_ROOT}" not in text,
            "no placeholder reached the model",
        )

    if PROOF_AGENT:
        _, bodies = turn(
            cfg,
            prov_ip,
            manifest,
            "agent",
            "toolcall",
            "Agent",
            {
                "subagent_type": PROOF_AGENT,
                "description": "proof",
                "prompt": "proof agent",
            },
        )
        declared = agents.get(PROOF_AGENT)
        ran = [body["model"] for body in bodies[1:] if body.get("model")]
        answer = results(bodies)
        common.check(
            "proof-agent-dispatched-by-label",
            "not found" not in answer and len(bodies) > 2,
            answer[:160] or "ran",
        )
        if declared:
            common.check(
                "proof-agent-runs-on-its-model",
                any(MODEL_IDS.get(declared, declared) in model for model in ran),
                f"declared {declared}, requests on {sorted(set(ran))}",
            )
