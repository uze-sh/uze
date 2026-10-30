## Context

`src/self_update.rs` keeps the installer's binary current. An open client
runs `watch`: a pass now and one every `CHECK_EVERY` (1h) after, each
asking GitHub only when the ledger's `checked_at` is an hour old. A release
is installed once it has been the latest for `SETTLE` (24h). The ledger is
shared by every process, but the open client stamps it only after the
network answered, so two clients that tick together both ask.

Plugins are kept current by `Plugins::auto_update`, which the workspace
calls once, when it opens (`spawn_startup`). It decides from the head each
marketplace mirror last recorded, a JSON read, so it never reaches the
network. A mirror is fetched only when a catalogue is read and its entry is
over `MAX_AGE` (1h) old, and keeping plugins current reads none. In a
client left open, nothing fetches again.

A second review of the proposed 15-minute interval measured two costs:
`curl -L` on `releases/latest` downloads the tag page (about 170 KB, about
1 s), and `fetch +refs/*:refs/*` brings every `refs/pull/*` of a GitHub
marketplace (5,794 of 6,662 refs on `claude-plugins-official`). No
comparable tool asks the network more often than hourly.

## Goals / Non-Goals

**Goals:** an open workspace learns of a release or a plugin update within
about an hour, says so when it happens, and asks the network no more often
than it does today, from one process per machine.

**Non-goals:** push delivery (a webhook needs an endpoint the operator's
machine does not have), changing what a CLI command does on its way out,
changing the trust rules for a plugin update, a "check now" action.

## Decisions

**A local tick separate from the network's age.** The client wakes every
15 minutes ±20% and reads the shared answers; it asks the network only when
an answer is over an hour old. Waking often is what makes an hour-old
answer get refreshed promptly in a client that has been open for days;
the hour is what keeps the load where it is. The alternative, a 15-minute
network interval, multiplied requests by four for a latency nobody asked
for and a release cadence of one every one to six days.

**A lock, and the attempt stamped before the request.** The release check
takes `state/update.lock` and the marketplace pass `state/marketplace-fetch.lock`
with `try_lock`: a process that finds either held skips the network this
tick and reads what the holder writes. The stamp is written first, so a
check that fails waits the hour like one that succeeded; offline is a state
that lasts (the ledger already says so). Locks live in `state/` because
the cache tier may be deleted under a holder.

**Release channel by environment variable.** `UZE_RELEASE_CHANNEL` beside
`UZE_AUTOUPDATE`, which already governs the updater: `latest` (default)
treats a release as settled when found, `stable` keeps `SETTLE`. A key in
`config.toml` was the alternative; the updater reads no config today, and
one variable beside the other is what an operator setting the first will
look for.

**The redirect, not the page.** `curl -sS -o NUL -w %{redirect_url}`
without `-L` reads the `Location` header. The response is a 302 of about
1 KB.

**Branches and tags only.** The fetch refspec becomes
`+refs/heads/*:refs/heads/*` and `+refs/tags/*:refs/tags/*`. A marketplace
ref is a branch, a tag or a commit reachable from one; nothing resolves a
pull request's head. `--prune` then prunes within those namespaces only, so
pull refs an earlier build fetched stay until the mirror is made again,
which is harmless.

**The plugin pass is the startup pass, tied to the fetch.**
`Plugins::keep_current` fetches the stale marketplaces and tries the
pending updates of the ones it fetched, moved or not: trying an update is a
fetch of its own outside the claim, so tying it to the claimed fetch keeps
it to one process an hour, held updates and failures included. Opening the
workspace (`catch_up`) fetches the stale ones and tries every pending
update, as it did before. The workspace runs it on its own tick
thread and raises one toast for what was applied and one, staying, for
what waits for confirmation, each naming the plugins. The management modal
reads the change on its next opening through the plugins' revision.

## Risks / Trade-offs

- [`latest` installs a release the moment it is found, so a bad one
  reaches every open machine within the hour] → `stable` keeps the day,
  and `uze upgrade` installs a fixed release at once.
- [A release that changes the terminal protocol replaces the running
  server at the next client launch sooner than before] → unchanged in
  kind; `docs/versioning.md` already says so.
- [A fleet waking at once asks GitHub together] → the jitter on the tick;
  the hour-old answer means most wakes ask nothing.
