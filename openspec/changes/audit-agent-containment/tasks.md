## 1. Platform concepts

- [x] 1.1 `uze_platform::seal`: a sealed spawn (thread-scoped Landlock domain scoping abstract Unix sockets), whether sealing works here, and whether this process or a pid runs confined (`no_new_privs`); unavailable elsewhere
- [x] 1.2 `uze_platform::seal`: a sealed endpoint (abstract socket on Linux) to bind and connect, peer uid checked as for the pathname one

## 2. TERM-1 - the person's endpoint

- [x] 2.1 Regression test: a sealed process (escaped with `setsid`) offering the key cannot attach, on either endpoint (fails on the audited commit)
- [x] 2.2 Fix: server binds the operator endpoint and publishes its name; Attach/Stop refused on the shared endpoint while it stands; program panes spawned sealed; clients connect to the operator endpoint; a confined server is retired by an unconfined client; protocol version bump
- [x] 2.3 Exploit re-run: `pane.sh` (escaped helper with the key) against the fixed binary in the audit sandbox

## 3. TERM-2 - pane-opened spaces

- [x] 3.1 Regression test: 25 `OpenSpace` from a pane open only the bound and persist none (fails on the audited commit)
- [x] 3.2 Fix: per-pane and total bound on unvisited spaces; unvisited spaces left out of `workspace.json` until selected
- [x] 3.3 Exploit re-run: `pane_dos.sh` against the fixed binary in the audit sandbox

## 4. WKS-4 - who asks

- [x] 4.1 Regression test: a process under `no_new_privs` without the stamp is an agent to the asker (fails on the audited commit); the writable roots named to a harness lie outside `state/`
- [x] 4.2 Fix: `asker()` reads `no_new_privs`; refusal wording says why
- [x] 4.3 Exploit re-run: `poc1-allow-bypass.sh` and `poc1b-forge.py` in the audit sandbox

## 5. WKS-5 - key-less claims

- [x] 5.1 Regression test: a claim naming a key-less record with any key is refused; a split child carries a key (fails on the audited commit)
- [x] 5.2 Fix: `admits_launch` refuses a record with no digest; split issues the child a key
- [x] 5.3 Exploit re-run: `poc4-child-impersonation.sh` in the audit sandbox

## 6. Close

- [x] 6.1 Gates: build, crate tests, touched domain suites, clippy, fmt, `openspec validate audit-agent-containment --strict`
- [x] 6.2 evidence.md
