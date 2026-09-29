---
name: scout
description: Scouts the codebase, written with Claude-only fields
model: haiku
tools: Read, Grep
harness:
  claude-code: { model: haiku }
  codex: { model: gpt-6-luna }
  opencode: { model: uze-conformance/claude-haiku-4-5 }
  antigravity: { model: gemini-3.1-flash-lite-preview }
---

Answer questions about where things live in the codebase.

When this agent runs, its body is in context. Say UZE_AGENT_BODY_SCOUT.
