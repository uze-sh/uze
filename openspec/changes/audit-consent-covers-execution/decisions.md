## D1 — Enumerate envelope execution into the trust request instead of refusing the envelope

- status:   pending
- task:     1.2
- commit:   squashed on delivery
- trigger:  the brief allowed either listing the envelope's executable content or refusing to deliver an envelope that carries execution canonical sources do not declare
- decided:  each integration answers `IntegrationPort::package_executions(root)`; the application joins every registered integration's answer to the canonical executions in one `TrustRequest`. Unknown manifest keys, unreadable files and uninterpreted components (`commands`, `bin`, `monitors`, `workflows`, `settings.json`, skill/agent paths outside the canonical dirs) are one `Plugin` execution each, so the package asks (fail closed)
- rejected: refusing the envelope — every real Claude/Codex plugin shipping `hooks/hooks.json` or `.mcp.json` would stop being deliverable natively, for no gain in consent (the author would move the same command into canonical files and be asked the same question)
- blast:    reverting reopens DELIV-1; the new `ExecutionKind::Plugin` variant is serialized in the trust request and shown by the CLI

## D2 — Ask every registered integration, not only detected harnesses

- status:   pending
- task:     1.2
- commit:   squashed on delivery
- trigger:  delivery only reaches detected harnesses, so asking only those would look sufficient
- decided:  all integrations in the registry answer, whether their harness is installed or not
- rejected: detected only — a harness installed after the package receives it on the next republish, and nothing asks then
- blast:    a package with only a Codex envelope asks even on a machine without Codex; reverting removes those prompts and reopens the gap for a later-installed harness

## D3 — Code identity is the whole package tree digest, on every execution but a requirement probe

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  ACQ-1; the existing intent ("rewrites `scripts/check` while keeping the line asks again") was implemented as the lexically named entries, which a sourced helper escapes
- decided:  `CodeIdentity.digest` is the links-aware package digest (`MaterializedPackage::digest` / `UzeStore::digest`, the value a lock pins and which agree by construction); `entries` stays as the named files, for the reader. Stamped also on executions that name no package file (`git status` in a Skill, `npx foo`), since a command can read its working directory or the package by other means. `digest::entries_sha256` lost its only caller and was removed
- rejected: following `source`/`import`/`require` (undecidable, per-language); excluding docs (any file can be read by a script); stamping only executions whose words name a package file (an interpreter started on `-m module` names none)
- blast:    every update that changes any byte of a package that executes anything now asks again (`TRUST_REQUIRED` without a terminal; `auto_update` already reports such a revision as refused rather than applying it). The unit test that asserted "a README change does not ask" was replaced by `rewriting_any_file_of_the_package_a_hook_runs_from_is_new_execution`. No persisted state changes: the installed side is recomputed from the Store

## D4 — A remote MCP server in an envelope is not listed

- status:   pending
- task:     1.2
- commit:   squashed on delivery
- trigger:  an envelope server with `url` and no `command`
- decided:  skipped, exactly as a canonical `mcp.json` remote server is (`mcp_server_execution` returns `None`), so one boundary keeps one rule
- rejected: listing it — would make a remote-only envelope ask while the same canonical package does not
- blast:    none beyond consistency; a vendor field on a remote server that runs a local helper would be missed (residual in evidence.md)
