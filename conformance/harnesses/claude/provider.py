#!/usr/bin/env python3
"""Minimal deterministic fake Anthropic API provider — the Claude Code stub.

Contract (derived from observed behavior of the REAL claude 2.1.245):
  * The interactive TUI runs a connectivity preflight against hardcoded hosts
    (api.anthropic.com, platform.claude.com, ...). This stub terminates TLS
    for those hosts (cert for all SANs, injected CA via NODE_EXTRA_CA_CERTS +
    /etc/hosts) and answers non-/v1/messages paths with a synthetic 200.
  * Model calls are `POST /v1/messages` (stream=true) in the Anthropic
    Messages API shape; the deterministic response is an SSE stream of
    message_start/content_block_start/content_block_delta/content_block_stop/
    message_delta/message_stop events.

Modes (PROVIDER_MODE):
  static   : every /v1/messages turn -> a text SSE (RESPONSE_TEXT).
  toolcall : /v1/messages carrying a user text turn -> tool_use SSE
             (MCP_TOOL); /v1/messages carrying a tool_result -> final text
             SSE (FINAL_TEXT). Proves the deep MCP tool-call path.

This stub records ONLY a structural summary of each request (skill markers,
tool names, tool_use/tool_result presence, MCP proof) — never the verbatim
body — into PROVIDER_STRUCT: the model-facing observation boundary.

Env:
  PROVIDER_MODE, PROVIDER_STRUCT, RESPONSE_TEXT, FINAL_TEXT, MCP_TOOL,
  MCP_PROOF, LEAF_CERT, LEAF_KEY
"""

import json
import os
import ssl
from http.server import BaseHTTPRequestHandler, HTTPServer

import capture
import markers
import variation

STRUCT_PATH = os.environ.get("PROVIDER_STRUCT", "/tmp/claude-struct.json")
MODE = os.environ.get("PROVIDER_MODE", "static")
RESPONSE_TEXT = os.environ.get("RESPONSE_TEXT", "UZE_CONFORMANCE_OK")
FINAL_TEXT = os.environ.get("FINAL_TEXT", "UZE_CONFORMANCE_PASS")
MCP_TOOL = os.environ.get("MCP_TOOL", "mcp__uze-conformance__uze_conformance")
MCP_PROOF = os.environ.get("MCP_PROOF", "UZE_MCP_CONFORMANCE_PROOF_1")
LEAF_CERT = os.environ.get("LEAF_CERT", "/app/leaf.crt")
LEAF_KEY = os.environ.get("LEAF_KEY", "/app/leaf.key")

# The hook scenarios script a tool call to the harness's native shell tool;
# the MCP phases keep their own default. TOOL_ARGS mirrors the `input` the
# hook's normalized ABI payload will carry.
TOOL_NAME = os.environ.get("TOOL_NAME", MCP_TOOL)
TOOL_ARGS = json.loads(os.environ.get("TOOL_ARGS", "{}"))
#: When set, the call is scripted only for a request carrying this text: a
#: subagent's first request is a user turn too.
TOOL_TRIGGER = os.environ.get("TOOL_TRIGGER", "")
#: Several calls, one per model step, as a JSON list of `{"name", "args"}`.
#: Step N+1 answers the request that carries step N's `tool_result`, so one
#: turn can drive every tool a hook scene intercepts.
TOOL_SEQUENCE = json.loads(os.environ.get("TOOL_SEQUENCE", "null") or "null") or [
    {"name": TOOL_NAME, "args": TOOL_ARGS}
]


#: Auto mode, Claude Code's default permission mode since 2.1.290, asks a
#: model whether each tool call should be blocked before it runs: a
#: tool-less request whose system prompt opens "You are a security
#: monitor", asked without streaming, whose answer must begin `<block>` and
#: which stops at `</block>` (measured, `--discovery`). A real model allows
#: the Lab's commands — a file under the project, a `printf` — so the
#: synthetic one does too. Answering it any other way is what the harness
#: reports as "Classifier unavailable", denying every call.
CLASSIFIER_ALLOW = "<block>no"


#: The model a response names when the request named none. A response
#: otherwise names the model its request asked for, as the API does.
DEFAULT_MODEL = "claude-opus-5"


def message(text, stop_sequence=None, model=None):
    """A whole Messages API response, for a request that did not ask to
    stream — the classifier's does not, and an event stream handed to it is
    a response it cannot read."""
    return {
        "id": "msg_uze_classifier",
        "type": "message",
        "role": "assistant",
        "model": model or DEFAULT_MODEL,
        "content": [{"type": "text", "text": text}],
        "stop_reason": "stop_sequence" if stop_sequence else "end_turn",
        "stop_sequence": stop_sequence,
        "usage": {"input_tokens": 10, "output_tokens": 3},
    }


def is_permission_classifier(body):
    system = json.dumps(body.get("system") or "")
    return "You are a security monitor" in system and not body.get("tools")


def tool_use_id(step):
    return f"toolu_uze_{step + 1}"


def scripted_step(messages):
    """The index of the call this request is answered with, or None.

    A request carrying the result of step N is answered with step N+1; one
    carrying the last step's result, or any result not of this sequence, is
    answered with the final text. A user turn with no result yet starts the
    sequence.
    """
    for step in range(len(TOOL_SEQUENCE) - 1, -1, -1):
        if f'"tool_use_id": "{tool_use_id(step)}"' in messages:
            following = step + 1
            return following if following < len(TOOL_SEQUENCE) else None
    if "tool_result" in messages:
        return None
    if MODE != "toolcall" or '"type": "text"' not in messages:
        return None
    if TOOL_TRIGGER and TOOL_TRIGGER not in messages:
        return None
    return 0


ISOLATION_MARKERS = ["already isolated", "UZE_CONFORMANCE_REBASE"]
#: One turn each. A single request carrying both is what proves a relaunched
#: process was given the conversation the first one made — the union across
#: requests cannot say that, since the first process's own request already
#: carried the first of them.
CONTINUITY_MARKERS = ["UZE_CONFORMANCE_ACORN", "UZE_CONFORMANCE_WALNUT"]
SKILL_MARKERS = [
    "flow:commit",
    "flow:review",
    # A Skill's *body* reaches the model only when the Skill was
    # invoked — a listing carries name and description alone. These are
    # what tell an invocation from an offer.
    "UZE_SKILL_BODY_COMMIT",
    "UZE_SKILL_BODY_REVIEW",
    "UZE_SKILL_BODY_ANALYZE",
]
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
COUNTER = {"n": 0}


def structural_summary(body_text):
    b = json.loads(body_text) if body_text else {}
    body = json.dumps(b)
    tools = [t.get("name") for t in b.get("tools", [])] if b.get("tools") else []
    has_tool_use = "tool_use" in body
    has_tool_result = "tool_result" in body
    return {
        **markers.summary(body),
        "content_types": [c.get("type") for c in b.get("content", [])],
        "tools": tools,
        "skill_markers": {m: (m in body) for m in SKILL_MARKERS},
        "isolation_markers": {m: (m in body) for m in ISOLATION_MARKERS},
        "continuity_markers": {m: (m in body) for m in CONTINUITY_MARKERS},
        "has_tool_use": has_tool_use,
        "has_tool_result": has_tool_result,
        "hook_markers": {m: (m in body) for m in HOOK_MARKERS},
        "mcp_proof_present": MCP_PROOF in body,
        "user_text_present": '"type": "text"' in body and '"role": "user"' in body,
        "preview": body[:900],
        "len": len(body),
    }


def sse(events):
    return "".join(f"event: {e}\ndata: {json.dumps(d)}\n\n" for e, d in events).encode()


def text_events(text, stop_sequence=None, model=None):
    return [
        (
            "message_start",
            {
                "type": "message_start",
                "message": {
                    "id": "msg_uze_1",
                    "type": "message",
                    "role": "assistant",
                    "model": model or DEFAULT_MODEL,
                    "content": [],
                    "stop_reason": None,
                    "usage": {"input_tokens": 10, "output_tokens": 1},
                },
            },
        ),
        (
            "content_block_start",
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""},
            },
        ),
        (
            "content_block_delta",
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": text},
            },
        ),
        ("content_block_stop", {"type": "content_block_stop", "index": 0}),
        (
            "message_delta",
            {
                "type": "message_delta",
                "delta": {
                    "stop_reason": "stop_sequence" if stop_sequence else "end_turn",
                    "stop_sequence": stop_sequence,
                },
                "usage": {"output_tokens": 3},
            },
        ),
        ("message_stop", {"type": "message_stop"}),
    ]


def tool_use_events(step, model=None):
    call = TOOL_SEQUENCE[step]
    return [
        (
            "message_start",
            {
                "type": "message_start",
                "message": {
                    "id": "msg_uze_2",
                    "type": "message",
                    "role": "assistant",
                    "model": model or DEFAULT_MODEL,
                    "content": [],
                    "stop_reason": None,
                    "usage": {"input_tokens": 10, "output_tokens": 1},
                },
            },
        ),
        # The real Messages API opens a tool_use block with an empty input
        # and streams the arguments as `input_json_delta`; the harness
        # accumulates only those deltas, so an input placed on the start
        # event is silently dropped and any tool with required parameters
        # is rejected as "Invalid tool parameters" before a hook can run.
        (
            "content_block_start",
            {
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "tool_use",
                    "id": tool_use_id(step),
                    "name": call["name"],
                    "input": {},
                },
            },
        ),
        (
            "content_block_delta",
            {
                "type": "content_block_delta",
                "index": 0,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": json.dumps(call["args"]),
                },
            },
        ),
        ("content_block_stop", {"type": "content_block_stop", "index": 0}),
        (
            "message_delta",
            {
                "type": "message_delta",
                "delta": {"stop_reason": "tool_use", "stop_sequence": None},
                "usage": {"output_tokens": 3},
            },
        ),
        ("message_stop", {"type": "message_stop"}),
    ]


class H(BaseHTTPRequestHandler):
    def _handle(self):

        body = capture.read_body(self).decode("utf-8", "replace")
        n = COUNTER["n"]
        COUNTER["n"] += 1
        rec = {
            "method": self.command,
            "path": self.path,
            "seq": n,
            "summary": structural_summary(body),
        }
        struct = []
        if os.path.exists(STRUCT_PATH):
            try:
                struct = json.load(open(STRUCT_PATH))
            except Exception:
                struct = []
        struct.append(rec)
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
        print(
            f"[claude-provider:{MODE}] {self.command} {self.path} req#{n}", flush=True
        )

        if self.path.startswith("/v1/messages"):
            b = json.loads(body) if body else {}
            msgs = json.dumps(b.get("messages", []))
            if is_permission_classifier(b):
                payload = json.dumps(
                    message(
                        CLASSIFIER_ALLOW, stop_sequence="</block>", model=b.get("model")
                    )
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            due = scripted_step(msgs)
            step = capture.next_scriptable(body, TOOL_SEQUENCE, due)
            if step is not None:
                payload = sse(tool_use_events(step, model=b.get("model")))
            elif due is not None or "tool_result" in msgs:
                payload = sse(text_events(FINAL_TEXT, model=b.get("model")))
            else:
                payload = sse(text_events(RESPONSE_TEXT, model=b.get("model")))
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
        else:
            payload = b'{"success":true}'
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
    srv = HTTPServer(("0.0.0.0", 443), H)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(LEAF_CERT, LEAF_KEY)
    srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
    srv.serve_forever()
