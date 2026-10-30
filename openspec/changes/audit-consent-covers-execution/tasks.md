## 1. DELIV-1: the trust request covers the envelope

- [x] 1.1 Regression test: a remote package whose only execution is in its Claude/Codex envelope or vendor files (inline hooks, `hooks/hooks.json`, `.mcp.json`, unknown key) fails non-interactive install with `TRUST_REQUIRED` naming the command (fails on 98d1210f)
- [x] 1.2 Fix: `IntegrationPort::package_executions`, the Claude, Codex and Antigravity enumerations, the terminal-control guard on envelope MCP, and the application joining them into one request
- [x] 1.3 Exploit re-run: the `deliv` PoC (`uze install` of `notes`) against the fixed binary in the audit sandbox

## 2. ACQ-1: an approval covers the whole package

- [x] 2.1 Regression test: an update that changes only a file the MCP command's script sources fails non-interactive with `TRUST_REQUIRED` (fails on 98d1210f)
- [x] 2.2 Fix: the code identity of every execution is the package tree digest; update the unit test that encoded the narrower identity
- [x] 2.3 Exploit re-run: `poc1-trust-update.sh` against the fixed binary in the audit sandbox

## 3. Close

- [x] 3.1 Update `docs/architecture/invariants.md` (consent boundary) with the new proving tests
- [x] 3.2 Gates: build, targeted tests, clippy, fmt, `openspec validate --strict`; write evidence.md
