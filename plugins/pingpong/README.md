# pingpong

One portable Agent Plugin declaring one of every capability UZE delivers,
each answering with a marker you can look for:

| capability | what it is | try | answers |
|---|---|---|---|
| rule, always on | [`pong`](rules/pong.md), copied into a declaring project's `.agents/rules/` | `ping` | `pong (UZE_PINGPONG_RULE: pong)` |
| rule, glob | [`txt`](rules/txt.md), the same | "write a hello.txt" | the file's last line is `UZE_PINGPONG_RULE: txt` |
| skill, model + user | [`ping`](skills/ping/SKILL.md) | `/pingpong:ping` | `pong (UZE_PINGPONG_SKILL: ping)` |
| skill, user only | [`hello`](skills/hello/SKILL.md) | `/pingpong:hello` | `hello, world (UZE_PINGPONG_SKILL: hello)` |
| skill, model only | [`secret`](skills/secret/SKILL.md) | "what is the playground's secret word?" | `tangerine (UZE_PINGPONG_SKILL: secret)` |
| agent | [`pong`](agents/pong.md) | "hand a ping to the pingpong:pong agent" | `pong (UZE_PINGPONG_AGENT: pong)` |
| hook, deny | `deny-ping` on `PreToolUse` | "run `echo ping-deny`" | refused: `pong (UZE_PINGPONG_HOOK: deny-ping …)` |
| hooks, observe | `SessionStart`, `PostToolUse`, `Stop` | any session | a line each in `$TMPDIR/uze-pingpong-hooks.log` |
| MCP | `pingpong`, [`mcp/server.rs`](mcp/server.rs) built as `pingpong-mcp` | "call the pingpong `ping` tool" | `pong (UZE_PINGPONG_MCP: ping)` |

The rules reach only a project that declares the plugin
(`uze pingpong@uze` in it), never one that only has it on the
machine (`uze install -m`). A playground world's demo project also keeps a rule of its own,
`.agents/rules/marco.md`: asked `marco`, an agent answers
`polo (UZE_PINGPONG_PROJECT_RULE)`. `uze status` in the project lists both.

The MCP server also has `hello`, `echo` and `add`. The hooks share one
handler, [`scripts/hook`](scripts/hook) (and its Windows twin, `hook.ps1`, run under the same name).
`secret` is the one a harness may only *degrade*: some cannot withhold a
skill from the person, and the install says which.

It has no vendor envelope, so it shows how uze delivers portable
capabilities to a harness with nothing harness-specific in the package. It
is one of the examples of uze's own marketplace, and what the playground's
worlds ([`playground/`](../../playground)) try first.

## On this machine

Outside a playground world, install it on your own machine with:

```sh
make pingpong-mcp             # pingpong-mcp on PATH
uze pingpong@uze              # in a project: its rules arrive there
uze install -m pingpong@uze   # the machine only: no rules
```

The marketplace is linked to this checkout, so an edit here reaches the
harnesses on the next `uze install` with no commit. `uze remove -m
pingpong@uze` takes it off again. Its MCP server is built from a uze
checkout, so outside one the rest of the plugin works and `pingpong` alone
does not start.
