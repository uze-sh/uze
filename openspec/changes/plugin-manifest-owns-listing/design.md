## Context

See proposal.md for why. Today a catalogue (`marketplace_catalogue::Catalogue`)
is the parsed `marketplace.json` plus how to reach a plugin's bytes:

- a local marketplace is read in place on every read;
- a Git marketplace is a mirror cloned with `--filter=blob:limit=1m`, so
  every `plugin.json` is already on disk after the fetch; its
  `catalogue.json` (cache tier) records the source, the time and the
  commit, and each read runs one `git show <commit>:<path>/marketplace.json`;
- the official marketplace is the `build.rs` table of embedded files.

`plugins_offered_by` builds each `MarketplacePluginSummary` from the entry's
`description`/`keywords`. The installer reads only `name` from `plugin.json`.

## Goals / Non-Goals

**Goals:**
- One reader of a plugin's listing fields, fed by the three catalogue
  kinds, so the plugins screen and the JSON reports cannot disagree.
- A plugin listing costs no more processes than it does today.

**Non-Goals:**
- Showing `version`, `author`, `license` or `homepage` (the commit is the
  version; showing the rest is its own decision).
- Reading a vendor-owned catalogue (`.claude-plugin/marketplace.json`).
- Refusing a manifest that still carries the old fields.

## Decisions

### The listing is computed when the catalogue is, and cached with it

A `Catalogue` gains `listings`: per plugin name, the `description` and
`keywords` its `plugin.json` holds at the catalogue's revision.

- Local: read from disk on each catalogue read, the way the manifest is;
  N small file reads beside the one the manifest already costs.
- Git: read once per refill, one `mirror::read_file` per plugin at the
  catalogue's commit, and recorded in `catalogue.json` next to that
  commit. A read served from the cache runs no process for the listing.
  An entry whose `catalogue.json` predates the field is filled on its
  next read and written back; it is cache, observed again, not a record
  with a shape to carry.
- Official: read from the embedded table by path, no process.

Alternative considered: one `git cat-file --batch` per read. Rejected: the
acquisition transport spawns Git with a null stdin on purpose, and opening
a stdin path in the one sanctioned spawn of untrusted repositories widens
it for a saving the cache already makes.

Alternative considered: keep the entry's fields as an override when present
(Claude Code's model). Rejected: it is exactly the second copy that
drifted, and the standard forbids another file overriding `plugin.json`.

### A listing never fails a catalogue

The reader takes a manifest's bytes and answers `Option` for each field: a
missing file, bytes that are not JSON, or a field of the wrong type all
read as absent. A plugin is listed without its description rather than
dropped, and a catalogue never fails because one of its plugins is broken.
`agent plugin check` remains where a broken manifest is reported.

### The entry type loses the fields; the check reads the raw entry

`MarketplacePluginEntry` keeps `name`, `source`, `category`. Unknown fields
are already ignored by its deserializer, so nothing refuses an old
manifest. `check_marketplace` reads the manifest as JSON as well, to find
`description`/`keywords` on an entry and compare them with the plugin's
`plugin.json`, producing one warning per field with one of three actions
(move, delete, reconcile) as the spec states.

## Risks / Trade-offs

- [A third-party marketplace whose plugins carry no `description` in
  `plugin.json` lists without descriptions after upgrading] → the check's
  warning names the move; the release notes say it.
- [A Git catalogue's listing is as old as its last refill, like its
  manifest] → the same TTL and refresh-on-`market add` apply; nothing
  newer is promised than for the manifest.
- [Refill cost grows by one `git show` per plugin] → paid once per hour
  per marketplace, in the background pass, against blobs already local.

## Migration Plan

No record changes shape: `catalogue.json` is cache. This repository and
its fixtures move their fields in the same change. No rollback concern
beyond reverting the change, since an older build reads entries' fields
and ignores nothing it needs.

## Candidate ADRs

- `plugin.json` owns every field that describes a plugin; a marketplace
  entry only locates and classifies it. Supersedes ADR-032's schema line.
