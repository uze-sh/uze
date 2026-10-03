# marketplace Specification

## Purpose
Registry of marketplace discovery sources that maps a marketplace name to a generic Git/local source and resolves `marketplace.json` for plugin discovery.
## Requirements
### Requirement: Marketplace registry stores generic Git/local sources
The system SHALL store marketplace entries as `{name, source: Git|Local}` in `~/.uze/state/marketplaces.json`, where Git is a generic URL (not GitHub-specific) and Local is a filesystem path. The registry SHALL NOT copy plugin bytes. A Git source SHALL be recorded as its canonical identity (see the `marketplace-access` capability), not as the spelling the operator typed: a short locator (`owner/repo`, `<alias>:owner/repo`), an `scp`-style locator (`git@host:owner/repo`) and an `ssh://git@host/…` URL with no port all record the same `https://` URL, and a source already registered in another spelling of the same repository SHALL be matched, not reported as a conflict. A local path SHALL be recognised only when it is spelled as one: starting with `/`, `./`, `../` or `~`, or a bare `.` or `..`. A single segment with no prefix (`ai`) SHALL be refused, suggesting `./ai` when that directory exists. A two-or-more-segment argument with no prefix is a short locator, and SHALL be refused as ambiguous when it is also an existing directory.

#### Scenario: Add local marketplace
- **WHEN** user runs `uze market add /home/hiukky/ai`
- **THEN** system records `ai → Local{path:/home/hiukky/ai}` and `marketplace.json` is readable

#### Scenario: Add Git marketplace
- **WHEN** user runs `uze market add https://github.com/hiukky/ai`
- **THEN** system records `ai → Git{url:https://github.com/hiukky/ai}` without cloning plugins

#### Scenario: Add the current directory
- **WHEN** user runs `uze market add .` inside a marketplace checkout
- **THEN** system records the checkout as a Local source, exactly as its absolute path would

#### Scenario: A bare word is not a path
- **WHEN** user runs `uze market add ai` and `./ai` is a marketplace checkout
- **THEN** system records nothing and fails suggesting `uze market add ./ai`

#### Scenario: Add Git marketplace by short locator
- **WHEN** user runs `uze market add hiukky/ai` and the machine's default host is `github`
- **THEN** system records `ai → Git{url:https://github.com/hiukky/ai}`, the same entry the full URL records

#### Scenario: Add Git marketplace by scp-style locator
- **WHEN** user runs `uze market add git@github.com:hiukky/ai.git`
- **THEN** system records `ai → Git{url:https://github.com/hiukky/ai}` and does not read the argument as a local path

#### Scenario: Short locator that is also a directory
- **WHEN** user runs `uze market add hiukky/ai` and `./hiukky/ai` exists in the working directory
- **THEN** system records nothing and fails naming both readings, `./hiukky/ai` for the directory and `github:hiukky/ai` for the repository

### Requirement: Marketplace add validates marketplace manifest
The system SHALL validate that the marketplace source contains a readable `marketplace.json` with `plugins[]` entries (`name`, `source`), and that the manifest's own `name` is lowercase kebab-case of at most 64 characters (see the `plugin` capability). The name SHALL be checked before anything is recorded or mirrored.

#### Scenario: Valid marketplace
- **WHEN** `marketplace.json` exists and is well-formed
- **THEN** `marketplace add` succeeds

#### Scenario: Invalid marketplace
- **WHEN** `marketplace.json` is missing or malformed
- **THEN** `marketplace add` fails with a clear error and records nothing

#### Scenario: A marketplace named outside the rule
- **WHEN** `marketplace.json` carries `"name": "My_Market"`
- **THEN** `marketplace add` fails, names `my-market` as the name it meant,
  and records nothing

### Requirement: Marketplace list and remove manage registry
The system SHALL list registered marketplaces, and `market remove <name>` SHALL be the marketplace's teardown on this machine: every package the Store holds from that marketplace is removed first, each through the same machine-removal path a per-package removal runs (inspect-before-detach, receipt-owned teardown, drift safety); the registry entry is removed last, only when none remains.

#### Scenario: List marketplaces
- **WHEN** user runs `uze marketplace list`
- **THEN** system shows `name`, `source` (path/URL), and plugin count

#### Scenario: Removing a marketplace takes its packages with it
- **WHEN** two packages installed from a marketplace and `uze market remove <name>` runs
- **THEN** both are detached (receipt-owned teardown each), their Store bytes are deleted, and the registry entry is removed

#### Scenario: A blocked package keeps the marketplace registered
- **WHEN** `uze market remove <name>` runs and one of its packages' teardown is blocked by drift
- **THEN** the marketplace stays registered, the answer names the blocked package and the ones that came off, and the exit status says the teardown did not complete

#### Scenario: The marketplace that installs nothing removes in one step
- **WHEN** `uze market remove <name>` runs with no package from it installed
- **THEN** only the registry entry (and any link it carries) is removed

#### Scenario: Remove marketplace
- **WHEN** user runs `uze market remove <name>` and no plugin from it is installed
- **THEN** registry entry is removed

### Requirement: Embedded official marketplace is pre-registered
The system SHALL treat the embedded official marketplace (`plugins/uze`, `marketplace.json` at repo root) as a pre-registered marketplace `uze-official` without requiring `marketplace add`.

#### Scenario: Official marketplace available
- **WHEN** system starts with no registry
- **THEN** `uze marketplace list` shows `uze-official` and `uze plugin list` can resolve `uze@uze-official`

### Requirement: A marketplace's catalogue is served from a cache
What a marketplace registered from a Git source offers SHALL be read from
a cached checkout under `UZE_HOME`'s cache directory rather than by
cloning the source: one checkout per marketplace, recording the source it
was read from and when. A catalogue SHALL stand for a bounded time, after
which the next read clones the source again; registering the same source
again SHALL refresh it immediately; removing the marketplace SHALL drop
it. A marketplace registered from a local path SHALL be read in place on
every read. The cache is reconstructable and never authoritative: deleting
it costs one clone, never correctness.

#### Scenario: A listing does not need the repository
- **WHEN** a marketplace was registered from a Git URL and its repository
  is later unreachable or gone
- **THEN** `market list`, `market inspect`, the plugin listing and
  inspecting one of its plugins still answer from the cached catalogue,
  with no clone attempted while the entry stands

#### Scenario: A catalogue is refreshed once per window, not once per screen
- **WHEN** a cached catalogue is older than its maximum age
- **THEN** the next read clones the source once and every read after it
  is served from the new checkout until it ages out in turn

#### Scenario: Registering the same source again refreshes the catalogue
- **WHEN** `market add` is run with a source already registered under the
  same name
- **THEN** the registry is unchanged and the cached catalogue is replaced
  by the clone that registration made

#### Scenario: A refill that fails answers with the last catalogue seen
- **WHEN** a cached catalogue has aged out and the source cannot be cloned
- **THEN** the read answers with the expired catalogue rather than nothing,
  and the refill is attempted again on the next read

#### Scenario: A local marketplace is never cached
- **WHEN** a marketplace was registered from a local path
- **THEN** every read parses its `marketplace.json` where it is, and no
  cache entry is written for it

#### Scenario: Removing a marketplace drops its catalogue
- **WHEN** `market remove` succeeds
- **THEN** the marketplace's cache entry is removed with its registration

### Requirement: Detach follows the receipt ledger, not detection
The per-package removal SHALL undo every harness artifact the receipts ledger owns for the package, whether or not the harness's vendor binary is detected in the context the command runs in: the ledger is what says UZE delivered something, so an undetected-but-delivered harness is a receipt to tear down through its integration, never a harness to skip.

#### Scenario: An undetected harness's delivered bytes come back
- **WHEN** a package was delivered to a harness whose vendor binary is not on the PATH the command runs with, and the package is removed
- **THEN** that harness's delivered bytes are torn down through its integration, from the receipts

