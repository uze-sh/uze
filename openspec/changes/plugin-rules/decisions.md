# Decisions made during implementation

Choices the design left open, recorded where they were made so review can
weigh them. One entry each: Context, Decision, Alternative.

## A plugin's rules are not a `CapabilityKind`

- **Context.** Task 2.1 says to recognise `rules/*.md` "as the Rule
  capability". Every `CapabilityKind` becomes a `Resource` that the Engine
  routes to each integration's machine-wide delivery.
- **Decision.** Rules are discovered by their own function,
  `uze_core::rules::plugin::shipped_rules(package_root)`, and never enter
  `package_resources_at`. `uze agent plugin check` lists each valid one as
  a `package:<id>:rules/<rule>.md` identity beside the resources, and the
  integrations keep withholding `rules/` from every envelope.
- **Alternative.** A `CapabilityKind::Rule` routed like the others, which
  every integration would have to refuse. That puts the "never
  machine-wide" guarantee in four places instead of making it structural.

## No receipt for a copy

- **Context.** D2 says a receipt is still written where UZE performs the
  copy, for the inspect-before-detach path, while the judgement never
  depends on one.
- **Decision.** No receipt is written. Inspect-before-detach compares each
  copy's bytes with the plugin's rule at the revision it was locked at
  (and at the new one, on update), which is a content identity check
  stronger than a receipt and the same on every machine.
- **Alternative.** A receipt per copy in `$UZE_HOME`. It would exist only
  on the machine that copied, so every other machine would still need the
  content check, and the two could disagree.

## What a copy is judged against when the lock pins nothing

- **Context.** A marketplace linked to a working copy on this machine pins
  nothing, so the lock has no entry and no integrity for its plugins.
- **Decision.** Such a plugin's copies are judged against, and copied
  from, the Store's bytes: what this machine installed from the working
  copy. The author's own loop then sees the rules in a project.
- **Alternative.** Copy only locked plugins. A project that is its own
  marketplace, or an author iterating on a linked one, would never see its
  rules until it published.

## When `status` acquires the locked revision

- **Context.** `status` judges copies on a fresh machine by acquiring the
  locked revision (D2). A machine past the lock, the ordinary state after a
  `uze update -m`, would otherwise acquire every plugin on every `status`.
- **Decision.** `status` reads the Store when it holds the locked bytes.
  Otherwise it acquires only when there is something to judge: a file of
  the plugin's namespace in the checkout, or a Store revision that ships
  rules. A plugin with neither is not reported. `uze install` asks the
  same way, since after reproduction the Store holds the locked bytes of
  every plugin it lacked. Add and update, whose revision just moved,
  always read the new one, which the Store then holds.
- **Alternative.** Always acquire. Correct in the one case skipped (a
  plugin not on this machine, with no copies, whose locked revision ships
  rules), which `uze install` corrects, at the cost of a Git read per
  plugin per `status`.

## Every shipped file is copied, valid or not

- **Context.** `plugin check` fails on a file that is not a deliverable
  rule, but an installed package may still carry one.
- **Decision.** Every `rules/*.md` is copied byte for byte. The project's
  rules report then names the copy as not a rule, with the same reason
  the plugin check gave.
- **Alternative.** Skip invalid files. The copies would stop being the
  authenticated set, and the project would not learn that its plugin
  ships a broken rule.

## An unknown revision replaces and removes nothing

- **Context.** On update or remove, the revision a copy was locked at may
  not be reachable (no network, a rewritten history).
- **Decision.** The previous rules are then taken as empty: a copy that
  matches the new revision is left, a missing one is written, and every
  other copy is reported (drifted or extra) and left. The report says the
  rules were unverified.
- **Alternative.** Fail the operation. A plugin update would then depend on
  reaching a revision the project is leaving.

## Removing leaves `.agents/rules/` and any file it did not recognise

- **Context.** D5 leaves an extra file in place; spec says remove deletes
  the namespace.
- **Decision.** Remove deletes every copy that matches the locked revision.
  An extra or drifted file stays, is reported, and later shows in `status`
  as a namespace of a plugin the project does not declare. The directory
  itself is left, since the project may have created it.
- **Alternative.** Delete the whole prefix. That would delete an edit,
  which drift discipline forbids.
