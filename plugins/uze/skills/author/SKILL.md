---
name: author
# Two lines deciding who finds this skill: the long text is what the model
# matches an invocation against, so it names the moment the skill answers —
# "create a plugin for me" — not what a scaffold is.
description: Guides creating a uze plugin end to end — marketplace create or select, plugin scaffold, capability flags, check before install, install from the linked marketplace, iterate, publish by pushing. Use when someone asks to create, author or scaffold a plugin, or to set up their own marketplace of plugins.
invoke:
  model: true
  user: true
slash: true
---

Creating a plugin with uze is a loop of four deterministic verbs. Run them
in this order; every one is non-interactive and fails with a reason, so a
failure is an answer, not a dead end.

## 1. The marketplace — create or select

Ask `uze market list` first. If a marketplace already fits, use its name in
every later step; nothing new is created.

Otherwise choose the frontier, and name it explicitly — the verb has one
spelling for each:

**Global — the marketplace has a checkout of its own** (the default choice
when the person has not said where it belongs):

```bash
uze agent market create <name> --at <directory> [--description "…"]
```

Choose `--at` outside every checkout — the operator's home (say
`~/marketplace-<name>`) is the natural place. A marketplace is machine
state, shared by every project on it; one created inside a worktree slot
dies with the slot, and one created inside any repository is a nested
repository that dirties that checkout's status.

This scaffolds the directory as a Git repository (`marketplace.json`,
`plugins/`, an initial commit), registers it with the machine, and links
it — the link is the point: installs read the working tree, including
files not committed yet, so authoring needs no publish step. If Git has no
identity configured the command says so with the two `git config` lines to
set; have the person run them (or run them with their consent), then
repeat the command.

**Local — the project is itself the marketplace** (only when the person
wants the marketplace versioned with this repository):

```bash
uze agent market create <name> --local [--plugins-dir <dir>]
```

This writes `marketplace.json` at the project root and the plugins in
`--plugins-dir` (default `plugins/`) — the layout this repository itself is
one in. A directory other than the default is recorded in the manifest, so
every later `plugin create` puts its plugin there too. The project needs a
commit first: the marketplace is linked to it, and a link reads a
repository with at least one revision. No Git state is written and no
commit is made: the project's own flow carries them, so an author working
in an isolated checkout reaches the marketplace through delivery, like
every other piece of project content. A project that
already carries a `marketplace.json` is refused with that fact — add the
plugin to it directly.

## 2. The plugin

```bash
uze agent plugin create <name> --market <market> [--description "…"] \
    [--category <word>] [--hook] [--mcp] [--agent] [--instructions]
```

Each field has one file that owns it. `plugin.json` describes the plugin:
`--description` is written there, and so are `keywords`, `author`,
`homepage`, `repository` and `license` when you add them — the listing
reads them from there and from nowhere else. The `marketplace.json` entry
only locates and files it: `name`, `source`, and `--category`, the one
word a catalogue is browsed by (`productivity`, `development`,
`security`, ...); reuse a category the marketplace already has before
inventing one.

Every name here (the marketplace's, the plugin's, each skill's) is
lowercase kebab-case: `a-z`, `0-9` and single `-` between them, at most
64 characters, the one spelling every harness accepts. A skill's `name`
equals its directory. The verbs and both checks refuse anything else and
say the name you meant; a display casing never goes in the name, and
`plugin.json` has no field for one, so say it in `description`.

The default is a skill plugin: `plugin.json` plus
`skills/<name>/SKILL.md` — edit the skill body, and choose the
`invoke:` policy deliberately (who may trigger it). `--hook` adds a
portable `hooks.json` and a handler stub obeying the `HOOK_*`/exit-code
contract, once per shell: `scripts/guard` (`sh`) and `scripts/guard.ps1`
(Windows PowerShell), each named as that shell's spelling of the one
`command` (`{"posix": …, "windows": …}`; a uze from before Windows
support refuses that form, so it pins the minimum). A handler with no `windows` spelling is not delivered on
Windows, and a guard (`deny`, `ask`) without one is refused there, the
package with it — `uze agent plugin check` names each; `--mcp` adds an `mcp.json` and a working stdio server stub under
`scripts/` (keep its stdout for the protocol alone — log to stderr);
`--agent` adds an agent definition under `agents/<name>.md`;
`--instructions` adds a prose contribution the project's `AGENTS.md`
composes when it reconciles. Every generated file carries commented field
documentation. A file the plugin ships is named `${PLUGIN_ROOT}/…` in
`hooks.json`, `mcp.json`, a `SKILL.md` and an agent definition alike — UZE
resolves it to the installed copy for every harness; anything else is
reached through `PATH`.

The scaffold is also a valid Agent Plugins 1.0 plugin, the open standard
Codex, Copilot, Cursor and Kiro load unchanged: keep `plugin.json`'s
`$schema`, only the standard's manifest fields at its top level (uze's
own namespace is `extensions["sh.uze"]`), and in `mcp.json` its
`$schema` and a `type` on every server. In `mcp.json` a bundled
executable is `"command": "./bin/server"`, never `${PLUGIN_ROOT}` in
`command`. `agents/`, `hooks.json`, `AGENTS.md`, `invoke:` and `harness:`
are uze's own surfaces, which the standard's clients ignore.

An agent is `agents/<name>.md`: frontmatter with `name` and `description`,
and its prompt as the body. Every harness offers it as
`<plugin>:<subdirectories>:<name>` — `agents/review/security.md` in plugin
`flow` is `flow:review:security`, and the frontmatter `name` replaces only
the last part — so a skill that dispatches it names it that way. `name`
and `description` are what every harness reads the same way. A model, a tool
list or a reasoning effort is one harness's vocabulary, so write it under
`harness`, one entry per harness (`claude-code`, `codex`, `opencode`,
`antigravity`), in that harness's own spelling:

```yaml
harness:
  claude-code: { model: haiku, tools: [Read, Grep] }
  opencode: { model: anthropic/claude-haiku-4-5 }
```

Each harness receives only its own entry; the block is never delivered. The
same block works in a `SKILL.md`. The check below validates it for every
harness: a value a harness would reject fails it, and a field a harness
would not receive is named.

## 3. Check, always before install

```bash
uze agent plugin check <the plugin's directory>
uze agent market check <market directory>
```

This runs the same parsers an install runs, offline. A clean check is the
licence to install; a finding names the file and the reason — fix the file,
check again. Never skip it: the feedback an install would have surfaced
arrives here, before anything is delivered. Its `Agent Plugins 1.0`
section says whether the plugin is a valid plugin of the standard or names
what keeps it from being one; that is advice, never a reason the check
fails.

A warning does not fail the check, but it is still yours to resolve: each
one names the file and the one change that clears it. A marketplace
written before `plugin.json` owned the describing fields is brought over
this way — `market check` warns on every `description` or `keywords` left
on an entry and says whether to move it into the plugin's `plugin.json`,
delete it because `plugin.json` already says the same, or keep one of two
values that differ. Apply each, then check again until no warning is left.

## 4. Install and iterate

```bash
uze install -m <name>@<market>
```

The first install is `install`. Because the marketplace was born linked,
it reads the working tree — but the Store is idempotent by origin: a
*repeated* `install` of a package it already holds hands back the stored
bytes without re-reading the source. So the loop's re-delivery verb is
**`update`**:

```bash
uze update <name> -m
```

Re-resolves the package from the linked marketplace, replaces the stored
bytes, asks the trust question against the revision it replaces, and puts
the old revision back if the new one cannot be delivered. Loop: edit →
check → `update -m`.

## Publishing

The marketplace is a normal Git repository the moment it is born. When it
is worth sharing, `git push` it to a host and `uze market add
<url>` on another machine — nothing about the workflow changes.

## Reference

The person-facing guide to this loop is
[uze.sh/docs/plugins/creating](https://uze.sh/docs/plugins/creating), and
everything a plugin may contain (skills, agents, MCP servers, hooks,
`${PLUGIN_ROOT}`) is specified at
[uze.sh/docs/reference/plugin-format](https://uze.sh/docs/reference/plugin-format).
Point the person there when they ask what a plugin can hold or how the
loop works without an agent.
