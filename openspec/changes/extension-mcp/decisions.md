# Decisions — extension-mcp

## D1 — A grant per tool, `mcp:<server>/<tool>`

- status:   pending
- task:     1.1
- commit:
- trigger:  A server exposes tools that read and tools that write (`rtc_query` beside `rtc_create`); consenting to a server would consent to all of them.
- decided:  The grant names one tool of one server. The consent lists each, worded "calls the tool `<tool>` of the MCP server `<server>`". No wildcard. Server and tool names are what MCP allows: letters, digits, `_`, `-`, `.`, at most 64.
- rejected: `mcp:<server>` for every tool: an extension that reads would silently gain every write the server offers, today's and tomorrow's.
- blast:    `uze-contract/src/manifest.rs::Grant`, `report.rs` (consent words).

## D2 — The server is the extension's plugin's, or the one an `mcp` node names

- status:   pending
- task:     1.2, 2.2
- commit:
- trigger:  The operator wants to reuse existing MCPs, which may live in another plugin (Cardinal's `rtc` is in its `std` plugin).
- decided:  Without an `mcp` node the server is looked up in the extension's own plugin. `mcp "<server>" plugin="<plugin>@<marketplace>"` names another installed plugin. The host never searches: a name two plugins deliver is not guessed.
- rejected: Searching every installed plugin for the server name: the consent could not say whose code runs.
- blast:    `manifest.rs::McpDecl`, `extensions/mcp.rs::launch_of`.

## D3 — The server runs as a harness runs it: unconfined, from the same launch

- status:   pending
- task:     3.2
- commit:
- trigger:  An MCP server needs the network and the operator's credentials (`rtc` reads `~/cardinal/.env` and logs in); confined like a program it could do neither.
- decided:  The host starts the command and arguments the plugin's `mcp.json` declares, `${PLUGIN_ROOT}` resolved to the delivered package (materialized first when no harness was handed it yet) and launched as delivery launches it, with the operator's environment, in the checkout. A server that declares an `env` or a `cwd` of its own is refused, as delivery refuses it. It is not confined. The extension supplies nothing to the launch: only tool names and arguments cross.
- rejected: Confining it: the server is the plugin's code the operator already runs for their agents, unconfined; the grant changes who asks, not what runs.
- blast:    `extensions/mcp.rs`, `uze-integrations/src/shared/mcp.rs::stdio_launch`.

## D4 — A hand-written stdio client, not `rmcp`'s

- status:   pending
- task:     2.1
- commit:
- trigger:  `rmcp` is in the tree only for test servers, with `tokio`; its client would put an async runtime into the binary for three requests.
- decided:  `uze-core::machine::mcp` speaks JSON-RPC 2.0 over newline-delimited stdio: `initialize` (protocol `2025-06-18`, any version the server answers accepted), `notifications/initialized`, `tools/call`. Notifications and requests from the server are read and ignored; a request it sends is answered "method not found". Lines are bounded at 4 MiB, stderr at 64 KiB kept for the error.
- rejected: `rmcp` with `transport-child-process`: a runtime, and a dependency surface, for a protocol of three messages.
- blast:    `uze-core/src/machine/mcp.rs`.

## D5 — One live process per extension and server, ended when idle

- status:   pending
- task:     3.2
- commit:
- trigger:  Starting `bun` and logging in costs seconds; a panel that refreshes every 30 s would pay it each time.
- decided:  The host keeps one session per (extension, server), started on the first call, calls to it one at a time, ended after five minutes without a call, when a call times out or fails at the transport, and when UZE exits. The next call starts a new one.
- rejected: One process per call: seconds of start-up and a login per refresh. One process shared by every extension: one extension's slow call would hold another's.
- blast:    `extensions/mcp.rs::Sessions`.

## D6 — What a call answers

- status:   pending
- task:     3.1
- commit:
- trigger:  MCP results carry content blocks; most servers today put JSON in a text block, newer ones `structuredContent`.
- decided:  `structuredContent` when present; else the text blocks joined, parsed as JSON when the whole parses, else the text. `isError` throws the text. A result over 1 MiB fails the call. Images and resources in the content are dropped.
- rejected: Handing the raw content array: every script would repeat the same unwrapping.
- blast:    `uze-core/src/machine/mcp.rs::result_value`.

## D7 — A call's time is declared, up to ten minutes

- status:   pending
- task:     1.2, 3.2
- commit:
- trigger:  The RTC timesheet takes minutes to read a month (Cardinal gives it 600 s), past every bound a script has.
- decided:  30 s by default; `mcp "<server>" timeout="5m"` raises it, at most `10m`, and the consent shows it. The call counts as one of the answer's 16 host calls; the time it waits is the call's own bound, not the 60 s a program or a request waits.
- rejected: The program's 10 s: the tracker would never answer.
- blast:    `manifest.rs::McpDecl::timeout`, `extensions/mcp.rs`.

## D8 — Tools are not checked against the server when the extension is checked

- status:   pending
- task:     1.2
- commit:
- trigger:  Knowing a server's tools means starting it, which `uze agent plugin check` must never do (it may log in, it may be another plugin's).
- decided:  The check verifies the spelling, the node and the plugin's name; a tool the server lacks fails at the call with the server's own error.
- rejected: Starting the server during the check.
- blast:    none beyond the check.
