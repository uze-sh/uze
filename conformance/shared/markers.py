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

import json
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

#: The model each harness's `harness:` block gives the vendor-fields agent
#: (`agents/scout.md`), which a dispatch of it must run on. Set per harness
#: in the fixture, and matched as a substring of the model the request
#: named: Claude resolves `haiku` to a dated id.
BLOCK_MODELS = {
    "claude": "haiku",
    "codex": "gpt-6-luna",
    # Served by the Lab's own OpenCode provider under this real model id.
    "opencode": "claude-haiku-4-5",
    # Declared unsupported by its bindings: Antigravity drops an agent that
    # carries `model`, so UZE leaves the field out.
    "antigravity": "gemini-3.1-flash-lite-preview",
}

#: Carried by the prompt of every turn the agent contract drives, so a turn
#: that reached the model can be told from one that never started.
AGENT_PROBE = "UZE_AGENT_PROBE"

#: Written into the scene's `AGENTS.md` and nowhere else.
CONTEXT = "UZE_CONTEXT_MARKER_AGENTS_MD"
#: Carried by the context scene's prompt, for the same reason as above.
CONTEXT_PROBE = "UZE_CONTEXT_PROBE"

#: The last line of the long `AGENTS.md` the context contract writes, past
#: the size any harness is known to cut a rules file at.
CONTEXT_TAIL = "UZE_CONTEXT_MARKER_TAIL"
#: The description of a Skill the project authored under `.agents/skills/`,
#: which a harness's catalog carries to the model.
PROJECT_SKILL = "UZE_CONTEXT_MARKER_PROJECT_SKILL"
#: The same, for an agent the project authored under `.agents/agents/`.
PROJECT_AGENT = "UZE_CONTEXT_MARKER_PROJECT_AGENT"

#: The Skill that names a file outside `skills/` through the plugin root.
ROOT_SKILL = "locate"
ROOT_SKILL_BODY = "UZE_SKILL_BODY_LOCATE"
#: What the file it names contains.
ROOT_FILE_MARKER = "UZE_PLUGIN_FILE_LOCATE"
#: Placeholders a harness's model must never be handed literally.
PLACEHOLDERS = ("${PLUGIN_ROOT}", "${CLAUDE_PLUGIN_ROOT}")

#: What each version of the lifecycle fixture puts in front of the model
#: (`plugins/lifecycle-plugin`): a Skill's and an agent's description.
LIFECYCLE = {
    version: (f"UZE_LIFECYCLE_SKILL_{version}", f"UZE_LIFECYCLE_AGENT_{version}")
    for version in ("V1", "V2")
}

#: The reasons the hooks contract's denying handlers relay
#: (`_fixtures/.../scripts/probe`). Composed by the handler from its own
#: label at run time, so neither the prompt nor a scripted call carries one.
HOOK_DENIALS = (
    *(
        f"lab-hook-denied:{label}"
        for label in (
            "effect-guard",
            "effect-ask",
            "post-guard",
            "exec-guard",
            "read-guard",
        )
    ),
    # The wrapper's own reason when a guard cannot run (fail-closed): its
    # shell's exit, or, where a handler is spawned from its words with no
    # shell between (OpenCode's bridge), the spawn that never started.
    "handler failed (exit",
    "handler failed to start",
)

#: What the files the hooks contract's read scene opens hold: only the
#: read tool's own output can carry one to the model (`contract/hooks.py`
#: writes them into the project before the session starts).
HOOK_READS = ("UZE_HOOK_READ_DENIED_BODY", "UZE_HOOK_READ_ALLOWED_BODY")

#: The briefing contract's markers (`contract/briefing.py`), whose place in
#: the request is recorded as evidence: an agent's context reaches the model
#: in a system, developer or user part depending on the harness, and
#: `AGENTS.md` itself rides in a user part on two of them, so where it lands
#: is evidence, never the verdict.
CONTEXT_PLACED = ("UZE_CONFORMANCE_PECAN", "UZE_CONFORMANCE_ALMOND")

#: The rules contract's markers (`contract/rules.py`): each rule's body
#: carries one, the `model_decision` rule's description another, and each
#: turn's prompt its own, so a request can be told apart by what it carries.
RULES = {
    "glob": "UZE_RULES_MARKER_GLOB",
    "always": "UZE_RULES_MARKER_ALWAYS",
    "decide_index": "UZE_RULES_MARKER_DECIDE_INDEX",
    "decide_body": "UZE_RULES_MARKER_DECIDE_BODY",
    # The `rules-plugin` fixture's glob rule, copied into the projects that
    # declare the plugin.
    "plugin_glob": "UZE_RULES_MARKER_PLUGIN_GLOB",
}
RULES_TURNS = ("UZE_RULES_TURN_ONE", "UZE_RULES_TURN_TWO")
#: What every provider answers a compaction with, so the request after one
#: says it follows a compaction by what it carries.
COMPACTION = "UZE_LAB_COMPACTION_SUMMARY"

#: The launch-instructions contract's markers (`contract/instructions.py`).
#: `lead` opens the instructions UZE composes for a launch
#: (`uze_workspace::agent_context::INSTRUCTIONS_LEAD`), and `first_turn` is
#: the visible turn that points a launch with no channel at them
#: (`uze_workspace::briefing::instructions_turn`): text only UZE writes.
#: The two `AGENTS.md` markers are the primary checkout's and a slot's, as
#: two branches carry them; the prompts tell the parent's turn from the
#: subagent's.
INSTRUCTIONS = {
    "lead": "UZE instructions for this launch.",
    # The last sentence of an isolated agent's instructions: what a cut
    # policy loses first.
    "tail": "UZE only knows the checkouts it made.",
    "first_turn": "holds UZE's instructions for this launch",
    "primary_agents_md": "UZE_INSTRUCTIONS_PRIMARY_AGENTS_MD",
    "slot_agents_md": "UZE_INSTRUCTIONS_SLOT_AGENTS_MD",
    "spawn": "UZE_INSTRUCTIONS_SPAWN",
    "subagent_turn": "UZE_INSTRUCTIONS_SUBAGENT_TURN",
    "turn": "UZE_INSTRUCTIONS_TURN",
}

#: How a request states the directory the agent works in, one form per
#: dialect a harness writes it in. Case-sensitive on purpose: each form is
#: distinct from the others' wording only by its case.
_WORKING_DIRECTORY = (
    re.compile(r"Command Working Directory: ([^\s\\\"<]+)"),
    re.compile(r"Primary working directory: ([^\s\\\"<]+)"),
    re.compile(r"Working directory: ([^\s\\\"<]+)"),
    re.compile(r"<cwd>([^<\\\"]+)</cwd>"),
)

#: Where a tool call's input sits: a field naming its arguments, or the
#: `input` of an item whose type is a call.
_CALL_FIELDS = ("arguments", "tool_calls", "functionCall")
_CALL_TYPES = ("tool_use", "function_call", "custom_tool_call")

#: Where a dialect carries a tool's result back to the model: Anthropic
#: `tool_result`, OpenAI Responses `function_call_output` (and its freeform
#: twin), chat completions' `tool` role, Gemini's `functionResponse`.
_RESULT_TYPES = ("tool_result", "function_call_output", "custom_tool_call_output")

_ROOT_REF = re.compile(r"UZE_ROOT_REF=([^\s\"'\\`]+)")
_MODEL = re.compile(r'"model"\s*:\s*"([^"]+)"')


def summary(body):
    """The shared part of one request's structural summary."""
    body = body or ""
    agent_markers = {AGENT_PROBE: AGENT_PROBE in body}
    for label, marker in AGENTS.values():
        agent_markers[label] = label in body
        agent_markers[marker] = marker in body
    return {
        "agent_markers": agent_markers,
        "context_markers": {
            m: m in body
            for m in (
                CONTEXT,
                CONTEXT_PROBE,
                CONTEXT_TAIL,
                PROJECT_SKILL,
                PROJECT_AGENT,
            )
        },
        "root_markers": {
            ROOT_SKILL_BODY: ROOT_SKILL_BODY in body,
            **{p: p in body for p in PLACEHOLDERS},
        },
        "root_refs": sorted(set(_ROOT_REF.findall(body))),
        "hook_denials": {m: m in body for m in HOOK_DENIALS},
        "hook_reads": {m: m in body for m in HOOK_READS},
        "lifecycle_markers": {
            m: m in body for markers in LIFECYCLE.values() for m in markers
        },
        # The model a request asked for, where the body names it (Gemini
        # names it in the path instead, which the request record keeps).
        "model": next(iter(_MODEL.findall(body)), None),
        "context_placement": {
            m: placement(body, m) for m in CONTEXT_PLACED if m in body
        },
        "rule_markers": {
            m: m in body for m in (*RULES.values(), *RULES_TURNS, COMPACTION)
        },
        # How many times each rule's text is in the request: one delivery
        # read twice (a flat file and a registered folder) shows as two.
        "rule_counts": {m: body.count(m) for m in RULES.values() if m in body},
        "instruction_markers": {m: m in body for m in INSTRUCTIONS.values()},
        # A prompt carried by a message of the request's own, not only by
        # the call that spawned it: what tells a subagent's request apart.
        "instruction_turns": {
            m: outside_calls(body, m)
            for m in (INSTRUCTIONS["spawn"], INSTRUCTIONS["subagent_turn"])
            if m in body
        },
        "working_directories": sorted(
            {path for form in _WORKING_DIRECTORY for path in form.findall(body)}
        ),
        **_shape(body),
    }


def _shape(body):
    """How many tool results a request carries back, whether it offers the
    model tools, and whether it only continues the request before it: what
    tells the turn's model calls from a side call (a title, a summary), and
    each step of a scripted turn from the next."""
    try:
        document = json.loads(body)
    except ValueError:
        return {"tool_results": None, "offers_tools": None}
    results = 0

    def walk(value):
        nonlocal results
        if isinstance(value, dict):
            if value.get("type") in _RESULT_TYPES or value.get("role") == "tool":
                results += 1
            if "functionResponse" in value:
                results += 1
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    walk(document)
    offers = False
    continues = False
    if isinstance(document, dict):
        for holder in (document, document.get("request")):
            if isinstance(holder, dict) and holder.get("tools"):
                offers = True
        # Codex 0.163 declares its tools in an `additional_tools` input item.
        items = document.get("input")
        if isinstance(items, list):
            offers = offers or any(
                isinstance(i, dict) and i.get("type") == "additional_tools"
                for i in items
            )
        # A Responses request naming the one before it carries only what
        # was added since (Codex over WebSocket).
        continues = bool(document.get("previous_response_id"))
    return {"tool_results": results, "offers_tools": offers, "continues": continues}


def outside_calls(body, needle):
    """Whether `needle` sits in a string of a JSON request outside every
    tool call's input."""
    try:
        document = json.loads(body)
    except ValueError:
        return False

    def walk(value, in_call):
        if isinstance(value, dict):
            call = in_call or value.get("type") in _CALL_TYPES
            return any(
                walk(child, call or key in _CALL_FIELDS) for key, child in value.items()
            )
        if isinstance(value, list):
            return any(walk(child, in_call) for child in value)
        return isinstance(value, str) and needle in value and not in_call

    return walk(document, False)


def placement(body, needle):
    """Where `needle` sits in a JSON request, as the roles of the parts that
    carry it: the nearest enclosing `role`, or the top-level field a dialect
    carries instructions in (Anthropic `system`, OpenAI Responses
    `instructions`, Gemini `systemInstruction`). Sorted, deduplicated."""
    try:
        document = json.loads(body)
    except ValueError:
        return ["unparsed"]
    found = set()

    def walk(value, role):
        if isinstance(value, dict):
            if isinstance(value.get("role"), str):
                role = value["role"]
            for child in value.values():
                walk(child, role)
        elif isinstance(value, list):
            for child in value:
                walk(child, role)
        elif isinstance(value, str) and needle in value:
            found.add(role)

    if isinstance(document, dict):
        for key, value in document.items():
            walk(value, key)
    return sorted(found)
