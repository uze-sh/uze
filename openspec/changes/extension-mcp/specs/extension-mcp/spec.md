## Purpose

Lets an extension call the tools of an MCP server an installed plugin
delivers, the way an agent does, so a server's logic serves both.

## ADDED Requirements

### Requirement: An extension calls only the MCP tools it was granted
A script SHALL call a tool with `mcp(server, tool, args)` only under a
grant `mcp:<server>/<tool>` the operator accepted, and the host SHALL
refuse any other before starting or reaching a server.

#### Scenario: A tool the grants do not name
- **WHEN** an extension granted `mcp:rtc/rtc_timesheet` calls `mcp("rtc", "rtc_create", #{})`
- **THEN** the call fails naming the grant it lacks, and no request reaches the server

### Requirement: The server is the one a harness is handed
The host SHALL start the server named by the extension's own plugin, or by
the plugin an `mcp` node names, from that plugin's `mcp.json` with
`${PLUGIN_ROOT}` resolved, over stdio, and SHALL refuse a server the plugin
does not deliver or a plugin not installed.

#### Scenario: A server of another plugin
- **WHEN** the manifest declares `mcp "rtc" plugin="std@cardinal"` and the grant `mcp:rtc/rtc_timesheet`
- **THEN** the call starts the `rtc` server of `std@cardinal` as an agent's harness would

### Requirement: A call answers the tool's result as data, within its time
A call SHALL answer the tool's structured content, or its text parsed as
JSON when it is JSON, else the text; a tool error SHALL be thrown with its
text. A call SHALL end after its declared `timeout` (default 30 seconds, at
most ten minutes), ending the server, which the next call starts again.

#### Scenario: A slow tracker
- **WHEN** `mcp "rtc" timeout="5m"` and the tool answers after two minutes
- **THEN** the script receives the answer; at five minutes it would have failed and the server been ended
