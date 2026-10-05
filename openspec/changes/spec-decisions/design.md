## Context

The spec surface finds units through `dialect::Dialect` tables, each keyed
on a marker its tool defines, and gives every file a `Role`. The places a
project declares for its artifacts reach only the architect today, as
`architect::ArtifactSource`, resolved by the host
(`src/ui/extension_host.rs::artifacts_declared_in`) through
`uze_application::project_artifacts`. Extensions read only through `Host`
(`read_file`, `list_dir`) and never draw.

## Goals / Non-Goals

**Goals:**
- Decisions listed and rendered in the spec surface, from any place the
  project declares, with status and supersession.
- One recognition rule, used both for kept decisions and for a decision
  inside a change.

**Non-Goals:**
- Writing ADRs, numbering them, or changing a status: the surface reads,
  the code surface edits, the `adr` skill authors.
- A dialect for ADR tools. A decision is not a tool's unit.
- Validating ADRs (a lint like adrkit's). A file that fails the format
  test is not a decision, and is not reported.

## Decisions

**Decisions are found by format, in declared places, not by a marker.**
The dialect rule exists because a tool decides where its files live. No
tool decides where ADRs live, so the project's declaration is the only
honest source. *Alternatives:* a fixed list of directories (`adr/`,
`docs/adr/`, `docs/decisions/`) — rejected, it guesses on the project's
behalf and silently misses a project that chose another; a dedicated
`workspace.decisions` key — rejected, it is a key per kind, which
`workspace.artifacts` was shaped to avoid.

**Recognition uses only signals that hold in any language.** A record is
a numbered Markdown file (`NNNN-slug.md`), the one convention adr-tools,
MADR and log4brains share and no translation changes. The title comes from
front matter or the first `# ` heading, the status from front matter, the
header's first non-date `Label: value` line, or a Nygard record's one-line
first section. *Alternative, the first cut:* matching `## Decision`,
`Status:` and `Supersedes` in English. Rejected after it shipped: a record
in Portuguese vanished without a word, which is the failure that looks
like a lost file.

**Supersession comes only from front matter.** `supersedes` and
`superseded-by` name records by file or number; the inverse edge is
derived. A header link to another record is a *reference*, shown both
ways, because whether a sentence says "replaces" or "mentions" is prose.

**The places move to `shared/`.** A second surface now reads them, which
is the extraction rule the crate states. `ArtifactSource` becomes
`shared::places::Places` (declared roots, or why there are none), handed
to both surfaces by the host. The architect keeps reading only Mermaid
from them.

**Reading stays off the render path.** The catalog is built on the
surface's existing background read, like the units are, so opening the
decisions subject never waits on a walk of `docs/`.

## Risks / Trade-offs

- [A project's ADRs live outside every declared place] → the subject is
  not offered, and the empty-state text names `workspace.artifacts`; this
  is the same cost the architect already has.
- [A `docs/` with thousands of Markdown files] → only Markdown is opened, on
  the background read; measured against this repository's `docs/` by
  hand-validation.
- [Free-text supersession that does not link a file] → shown as written,
  never inferred, so a wrong edge is never drawn.
