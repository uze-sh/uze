## 1. Contract

- [ ] 1.1 `Grant::Mcp { server, tool }` spelled `mcp:<server>/<tool>`; `McpDecl { server, plugin, timeout }` and `Manifest::mcp`
- [ ] 1.2 The decoder reads `mcp`; the check refuses a node no grant uses, a repeat, a timeout past ten minutes, a plugin ill-spelt

## 2. Client

- [ ] 2.1 `uze-core::machine::mcp`: a stdio session — start, `initialize`, `tools/call`, a bounded reader, a deadline, the group ended on drop
- [ ] 2.2 `uze-integrations`: a server's launch from a stored package's `mcp.json`, shared with delivery

## 3. Host

- [ ] 3.1 `uze-script`: `mcp(server, tool, args)` under the grant, `Reach::mcp`
- [ ] 3.2 `uze-application`: the session pool (lazy start, idle end, restart after a failure), the reach, the consent's words

## 4. Proof and documentation

- [ ] 4.1 Tests: grant parsing and check; the client against a fake server; a script calling a tool end to end
- [ ] 4.2 `docs/extensions/`, the `uze:author` extension reference
- [ ] 4.3 The sandbox timesheet calls a stand-in `rtc_timesheet` tool; a patch adding the tool to Cardinal's `rtc` server
