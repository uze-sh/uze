"""What the synthetic providers record about the harness's requests.

The provider is the endpoint the real harness talks to — it already sees
every request the harness sends, with no proxy topology needed. Every run
records the tools the harness declares to the model (`declared_tools`),
which is the vocabulary UZE's hook bindings are checked against. When a
sandbox/experiment run passes `--discovery`, each provider appends the raw
request (method, path, headers, body) to `/app/raw-requests.log`; the lab
pulls that file beside the run evidence. Raw captures never enter the
repository (outdirs are ephemeral / CI artifacts only) — the same rule as
`conformance/discovery/`.
"""

from __future__ import annotations

import json
import os


def _read_chunked(stream) -> bytes:
    """The body of a `Transfer-Encoding: chunked` request.

    `BaseHTTPRequestHandler` does not decode chunked bodies, and a harness
    that streams its request (Antigravity's signed-in CloudCode path does)
    would otherwise hand every provider an empty body — a request that
    declares tools the provider never sees, and a turn where nothing
    happens. A malformed stream stops the read rather than blocking: the
    provider must always answer.
    """
    body = b""
    while True:
        line = stream.readline()
        if not line:
            break
        try:
            size = int(line.split(b";", 1)[0].strip() or b"0", 16)
        except ValueError:
            break
        if size == 0:
            stream.readline()  # the trailer's terminating CRLF
            break
        body += stream.read(size)
        stream.readline()  # the CRLF that closes this chunk
    return body


#: Where every provider accumulates the tools the harness declared, across
#: every request of the provider's life. The lab pulls it before the
#: provider is replaced, so a run's vocabulary is the union of its phases.
DECLARED_TOOLS_PATH = "/app/declared-tools.json"

#: The keys a tool declaration carries its input schema under, across the
#: four wire dialects: Anthropic `input_schema`, OpenAI `parameters`,
#: Gemini `parameters`/`parametersJsonSchema`, and MCP's `inputSchema`.
_SCHEMA_KEYS = ("input_schema", "parameters", "parametersJsonSchema", "inputSchema")

#: The keys a request lists its tool declarations under.
_DECLARATION_LISTS = ("tools", "functionDeclarations")


def declared_tools(document) -> dict[str, list[str]]:
    """Every tool a request declares to the model, with its input fields.

    Walks the request rather than reading one dialect's path, because the
    same declaration sits at `tools[]` (Anthropic, OpenAI Responses),
    `tools[].function` (chat completions), `tools[].functionDeclarations[]`
    (Gemini) or under `request.` (CloudCode). A declaration is a named
    member of a `tools` or `functionDeclarations` list, or the `function` of
    one; a tool *call* is a member of a message's content or a `tool_calls`
    list, so the two cannot be confused. A declaration may carry no schema
    at all (a tool with no parameters). A namespaced group (Codex's
    `{"type": "namespace"}`) prefixes its members' names the way the harness
    exposes them.
    """
    found: dict[str, set[str]] = {}

    def declare(node, namespace):
        name = node.get("name")
        if not isinstance(name, str):
            return
        if node.get("type") == "namespace":
            for member in node.get("tools", []):
                if isinstance(member, dict):
                    declare(member, f"{namespace}{name}.")
            return
        schema = next(
            (node[k] for k in _SCHEMA_KEYS if isinstance(node.get(k), dict)), {}
        )
        found.setdefault(namespace + name, set()).update(schema.get("properties") or {})

    def visit(node, key):
        if isinstance(node, list):
            for item in node:
                if key in _DECLARATION_LISTS and isinstance(item, dict):
                    function = item.get("function")
                    declare(function if isinstance(function, dict) else item, "")
                    if item.get("type") == "namespace":
                        continue
                visit(item, None)
            return
        if isinstance(node, dict):
            for child_key, value in node.items():
                if isinstance(value, (dict, list)):
                    visit(value, child_key)

    visit(document, None)
    return {name: sorted(fields) for name, fields in found.items()}


def record_declared_tools(body: bytes | str) -> None:
    try:
        tools = declared_tools(json.loads(body))
    except ValueError:
        return
    if not tools:
        return
    try:
        with open(DECLARED_TOOLS_PATH) as f:
            known = json.load(f)
    except (OSError, ValueError):
        known = {}
    for name, fields in tools.items():
        known[name] = sorted(set(known.get(name, [])) | set(fields))
    # Written whole or not at all, for the same reason as the request log:
    # the lab reads it from outside the container at any moment.
    pending = f"{DECLARED_TOOLS_PATH}.pending"
    with open(pending, "w") as f:
        json.dump(known, f, indent=1, sort_keys=True)
    os.replace(pending, DECLARED_TOOLS_PATH)


#: Every tool call a provider was asked to script that the request it
#: answered did not declare. The lab fails the run on any entry.
UNDECLARED_CALLS_PATH = "/app/undeclared-calls.json"


def scriptable(body: bytes | str, tool: str, continuation: bool = False) -> bool:
    """Whether the request being answered declares `tool`.

    A request that declares no tool at all is a side call — a title, a
    lighter model's summary — and is never a place to script one: it is
    answered with text and not recorded. A `continuation` (the request
    carrying the previous step's result) may lean on what the session
    already declared, since some harnesses resend only the new output.

    A provider that scripts a call to a tool the harness never offered is
    not speaking the real wire: the harness answers `Unknown tool`, no hook
    runs, and every check downstream measures an error. That is how the
    OpenCode hook phase stayed green on a tool name V2 never had. So the
    provider asks first, and a call it may not make is recorded for the lab
    to fail on — and answered with text, so the turn ends instead of
    pretending to be the scenario.
    """
    try:
        declared = declared_tools(json.loads(body))
    except ValueError:
        declared = {}
    if not declared and not continuation:
        return False
    if continuation:
        try:
            with open(DECLARED_TOOLS_PATH) as f:
                declared = {**json.load(f), **declared}
        except (OSError, ValueError):
            pass
    if tool in declared:
        return True
    try:
        with open(UNDECLARED_CALLS_PATH) as f:
            calls = json.load(f)
    except (OSError, ValueError):
        calls = []
    calls.append({"tool": tool, "declared": sorted(declared)})
    pending = f"{UNDECLARED_CALLS_PATH}.pending"
    with open(pending, "w") as f:
        json.dump(calls, f, indent=1)
    os.replace(pending, UNDECLARED_CALLS_PATH)
    return False


def next_scriptable(body, steps, step, continuation=lambda step: False):
    """The first step from `step` on whose tool this request declares, or
    None. A step it does not declare is recorded as refused
    (`scriptable`) and passed over, so one missing tool costs its own check
    and not every call scripted after it."""
    while step is not None and step < len(steps):
        if scriptable(body, steps[step]["name"], continuation=continuation(step)):
            return step
        step += 1
    return None


def read_body(handler) -> bytes:
    """Reads the request body once, recording the tools it declares and,
    when the lab enabled `DISCOVERY=1`, appending the raw request to the
    provider's capture log.

    The body is a stream: whoever reads it owns it. Capturing and serving
    used to read it twice, and the second read blocked on a socket with
    nothing left — every discovery run hung on its first model call. An
    OSError on either record is swallowed: a capture failure must never
    break the provider serving a harness. A failure to record tools still
    surfaces, as a run whose capture is empty, which the lab refuses.
    """
    if "chunked" in (handler.headers.get("Transfer-Encoding", "") or "").lower():
        body = _read_chunked(handler.rfile)
    else:
        length = int(handler.headers.get("Content-Length", "0") or 0)
        body = handler.rfile.read(length) if length else b""
    try:
        record_declared_tools(body)
    except OSError:
        pass
    if not os.environ.get("DISCOVERY"):
        return body
    try:
        with open("/app/raw-requests.log", "ab") as f:
            f.write(f"### {handler.command} {handler.path}\n".encode())
            for key, value in handler.headers.items():
                f.write(f"{key}: {value}\n".encode())
            f.write(body + b"\n\n")
    except OSError:
        pass
    return body
