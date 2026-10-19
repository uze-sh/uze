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

## UZE's entry is an object holding only its `path`

- **Context.** D7 gives UZE the entries naming a plugin folder it
  declares, and leaves a folder the operator registered as they wrote it.
  Both can name the same folder.
- **Decision.** UZE's entry is exactly `{"path": ".agents/rules/<plugin>@<market>"}`.
  Install adds one only when no entry names the folder, compared after
  normalizing the path (`./`, a trailing `/`, an absolute path inside the
  checkout). Remove takes back only an entry of that exact form. An entry
  naming the folder with a filter, another key or another spelling is the
  operator's and outlives the plugin.
- **Alternative.** Own every entry naming the folder. Removing the plugin
  would then delete an `exclude` the operator wrote.

## The entry goes only when the folder holds nothing

- **Context.** Remove leaves a drifted or extra file in the plugin's folder
  (the decision above on removing).
- **Decision.** The folder and its entry are taken back only when the
  folder is empty after the pass. A folder still holding an edit keeps its
  entry, so the file stays a rule on every harness and `uze status` names
  it as an undeclared plugin's. An install of a plugin that ships no rules
  reads the manifest but reports an unreadable one only when it emptied a
  folder, so a broken manifest is not reported once per plugin.
- **Alternative.** Take the entry back with the plugin. The edited file
  would stay in the checkout and silently stop reaching every agent.

## The entry is written before the first copy

- **Context.** An install writes the copies and the entry, and can stop
  between the two.
- **Decision.** The entry first. An entry for a folder not yet written
  registers nothing; a copy without its entry would sit in the checkout
  read by no harness.
- **Alternative.** Copies first. Their crash window is the one that loses
  rules silently.

## An unreadable manifest stops the whole plugin's copy, not only the entry

- **Context.** D7: a manifest that does not parse is never written, and no
  copy that needs it is made.
- **Decision.** Every copy of the plugin needs the entry, so none is
  written, drifted or extra files are not judged on that pass, and the
  report names the manifest. Removal still deletes current copies, which
  needs no entry, and leaves the manifest as it is. A JSON value that is not
  an object, or whose `entries` is not a list, is unreadable the same way:
  UZE cannot add an entry to it.
- **Alternative.** Copy anyway. The files would land in a folder no harness
  reads, and look delivered.

## A manifest UZE rewrites is pretty-printed, whoever wrote it

- **Context.** Keeping the operator's entries and keys means parsing and
  writing the manifest back; `serde_json` with `preserve_order`, already in
  the workspace, keeps their order and values.
- **Decision.** Every write is 2-space pretty JSON with a final newline. An
  operator's own whitespace is not kept, their content and order are. A
  pass that changes nothing writes nothing, so an untouched manifest keeps
  its bytes.
- **Alternative.** A byte-preserving JSON editor. No crate in the workspace
  does it, and the manifest is small enough for the diff to stay readable.

## What both readers take from an entry

- **Context.** Antigravity's handling of odd entries was not measured, and
  the engine and `uze-core` must agree.
- **Decision.** An entry whose `path` is not a non-empty string free of
  control characters is no entry; a filter that is not a list of strings is
  no filter, and a non-string item in one is skipped. `include_only` and
  `exclude` items are exact paths relative to the entry, never patterns,
  matching what was measured (`exclude: ["sub/"]` changed nothing). A path
  that leaves the checkout registers nothing, since no rule identity can
  name it. Hidden files in a folder follow the flat directory's existing
  reading on each side.
- **Alternative.** Treat filters as globs. Nothing measured says
  Antigravity does, and a second glob dialect would be the one place the
  readers could part.
