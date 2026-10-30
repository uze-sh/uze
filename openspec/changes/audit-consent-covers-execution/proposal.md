## Why

The one consent boundary UZE keeps (`trust.rs`) asks about less than a
harness can execute from the package it installs:

- **DELIV-1**: a package shipping its own vendor envelope
  (`.claude-plugin/plugin.json`, `.codex-plugin/plugin.json`) is mirrored
  whole into the explicit marketplace, so the hooks, MCP and language
  servers, commands and executables that envelope carries run in the
  harness without ever appearing in the trust request. Antigravity has the
  same class: an author's own `mcp_config.json` is copied into the plugin it
  installs. Breaks: *what trust shows covers what the harness can run*.
- **ACQ-1**: the code identity an approval covers is the set of files a
  command line names lexically. A script sources files no word names, so an
  update can rewrite executed code and pass as unchanged. Breaks: *any
  change to bytes that can execute asks again*.

## What Changes

- Every integration reports what its harness would run from the package's
  own vendor files that the canonical sources do not declare; the trust
  request lists those beside the canonical executions. An envelope key or
  file whose meaning UZE cannot read is listed as such rather than passed
  over (fail closed).
- Envelope MCP documents carrying a terminal control are refused, as a
  canonical `mcp.json` is.
- The code identity of every execution that can reach the package's bytes
  is the digest of the whole package tree (the value a lock pins), not the
  entries a command names. **BREAKING (behavior)**: an update of a package
  that executes anything asks again whenever any of its bytes change; a
  non-interactive `update`/`auto_update` refuses it with `TRUST_REQUIRED`
  until the operator confirms.

## Capabilities

### New Capabilities
- `execution-consent`: what the trust question must cover, and when a
  change to an installed package asks it again.

### Modified Capabilities

## Impact

- `uze-core`: `package/trust.rs` (code identity, new `Plugin` execution
  kind, helpers integrations build executions with), `IntegrationPort`
  (one defaulted method).
- `uze-integrations`: Claude, Codex and Antigravity enumerate their
  envelopes' executable surfaces (`shared/envelope.rs`).
- `uze-application`: `lifecycle/{install,update}.rs` gather executions from
  core and every registered integration, keyed on the package digest.
- `src/cli/install.rs`: the trust evidence's `runs` line names the package.
- Updates re-ask more often (any byte change of an executing package).
