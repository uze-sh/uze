Audited commit: 98d1210f · Date: 2026-10-30 · Skill: `security-audit`

These findings share one root cause. The workspace's git writes, and the
values it reads back (from git config and from harness session stores),
are trusted as UZE's own. They are written by whoever can write the
repository's common git dir or the harness's state, and that includes every
agent.

## WKS-2 - Hooks and transports named by the repository's git config run inside UZE's own git operations

- **Severity**: Medium. Code runs unapproved in the UZE process, outside any
  harness sandbox and outside the ADR-055 approval. It is triggered by
  placement, by delivery, and by the 3-minute sync clock.
- **Status**: CONFIRMED for `post-checkout` on placement. PLAUSIBLE for the
  clock's `fetch`.
- **Threat actor**: an agent (3), which must be able to write the common git
  dir in order to commit. Also a repository that arrives with its `.git`,
  for example in an archive (2).
- **Location**:
  - `crates/uze-git/src/lib.rs:79-85,~385-415`: writes force only
    `core.fsmonitor=false`.
  - Placement: `crates/uze-workspace/src/checkout/pool.rs:503` (`worktree
    add`).
  - Delivery: `crates/uze-workspace/src/landing/deliver.rs:230` (rebase).
  - Sync: `crates/uze-workspace/src/landing/sync.rs:89` (`fetch_private`),
    where `core.sshCommand`, `protocol.ext.allow` and `ext::` URLs come from
    the repository's config.
- **Attack**: the agent writes `$(git rev-parse --git-common-dir)/hooks/post-checkout`.
  The next placement (`uze agent work split`, or a launch) runs it while
  `setup` is still waiting for approval.
- **Evidence**: `! setup was not run: this project's commands await your
  approval …`, then the canary `PWNED-WKS2` reads `hook ran by: git -C
  …/proj2 -c core.fsmonitor=false worktree add`.
- **Root cause**: the hardening that UZE's git reads have (hooks,
  fsmonitor, textconv and ext-diff off) is missing from its writes and
  fetches.

## WKS-1 - A session id read back from a harness's store is placed in the relaunch argv unvalidated

- **Severity**: Medium. Write access to the harness's own session store
  becomes a sandbox and approval bypass on the next relaunch.
- **Status**: CONFIRMED through the real PATH shim and launch plan.
- **Threat actor**: an agent (3), or any process able to write
  `~/.codex/sessions/**` or `~/.claude/projects/<slug>/*.jsonl`.
- **Location**:
  - `crates/uze-integrations/src/codex/session.rs:44-62,124-143` takes the
    id from `payload.session_id` in the rollout file.
  - `crates/uze-integrations/src/claude/session.rs:91-117` takes the id from
    the file stem.
  - `crates/uze-core/src/delivery/session.rs:28`: `SessionId::new` accepts
    anything.
  - `crates/uze-workspace/src/continuity.rs:303-315` builds `resume <id>` or
    `--resume <id>` without `--`.
- **Attack**: plant a rollout whose first line holds
  `"session_id":"--dangerously-bypass-approvals-and-sandbox"` and whose cwd
  is the agent's slot.
- **Evidence**: on the second launch, the real `codex` sees `resume
  --dangerously-bypass-approvals-and-sandbox -c hooks.SessionStart=…`.
- **Root cause**: an identifier from outside UZE crosses into argv with no
  shape check.

## WKS-3 - A push URL from repository config reaches `git ls-remote` as an option

- **Severity**: Low to Medium. It breaks the read invariant ("a read runs
  nothing that directory's configuration names"), on the clock and with no
  operator action.
- **Status**: PLAUSIBLE end to end. The git behaviour is CONFIRMED: `git
  ls-remote --heads "--upload-pack=touch …; false" refs/heads/x` created
  the canary.
- **Threat actor**: an agent (3), or a repository with its `.git` (2).
- **Location**: `crates/uze-workspace/src/landing/request.rs:208-218`.
  `pushed_to` comes from `remote get-url --push origin` and is passed
  without `--`. It is reached from `sync_target` → `forget_unpublished`
  whenever an agent has `refs/remotes/origin/<branch>`, which the agent can
  create itself.
- **Root cause**: a value read from git config is re-passed to git as a
  positional without an end-of-options marker.

## Defenses that held

- Branch names and topics are strictly validated, and `--end-of-options` or
  `--` is used almost everywhere. `name_is_taken` is checked.
- `workspace.link` refuses absolute paths, `..` and tracked files.
- The slot's `.git` is pinned through an anchor.
- Reads disable fsmonitor, hooks, ext-diff and textconv.
- The policy is always read from the primary checkout, never from the
  agent's.
- Agent `report` fields are stripped of control characters.
- Launch overrides are typed by the operator only.
