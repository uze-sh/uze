## Context

See proposal.md (Why) and findings.md. The trust request is built in
`uze-application` (`lifecycle/install.rs::authorize`, and
`lifecycle/update.rs::executable_capabilities_of` for the installed side)
from `uze_core::trust::package_executions`, which reads only the canonical
resources the Engine discovers. What a harness actually receives is decided
per integration: Claude and Codex mirror the whole package into their
explicit marketplace when it ships the vendor envelope (`has_envelope`,
`materialize_explicit_package`); Antigravity copies an author's own
`mcp_config.json` into the plugin it generates; OpenCode delivers capability
by capability from canonical sources only.

Only `uze-integrations` may name a harness, so core cannot know what
`.claude-plugin/plugin.json` means.

## Goals / Non-Goals

**Goals:** trust shows a superset of what a harness can execute from the
package; any change to bytes that can execute asks again; both fail closed
without a terminal (`TRUST_REQUIRED`).

**Non-Goals:** a sandbox; reading what a harness downloads at run time
(`npx foo`); changing how envelopes are delivered.

## Decisions

### DELIV-1: integrations enumerate their envelope into the same request

`IntegrationPort` gains `package_executions(package_root)` (default: none),
answering "what would this harness run from the package's own vendor files
that canonical sources do not declare". The application asks **every
registered** integration, not only the detected ones, because a harness
installed later receives the package through a republish that never asks
again. The answers join the canonical executions in one `TrustRequest`
(identical entries deduplicated), so there is still one consent boundary.

Per vendor (`shared/envelope.rs` holds the readers):
- **Claude** (`.claude-plugin/plugin.json` present): inline or referenced
  `hooks` and `mcpServers`, `lspServers`; auto-discovered
  `hooks/hooks.json`, `.mcp.json`, `.lsp.json`; `commands`, `bin`,
  `monitors`, `workflows`, `settings.json` as named components; `skills`
  or `agents` paths outside the canonical directories as named components.
- **Codex** (`.codex-plugin/plugin.json` present): `hooks`, `mcpServers`
  (path or inline), auto-discovered `hooks/hooks.json` and `.mcp.json`;
  `skills` outside `skills/` as a named component.
- **Antigravity**: an author `mcp_config.json`, which the generated plugin
  copies verbatim.
- **OpenCode**: none; it delivers canonical sources only.

Fail closed: a manifest key outside a small declarative allow-list
(`name`, `version`, `description`, `author`, `homepage`, `repository`,
`license`, `keywords`, `$schema`, plus Codex `interface` and Claude
`outputStyles`) becomes a `Plugin` execution showing the key and its value;
a vendor file that does not parse becomes one saying so; a hook handler
with no `command` is listed with its JSON. MCP documents from an envelope
pass the same `terminal_control_in` guard canonical `mcp.json` does.

*Rejected:* refusing to deliver an envelope that carries execution the
canonical sources do not declare. Real Claude and Codex plugins ship
`hooks/hooks.json` and `.mcp.json`; refusing would make every one of them
undeliverable natively, and an author would only move the same commands
into canonical files to be asked the same question. Enumerating keeps one
question with the evidence in it. *Rejected:* one opaque "this package
ships an envelope" entry for every envelope: it asks about metadata-only
envelopes and shows nothing a person can judge.

### ACQ-1: the code identity is the package digest

`CodeIdentity.digest` becomes the package's tree digest (the links-aware
value a lock pins: `MaterializedPackage::digest` / `UzeStore::digest`, which
agree by construction), stamped on every execution except a requirement's
version probe (which runs a program of the machine). `entries` keeps the
files the command names, for the reader only. The existing intent in
`trust.rs` ("a revision that rewrites `scripts/check` while keeping the
line asks again") is kept and generalised: a command reaches files no word
names (a sourced helper, an imported module, a `cwd` the interpreter reads),
so no lexical subset is sound; the package tree is the only set that is.

*Rejected:* following `source`/`import` statements (undecidable in
general); excluding documentation files (a script can read any of them).

Consequence: an executing package now re-asks on any byte change, prose
included. `update` already asks through the same authority, and
`auto_update` already runs under `NoTrustAuthority` and reports a refused
revision rather than applying it, so no new UX path is needed; the
`rewriting_the_script_a_hook_runs_is_new_execution` unit test, which
encoded "a README change does not ask", is replaced by
`rewriting_any_file_of_the_package_a_hook_runs_from_is_new_execution`.
Previous executions are recomputed from the Store, never persisted, so no
migration is needed.

## Risks / Trade-offs

- [A vendor adds a new auto-discovered file] → it is not enumerated until
  an integration learns it; the whole-tree digest still makes any change to
  it ask again on update, and unknown manifest keys already ask.
- [More prompts on update] → accepted; the alternative is silent code
  execution. Operators who trust a source pass `--trust`.
- [A remote MCP server (no `command`) in an envelope] → not listed, as for
  canonical `mcp.json`: nothing runs locally.
