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

## I6. The cleanup reads no declaration

**Context.** `uze agent context reconcile` refused an `agents.yaml` that did
not parse, because it composed one region per declared package and a typo
would have taken them back.

**Decision.** Nothing the cleanup removes depends on the declaration, so it
no longer reads it: a retired region goes whatever `agents.yaml` says, and
`uze update` no longer reconciles the context at all (a moved pin changes
nothing in `AGENTS.md`). `UpdateReport::reconciled` goes with it.

**Alternative.** Keep the refusal. It would protect nothing, and leave old
text in place over an unrelated typo.

## I7. The bridge goes when no package region is left, on every declared bridge file

**Context.** The bridge was planned only for a *detected* harness declaring
`ContextDelivery::Bridge`, and only while a package contribution matched.

**Decision.** The cleanup looks at every registered integration's bridge
file, detected or not, since a leftover is in the project either way. It is
removed only while it still holds `@AGENTS.md` (content-checked, like a
`project:` region); an edited one is reported and kept. It stays while a
package region is kept (malformed markers), so a harness is never left
without the import while the region it imported is still there. A file
left with nothing outside UZE's regions is deleted.

**Alternative.** Remove it structurally, like a package region. Its expected
content is known, so there is no reason to give up the check.

## I8. `ContextDelivery::Bridge` stays as a declaration; the read models stop reporting a bridge

**Context.** `resolve-instruction-files` will decide what Claude declares.
Until then the declaration also names the file the cleanup must look at.

**Decision.** The core vocabulary keeps `Bridge { file_name }`, documented as
"reads `AGENTS.md` itself beside its own file". Everything that reported a
bridge being needed goes: `HarnessContextDelivery::Bridge { needed, state }`
becomes `ReadsNatively` (so `uze status` still says "reads AGENTS.md
natively"), `ContextMechanism::Bridge`, `ResourceDelivery::Bridged`,
`UndeliveredReason::Bridge` and `Portability::PartiallyPortable` are removed
(nothing can produce them), and a bridge-declaring harness without a live
shim is `Native`. `InstructionsFile::managed_regions`,
`StatusReport::packages_contributing_here` and `SpacePlugin::contributing`
go too: there is nothing UZE manages in the file to count.

**Alternative.** Switch Claude to `Native` here. That is
`resolve-instruction-files`' decision, and the two changes land in either
order.

## I9. A plugin's root `AGENTS.md` is a check warning, not a finding

**Context.** Task 4.3 asks `uze agent plugin check` to report it.

**Decision.** A warning, beside the other "installs but reaches nothing"
cases (an entry under `rules/` that is not a rule). The plugin still
installs and every other capability still reaches its harnesses; refusing
it would break every published plugin that shipped one, for text that is
now merely inert.

**Alternative.** A finding, failing the check. It would block publishing a
plugin over a file that harms nothing.

## I10. `--instructions` is a hidden flag that refuses

**Context.** Removing the flag from the grammar makes clap answer
"unexpected argument", which names no replacement.

**Decision.** The flag stays parsed, hidden from help, and the command fails
before writing anything, naming `--rules` and `trigger: always_on`. It goes
from the grammar with the other retired spellings once no author can still
reach for it.

**Alternative.** A `removed_spelling` entry in `src/main.rs`. That table
matches leading words, not a flag anywhere on the line.

## I11. What the "only writer" rule can see

**Context.** Task 3.5's rule is static: it cannot follow a path through a
variable.

**Decision.** A production file is a writer of `AGENTS.md` when it names the
file and calls a region writer (`text_region::attach|detach|reconcile|
converge|remove_unconditionally`) anywhere, or names it on the same line as a
file write. Only `retired_regions.rs` is sanctioned, and the test fails if
that sanction stops matching anything. The workspace's isolate undo
(`checkout/lifecycle.rs`), which puts the *committed* `AGENTS.md` back in a
slot it has just copied the primary's changes into, writes through a
variable and is not seen; it restores the branch's own bytes rather than
writing UZE text, and it is the retired-region case of D6.

**Alternative.** Sanction it too. A sanction the rule never matches would be
documentation pretending to be enforcement.

## L1. The policy is shortened to fit its 2,000-byte share

**Context.** Task 2.3 measures the rendered policy against D4's shares.
Measured on 2026-10-19 with `instructions_part` (the lead and the policy,
isolated, branch `agent/ab12cd`, target `main`): with Conventional Commits
and `delivery: push`, the shape this repository declares, the part came to
2,191 bytes for a 30-byte checkout path, 2,195 for a 34-byte one and 2,267
for a 106-byte one. Gitflow and the flat preset came within 35 bytes of
those, and only the `agent` preset (no naming clause) fit, at 1,198 to
1,418. Every naming vocabulary therefore had its last bullet cut, and the
sentence lost first was "Never make a worktree with Git for this". The
Antigravity signed-out run in the Lab showed it as a `context.md` of
2,001 bytes.

**Decision.** The policy text is shortened, never the share raised. The
naming clause drops its counter-example and two restatements, and the
delivery, rebase and split clauses lose their repetitions. After the cut
the same part measures 1,891 (30-byte path), 1,895 (34) and 1,967 (106) with
Conventional Commits and push, 1,757 to 1,833 with no delivery, and 1,261 in
place. `the_longest_policy_fits_its_share_uncut` holds the longest preset
vocabulary, every delivery clause, a 106-byte checkout and a long branch and
target under the share. A project's own `types:` list longer than
Conventional Commits' can still be cut, and is then cut from the end like
any section. The Lab's `instructions-arrive-whole` checks the last sentence
reaches the model.

**Alternative.** Move bytes from the briefing share to the instructions.
D4 orders it the other way: the instructions shrink before the briefing
does.

## L2. No recorded briefing is cut by the 4,000-byte share

**Context.** Task 2.3 also asks for the largest real briefing. The Lab
records no briefing UZE rendered from a real switch: the briefing contract
seeds a 48-byte one, and `experiments/launch_instructions_probe.py` pads a
synthetic one to 4,700 bytes on purpose, to bring the document near the
bound. This machine holds no briefing either (none of 65 conversation
records carries one, since a delivered briefing is retired).

**Decision.** The share stays at 4,000. A briefing UZE renders is bounded by
`briefing::BOUND` (`BRIEFING_SHARE - 32`, 3,968 bytes) before it is
composed, so `compose` never cuts one. Inside that bound the renderer drops
whole sections from the end when the conversation fills them (their own
bounds add up to about 5,700 bytes), Git first and the goal last, which
`when_the_whole_does_not_fit_git_is_cut_before_the_goal` pins. That held at
the old 5,000 share too (4,968 bytes), so D4's reduction changes how much
of a very long conversation reaches the next harness, never whether the
briefing is cut mid-section.

**Alternative.** Record a real switch briefing in the Lab first. A Lab
transcript is one synthetic turn (`tests/_fixtures/foreign/transcripts`),
so the briefing it renders is far below the share and would prove nothing
about the bound.

## P1. The launch renders a declaration refused only for an unignored link

**Context.** `declaration::declared` refuses a manifest whose `workspace.link`
names a file Git does not ignore, which takes one `git check-ignore` per
link. The launch path runs on every relaunch, in the shim.

**Decision.** `declaration::for_instructions` reads the section with every
check but that one. A declaration refused only for an unignored link is
rendered as written, and a section that does not read for any other reason
gives the default policy and a note on the launch. The refusal itself is
reported where the workspace places a checkout.

**Alternative.** Run the full check at launch. That is a Git process per
link for every agent at every relaunch, the cost `branch_on_disk` was
written to avoid.
