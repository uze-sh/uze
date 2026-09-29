---
name: probe
description: Parity probe skill. Use when asked to run the parity probe.
---

PARITY_SKILL_BODY_PROBE
root=${PLUGIN_ROOT}
phase=!`cat "${PLUGIN_ROOT}/phases/plan.md"`
script=!`sh "${PLUGIN_ROOT}/skills/probe/scripts/hello.sh"`
