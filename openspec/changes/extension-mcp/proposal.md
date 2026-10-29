## Why

A plugin's logic often already lives in an MCP server its agents use: the
RTC timesheet the operator wants on a panel is a script that imports the
`rtc` server's client as a library, run by hand with `bun`. Reimplementing
it in Rhai would fork the login, the cookies and every quirk of the
server's API from the copy the agents use, and `fetch` cannot hold a
session anyway. The operator asked to let an extension consume an MCP
server the way an agent does, so one server serves both.

## What Changes

- A grant family `mcp:<server>/<tool>`: the extension may call this tool
  of this server, and no other. The consent names each tool.
- An optional `mcp "<server>"` node in the manifest: which plugin's server
  it is (`plugin="<plugin>@<marketplace>"`, the extension's own plugin when
  absent) and how long a call may take (`timeout`, up to ten minutes).
- A script host function, `mcp(server, tool, args)`, answering the tool's
  result as data: its structured content, or its text read as JSON when it
  is JSON, else the text; a tool error is a thrown error.
- The host starts the server exactly as a harness is handed it, from the
  plugin's `mcp.json` with `${PLUGIN_ROOT}` resolved, speaks MCP over stdio,
  keeps it alive between calls and ends it when idle.

## Capabilities

### New Capabilities
- `extension-mcp`: calling an installed plugin's MCP tools from an
  extension's scripts, under a grant per tool.

### Modified Capabilities
None.

## Impact

`uze-contract` (`Grant::Mcp`, `McpDecl`), the manifest decoder and check,
`uze-script` (the `mcp` host function, `Reach::mcp`), `uze-core` (an MCP
stdio client in `machine/`), `uze-integrations` (the server's launch read
from `mcp.json`, shared with delivery), `uze-application` (the session
pool, the reach, the consent's words), docs, the `uze:author` reference.
