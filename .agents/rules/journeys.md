---
trigger: glob
globs: "**/journeys/**, **/.github/workflows/journeys.yml"
---

`journeys/` is the third test tier: a user's flow performed through the
real CLI and TUI in a disposable world, then checked against the machine it
left behind. The `journey-author` skill covers how to write one (never
validate UZE with UZE, screen text is a gate, chapters, scene or file,
load-bearing scenes); `journeys/README.md` holds the vocabulary. What
governs the suite beyond that:

- **Tags say when it runs, numbers say where it sits.** `gate` is the
  pull-request set; everything runs nightly. A journey that becomes slow
  changes its tag, never its chapter.
- `07-upgrade` is the only chapter that runs a *second* binary: the
  previously released `uze`, built once from its tag and kept, never
  downloaded, so a run is not counted as somebody installing that release.
  It costs a build, so it is nightly.
- **The index is `journey list`**, read from the files. No table of what
  the suite proves is kept anywhere.
- A journey backing a user-facing claim names that page in `proves:`, the
  way `docs/architecture/invariants.md` names the test behind each
  property. `journey validate` fails a `proves:` that no longer resolves,
  which tells whoever changes a flow which page to re-read.
