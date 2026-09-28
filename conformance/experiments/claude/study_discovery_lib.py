"""Shared fragments for the `study_discovery` experiments (one per vendor).

The question: which discovery locations each harness reads (skills, agents,
MCP, hooks), and which frontmatter it tolerates, measured with no UZE in
the loop — every file is written by hand inside the container, then one
headless turn is run and the raw requests are read. Nothing here asserts:
the facts are read off `study.json` and `requests.log`.

Each vendor module supplies a `turn(tag)` shell fragment and its own
container; the cases below are shared so that every harness is asked the
same question. Select with `STUDY_CASE=<case>` (see `CASES`).
"""

import json
import os
import re

from shared import common

H = "/work/home"
P = "/work/proj"

# Every candidate skill directory, across all four vendors: each harness is
# offered all of them and the wire says which it read.
SKILL_DIRS = {
    "agents-global": f"{H}/.agents/skills",
    "agents-project": f"{P}/.agents/skills",
    "agents-subdir-parent": f"{P}/.agents/skills-unused",  # placeholder, never written
    "claude-global": f"{H}/.claude/skills",
    "claude-project": f"{P}/.claude/skills",
    "codex-global": f"{H}/.codex/skills",
    "codex-project": f"{P}/.codex/skills",
    "opencode-global-skill": f"{H}/.config/opencode/skill",
    "opencode-global-skills": f"{H}/.config/opencode/skills",
    "opencode-project-skill": f"{P}/.opencode/skill",
    "opencode-project-skills": f"{P}/.opencode/skills",
    "gemini-global": f"{H}/.gemini/skills",
    "gemini-project": f"{P}/.gemini/skills",
    "gemini-config-global": f"{H}/.gemini/config/skills",
    "agycli-global": f"{H}/.gemini/antigravity-cli/skills",
    "antigravity-global": f"{H}/.gemini/antigravity/skills",
    "agent-project": f"{P}/.agent/skills",
    "agent-global": f"{H}/.agent/skills",
}
del SKILL_DIRS["agents-subdir-parent"]

# Candidate agent directories (markdown agents; Codex gets a TOML twin).
AGENT_DIRS = {
    "agents-global": f"{H}/.agents/agents",
    "agents-project": f"{P}/.agents/agents",
    "claude-project": f"{P}/.claude/agents",
    "codex-project": f"{P}/.codex/agents",
    "opencode-project-agent": f"{P}/.opencode/agent",
    "opencode-project-agents": f"{P}/.opencode/agents",
    "gemini-project": f"{P}/.gemini/agents",
    "gemini-config-project": f"{P}/.gemini/config/agents",
    "agent-project": f"{P}/.agent/agents",
    "agents-global-nested": f"{H}/.agents/agents/sub",
    "agent-global": f"{H}/.agent/agents",
    "agents-project-plugin": f"{P}/.agents/plugins/studyplug/agents",
    # Positive controls: each vendor's known global directory.
    "ctl-claude": f"{H}/.claude/agents",
    "ctl-codex": f"{H}/.codex/agents",
    "ctl-opencode": f"{H}/.config/opencode/agents",
    "ctl-agy": f"{H}/.gemini/config/agents",
}

PRELUDE = f"""
export HOME={H}
mkdir -p {P}
cd {P}
git -c init.defaultBranch=main init -q . 2>/dev/null || true
mkskill() {{ # dir name description [extra frontmatter lines]
  mkdir -p "$1/$2"
  {{ echo '---'; echo "name: $2"; echo "description: $3"; [ -n "$4" ] && printf '%b\\n' "$4"; echo '---'; echo "Body of $2: STUDYBODY_$(echo "$2" | tr -c 'A-Za-z0-9\\n' _)"; }} > "$1/$2/SKILL.md"
}}
inventory() {{
  echo "=== inventory $1"
  find {H} {P} \\( -path '*/.uze' -o -path '*/.git' -o -path '*/node_modules' -o -path '*/.cache' -o -path '*/.local' \\) -prune -o \\( -name SKILL.md -o -path '*agents/*' -o -name '*.toml' -o -name '*.json' \\) -print 2>/dev/null | sort
}}
set +e
"""


def _skill_locations():
    lines = []
    for loc, d in SKILL_DIRS.items():
        lines.append(f"mkskill {d} loc-{loc} STUDYLOC_{loc.replace('-', '_')}")
    return "\n".join(lines)


def _skill_dups():
    lines = []
    for loc, d in SKILL_DIRS.items():
        lines.append(f"mkskill {d} study-dup STUDYDUP_{loc.replace('-', '_')}")
    return "\n".join(lines)


PAIRS = [
    ("agents-global", "agents-project"),
    ("agents-project", "claude-project"),
    ("agents-global", "claude-global"),
    ("agents-global", "claude-project"),
    ("agents-project", "claude-global"),
    ("agents-project", "codex-project"),
    ("agents-global", "codex-global"),
    ("agents-project", "opencode-project-skills"),
    ("agents-global", "opencode-global-skills"),
    ("opencode-global-skills", "opencode-project-skills"),
    ("claude-project", "opencode-project-skills"),
    ("claude-global", "claude-project"),
    ("agents-project", "agent-project"),
    ("agents-project", "gemini-config-global"),
]


def _skill_pairs():
    """One skill name per pair of directories, written to both; the
    description the model sees names the winner."""
    lines = []
    for i, (a, b) in enumerate(PAIRS):
        for loc in (a, b):
            lines.append(
                f"mkskill {SKILL_DIRS[loc]} pp{i:02d} STUDYDUP_pp{i:02d}_{loc.replace('-', '_')}"
            )
    return "\n".join(lines)


# Frontmatter variants, each its own skill so a drop is attributable.
FRONTMATTER = {
    "ft-plain": "",
    "ft-invoke": "invoke:\n  model: false\n  user: true",
    "ft-harness": "harness:\n  claude-code:\n    model: haiku\n  codex:\n    x: 1",
    "ft-meta-strings": 'metadata:\n  codex/x: "y"\n  uze/origin: "kit"',
    "ft-meta-nonstring": "metadata:\n  count: 3\n  flag: true\n  nested:\n    a: 1",
    "ft-meta-ocauto": 'metadata:\n  opencode/autoinvoke: "false"',
    "ft-allowed": "allowed-tools: Read Grep",
    "ft-dmi": "disable-model-invocation: true",
    "ft-userinv": "user-invocable: false",
    "ft-license": "license: MIT\ncompatibility: any",
}


def _frontmatter(d):
    lines = [
        f"mkskill {d} {name} STUDYFT_{name.replace('-', '_')} {json.dumps(extra)}"
        for name, extra in FRONTMATTER.items()
    ]
    # A name with `:` — directory equal to the name, and a colon-free
    # directory holding a colon name (the name/dir mismatch case).
    lines.append(f"mkskill {d} 'kit:colon' STUDYFT_colon_samedir")
    lines.append(
        f"mkdir -p {d}/kit-colon && printf -- '---\\nname: kit:colondir\\ndescription: STUDYFT_colon_otherdir\\n---\\nBody STUDYBODY_colondir\\n' > {d}/kit-colon/SKILL.md"
    )
    lines.append(
        f"mkdir -p {d}/ft-mismatch && printf -- '---\\nname: other-name\\ndescription: STUDYFT_mismatch\\n---\\nBody STUDYBODY_mismatch\\n' > {d}/ft-mismatch/SKILL.md"
    )
    return "\n".join(lines)


def _agent(d, name, fmt, extra=""):
    if fmt == "toml":
        return (
            f'mkdir -p {d} && printf \'name = "{name}"\\ndescription = "STUDYAG_{name.replace("-", "_")}"\\n'
            f'developer_instructions = "Body STUDYAGBODY_{name.replace("-", "_")}"\\n\' > {d}/{name}.toml'
        )
    return (
        f"mkdir -p {d} && printf -- '---\\nname: {name}\\ndescription: STUDYAG_{name.replace('-', '_')}\\n"
        f"mode: subagent\\n{extra}---\\nBody STUDYAGBODY_{name.replace('-', '_')}\\n' > {d}/{name}.md"
    )


def _agent_locations():
    lines = [
        f'mkdir -p {P}/.agents/plugins/studyplug && echo \'{{"name": "studyplug"}}\' > {P}/.agents/plugins/studyplug/plugin.json'
    ]
    for loc, d in AGENT_DIRS.items():
        lines.append(_agent(d, f"ag-{loc}", "md"))
        lines.append(_agent(d, f"agt-{loc}", "toml"))
    return "\n".join(lines)


AGENT_FRONTMATTER = {
    "agf-plain": "",
    "agf-harness": "harness:\\n  claude-code:\\n    model: haiku\\n  opencode:\\n    temperature: 0.1\\n",
    "agf-invoke": "invoke:\\n  model: false\\n  user: true\\n",
    "agf-list": "labels:\\n  - a\\n  - b\\n",
}


def _agent_frontmatter(d):
    return "\n".join(_agent(d, n, "md", e) for n, e in AGENT_FRONTMATTER.items())


MCP_BIN = "/usr/local/bin/uze-mcp-conformance-fixture"


def _server(name):
    return {"command": MCP_BIN, "args": ["--proof", f"STUDYMCP_PROOF_{name}"]}


def _json(path, doc):
    return f"mkdir -p $(dirname {path}) && cat > {path} <<'JSON_EOF'\n{json.dumps(doc, indent=1)}\nJSON_EOF"


def _claude_hooks(mark):
    h = [{"hooks": [{"type": "command", "command": f"touch /work/hm-{mark}"}]}]
    return {"hooks": {"UserPromptSubmit": h, "SessionStart": h}}


def _agy_hooks(mark):
    h = [{"type": "command", "command": f"touch /work/hm-{mark}"}]
    return {"study": {"PreInvocation": h, "Stop": h}}


OPENCODE_PLUGIN = """import fs from "fs";
fs.writeFileSync("/work/hm-{mark}-imported", "x");
export default {{
  id: "study-{mark}",
  async setup(ctx) {{ fs.writeFileSync("/work/hm-{mark}", "x"); }},
}};
"""


def _mcp_hooks():
    """Every candidate project-level (and a few shared global) MCP and hook
    location, each naming its own server/marker, in its native format."""
    mcp = lambda n: {"mcpServers": {f"studymcp_{n}": _server(n)}}  # noqa: E731
    parts = [
        _json(f"{P}/.mcp.json", mcp("dotmcp")),
        _json(f"{P}/.gemini/settings.json", mcp("geminiproj")),
        _json(f"{P}/.agents/mcp_config.json", mcp("agentsroot")),
        _json(f"{P}/.agent/mcp_config.json", mcp("agentroot")),
        _json(f"{P}/.agents/mcp.json", mcp("agentsmcpjson")),
        _json(f"{H}/.agents/mcp_config.json", mcp("agentsglobal")),
        _json(f"{H}/.agents/mcp.json", mcp("agentsglobalmcpjson")),
        _json(
            f"{P}/opencode.json",
            {
                "mcp": {
                    "studymcp_ocroot": {
                        "type": "local",
                        "command": [MCP_BIN, "--proof", "x"],
                    }
                }
            },
        ),
        _json(
            f"{P}/.opencode/opencode.json",
            {
                "mcp": {
                    "studymcp_ocdir": {
                        "type": "local",
                        "command": [MCP_BIN, "--proof", "x"],
                    }
                }
            },
        ),
        f'mkdir -p {P}/.codex && printf \'[mcp_servers.studymcp_codexproj]\\ncommand = "{MCP_BIN}"\\nargs = ["--proof", "x"]\\n\' > {P}/.codex/config.toml',
        _json(f"{P}/.claude/settings.json", _claude_hooks("claude-settings")),
        _json(
            f"{P}/.claude/settings.local.json", _claude_hooks("claude-settings-local")
        ),
        _json(f"{P}/.codex/hooks.json", _claude_hooks("codex-hooks")),
        _json(f"{P}/.agents/hooks.json", _agy_hooks("agents-hooks")),
        _json(f"{P}/.agent/hooks.json", _agy_hooks("agent-hooks")),
        _json(f"{H}/.agents/hooks.json", _agy_hooks("agents-global-hooks")),
        _json(f"{P}/.gemini/hooks.json", _agy_hooks("gemini-proj-hooks")),
        f"mkdir -p {P}/.opencode/plugin {P}/.opencode/plugins {P}/.agents/plugins",
        f"cat > {P}/.opencode/plugin/study.js <<'JS_EOF'\n{OPENCODE_PLUGIN.format(mark='oc-plugin')}JS_EOF",
        f"cat > {P}/.opencode/plugins/study.js <<'JS_EOF'\n{OPENCODE_PLUGIN.format(mark='oc-plugins')}JS_EOF",
        f"mkdir -p {H}/.config/opencode/plugins && cat > {H}/.config/opencode/plugins/study.js <<'JS_EOF'\n{OPENCODE_PLUGIN.format(mark='oc-global-plugins')}JS_EOF",
    ]
    return "\n".join(parts)


def case_script(case, turn, vendor_skill_dir, vendor_agent_dir, extra=""):
    """The probe for `case`; `turn(tag)` is the vendor's headless turn."""
    body = {
        "skills-loc": _skill_locations()
        + "\ninventory before\n"
        + turn("LOC")
        + "\n"
        + _skill_dups()
        + "\n"
        + turn("DUP"),
        "skills-prec": _skill_pairs() + "\n" + turn("PREC"),
        "skills-ft": _frontmatter(vendor_skill_dir) + "\ninventory ft\n" + turn("FT"),
        "agents-loc": _agent_locations() + "\ninventory agents\n" + turn("AGLOC"),
        "agents-ft": _agent_frontmatter(vendor_agent_dir) + "\n" + turn("AGFT"),
        "mcp-hooks": _mcp_hooks()
        + "\ninventory mcp\n"
        + turn("MCP")
        + "\necho '=== hook-markers'; ls /work/hm-* 2>&1",
    }[case]
    return PRELUDE + extra + "\n" + body


def turn_marker(tag):
    return f"STUDY_TURN_{tag}"


MARK = re.compile(
    r"STUDY(?:LOC|DUP|FT|BODY|AG|AGBODY|MCP)_[A-Za-z0-9_]+|studymcp_[a-z]+"
)


def analyze(outdir):
    """Per turn (identified by the prompt marker), the markers the model saw."""
    path = os.path.join(outdir, "requests.log")
    try:
        text = open(path, errors="replace").read()
    except OSError:
        return {}
    turns = {}
    for block in re.split(r"^### ", text, flags=re.M)[1:]:
        m = re.search(r"STUDY_TURN_([A-Z0-9_]+)", block)
        tag = m.group(1) if m else "untagged"
        seen = turns.setdefault(tag, {"requests": 0, "markers": {}})
        seen["requests"] += 1
        for mark in MARK.findall(block):
            seen["markers"][mark] = seen["markers"].get(mark, 0) + 1
    for t in turns.values():
        t["markers"] = dict(sorted(t["markers"].items()))
    with open(os.path.join(outdir, "study.json"), "w") as f:
        json.dump(turns, f, indent=1)
    for tag, t in turns.items():
        print(f"[study] turn {tag}: {t['requests']} requests: {sorted(t['markers'])}")
    common.check("study-ran", True, "observation only")
    return turns
