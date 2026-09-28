"""What the providers look for in a request, for the contracts that are new
enough to share one list.

Each provider used to keep its own marker lists, and every contract that
read them had to know which copy it was talking to. The `agent`, `context`
and plugin-root checks are the same question on every harness, so their
vocabulary lives here once: the providers import it (mounted beside them at
`/app/markers.py`) to summarise a request, and the contracts import it to
read that summary back. A label renamed in one place cannot drift from the
other.

Only presence is recorded — never the body — except for the one value the
plugin-root check needs: the path a Skill named after the harness resolved
its placeholder, so the contract can look for that file in the container.
"""

import re

#: The `flow` fixture's agents, by the label UZE commits to on every
#: harness: `<plugin>:<subdirs...>:<name>`, where the name is the
#: frontmatter `name` (the file stem only when there is none).
#:
#: None of them may contain `flow:review` — the user-only Skill's label,
#: whose *absence* from model requests other checks assert.
AGENTS = {
    "flat": ("flow:auditor", "UZE_AGENT_BODY_AUDITOR"),
    "nested": ("flow:checks:security", "UZE_AGENT_BODY_SECURITY"),
    "renamed": ("flow:linter", "UZE_AGENT_BODY_LINTER"),
    "vendor-fields": ("flow:scout", "UZE_AGENT_BODY_SCOUT"),
}

#: Carried by the prompt of every turn the agent contract drives, so a turn
#: that reached the model can be told from one that never started.
AGENT_PROBE = "UZE_AGENT_PROBE"

#: Written into the scene's `AGENTS.md` and nowhere else.
CONTEXT = "UZE_CONTEXT_MARKER_AGENTS_MD"
#: Carried by the context scene's prompt, for the same reason as above.
CONTEXT_PROBE = "UZE_CONTEXT_PROBE"

#: The Skill that names a file outside `skills/` through the plugin root.
ROOT_SKILL = "locate"
ROOT_SKILL_BODY = "UZE_SKILL_BODY_LOCATE"
#: What the file it names contains.
ROOT_FILE_MARKER = "UZE_PLUGIN_FILE_LOCATE"
#: Placeholders a harness's model must never be handed literally.
PLACEHOLDERS = ("${PLUGIN_ROOT}", "${CLAUDE_PLUGIN_ROOT}")

_ROOT_REF = re.compile(r"UZE_ROOT_REF=([^\s\"'\\`]+)")


def summary(body):
    """The shared part of one request's structural summary."""
    body = body or ""
    agent_markers = {AGENT_PROBE: AGENT_PROBE in body}
    for label, marker in AGENTS.values():
        agent_markers[label] = label in body
        agent_markers[marker] = marker in body
    return {
        "agent_markers": agent_markers,
        "context_markers": {m: m in body for m in (CONTEXT, CONTEXT_PROBE)},
        "root_markers": {
            ROOT_SKILL_BODY: ROOT_SKILL_BODY in body,
            **{p: p in body for p in PLACEHOLDERS},
        },
        "root_refs": sorted(set(_ROOT_REF.findall(body))),
    }
