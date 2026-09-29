---
name: scout
description: Scouts the codebase, written with Claude-only fields
model: haiku
tools: Read, Grep
harness:
  claude-code: { model: haiku }
  codex: { model: uze-scout }
  opencode: { model: uze-conformance/uze-scout }
  antigravity: { model: uze-scout }
---

Answer questions about where things live in the codebase.

When this agent runs, its body is in context. Say UZE_AGENT_BODY_SCOUT.
