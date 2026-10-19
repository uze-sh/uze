# Decisions taken while the change is open

## I1. The branch is read from the `HEAD` file, not asked of Git

**Context.** D3 reads the branch at compose time. `continuity::plan` runs
inside the shim on every relaunch, and its module doc holds that path to "a
few `stat`s and one small read".

**Decision.** `checkout::branch_on_disk` reads the checkout's `.git` pointer
and its `HEAD` file. It falls back to `agent/<id>` when neither answers (a
reftable store, a checkout with no `.git`).

**Alternative.** `git symbolic-ref` through `crate::git`, which is one spawn
per relaunch for every agent. It would also answer a reftable repository,
where the fallback can name a branch the agent already renamed.

## I2. "Main working tree" stands in for "primary checkout"

**Context.** The cleanup runs only in the primary checkout (D6). The module
that runs it, `uze-application`'s `retired_regions.rs`, is on the package
manager's side, and `the_package_manager_never_names_the_workspace` forbids
it from asking `uze-workspace` whether a path is a slot.

**Decision.** The cleanup runs where `.git` is a directory. A linked
worktree's `.git` is a file, so it is never cleaned, whoever made it. The
retired prefixes moved to `uze-core::retired_region` for the same reason,
and `uze-workspace` no longer names `POLICY_REGION_PREFIX`.

**Alternative.** Move the cleanup into the workspace's half. `uze install`
would then depend on the workspace, which is what the layering rule exists
to stop.

## I3. The checkout-accounting discount stays until section 4

**Context.** Task 3.4 drops the managed-region discount. But `uze install`
still writes `package:*` regions, in any checkout, until section 4 retires
them. An install inside a slot would then park it over UZE's own bytes.

**Decision.** 3.4 moves after 4.1. The discount is deleted together with the
last region UZE writes.

**Alternative.** Drop it now and accept that a slot gets parked by a package
region. That is the slot-hijack class `checkout-accounting` was written
against.

## I4. A harness with no continuity gets the instructions turn on every launch

**Context.** Spec `launch-instructions`: a degraded channel gives one first
turn on a new conversation. A harness declaring
`SessionContinuity::Unsupported` starts a new conversation every time.

**Decision.** Every launch of such a harness on a degraded channel is pointed
at `context.md`. That is the spec read literally: each launch really is a new
conversation, with nothing carried over that would already hold the
instructions.

**Alternative.** Give it once per agent. A second conversation would then
start with no instructions at all.

## I5. The "nothing writes AGENTS.md" architecture rule waits for section 4

**Context.** Task 3.5 asks for a rule that no production code outside the
retired-region cleanup writes `AGENTS.md`. Until section 4 retires them,
`project::context` still writes `package:*` regions, so the rule would have
to sanction the very writer this change is removing.

**Decision.** 3.5 lands with 4.1, when the cleanup is the only writer left.
Until then the citations in `docs/architecture/invariants.md` ("The
workspace never writes `AGENTS.md`, and retired regions are removed") cover
the workspace and the launch. Those are the writers this cut removes.

**Alternative.** Add the rule now with `project::context` sanctioned, and
remove the sanction in section 4. That is two edits to the suite where one
does the job, and a sanction that exists only to be deleted.
