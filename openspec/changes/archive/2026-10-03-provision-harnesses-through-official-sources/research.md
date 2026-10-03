# Official provisioning routes (2026-08-21)

This record limits v0 automation to Linux, macOS and WSL. A route is invoked only by
the owning integration during explicit `uze setup` (or the setup the workspace runs
when it opens); `uze install` never invokes one.

| Harness | Missing executable | Existing executable | Verify | Evidence |
|---|---|---|---|---|
| Claude Code | `curl -fsSL https://claude.ai/install.sh | bash` | `claude update` | `claude --version` | [Claude Code installation](https://code.claude.com/docs/en/installation), [CLI reference](https://docs.anthropic.com/en/docs/claude-code/cli-usage) |
| Codex | `curl -fsSL https://chatgpt.com/codex/install.sh | sh` | `codex update` | `codex --version` | [OpenAI Codex README](https://github.com/openai/codex/blob/main/README.md), [OpenAI Help](https://help.openai.com/en/articles/11096431) |
| OpenCode | `curl -fsSL https://opencode.ai/v2/install | bash` | `opencode upgrade` (the installer again for a legacy `opencode2`) | `opencode --version` | [OpenCode install](https://opencode.ai/docs), [OpenCode CLI](https://dev.opencode.ai/docs/cli/) |
| Antigravity CLI | `curl -fsSL https://antigravity.google/cli/install.sh | bash` | `agy update` | `agy --version` | [Antigravity install](https://antigravity.google/docs/cli/install/), `docs/architecture/antigravity-compatibility.md` |

Antigravity CLI replaced Gemini CLI as the Google-family harness (ADR-027);
Gemini's npm route is no longer provisioned.

Corrections found while dogfooding, now the implemented routes: Codex's
update verb is `codex update` (`--upgrade` is not recognized by
codex-cli 0.148.0); OpenCode's installer is `https://opencode.ai/v2/install`,
used both to install and to update a legacy `opencode2`; every `curl | sh`
route is fetched in full before its interpreter runs, so a failed download
fails the install instead of running a partial script.

The Windows-specific official PowerShell routes are intentionally deferred
until their command contracts are tested in a Windows runner. macOS follows
the Unix routes above where vendors document them. An unsupported platform is
reported as `BLOCKED` with the vendor's installation page in the reason, and
is never routed through another package manager. The pages named are:

| Harness | Manual route named on an unsupported platform |
|---|---|
| Claude Code | https://code.claude.com/docs/en/installation |
| Codex | https://github.com/openai/codex/blob/main/README.md |
| OpenCode | https://opencode.ai/docs |
| Antigravity CLI | https://antigravity.google/docs/cli/install/ |

Installer output is never recorded in provisioning state: it goes to a
per-harness log, `$UZE_HOME/cache/logs/setup-<harness>.log`, replaced on
each run. The record under `$UZE_HOME/state/provisioning.json` keeps only
action, result, official-method label, platform, observed version, and
timestamp.
