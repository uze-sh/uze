## Why

The workspace client is the surface people leave open for days, and it is
the one that learns about updates least. A release reaches a machine a day
or more after it is published: the background pass installs one only once
it has been the latest for 24 hours, and an open client asks about it once
an hour from the moment it started. Plugins are worse: pending updates are
applied only when the client opens, and decided from the head each
marketplace mirror last recorded, which nothing in an open client fetches
again.
A workspace open since Monday never hears of Tuesday's plugin release.

Two of the costs per check are also larger than they look. The release
check follows the `releases/latest` redirect and downloads the tag's HTML
page (about 170 KB) to read one version, and a marketplace fetch mirrors
`refs/*`, which on GitHub includes every pull request's head.

## What Changes

- The workspace client runs one **update tick** about every 15 minutes,
  jittered. A tick reads what the machine last learned and reaches the
  network only when that answer is older than an hour and no other process
  is already asking, under a lock, stamping the attempt before it asks.
- **Release channel** (`UZE_RELEASE_CHANNEL`): `latest`, the default,
  installs a release as soon as a check finds it; `stable` keeps the
  one-day wait. The sidebar's "restart to use it" notice appears as soon as
  the install lands.
- The release check reads the redirect's `Location` and downloads no page.
- **Plugins stay current while the workspace is open**: each tick fetches
  the registered Git marketplaces whose answer is over an hour old, one
  process at a time, then applies the pending updates that need no new
  trust, exactly as opening the workspace does today. A toast says what was
  updated, and one says which update waits for the operator's confirmation.
  The consent rules do not change: a revision that asks to execute
  something new is never applied in the background.
- A marketplace mirror fetches only branches and tags.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `binary-updates`: release channels, the open client's tick, and a check
  that downloads no page.
- `plugin`: an open workspace keeps plugins current, and a marketplace
  mirror fetches only branches and tags.

## Impact

- `src/self_update.rs`: channel, locked and stamped checks, the tick,
  redirect-only check.
- `crates/uze-application`: a background pass over marketplaces and
  plugins (`Plugins::keep_current`) under a lock.
- `crates/uze-core`: the mirror's refspec; two lock paths in `UzeHome`.
- `src/ui/`: the workspace runs the plugin pass on the tick and raises its
  toasts.
- `docs/versioning.md`: the channel and the new cadence.
