---
name: probe
description: Parity probe skill. Use when asked to run the parity probe.
---

PARITY_SKILL_BODY_PROBE
root=${CLAUDE_PLUGIN_ROOT}
phase=!`cat "${CLAUDE_PLUGIN_ROOT}/phases/plan.md"`
script=!`sh "${CLAUDE_PLUGIN_ROOT}/skills/probe/scripts/hello.sh"`
