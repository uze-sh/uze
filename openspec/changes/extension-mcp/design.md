## Context

See proposal.md. A harness starts a plugin's stdio MCP server from the
canonical `mcp.json`, resolved by `uze-integrations::shared::mcp`. A
script reaches the host only through `Reach`, under its grants; programs
run confined and offline, `fetch` speaks to one granted host without a
session.

## Goals / Non-Goals

**Goals:** one server's logic serves agents and panels; the operator
consents to each tool by name; a call can take as long as a slow tracker
needs, bounded and declared.

**Non-Goals:** HTTP or SSE servers; MCP resources, prompts or sampling;
servers the operator wrote into a harness's own config rather than a
plugin; confining the server.

## Decisions

- **A grant per tool** (D1), **the server is the plugin's** (D2).
- **The server runs as a harness runs it** (D3): unconfined, from the
  same launch, because the operator already trusted it for agents.
- **A hand-written stdio client** (D4) rather than `rmcp`'s client.
- **One live process per extension and server, ended when idle** (D5).
- **What a call answers** (D6), **how long it may take** (D7).
- **Tools are not checked against the server at compile time** (D8).
