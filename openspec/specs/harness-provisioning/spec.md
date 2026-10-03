# harness-provisioning Specification

## Purpose
TBD - created by archiving change provision-harnesses-through-official-sources. Update Purpose after archive.
## Requirements
### Requirement: Explicit setup provisions a selected supported harness

When a user invokes `uze setup <harness>`, UZE SHALL use only that
integration's documented official installation or update route to ensure its
CLI executable is available at the latest stable channel, then verify the
executable before preparing UZE-owned integration state.

After a verified setup, UZE SHALL create or refresh the generic runtime shim
for that harness. Setup SHALL be the one writer of runtime shims: preparing
a harness on the way through a package install SHALL NOT create one. Opening
the workspace invokes the same setup for the harnesses it finds installed
and not yet set up, verifying the executable already there without taking
the vendor's install or update route; only on a machine with no harness
does it ask which harnesses to provision, and those take the official
route. The shim SHALL be a transparent launch boundary unless the owning
integration declares additional runtime contribution.

The vendor installer's output SHALL NOT interleave with UZE's report: it is
written to a per-harness log, `$UZE_HOME/cache/logs/setup-<harness>.log`,
replaced on each run, and the installer runs in a session of its own with
no controlling terminal and no standard input. The setup report names that
log for each harness.

#### Scenario: Harness executable is absent

- **WHEN** `uze setup opencode` is invoked and OpenCode is not detected
- **THEN** UZE runs OpenCode's official platform-appropriate install route
- **AND** verifies that `opencode --version` succeeds before marking setup
  ready
- **AND** reports an actionable provision failure without claiming the
  integration was prepared.

#### Scenario: Installer places the executable outside the running shell's PATH

- **WHEN** `uze setup opencode` installs OpenCode and its official installer
  places `opencode` in a documented directory the current `PATH` does not
  reach, having only edited the shell's rc files
- **THEN** UZE verifies the executable at that documented location
- **AND** the setup report names that location and says a new shell is
  needed to run it by name.

#### Scenario: Harness executable already exists

- **WHEN** `uze setup codex` is invoked and Codex is detected
- **THEN** UZE uses Codex's official update route for the supported platform
- **AND** verifies the executable after the update attempt and records the
  attempt
- **AND** prepares UZE-owned integration prerequisites only after verification.

#### Scenario: The workspace opens on an installed harness that is not set up

- **WHEN** `uze workspace` opens and a harness is installed but not set up
- **THEN** setup runs for it from the executable already there, verified by
  its version and not updated
- **AND** its shim is placed as for an explicit `uze setup`.

### Requirement: Setup uses vendor-owned provisioning semantics

UZE SHALL keep vendor installer URLs, commands, platform restrictions, and
version parsing inside the owning integration. UZE Core, Store,
CapabilityRouter, PackageExposurePlan, and attachment ledger SHALL not
interpret them.

#### Scenario: A platform has no supported official automatic route

- **WHEN** a selected integration has no documented official automatic route
  for the current platform
- **THEN** setup returns a structured unsupported/blocking result
- **AND** it identifies the official manual route when known
- **AND** it SHALL NOT run an unofficial installer or package-manager command.

### Requirement: Plugin install never provisions harness executables implicitly

`uze install <name>@<marketplace>` SHALL prepare and attach to already
detected harnesses but SHALL NOT install, update, or remove a harness
executable.

#### Scenario: A supported harness is absent during plugin install

- **WHEN** a plugin is installed and a harness executable is absent
- **THEN** UZE installs the package once in its Store
- **AND** delivers it to the detected harnesses only
- **AND** does not invoke networked provision commands.

### Requirement: Setup exposes already installed packages

After a selected harness is verified and UZE preparation succeeds, setup
SHALL plan and deliver every stored package to that harness through the
existing package-first exposure rules. Native package delivery SHALL continue
to suppress duplicate capability attachments.

#### Scenario: Package was added before the harness existed

- **WHEN** a package is already in the Store and its target harness becomes
  available through `uze setup <harness>`
- **THEN** setup exposes the package using that integration's delivery plan
- **AND** records normal attachment receipts for persistent artifacts.

### Requirement: Provisioning provenance is separate from attachment ownership

UZE SHALL persist secret-free facts about a provisioning attempt separately
from attachment receipts: its action, status, method label, platform,
verified version and time, and never a command, URL, credential or command
output. A provisioning record is evidence that UZE invoked a route; it SHALL
NOT be treated as evidence that a live executable or a harness-managed
artifact is safe to remove.

#### Scenario: A failed attempt is still history

- **WHEN** an official install route fails
- **THEN** the provisioning record says the attempt failed
- **AND** no integration is prepared and no attachment receipt is written.

### Requirement: Setup conformance covers every registered harness

The deterministic conformance suite SHALL exercise `uze setup <harness>` for
every integration registered by the product. Each scenario SHALL prove that
setup dispatches the integration's documented update route through the
vendor executable resolved on `PATH`, reports the harness ready, and places
its default runtime shim, without invoking a network installer. Adding a
registered harness without its setup scenario SHALL fail the suite.

#### Scenario: A legacy executable has incompatible update syntax

- **WHEN** only OpenCode's legacy `opencode2` executable is detected
- **THEN** `uze setup opencode` uses OpenCode's documented installer route
- **AND** it SHALL NOT invoke `opencode2 upgrade`
- **AND** setup verifies the legacy executable before it prepares UZE state.

