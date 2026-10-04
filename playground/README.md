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
uze install -m playground@playground
uze workspace
```

The worlds have network access, so a harness installed there can be signed
into through the world's own browser.

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

[`plugin/`](plugin) is one portable Agent Plugin (`playground`):

- `plan`: short, explicit implementation planning;
- `review`: focused repository review;
- `release`: a safe local release checklist;
- `tools`, an MCP server ([`mcp_server.rs`](mcp_server.rs), built as
  `playground-mcp`) with deterministic `echo`, `add` and `status` tools.

It has no vendor envelope, so it shows how uze delivers portable
capabilities to a harness with nothing harness-specific in the package.

To try a skill in a harness, ask for it by name:

```text
Use the plan skill to create a three-step plan for adding a small CLI command. Start with the skill's activation marker.
```

And the MCP server:

```text
Use the tools MCP server tool `add` to calculate 19 plus 23. Include the tool result in your answer.
```
