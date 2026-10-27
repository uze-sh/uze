## Why

Declared extensions draw from fixtures and record their actions: nothing a
plugin ships runs. This is change 4 of `extension-platform`'s split,
the least that runs a third-party plugin end to end: a source answered by a
Rhai script, an action that does something, events that refresh a surface,
and the consent that has to come before any of it. Scripts run confined by
the engine on every platform, so this change offers no grant the engine
cannot enforce on its own; programs, `exec:` and OS confinement are change 5.

## What Changes

- The full `extension.kdl`: `source { script | run | fixture, input,
  every, refresh-on }`, `action { id, label, on, key, confirm, script |
  refresh | clipboard | open-path | open }`, `wants`. The check accepts
  only the grants this change enforces (`read:checkout`, `clipboard`,
  `open-path`) and refuses `run`, `exec:` and every later grant with the
  change that brings it. `timeline` joins the reserved ids.
- Consent: `state/extensions.json` records, per identity
  `plugin@marketplace/id`, the grants accepted, the digest of the code they
  were accepted for, and the projects the extension is enabled in. Consent
  is per machine; enabling is per project; enabling is refused from inside
  an agent's pane; an update that widens the grants is held.
- Extension actions reach the keymap through `Action::Extension` and a
  table of each extension's own, resolved after the host's.
- A new leaf crate, `uze-script`, runs Rhai scripts in a raw engine with
  operation, depth, size and time bounds and only the host functions the
  extension's grants register. The declared engine asks the host for an
  answer by declaration id (`Host::answer`); the runner in
  `uze-application` resolves it from the consented compile and keeps
  answers in memory for the session.
- Sources and actions run in the declared engine: staleness by question,
  item menus, the footer, confirmation with the resolved arguments, the
  minimal envelope (`data`, `refresh`, `toast`, `notice`), and the
  attribution strip on every third-party surface.
- `every`, `paths.changed` and `checkout.changed` refresh sources.
- The Extensions screen, `uze status` and `uze inspect` say which
  extensions are enabled, where, with which grants, or why not; `uze
  install` asks on a terminal outside an agent's pane.
- **BREAKING (authors):** a declared extension that is not consented to and
  enabled in the project draws nothing; fixtures stay the default for
  linked marketplaces' authoring.

## Capabilities

### New Capabilities
- `extension-scripts`: sources answered by Rhai scripts, actions, the
  answer envelope, refresh by events, and what a script may reach.
- `extension-consent`: grants, the consent record, enabling per project,
  the update rule, the checkout an extension may read, and where consent
  may be given.

### Modified Capabilities
- `extension-views`: data comes from a fixture or a script, and an action
  runs instead of being recorded.
- `input-keymap`: extension actions are keymap actions in a table of their
  own.

## Impact

- New crate `crates/uze-script` (dependency: `rhai`, reviewed in its PR).
- `uze-contract`: manifest types for sources, actions, grants.
- `uze-core`: the decoder and the check.
- `uze-workspace`/`uze-application`: the consent record, the
  `ExtensionRunner`, the checkout definition.
- `uze-keys`: `Action::Extension` and the per-extension table.
- `uze-extensions`: `Host::answer`, sources and actions in `declared/`.
- `src/ui`: the Extensions screen, the attribution strip, the action
  wiring; `src/cli`: `status`, `inspect`, `install`.
- Docs: ADR-041 amended, the extensions rule, invariants, diagrams, a
  minimal `uze:extension` Skill.
