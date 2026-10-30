## 1. WKS-2 - UZE's own Git operations run nothing the repository names

- [x] 1.1 Regression tests in `uze-git`: a write (`worktree add`, `rebase`,
  `merge`), a fetch and a `status` read run no hook, filter, merge driver,
  SSH, askpass, credential helper, upload pack or `ext::` transport the
  repository's configuration names; the operator's global helper still runs
- [x] 1.2 Fix: one hardening set on every invocation and the scoped
  override for operations that can reach a program
- [x] 1.3 Exploit re-run: `poc2-hooks.sh` and `poc5-clock-fetch.sh` in the
  audit sandbox against the fixed binary

## 2. WKS-1 - A session id never reaches argv as an option

- [x] 2.1 Regression tests: `SessionId::parse` refuses option-, path- and
  whitespace-shaped ids, and a Codex rollout naming
  `--dangerously-bypass-approvals-and-sandbox` is not observed
- [x] 2.2 Fix: `SessionId::parse`, serde through it, and per-harness shapes
  in each integration's read-back
- [x] 2.3 Exploit re-run: `poc3-resume-inject.sh` in the audit sandbox

## 3. WKS-3 - A push URL is an operand of `ls-remote`

- [x] 3.1 Regression test: a push URL of `--upload-pack=<command>` does not
  run the command when unpublished branches are forgotten
- [x] 3.2 Fix: `--` before the URL; sweep `uze-workspace` and `uze-git` for
  other values read from Git and re-passed
- [x] 3.3 Exploit re-run: the regression scenario against the fixed binary
  in the audit sandbox

## 4. Close

- [x] 4.1 Update `docs/architecture/invariants.md` and `uze-git`'s doc
- [x] 4.2 Gates and `evidence.md`
