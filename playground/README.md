# Playground

Two disposable worlds for trying this checkout by hand, the way a person
meets uze: a fresh machine, uze installed through the real installer, Git,
and a small plugin to install. Nothing is installed on your own system, and
each world starts over every time it is opened.

Both are started from WSL.

| | Windows | Linux |
|---|---|---|
| Open | `make playground-windows` | `make playground-linux` |
| World | Windows Sandbox | a WSL distribution named `uze-playground` (Ubuntu 24.04) |
| Shell | Windows Terminal, as the ordinary user `person` | bash, as the user `person` |
| Gone | when the Sandbox window closes | `make playground-linux-down` (or the next `make playground-linux`) |

Each one builds uze and the playground's MCP server from this checkout,
stages a local release beside `install.ps1` or `install.sh`, and opens the
world, which then:

- installs Git;
- installs uze through the installer, from that local release;
- puts `playground-mcp` on `PATH`;
- registers the `playground` marketplace (a Git repository made from
  [`plugin/`](plugin));
- creates `~/projects/demo`, a Git repository with an `AGENTS.md`;
- opens a shell there, as an ordinary user.

Then try, for example:

```sh
uze doctor
uze setup claude-code            # or codex, opencode, antigravity: the real installers
cd ~/projects/demo && uze playground@playground
uze workspace
```

## Your sessions

Each world borrows the harness sessions signed in on this machine, so a
harness installed there starts signed in: Claude Code, Codex, OpenCode and
Antigravity. [`sessions.py`](sessions.py) reads them from your home when the
world is staged and lends only the access token, never the token that renews
it: Claude and Codex rotate that one on use, and a world renewing with its
copy would sign this machine out. A lent session therefore ends when its
access token does, which outlasts a playground; API keys are lent as they
are. The world copies them into its user's home and deletes them from the
stage, and nothing is written to the repository.

`up.sh` prints what it lent and why it lent nothing for a harness. A session
whose access token has already expired here is not lent: run that harness on
this machine once, which renews it, and open the world again.
`UZE_PLAYGROUND_SESSIONS=0` lends none.

The worlds have network access, so a harness can also be signed into through
the world's own browser.

## Windows

Needs Windows 10/11 Pro or Enterprise with Windows Sandbox enabled. In an
administrator PowerShell, then restart Windows:

```powershell
Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All
```

`uze.exe` is cross-built from WSL. The first run installs
[`xwin`](https://github.com/Jake-Shadle/xwin) into
`~/.cache/uze-playground/tools` and, once you accept Microsoft's license
terms, downloads the Windows CRT and SDK (about 650 MB) into
`~/.cache/uze-playground/xwin`; linking uses the Rust toolchain's own
`rust-lld`. Set `UZE_PLAYGROUND_ACCEPT_XWIN_LICENSE=1` to accept without the
prompt.

Windows Sandbox signs in as an administrator with UAC off, so everything it
starts is elevated, which is not what a person's own session is: Codex, for
one, refuses to start its daemon elevated, and an administrator does not meet
the limits an ordinary account does. So the world creates an ordinary account,
`person`, installs uze for it and opens Windows Terminal as that account (its
unpackaged build, which needs no Store and runs for any account).
`UZE_PLAYGROUND_USER=admin make playground-windows` keeps the administrator
instead.

The world is staged in `%LOCALAPPDATA%\uze-playground\windows`, mapped into
the Sandbox as `C:\playground`. Preparing takes about a minute after the
Sandbox opens; `C:\playground\prepare.log` (the same file on your side) says
how far it got, and why, if a step failed.

Windows runs one Sandbox at a time, so opening the world closes one already
open.
A freshly built `uze.exe` runs in the Sandbox even where Smart App Control
blocks it on your own Windows.

## Linux

Needs WSL 2. The Ubuntu 24.04 image is downloaded once into
`%LOCALAPPDATA%\uze-playground\linux\cache`; the distribution lives in
`%LOCALAPPDATA%\uze-playground\linux\distro`. `make playground-linux`
replaces the previous one and drops you into its shell; `exit` leaves it
running, and `wsl -d uze-playground` returns to it.

## The plugin

[`plugin/`](plugin) is one portable Agent Plugin (`playground`) declaring one
of every capability UZE delivers, each answering with a marker you can look
for:

| capability | what it is | try | answers |
|---|---|---|---|
| rule, always on | [`pong`](plugin/rules/pong.md), copied into a declaring project's `.agents/rules/` | `ping` | `pong (UZE_PLAYGROUND_RULE: pong)` |
| rule, glob | [`txt`](plugin/rules/txt.md), the same | "write a hello.txt" | the file's last line is `UZE_PLAYGROUND_RULE: txt` |
| skill, model + user | [`ping`](plugin/skills/ping/SKILL.md) | `/playground:ping` | `pong (UZE_PLAYGROUND_SKILL: ping)` |
| skill, user only | [`hello`](plugin/skills/hello/SKILL.md) | `/playground:hello` | `hello, world (UZE_PLAYGROUND_SKILL: hello)` |
| skill, model only | [`secret`](plugin/skills/secret/SKILL.md) | "what is the playground's secret word?" | `tangerine (UZE_PLAYGROUND_SKILL: secret)` |
| agent | [`pong`](plugin/agents/pong.md) | "hand a ping to the playground:pong agent" | `pong (UZE_PLAYGROUND_AGENT: pong)` |
| hook, deny | `deny-ping` on `PreToolUse` | "run `echo ping-deny`" | refused: `pong (UZE_PLAYGROUND_HOOK: deny-ping …)` |
| hooks, observe | `SessionStart`, `PostToolUse`, `Stop` | any session | a line each in `$TMPDIR/uze-playground-hooks.log` |
| MCP | `pingpong`, [`mcp_server.rs`](mcp_server.rs) built as `playground-mcp` | "call the pingpong `ping` tool" | `pong (UZE_PLAYGROUND_MCP: ping)` |

The rules reach only a project that declares the plugin
(`uze playground@playground` in it), never one that only has it on the
machine (`uze install -m`). The demo project also keeps a rule of its own,
`.agents/rules/marco.md`: asked `marco`, an agent answers
`polo (UZE_PLAYGROUND_PROJECT_RULE)`. `uze status` in the project lists both.

The MCP server also has `hello`, `echo` and `add`. The hooks share one
handler, [`scripts/hook`](plugin/scripts/hook) (and its Windows spelling).
`secret` is the one a harness may only *degrade*: some cannot withhold a
skill from the person, and the install says which.

It has no vendor envelope, so it shows how uze delivers portable
capabilities to a harness with nothing harness-specific in the package.
[`marketplace.json`](marketplace.json) makes this directory its
marketplace.

### On this machine

Outside a playground world, install it on your own machine with:

```sh
make playground-plugin                 # playground-mcp on PATH, marketplace linked here
uze playground@playground              # in a project: its rules arrive there
uze install -m playground@playground   # the machine only: no rules
```

The marketplace is linked to this checkout, so an edit to `plugin/` reaches
the harnesses on the next `uze install` with no commit. `uze remove -m
playground@playground` takes it off again.
