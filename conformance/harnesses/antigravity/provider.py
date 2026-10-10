#!/usr/bin/env python3
"""Minimal deterministic fake Gemini provider — the permanent runtime stub.

Contract (derived from observed behavior of the REAL AGY 1.1.20):
  * AGY (API-key mode) POSTs to `{GOOGLE_GEMINI_BASE_URL}/v1beta/models/
    {model}:streamGenerateContent?alt=sse` with a GenerateContent JSON body.
  * The response is an SSE stream of `data: {json}` lines. IMPORTANT: it must
    NOT contain a `data: [DONE]` terminal — AGY's stream parser fails on it
    (observed: "error unmarshalling data data: [DONE]").

It also serves the harness's **signed-in plane** over TLS on 443: the
feature flags that decide whether `hooks.json` hooks execute at all, the
identity/account endpoints, and — when the vertical runs signed in — the
model path itself. Observed on a real, logged-in session (AGY 1.1.22
binary, backend UA `antigravity/cli/1.1.24`, 2026-09-02):

  * the language-server process polls Unleash at
    `GET https://antigravity-unleash.goog/api/client/features` (and
    `POST /api/client/register`), evaluating strategies locally with
    `unleash-client-go`; the flag that gates JSON hooks is
    `json-hooks-enabled` ("Whether to enable hooks based on json files"),
    a `flexibleRollout` at 100% constrained to `ide IN [jetski]` — which
    is the context this CLI reports;
  * `POST https://daily-cloudcode-pa.googleapis.com/v1internal:listExperiments`
    answers the same flag as `{"name":"json-hooks-enabled","boolValue":true}`.
    Since 1.3 the same list also gates every customization kind (skills,
    rules, agents, MCP, hooks): see `LIST_EXPERIMENTS`.

Both are served here exactly as recorded — the strategy is replayed, not
flattened to a bare `default`, so the harness's own evaluation is what
decides. The remaining `v1internal:*` endpoints and `play.googleapis.com/log`
are answered with the smallest shape that keeps the harness moving; no tier,
quota or entitlement semantics are invented.

**Signed-in ("consumer") mode.** With a `consumer` token file in place the
CLI stops honouring `GOOGLE_GEMINI_BASE_URL` and speaks the CloudCode
protocol instead. Shapes observed on the same session (structure only; every
value served here is synthetic):

  * `GET https://www.googleapis.com/oauth2/v2/userinfo` ->
    `{id, email, verified_email, name, given_name, picture}`;
  * `POST https://oauth2.googleapis.com/token` (only if a refresh is
    attempted) -> `{access_token, token_type, expires_in}`;
  * `POST https://daily-cloudcode-pa.googleapis.com/v1internal:<rpc>` with
    JSON bodies: `fetchUserInfo` -> `{userSettings, regionCode}`,
    `loadCodeAssist` -> a tier document, `listExperiments` -> the flag list
    above, and `setUserSettings` / `retrieveUserQuotaSummary` /
    `fetchAdminControls` / `fetchAvailableModels` /
    `recordCodeAssistMetrics` / `writeTrajectoryAcls` -> `{}`;
  * `POST …/v1internal:streamGenerateContent`, whose body wraps the same
    GenerateContent request as `{project, requestId, model, userAgent,
    requestType, request:{…}}` and whose SSE events wrap each
    GenerateContentResponse as `{"response": …, "traceId": …, "metadata":
    {}}`.

The model logic is the one below, unchanged: the CloudCode listener unwraps
`request` before the same summary/decision path and re-wraps every event it
emits, so `static`/`toolcall`, `scripted_step` and the variation
machinery behave identically in both auth modes.

Modes (PROVIDER_MODE):
  static   : serve the synthetic SSE fixture to every request (default).
  toolcall : request #1 -> functionCall(call_mcp_tool, FC_ARGS) so the REAL
             harness executes a real MCP server; request #2+ -> FINAL text.
             This proves the deep MCP tool-call path with zero model.

This stub records ONLY a structural summary of each request — never the
verbatim body — into a JSON file (PROVIDER_STRUCT). That summary is the
conformance observation boundary: it lets tests assert what the REAL AGY sent
toward its provider (model-visible vs user-only Skills, MCP tool execution)
without persisting vendor internal payloads.

Env:
  PROVIDER_MODE    : static | toolcall
  PROVIDER_STRUCT  : path to write the structural summary (JSON list)
  RESP_SSE         : path to the synthetic SSE response to serve (static)
  FC_ARGS          : JSON args for the call_mcp_tool functionCall (toolcall)
  FINAL_TEXT       : deterministic text served after request #1 (toolcall)
  PORT (argv[1])   : listen port
"""

import json
import os
import ssl
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer, ThreadingHTTPServer

import capture
import markers
import variation

LEAF_CERT = os.environ.get("LEAF_CERT", "/app/leaf.crt")
LEAF_KEY = os.environ.get("LEAF_KEY", "/app/leaf.key")

STRUCT_PATH = os.environ.get("PROVIDER_STRUCT", "/tmp/agy-provider-struct.json")
RESP_SSE = os.environ.get("PROVIDER_RESP", "")
MODE = os.environ.get("PROVIDER_MODE", "static")
# Arguments are scripted in the shape the tool declares to the model
# (`parametersJsonSchema`, PascalCase plus the `toolSummary`/`toolAction`
# pair every AGY tool requires): the harness validates a call against that
# schema before any hook runs or any tool executes, and rejects the rest as
# "invalid arguments" — a turn that settles with no hook and no tool.
#
# `ServerName` is the name the harness gives the server at load time, not
# the one the plugin declares: since 1.2.2 a server bundled in a plugin is
# namespaced `<plugin>_<server>` ("Fixed MCP servers bundled inside plugins
# colliding … by automatically namespacing plugin MCP servers", 1.2.2
# release notes), and `/mcp` lists it that way. A call naming the bare
# server answers a functionResponse without the proof, which reads as
# "the tool never ran".
FC_ARGS = json.loads(
    os.environ.get(
        "FC_ARGS",
        '{"ServerName":"uze-mcp-conformance_uze-conformance","ToolName":"uze_conformance","Arguments":{},"toolSummary":"Conformance proof","toolAction":"Calling MCP tool"}',
    )
)
FINAL_TEXT = os.environ.get("FINAL_TEXT", "UZE_CONFORMANCE_PASS")
MCP_PROOF = os.environ.get("MCP_PROOF", "UZE_MCP_CONFORMANCE_PROOF_1")

# The hook scenarios script a functionCall to the harness's native shell
# tool (`run_command`); the MCP phases keep the default below.
FC_NAME = os.environ.get("TOOL_NAME", "call_mcp_tool")
#: Several calls, one per model step, as a JSON list of `{"name", "args"}`.
#: Gemini resends the whole conversation, so the step a request is answered
#: with is the number of function responses it already carries.
TOOL_SEQUENCE = json.loads(os.environ.get("TOOL_SEQUENCE", "null") or "null") or [
    {"name": FC_NAME, "args": FC_ARGS}
]

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
# The signed-in listener is threaded (the harness polls its flag plane while
# a turn streams), so the evidence file is a shared resource: one writer at
# a time, or a run loses requests it did observe.
RECORD_LOCK = threading.Lock()
# The summaries this process has recorded. Held in memory and rewritten
# whole: re-reading the file per request made a retrying harness quadratic.
STRUCT = []


def structural_summary(body_text):
    b = json.loads(body_text) if body_text else {}
    body = json.dumps(b)
    has_fc = "functionCall" in body
    has_fr = "functionResponse" in body
    return {
        **markers.summary(body),
        "content_roles": [c.get("role") for c in b.get("contents", [])],
        # Every declaration across every `tools` entry: signed in, the
        # harness sends one entry per tool, so reading only the first left
        # the provider believing the turn declared a single tool and never
        # answering with the call the scenario scripted.
        "tools": [
            declaration.get("name")
            for entry in (b.get("tools") or [])
            for declaration in entry.get("functionDeclarations", [])
        ],
        "tool_config": b.get("toolConfig"),
        "skill_markers": {m: (m in body) for m in SKILL_MARKERS},
        "isolation_markers": {m: (m in body) for m in ISOLATION_MARKERS},
        "continuity_markers": {m: (m in body) for m in CONTINUITY_MARKERS},
        "has_function_call": has_fc,
        "has_function_response": has_fr,
        "hook_markers": {m: (m in body) for m in HOOK_MARKERS},
        "mcp_proof_present": MCP_PROOF in body,
        "has_user_request_tag": "<USER_REQUEST>" in body,
    }


def sse(obj):
    return f"data: {json.dumps(obj)}\n\n".encode()


def scripted_step(body_text):
    """The index of the call this request is answered with, or None once
    the sequence is done.

    Gemini resends the whole conversation and names no call ids, so the
    step is found by replaying it: each `functionCall` already in the
    conversation is matched, in order, to the next step that names that
    tool. A step the provider passed over (`capture.next_scriptable`) is
    simply never matched, and the sequence goes on after it.
    """
    try:
        contents = json.loads(body_text).get("contents") or []
    except (ValueError, AttributeError):
        contents = []
    made = [
        part["functionCall"].get("name")
        for content in contents
        for part in content.get("parts", [])
        if isinstance(part, dict) and isinstance(part.get("functionCall"), dict)
    ]
    step = 0
    for name in made:
        following = next(
            (
                i
                for i in range(step, len(TOOL_SEQUENCE))
                if TOOL_SEQUENCE[i]["name"] == name
            ),
            None,
        )
        if following is None:
            continue
        step = following + 1
    return step if step < len(TOOL_SEQUENCE) else None


def unwrap_consumer_request(body_text):
    """The GenerateContent request the signed-in envelope carries.

    CloudCode wraps it as `{project, requestId, model, userAgent,
    requestType, request:{contents, systemInstruction, …}}`; everything the
    provider decides on (declared tools, roles, markers) lives in `request`,
    so the model path sees exactly the body it sees in API-key mode."""
    try:
        wrapper = json.loads(body_text) if body_text else {}
    except ValueError:
        return body_text
    if isinstance(wrapper, dict) and isinstance(wrapper.get("request"), dict):
        return json.dumps(wrapper["request"])
    return body_text


def consumer_model(body_text):
    """The model the signed-in envelope asks for: CloudCode names it beside
    `request`, never inside it, so a dispatched agent's own model is read
    here or not at all."""
    try:
        wrapper = json.loads(body_text) if body_text else {}
    except ValueError:
        return None
    return wrapper.get("model") if isinstance(wrapper, dict) else None


def wrap_consumer_stream(payload):
    """Each `data: <GenerateContentResponse>` frame, re-framed as the
    signed-in envelope `data: {"response": …, "traceId": …, "metadata": {}}`.

    Framing stays `\\n\\n` — the same delimiter `variation.emit` splits on,
    so the adversarial variations keep working in this mode too."""
    out = b""
    for frame in payload.split(b"\n\n"):
        frame = frame.strip()
        if not frame.startswith(b"data: "):
            continue
        try:
            response = json.loads(frame[len(b"data: ") :])
        except ValueError:
            continue
        out += sse({"response": response, "traceId": "synthetic", "metadata": {}})
    return out


def record_request(handler, body_text, model=None):
    """Appends this request's structural summary to the run's evidence and
    returns it. Never the verbatim body — that boundary is the whole point
    of the summary (see the module docstring). `model` is the one the
    signed-in envelope names, which the unwrapped body no longer carries."""
    with RECORD_LOCK:
        n = COUNTER["n"]
        COUNTER["n"] += 1
        rec = {
            "method": handler.command,
            "path": handler.path,
            "seq": n,
            "summary": structural_summary(body_text),
        }
        if model:
            rec["summary"]["model"] = model
        if not STRUCT and os.path.exists(STRUCT_PATH):
            try:
                STRUCT.extend(json.load(open(STRUCT_PATH)))
            except Exception:
                pass
        STRUCT.append(rec)
        # Written whole or not at all: the file is rewritten on every
        # request and a scenario reads it from outside this container
        # with `cat`, so a reader landing mid-write would see a truncated
        # document — which `provider_struct` cannot tell from "no request
        # carried that marker", and which fails a check about the harness
        # over something that never had to do with it.
        pending = f"{STRUCT_PATH}.pending"
        with open(pending, "w") as f:
            json.dump(STRUCT, f, indent=1)
        os.replace(pending, STRUCT_PATH)
    print(f"[provider:{MODE}] {handler.command} {handler.path} req#{n}", flush=True)
    return rec


def text_frame(text):
    return sse(
        {
            "candidates": [
                {
                    "content": {"parts": [{"text": text}], "role": "model"},
                    "finishReason": "STOP",
                    "index": 0,
                }
            ]
        }
    )


def is_user_turn(body_text):
    """Whether the conversation carries the person's request: every turn
    agy sends on a person's behalf wraps it in `<USER_REQUEST>`, and the
    whole conversation is resent with each request."""
    try:
        contents = json.loads(body_text).get("contents") or []
    except (ValueError, AttributeError):
        return False
    return "<USER_REQUEST>" in json.dumps(contents)


def model_payload(body_text):
    """The SSE the mode dictates for this request — the one decision path,
    shared by the API-key listener and the signed-in one.

    The harness also makes side requests (a lighter model, no tools
    declared) around the user's turn; `capture.scriptable` answers those
    with text, so the call lands on the turn that offered the tool.

    A dispatched agent's own conversation is not the user's turn either:
    it opens on a `<SYSTEM_MESSAGE>` instead of a `<USER_REQUEST>`, declares
    a narrower set of tools, and would otherwise be handed the very call
    that dispatched it (observed on 1.3.3). It is answered with text."""
    due = (
        scripted_step(body_text)
        if MODE == "toolcall" and is_user_turn(body_text)
        else None
    )
    step = capture.next_scriptable(body_text, TOOL_SEQUENCE, due)
    if step is not None:
        call = TOOL_SEQUENCE[step]
        fc = {"functionCall": {"name": call["name"], "args": call["args"]}}
        return sse(
            {
                "candidates": [
                    {
                        "content": {"parts": [fc], "role": "model"},
                        "finishReason": "STOP",
                        "index": 0,
                    }
                ]
            }
        )
    if MODE == "toolcall":
        return text_frame(FINAL_TEXT)
    if RESP_SSE and os.path.exists(RESP_SSE):
        with open(RESP_SSE, "rb") as f:
            payload = f.read()
        if payload:
            return payload
    return text_frame(FINAL_TEXT)


def serve_model(handler, body_text, consumer=False, model=None):
    """Records the request and streams the answer; `consumer` re-frames
    every event into the signed-in envelope."""
    record_request(handler, body_text, model)
    payload = model_payload(body_text)
    if consumer:
        payload = wrap_consumer_stream(payload)
    handler.send_response(200)
    handler.send_header("Content-Type", "text/event-stream; charset=UTF-8")
    handler.send_header("Content-Length", str(len(payload)))
    handler.end_headers()
    variation.emit(handler.wfile, payload)


class H(BaseHTTPRequestHandler):
    def _handle(self):
        body = capture.read_body(self).decode("utf-8", "replace")
        serve_model(self, body)

    do_POST = _handle
    do_GET = _handle

    def log_message(self, *a):
        pass


# The Unleash feature set, replayed from the recorded response. Only the
# flag the Lab depends on is served: the real list carries ~450 entries the
# harness never asks about individually, and an invented one would be a
# claim nobody measured.
UNLEASH_FEATURES = {
    "version": 2,
    "features": [
        {
            "name": "json-hooks-enabled",
            "type": "release",
            "description": "Whether to enable hooks based on json files",
            "enabled": True,
            "stale": False,
            "impressionData": False,
            "project": "default",
            "strategies": [
                {
                    "name": "flexibleRollout",
                    "constraints": [
                        {
                            "contextName": "ide",
                            "operator": "IN",
                            "caseInsensitive": False,
                            "inverted": False,
                            "values": ["jetski"],
                        }
                    ],
                    "parameters": {
                        "groupId": "json-hooks-enabled",
                        "rollout": "100",
                        "stickiness": "default",
                    },
                    "variants": [],
                }
            ],
            "variants": [],
        }
    ],
}

# The second delivery path for the same gate, and since 1.3 the one every
# customization kind is resolved against: an absent
# `enable-customization-kind-*` reads false, and the session then loads no
# skill, rule, agent, MCP server or hook at all — the user's own as much as
# UZE's — with the system prompt's `skills` and `user_rules` sections empty.
# Recorded on a real signed-in 1.3.3 session (2026-10-10). Only the flags
# naming a customization kind are served, at the values observed there; the
# official marketplace URL is left out because the Lab has no Internet.
LIST_EXPERIMENTS = {
    "experimentIds": [],
    "flags": [
        {
            "name": "Chat__code_customization_enable_learn_more_message",
            "boolValue": True,
        },
        {"name": "Chat__enable_agent_mode_slash_commands", "boolValue": True},
        {"name": "Chat__enable_agentic_chat_ij", "boolValue": True},
        {"name": "Chat__enable_chat_agentic_mcp_chat", "boolValue": False},
        {"name": "Chat__enable_chat_crescendo_agents", "boolValue": True},
        {"name": "Chat__enable_code_customization_webview", "boolValue": True},
        {"name": "Chat__enable_mcp_server", "boolValue": False},
        {"name": "Chat__enable_mcp_server_ij", "boolValue": True},
        {"name": "DatacloudMcp__enable_stdio_mcp_proxy", "boolValue": True},
        {"name": "GcaAipluginSwingToCompose__enable_compose", "boolValue": False},
        {
            "name": "GcliAgentHistoryTruncation__agent_history_retained_messages",
            "flagId": 45773190,
            "intValue": "15",
        },
        {
            "name": "GcliAgentHistoryTruncation__agent_history_truncation_threshold",
            "flagId": 45773189,
            "intValue": "30",
        },
        {
            "name": "GcliAgentHistoryTruncation__enable_agent_history_truncation",
            "flagId": 45773188,
            "boolValue": False,
        },
        {"name": "SDLCAgents__enable_anthropic_model_connection", "boolValue": False},
        {"name": "SDLCAgents__enable_azure_model_connection", "boolValue": False},
        {"name": "SDLCAgents__enable_gemini_model_connection", "boolValue": False},
        {"name": "SDLCAgents__enable_rest_model_connection", "boolValue": False},
        {"name": "agent-retry-config", "stringValue": ""},
        {"name": "agent-script-reroute", "stringValue": ""},
        {"name": "agy-plugin-shelves", "stringValue": ""},
        {"name": "browser-subagent-model", "stringValue": "MODEL_PLACEHOLDER_M18"},
        {"name": "cascade-agent-api-config", "stringValue": ""},
        {"name": "convenient-multi-agent-coordination", "boolValue": False},
        {"name": "customization-token-budget", "intValue": "20000"},
        {"name": "deepagent-arm", "stringValue": ""},
        {"name": "default_subagent_interaction_timeout_seconds", "intValue": "60"},
        {"name": "disable-tool-hook-notices", "boolValue": False},
        {"name": "enable-agent-omnitrace", "boolValue": False},
        {"name": "enable-agent-plugins-spec", "boolValue": False},
        {"name": "enable-agent-team", "boolValue": False},
        {"name": "enable-battle-mode-custom-agents", "boolValue": True},
        {"name": "enable-browser-subagent-v2", "boolValue": True},
        {"name": "enable-concierge-agent", "boolValue": False},
        {"name": "enable-customization-kind-agent", "boolValue": True},
        {"name": "enable-customization-kind-flow", "boolValue": False},
        {"name": "enable-customization-kind-hooks", "boolValue": True},
        {"name": "enable-customization-kind-mcp", "boolValue": True},
        {"name": "enable-customization-kind-plugin", "boolValue": True},
        {"name": "enable-customization-kind-rule", "boolValue": True},
        {"name": "enable-customization-kind-skill", "boolValue": True},
        {"name": "enable-customization-kind-workflow", "boolValue": True},
        {"name": "enable-customization-load-error-notice", "boolValue": False},
        {"name": "enable-customization-skills", "boolValue": True},
        {"name": "enable-deepagent", "boolValue": False},
        {"name": "enable-fork-with-subagents", "boolValue": False},
        {"name": "enable-generative-hooks", "boolValue": False},
        {"name": "enable-hook-status", "boolValue": True},
        {"name": "enable-image-generator-subagent", "boolValue": True},
        {"name": "enable-list-plugin-accounts-tool", "boolValue": False},
        {"name": "enable-markdown-agents", "boolValue": True},
        {"name": "enable-mcp-apps", "boolValue": False},
        {"name": "enable-mcp-non-blocking-turn-load", "boolValue": True},
        {"name": "enable-mcp-plugin-auth", "boolValue": False},
        {"name": "enable-plugin-auth", "boolValue": False},
        {"name": "enable-plugin-bundles", "boolValue": False},
        {"name": "enable-plugin-context-providers", "boolValue": False},
        {"name": "enable-plugin-sidebar-section", "boolValue": False},
        {"name": "enable-plugin-themes", "boolValue": False},
        {"name": "enable-plugins", "boolValue": False},
        {"name": "enable-remote-agent-fastpush-pin", "boolValue": True},
        {"name": "enable-skill-accumulator", "boolValue": False},
        {"name": "enable-skill-icons", "boolValue": False},
        {"name": "enable-skill-search-tool", "boolValue": False},
        {"name": "enable-subagent-hub", "boolValue": False},
        {"name": "enable-subagent-user-messaging", "boolValue": False},
        {"name": "enable-teamwork-subagent", "boolValue": False},
        {"name": "inject_3_memories_1_skill", "boolValue": False},
        {
            "name": "invoke-subagent-config",
            "stringValue": '{"enabled": true, "always_inherit_model": false}',
        },
        {"name": "jetski-plugin-promotions", "stringValue": ""},
        {"name": "jetski-unified-customizations-panel-enabled", "boolValue": False},
        {"name": "json-hooks-enabled", "boolValue": True},
        {"name": "mcp-lazy-load-tools", "boolValue": True},
        {"name": "plugin-kill-switch", "stringValue": ""},
        {"name": "rules-token-budget", "intValue": "20000"},
        {"name": "send-subagent-initial-prompt-as-message", "boolValue": True},
    ],
}


# The signed-in identity. Every field is a literal: the Lab's account is
# nobody's, and `uze.invalid` can never resolve.
USERINFO = {
    "id": "synthetic",
    "email": "conformance@uze.invalid",
    "verified_email": True,
    "name": "UZE Conformance",
    "given_name": "UZE",
    "picture": "https://lh3.googleusercontent.com/synthetic",
}

# Served only if the CLI decides to refresh; the token fixture's expiry is
# far enough away that it should not. Answering with the same synthetic
# access token keeps a refresh from being the thing that ends a run.
REFRESHED_TOKEN = {
    "access_token": "synthetic-access-token",
    "token_type": "Bearer",
    "expires_in": 3600,
}

# The account shapes the CLI reads before a turn. Minimal by intent: a tier
# document with an id and a name, no privacy-notice text, no entitlement
# semantics invented.
FETCH_USER_INFO = {"userSettings": {"telemetryEnabled": False}, "regionCode": "us"}

# The model catalogue. In API-key mode the CLI carries its own Gemini
# catalogue (`modelProvider: gemini` + GEMINI_API_KEY); signed in, it has
# none until the backend answers `fetchAvailableModels`, and every turn dies
# with "neither PlanModel nor RequestedModel specified" — the executor
# resolves a model to an `exa.codeium_common_pb.Model` enum, and an id
# without one resolves to nothing.
#
# The response shape is
# `google.internal.cloud.code.v1internal.FetchAvailableModelsResponse`, read
# from the descriptor this binary embeds (the CLI parses it with protojson,
# so a wrong cardinality or an unknown enum name is a hard error). The
# catalogue itself is the Lab's, not a recording: two models, because the
# harness uses two (the user's turn, and a lighter side call), named for the
# newest Gemini the binary's own enum knows — the 3.x ids it ships are
# `MODEL_PLACEHOLDER_*` in this build, so no honest id/enum pair exists for
# them here. Everything else stays default.
AGENT_MODEL_ID = "gemini-3.1-pro-preview"
SIDE_MODEL_ID = "gemini-3.1-flash-lite-preview"


def model_details(display_name, model):
    return {
        "displayName": display_name,
        "model": model,
        "modelProvider": "MODEL_PROVIDER_GOOGLE",
        "maxTokens": 1000000,
        "maxOutputTokens": 65536,
    }


FETCH_AVAILABLE_MODELS = {
    "models": {
        AGENT_MODEL_ID: dict(
            model_details("Gemini 3.1 Pro", "MODEL_PLACEHOLDER_M50"),
            recommended=True,
        ),
        SIDE_MODEL_ID: model_details("Gemini 3.1 Flash Lite", "MODEL_PLACEHOLDER_M51"),
    },
    "defaultAgentModelId": AGENT_MODEL_ID,
    "agentModelSorts": [
        {
            "displayName": "Models",
            "groups": [
                {"displayName": "Gemini", "modelIds": [AGENT_MODEL_ID, SIDE_MODEL_ID]}
            ],
        }
    ],
    "commandModelIds": [SIDE_MODEL_ID],
    "commitMessageModelIds": [SIDE_MODEL_ID],
    "webSearchModelIds": [SIDE_MODEL_ID],
    "mqueryModelIds": [SIDE_MODEL_ID],
    "tabModelIds": [SIDE_MODEL_ID],
    # Each tier is a *repeated* string in the descriptor, not a single id.
    "tieredModelIds": {
        "flashLite": [SIDE_MODEL_ID],
        "flash": [SIDE_MODEL_ID],
        "pro": [AGENT_MODEL_ID],
    },
    "experimentIds": [],
}
LOAD_CODE_ASSIST = {
    "currentTier": {
        "id": "free-tier",
        "name": "Antigravity",
        "description": "Synthetic tier served by the Conformance Lab",
    },
    "allowedTiers": [{"id": "free-tier", "name": "Antigravity", "isDefault": True}],
}


def unleash_features():
    """The recorded feature set, or — under `UNLEASH_UNCONSTRAINED=1` — the
    same flag with its `ide IN [jetski]` constraint dropped. The switch is a
    diagnostic: it separates "the harness never received the flag" from
    "the harness received it and its own context did not satisfy the
    strategy". A canonical run always serves the recording."""
    if not os.environ.get("UNLEASH_UNCONSTRAINED"):
        return UNLEASH_FEATURES
    relaxed = json.loads(json.dumps(UNLEASH_FEATURES))
    for feature in relaxed["features"]:
        for strategy in feature["strategies"]:
            strategy["constraints"] = []
    return relaxed


class SignedIn(BaseHTTPRequestHandler):
    """The harness's signed-in plane: feature flags, the identity and
    account endpoints it calls around them, and — when the vertical runs
    signed in — the CloudCode model path. Every request is logged with its
    Host so a run records which delivery path this harness build used, and
    an unrecognized one is answered `{}` and named in the log rather than
    guessed at."""

    def _answer(self, status, payload):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _handle(self):
        body = capture.read_body(self).decode("utf-8", "replace")
        host = self.headers.get("Host", "?")
        print(f"[provider:flags] {self.command} {host}{self.path}", flush=True)
        # The RPC is the path without its query: the model call arrives as
        # `…:streamGenerateContent?alt=sse`, and matching the raw path sent
        # it to the catch-all — a harness retrying its own turn forever.
        route = self.path.split("?", 1)[0]
        if route.endswith(":streamGenerateContent"):
            serve_model(
                self,
                unwrap_consumer_request(body),
                consumer=True,
                model=consumer_model(body),
            )
        elif route.startswith("/api/client/register"):
            self._answer(202, {})
        elif route.startswith("/api/client/features"):
            self._answer(200, unleash_features())
        elif route.endswith(":listExperiments"):
            self._answer(200, LIST_EXPERIMENTS)
        elif route.startswith("/oauth2/v2/userinfo"):
            self._answer(200, USERINFO)
        elif route.startswith("/token"):
            self._answer(200, REFRESHED_TOKEN)
        elif route.endswith(":fetchUserInfo"):
            self._answer(200, FETCH_USER_INFO)
        elif route.endswith(":loadCodeAssist"):
            self._answer(200, LOAD_CODE_ASSIST)
        elif route.endswith(":fetchAvailableModels"):
            self._answer(200, FETCH_AVAILABLE_MODELS)
        else:
            # Everything else the CLI touches online (setUserSettings,
            # retrieveUserQuotaSummary, fetchAdminControls,
            # recordCodeAssistMetrics, writeTrajectoryAcls, /log): answered
            # with an empty object, which is the smallest shape that keeps
            # the harness moving and invents nothing.
            self._answer(200, {})

    do_POST = _handle
    do_GET = _handle
    do_PUT = _handle

    def log_message(self, *a):
        pass


def serve_flags():
    server = ThreadingHTTPServer(("0.0.0.0", 443), SignedIn)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(LEAF_CERT, LEAF_KEY)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    server.serve_forever()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9999
    if os.path.exists(LEAF_CERT):
        threading.Thread(target=serve_flags, daemon=True).start()
    HTTPServer(("0.0.0.0", port), H).serve_forever()
