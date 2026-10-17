"""Where an injected agent-context document lands in the model request.

The workspace wants to hand an agent it launched one extra document — the
agent's own context — without it ever being a turn the person typed and
without writing into the project. Each vendor has a candidate channel
(`experiments/<vendor>/agent-context.py`); this module is the part they
share: reading the provider's raw request log (`DISCOVERY=1`) and saying,
for each process of the run, where each marker sits.

Four questions per channel, each answered from the wire:

1. reaches the model: the marker is in the process's model request;
2. not a user turn: no occurrence is inside user-role content (Anthropic
   `messages[role=user]`, Responses `input[role=user]`, Gemini
   `contents[role=user]`, chat `messages[role=user]`);
3. holds on resume: a second process resuming the conversation carries it;
4. current on relaunch: the document changed between the processes
   (marker A -> B) and the resumed request carries B.

A run is two headless processes: the first sends `TURN_ONE` with marker A
in the document, the second resumes the same conversation sending
`TURN_TWO` with marker B in it. A process's request is the first request
whose body carries its turn token and declares tools (the model call, not
a title or a classifier side call); every other request is still listed.
"""

import json
import os
import re
import subprocess

from shared import capture, common

MARKER_A = "UZE_CTX_MARKER_ALPHA"
MARKER_B = "UZE_CTX_MARKER_BRAVO"
TURN_ONE = "AGENTCTX_TURN_ONE"
TURN_TWO = "AGENTCTX_TURN_TWO"


def write_document(path, marker, task):
    """The shell line writing the document: several lines, so a channel
    that keeps only the first line would show."""
    return (
        f"printf '%s\\n' '# Agent context' 'This agent works on lab task {task}.' "
        f"'{marker}' > {path}"
    )


def say(line):
    print(f"    {line}", flush=True)


def provider_log(world):
    """The provider's raw request log, read while it is alive."""
    out = subprocess.run(
        ["docker", "exec", world.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    )
    return out.stdout


#: A request's header line; a body may itself hold `### ` (a Markdown
#: heading in a system prompt), so only a method after it starts a request.
_HEADER = re.compile(r"^### (?=(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|WS) )", re.M)


def requests(log):
    """[(header, body-as-json-or-None, raw-body)] in arrival order."""
    found = []
    for chunk in _HEADER.split(log)[1:]:
        header, _, rest = chunk.partition("\n")
        # The body is the first line that opens a JSON document; headers
        # precede it (HTTP) or there are none (a WebSocket frame).
        lines = rest.splitlines()
        body_text = ""
        for i, line in enumerate(lines):
            if line.startswith("{") or line.startswith("["):
                body_text = "\n".join(lines[i:]).strip()
                break
        try:
            body = json.loads(body_text)
        except ValueError:
            body = None
        found.append((header.strip(), body, rest))
    return found


def locate(node, needle, path=""):
    """Every JSON path whose string value contains `needle`, with the role
    of the nearest enclosing object that names one."""
    hits = []

    def walk(value, here, role):
        if isinstance(value, dict):
            role = (
                value.get("role", role) if isinstance(value.get("role"), str) else role
            )
            for key, child in value.items():
                walk(child, f"{here}.{key}" if here else key, role)
        elif isinstance(value, list):
            for i, child in enumerate(value):
                walk(child, f"{here}[{i}]", role)
        elif isinstance(value, str) and needle in value:
            at = value.find(needle)
            hits.append(
                {
                    "path": here,
                    "role": role,
                    "excerpt": value[max(0, at - 160) : at + 80],
                }
            )

    walk(node, path, None)
    return hits


def _message_of(path):
    """`messages[3].content[0].text` -> `messages[3]`: the message a part
    belongs to (`request.contents[1]`, `input[7]`)."""
    import re as _re

    m = _re.match(r"^(.*?\[\d+\])\.(?:content|parts)", path)
    return m.group(1) if m else path


def is_user(hit):
    return hit["role"] == "user"


def process_request(reqs, token):
    """The model request of the process that sent `token`: the first body
    carrying it that declares tools; failing that, the first carrying it."""
    carrying = [
        (i, h, b)
        for i, (h, b, raw) in enumerate(reqs)
        if token in raw and not (token == TURN_ONE and TURN_TWO in raw)
    ]
    for i, h, b in carrying:
        if b is not None and capture.declared_tools(b):
            return i, h, b
    return carrying[0] if carrying else (None, None, None)


def analyse(cfg, tag, log, *, pane_one="", pane_two=""):
    """Writes `<tag>-requests.json` (every request's marker locations) and
    records the four outcomes as checks. Returns the summary."""
    reqs = requests(log)
    with open(os.path.join(cfg.outdir, f"{tag}-raw-requests.log"), "w") as f:
        f.write(log)
    listing = []
    for i, (header, body, raw) in enumerate(reqs):
        entry = {"index": i, "request": header, "json": body is not None}
        if body is not None:
            for marker in (MARKER_A, MARKER_B, TURN_ONE, TURN_TWO):
                hits = locate(body, marker)
                if hits:
                    entry[marker] = hits
        else:
            entry["raw_has"] = [
                m for m in (MARKER_A, MARKER_B, TURN_ONE, TURN_TWO) if m in raw
            ]
        listing.append(entry)
    with open(os.path.join(cfg.outdir, f"{tag}-requests.json"), "w") as f:
        json.dump(listing, f, indent=1)

    # Every model call of the run, with how many copies of each marker it
    # carries: a channel that injects per invocation and persists what it
    # injected grows by one copy per call, which only this shows.
    for i, (header, body, raw) in enumerate(reqs):
        if body is None or not capture.declared_tools(body):
            continue
        if TURN_ONE not in raw and TURN_TWO not in raw:
            continue
        say(
            f"[{tag}] model call #{i}: copies A={len(locate(body, MARKER_A))} "
            f"B={len(locate(body, MARKER_B))}; turn two={TURN_TWO in raw}"
        )
    first_i, first_h, first = process_request(reqs, TURN_ONE)
    second_i, second_h, second = process_request(reqs, TURN_TWO)
    say(
        f"[{tag}] {len(reqs)} requests; turn one at #{first_i} ({first_h}); "
        f"turn two at #{second_i} ({second_h})"
    )

    def where(body, marker):
        hits = locate(body, marker) if body is not None else []
        return hits, [f"{h['path']} (role={h['role']})" for h in hits]

    a1, a1_paths = where(first, MARKER_A)
    b2, b2_paths = where(second, MARKER_B)
    a2, a2_paths = where(second, MARKER_A)
    user_one, user_one_paths = where(first, TURN_ONE)
    common.check(
        f"{tag}-1-reaches-model",
        bool(a1),
        f"request #{first_i}: {MARKER_A} at {a1_paths}; the prompt itself at {user_one_paths}",
    )
    # "Not a user turn" is judged the way AGENTS.md itself travels: a
    # user-role item of its own (Codex `<user_instructions>`, a Claude
    # system-reminder part) is how instructions reach the model, so the
    # role is recorded, and only sharing the typed prompt's own content
    # item — or its message, as the transcript would show it — fails.
    prompt_hits = user_one + (locate(second, TURN_TWO) if second is not None else [])
    typed = {h["path"] for h in prompt_hits}
    typed_messages = {_message_of(h["path"]) for h in prompt_hits}
    carried = a1 + b2
    in_prompt_item = [h["path"] for h in carried if h["path"] in typed]
    in_prompt_message = [
        h["path"] for h in carried if _message_of(h["path"]) in typed_messages
    ]
    common.check(
        f"{tag}-2-not-a-user-turn",
        bool(carried) and not in_prompt_item,
        f"marker roles (first, resumed): {[h['role'] for h in a1]}, "
        f"{[h['role'] for h in b2]}; inside the typed prompt's item: {in_prompt_item}; "
        f"in the typed prompt's message (other part): {in_prompt_message}; excerpt: "
        + (repr(a1[0]["excerpt"][-200:]) if a1 else "-"),
    )
    if pane_one or pane_two:
        shown = [m for m in (MARKER_A, MARKER_B) if m in pane_one + pane_two]
        common.check(
            f"{tag}-2b-not-rendered",
            not shown,
            f"markers on the pane: {shown}",
        )
    common.check(
        f"{tag}-3-holds-on-resume",
        bool(b2) or bool(a2),
        f"request #{second_i}: {MARKER_B} at {b2_paths}; {MARKER_A} at {a2_paths}",
    )
    common.check(
        f"{tag}-4-current-on-relaunch",
        bool(b2),
        f"{MARKER_B} at {b2_paths}; stale {MARKER_A} also present: {bool(a2)} "
        f"({a2_paths})",
    )
    return {
        "first": first_i,
        "second": second_i,
        "a1": a1_paths,
        "b2": b2_paths,
        "a2": a2_paths,
    }


#: For every JSONL line (or JSON document) of the files given that carries
#: a marker: the fields that say who a transcript entry is from, so "shown
#: as a user message in the harness's own transcript" is read, not guessed.
TRANSCRIPT_JS = r"""
const fs = require("fs");
const markers = ["UZE_CTX_MARKER_ALPHA", "UZE_CTX_MARKER_BRAVO"];
const pick = (o) => o && typeof o === "object" ? {
  type: o.type, role: o.role, isMeta: o.isMeta, subtype: o.subtype,
  payloadType: o.payload?.type, payloadRole: o.payload?.role,
  messageRole: o.message?.role, attachment: o.attachment?.type,
} : typeof o;
for (const file of process.argv.slice(2)) {
  let text = "";
  try { text = fs.readFileSync(file, "utf8"); } catch (e) { continue; }
  text.split("\n").forEach((line, n) => {
    const found = markers.filter((m) => line.includes(m));
    if (!found.length) return;
    let o = null;
    try { o = JSON.parse(line); } catch (e) {}
    console.log(JSON.stringify({ file, line: n, markers: found, entry: o ? pick(o) : line.slice(0, 160) }));
  });
}
"""


def transcript_report(files):
    """The shell lines printing `TRANSCRIPT_JS`'s report for `files` (a
    shell glob), under `=== transcript`."""
    return (
        "cat > /work/transcript-roles.js <<'UZE_TR_EOF'\n"
        + TRANSCRIPT_JS
        + "UZE_TR_EOF\n"
        + "echo '=== transcript'\n"
        + f"node /work/transcript-roles.js {files} 2>&1 | head -40\n"
    )
