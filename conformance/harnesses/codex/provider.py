#!/usr/bin/env python3
"""Minimal deterministic fake OpenAI API provider — the Codex stub.

Contract (derived from observed behavior of the REAL codex-cli 0.149.1):
  * Codex first attempts a WebSocket to `wss://api.openai.com/v1/responses`
    (host resolved via /etc/hosts, CA injected via CODEX_CA_CERTIFICATES +
    SSL_CERT_FILE). Accepting the upgrade and cleanly closing the socket
    makes codex fall back to HTTPS.
  * The HTTPS model call is `POST /v1/responses` (stream=true, OpenAI
    Responses API), answered with SSE events (response.created /
    output_item.added / content_part.added / output_text.delta / completed).
  * Model catalog: `GET /v1/models` (served for the TUI boot panel).
  * (0.154.0) While a turn streams, the CLI names the session on a
    **second connection**, asking for a title under a strict
    `text.format` JSON schema. Both facts are load-bearing: the server
    must be threaded, or that connection waits in the backlog and the
    `renaming...` spinner never stops; and a schema-constrained request
    must be answered with the JSON it asked for, or the rename never
    completes and its thread runs the scripted tool call of its own.

This stub records ONLY a structural summary of each request (skill markers,
catalog presence, user-text presence) — never the verbatim body — into
PROVIDER_STRUCT: the model-facing observation boundary.

Env: PROVIDER_STRUCT, RESPONSE_TEXT, LEAF_CERT, LEAF_KEY, CA_TRUST_FILE
"""

import base64
import hashlib
import json
import os
import ssl
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import capture
import markers
import variation
import websocket

STRUCT_PATH = os.environ.get("PROVIDER_STRUCT", "/tmp/codex-struct.json")
RESPONSE_TEXT = os.environ.get("RESPONSE_TEXT", "UZE_CONFORMANCE_OK")
MODE = os.environ.get("PROVIDER_MODE", "static")
LEAF_CERT = os.environ.get("LEAF_CERT", "/app/leaf.crt")
LEAF_KEY = os.environ.get("LEAF_KEY", "/app/leaf.key")
WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

ISOLATION_MARKERS = ["already isolated", "UZE_CONFORMANCE_REBASE"]
#: One turn each. A single request carrying both is what proves a relaunched
#: process was given the conversation the first one made — the union across
#: requests cannot say that, since the first process's own request already
#: carried the first of them.
CONTINUITY_MARKERS = ["UZE_CONFORMANCE_ACORN", "UZE_CONFORMANCE_WALNUT"]
SKILL_MARKERS = [
    "flow:commit",
    "flow:review",
    "flow:analyze",
    "North Star",
    "Review code",
    # A Skill's *body* reaches the model only when the Skill was
    # invoked — a listing carries name and description alone. These are
    # what tell an invocation from an offer.
    "UZE_SKILL_BODY_COMMIT",
    "UZE_SKILL_BODY_REVIEW",
    "UZE_SKILL_BODY_ANALYZE",
]
COUNTER = {"n": 0}
# Serving one connection at a time was enough until 0.154.0, which renames
# the session on a connection of its own while the turn streams on the
# WebSocket: the second connection sat in the listen backlog, the rename
# spinner never stopped, and no turn ever went quiet for an absence check.
# A threaded server makes the evidence file a shared resource — one writer
# at a time, or a run loses requests it did observe.
RECORD_LOCK = threading.Lock()

# The hook scenarios script a tool call to the harness's native shell tool
# (`Bash`); TOOL_ARGS mirrors the tool `input` the hook's normalized ABI
# payload will carry.
TOOL_NAME = os.environ.get("TOOL_NAME", "Bash")
TOOL_ARGS = os.environ.get("TOOL_ARGS", "{}")
#: Codex 0.158 groups its tools into namespaces (`functions`, `collaboration`,
#: ...) inside an `additional_tools` input item, and a call to a namespaced
#: tool names its namespace: `spawn_agent` without one is answered
#: `unsupported call: spawn_agent`.
TOOL_NAMESPACE = os.environ.get("TOOL_NAMESPACE", "")
#: When set, the first call is scripted only for a request carrying this
#: text. A subagent's own turn is a real turn too, and answering it with the
#: same `spawn_agent` call spawns agents recursively until the slots run out.
TOOL_TRIGGER = os.environ.get("TOOL_TRIGGER", "")
#: Several calls, one per model step, as a JSON list of
#: `{"name", "namespace"?, "args"}`. Step N+1 answers the request carrying the
#: output of step N's call id: a follow-up is sent with `previous_response_id`
#: and only the new output, so the trigger text is not in it. Dispatching a
#: subagent needs two — `codex exec` ends the process, and the subagent with
#: it, as soon as the root turn answers, so the root has to `wait_agent`.
TOOL_SEQUENCE = json.loads(os.environ.get("TOOL_SEQUENCE", "null") or "null") or [
    {"name": TOOL_NAME, "namespace": TOOL_NAMESPACE, "args": TOOL_ARGS}
]


def call_id(step):
    return f"fc_uze_{step + 1}"


def declared_name(step):
    """The step's tool as a declaration names it: namespaced tools are
    declared inside their namespace (`capture.declared_tools`)."""
    call = TOOL_SEQUENCE[step]
    namespace = call.get("namespace") or ""
    return f"{namespace}.{call['name']}" if namespace else call["name"]


def answered_step(body):
    """The scripted step this request is answered with: the one due, or the
    next whose tool the harness declared (`capture.next_scriptable`); None
    for text. A follow-up sent with `previous_response_id` carries only the
    new output, so it leans on what the session already declared."""
    named = [{"name": declared_name(step)} for step in range(len(TOOL_SEQUENCE))]
    return capture.next_scriptable(
        body, named, scripted_step(body), continuation=lambda step: step > 0
    )


def scripted_step(body):
    """The index of the scripted call `body` is answered with, or None."""
    has_turn = '"input"' in body or '"inputs"' in body
    if MODE != "toolcall" or not has_turn:
        return None
    for step in range(len(TOOL_SEQUENCE) - 1, -1, -1):
        if f'"call_id":"{call_id(step)}"' in body.replace(" ", ""):
            following = step + 1
            return following if following < len(TOOL_SEQUENCE) else None
    if '"function_call_output"' in body or '"custom_tool_call_output"' in body:
        return None
    if TOOL_TRIGGER and TOOL_TRIGGER not in body:
        return None
    return 0


# Conformance evidence markers carried by portable-hook denial reasons
# (ADR-033): presence/absence in the structural summary proves what the real
# harness relayed after the hook executed.
HOOK_MARKERS = [
    "blocked by protect-env",
    "first-handler-denied",
    "second-handler-ran",
    "second-handler-reached",
    "Denied by UZE hook",
    # The portable vocabulary row itself: the guard echoes the alias it was
    # handed, so a relayed denial proves the handler read `shell` (not the
    # harness's own tool name) whichever harness delivered the hook.
    "tool=shell",
    # The real tool stdout marker: only present when the intercepted tool
    # actually executed after an allow — the deny/allow contrast relies
    # on it, not on the ambiguous presence of a tool result.
    "plain output",
]


def structural_summary(body_text):
    body = body_text or ""
    tools = []
    try:
        doc = json.loads(body)

        def walk(items, prefix=""):
            for item in items or []:
                name = item.get("name")
                if item.get("type") == "custom" and name:
                    tools.append(f"{prefix}{name}")
                walk(item.get("tools"), prefix=f"{prefix}{name}.")

        walk(doc.get("input"))
        for entry in doc.get("input") or []:
            walk(entry.get("tools"))
    except Exception:
        tools = []
    return {
        **markers.summary(body),
        "skill_markers": {m: (m in body) for m in SKILL_MARKERS},
        "isolation_markers": {m: (m in body) for m in ISOLATION_MARKERS},
        "continuity_markers": {m: (m in body) for m in CONTINUITY_MARKERS},
        "custom_tools": tools,
        "mcp_proof_present": MCP_PROOF in body,
        "preview": body[:900],
        "has_available_skills": "### Available skills" in body,
        "has_user_text": '"role": "user"' in body or '"input"' in body,
        "has_function_call": '"type": "function_call"' in body,
        "hook_markers": {m: (m in body) for m in HOOK_MARKERS},
        "len": len(body),
    }


#: The model a response names when the request named none; the models
#: listing offers it. A response otherwise names the model its request asked
#: for, as the API does.
#: What the delivered MCP server answers with, read back from the request
#: that carries its result.
MCP_PROOF = os.environ.get("MCP_PROOF", "UZE_MCP_CONFORMANCE_PROOF_1")

DEFAULT_MODEL = "gpt-5.6-sol"


def requested_model(body):
    """The `model` a Responses request names, or `None`."""
    try:
        return json.loads(body).get("model")
    except (ValueError, AttributeError):
        return None


def text_events(text, model=None):
    rid, mid = "resp_uze_1", "msg_uze_1"
    evs = [
        (
            "response.created",
            {
                "type": "response.created",
                "response": {
                    "id": rid,
                    "object": "response",
                    "created_at": 1750000000,
                    "status": "in_progress",
                    "model": model or DEFAULT_MODEL,
                    "output": [],
                    "usage": None,
                },
            },
        ),
        (
            "response.output_item.added",
            {
                "type": "response.output_item.added",
                "output_index": 0,
                "item": {
                    "type": "message",
                    "id": mid,
                    "role": "assistant",
                    "status": "in_progress",
                    "content": [],
                },
            },
        ),
        (
            "response.content_part.added",
            {
                "type": "response.content_part.added",
                "item_id": mid,
                "output_index": 0,
                "content_index": 0,
                "part": {"type": "output_text", "text": "", "annotations": []},
            },
        ),
        (
            "response.output_text.delta",
            {
                "type": "response.output_text.delta",
                "item_id": mid,
                "output_index": 0,
                "content_index": 0,
                "delta": text,
            },
        ),
        (
            "response.output_text.done",
            {
                "type": "response.output_text.done",
                "item_id": mid,
                "output_index": 0,
                "content_index": 0,
                "text": text,
            },
        ),
        (
            "response.output_item.done",
            {
                "type": "response.output_item.done",
                "output_index": 0,
                "item": {
                    "type": "message",
                    "id": mid,
                    "role": "assistant",
                    "status": "completed",
                    "content": [
                        {"type": "output_text", "text": text, "annotations": []}
                    ],
                },
            },
        ),
        (
            "response.completed",
            {
                "type": "response.completed",
                "response": {
                    "id": rid,
                    "object": "response",
                    "created_at": 1750000000,
                    "status": "completed",
                    "model": model or DEFAULT_MODEL,
                    "output": [
                        {
                            "type": "message",
                            "id": mid,
                            "role": "assistant",
                            "status": "completed",
                            "content": [
                                {"type": "output_text", "text": text, "annotations": []}
                            ],
                        }
                    ],
                    "usage": {
                        "input_tokens": 10,
                        "output_tokens": 3,
                        "total_tokens": 13,
                    },
                },
            },
        ),
    ]
    return evs


def sse_bytes(evs):
    return "".join(f"event: {e}\ndata: {json.dumps(d)}\n\n" for e, d in evs).encode()


def responses_sse(text, model=None):
    return sse_bytes(text_events(text, model))


def function_call_events(step=0, model=None):
    """A tool-call response's event list (Responses API): one output item
    naming the step's tool with its input (TOOL_NAME with TOOL_ARGS unless a
    TOOL_SEQUENCE says otherwise). The harness executes the tool (through
    the UZE hook wrapper); the follow-up request carries the call's output,
    which the handler answers with the next step, or with the final text.

    A step marked `custom` is a freeform tool's call — a `custom_tool_call`
    whose `input` is raw text — which is how Codex's code-mode `exec` is
    called since 0.160: its input is JavaScript that calls the nested tools
    (`await tools.exec_command({cmd})`), measured with `--discovery`.
    """
    call = TOOL_SEQUENCE[step]
    custom = bool(call.get("custom"))
    payload = (
        call["args"] if isinstance(call["args"], str) else json.dumps(call["args"])
    )
    rid, fid = f"resp_uze_{step + 2}", call_id(step)
    item = {
        "type": "custom_tool_call" if custom else "function_call",
        "id": fid,
        "call_id": fid,
        "name": call["name"],
        "status": "in_progress",
    }
    key = "input" if custom else "arguments"
    item[key] = ""
    if call.get("namespace"):
        item["namespace"] = call["namespace"]
    stream = "custom_tool_call_input" if custom else "function_call_arguments"
    done = {**item, key: payload, "status": "completed"}
    return [
        (
            "response.created",
            {
                "type": "response.created",
                "response": {
                    "id": rid,
                    "object": "response",
                    "created_at": 1750000000,
                    "status": "in_progress",
                    "model": model or DEFAULT_MODEL,
                    "output": [],
                    "usage": None,
                },
            },
        ),
        (
            "response.output_item.added",
            {"type": "response.output_item.added", "output_index": 0, "item": item},
        ),
        (
            f"response.{stream}.delta",
            {
                "type": f"response.{stream}.delta",
                "item_id": fid,
                "output_index": 0,
                "delta": payload,
            },
        ),
        (
            f"response.{stream}.done",
            {
                "type": f"response.{stream}.done",
                "item_id": fid,
                "output_index": 0,
                key: payload,
            },
        ),
        (
            "response.output_item.done",
            {"type": "response.output_item.done", "output_index": 0, "item": done},
        ),
        (
            "response.completed",
            {
                "type": "response.completed",
                "response": {
                    "id": rid,
                    "object": "response",
                    "created_at": 1750000000,
                    "status": "completed",
                    "model": model or DEFAULT_MODEL,
                    "output": [done],
                    "usage": {
                        "input_tokens": 10,
                        "output_tokens": 3,
                        "total_tokens": 13,
                    },
                },
            },
        ),
    ]


def function_call_sse(step=0, model=None):
    return sse_bytes(function_call_events(step, model))


#: What a schema-constrained side call is answered with. Deliberately not
#: RESPONSE_TEXT: 0.154.0 asks for a session *title*, and a title carrying
#: the turn's own marker would sit on screen — and in the window title —
#: for every wait that looks for that marker.
SIDE_CALL_ANSWER = "UZE Lab session"


def schema_answer(body):
    """The JSON a request constraining its answer with `text.format` asked
    for, or None when the request is an ordinary turn.

    0.154.0 renames the session while the turn streams, on a connection of
    its own, by asking for a title under a strict JSON schema. A scripted
    tool call is not an answer to that: the rename never completes, its
    spinner never stops — no turn goes quiet, so no absence check may
    evaluate — and the title's own thread runs the scripted command through
    the hook, mixing its tool result into the evidence of the user's turn.
    """
    try:
        schema = json.loads(body)["text"]["format"]["schema"]
    except (ValueError, KeyError, TypeError):
        return None
    properties = schema.get("properties", {})
    return json.dumps(
        {
            name: SIDE_CALL_ANSWER[: properties.get(name, {}).get("maxLength", 64)]
            for name in schema.get("required", properties)
        }
    )


def respond(body, path):
    """The request → payload mapping shared by the HTTP and WebSocket paths
    (codex 0.150.1 speaks the Responses API over a real WebSocket)."""
    if path.startswith("/v1/responses"):
        # A tool call is only scripted for a real turn: the TUI also
        # sends a boot/connectivity request without `input`/`inputs`,
        # and answering that with a function call hangs its model load.
        side_call = schema_answer(body)
        if side_call is not None:
            return responses_sse(side_call, requested_model(body))
        step = answered_step(body)
        if step is not None:
            return function_call_sse(step, requested_model(body))
        return responses_sse(RESPONSE_TEXT, requested_model(body))
    if path.startswith("/v1/models"):
        return json.dumps(
            {
                "object": "list",
                "data": [
                    {
                        "id": DEFAULT_MODEL,
                        "object": "model",
                        "created_at": 1750000000,
                        "owned_by": "openai",
                    },
                    {
                        "id": "o3",
                        "object": "model",
                        "created_at": 1750000000,
                        "owned_by": "openai",
                    },
                ],
            }
        ).encode()
    return b'{"ok":true}'


def record(body, path, method):
    """Structural evidence for one request — shared by the HTTP handler
    and the per-message WebSocket loop (the real turn bodies arrive in WS
    frames after the upgrade, so an HTTP-only read would miss them)."""
    summary = structural_summary(body)
    with RECORD_LOCK:
        n = COUNTER["n"]
        COUNTER["n"] += 1
        struct = []
        if os.path.exists(STRUCT_PATH):
            try:
                struct = json.load(open(STRUCT_PATH))
            except Exception:
                struct = []
        struct.append({"method": method, "path": path, "seq": n, "summary": summary})
        # Written whole or not at all: the file is rewritten on every
        # request and a scenario reads it from outside this container
        # with `cat`, so a reader landing mid-write would see a truncated
        # document — which `provider_struct` cannot tell from "no request
        # carried that marker", and which fails a check about the harness
        # over something that never had to do with it.
        pending = f"{STRUCT_PATH}.pending"
        with open(pending, "w") as f:
            json.dump(struct, f, indent=1)
        os.replace(pending, STRUCT_PATH)
    print(f"[codex-provider] {method} {path} req#{n}", flush=True)


def ws_loop(conn, path):
    """Serves the real WebSocket the 0.150.1 harness uses for the Responses
    API: records each frame's body as evidence, answers with one JSON event
    per frame (codex's `ResponsesStreamEvent` deserializes each WS text
    frame directly — SSE framing is the HTTP transport's, not the WS
    protocol's), closes cleanly after `response.completed`."""

    def handle(text):
        if os.environ.get("DISCOVERY"):
            try:
                with open("/app/raw-requests.log", "ab") as f:
                    f.write(f"### WS {path}\n{text}\n\n".encode())
            except OSError:
                pass
        record(text, path, "WS")
        capture.record_declared_tools(text)
        side_call = schema_answer(text)
        step = answered_step(text) if side_call is None else None
        if side_call is not None:
            events = text_events(side_call, requested_model(text))
        elif step is not None:
            events = function_call_events(step, requested_model(text))
        else:
            events = text_events(RESPONSE_TEXT, requested_model(text))
        for _name, payload in events:
            websocket.send_text(conn, json.dumps(payload).encode())
        return None

    websocket.serve(conn, handle)


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _handle(self):

        body = capture.read_body(self).decode("utf-8", "replace")
        record(body, self.path, self.command)

        if self.headers.get("Upgrade", "").lower() == "websocket":
            key = self.headers.get("Sec-WebSocket-Key", "")
            accept = base64.b64encode(
                hashlib.sha1((key + WS_GUID).encode()).digest()
            ).decode()
            self.send_response(101)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", accept)
            self.end_headers()
            # codex 0.150.1 sends the Responses request over a real
            # WebSocket and treats accept-then-close as a mid-stream
            # disconnect (endless "Reconnecting..." loop). Serve the
            # protocol: read masked frames, answer each message, close
            # cleanly — the harness's own stream semantics.
            ws_loop(self.connection, self.path)
            return
        payload = respond(body, self.path)
        if self.path.startswith("/v1/responses"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
        elif self.path.startswith("/v1/models"):
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
        else:
            payload = b'{"ok":true}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        variation.emit(self.wfile, payload)

    do_POST = _handle
    do_GET = _handle

    def log_message(self, *a):
        pass


if __name__ == "__main__":
    srv = ThreadingHTTPServer(("0.0.0.0", 443), H)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(LEAF_CERT, LEAF_KEY)
    srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
    srv.serve_forever()
