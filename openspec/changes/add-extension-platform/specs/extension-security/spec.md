## Purpose

Keeps extension code, which UZE itself runs on the operator's machine, from
reaching more than it was granted, from deceiving the operator through the
terminal, and from being installed or widened without the operator saying so.

## ADDED Requirements

### Requirement: An extension declares every grant it needs
An extension manifest SHALL list, under `wants`, every capability its programs
and surfaces use: each external program by name (`exec:<name>`), network
access (`net`), writes to the checkout (`write:checkout`), the clipboard
(`clipboard`), and opening a path in the code surface (`open_path`). A
declaration that uses a capability it did not list SHALL be refused by the
check and at install.

> **Unresolved review findings** (design.md): S2, S3, S7.

#### Scenario: An undeclared program is refused
- **WHEN** an action runs `[gh, pr, list]` and `wants` has no `exec:gh`
- **THEN** `uze agent extension check` fails naming the action and the missing grant

#### Scenario: A packaged program needs no grant
- **WHEN** an action runs a program shipped inside the extension's own directory and `wants` has no `exec:` for it
- **THEN** the check accepts the action, since only external programs need an `exec:` grant

### Requirement: Grants are consented to, recorded, and re-asked when they widen
Installing a plugin that carries an extension SHALL show the operator the
extension's grants and SHALL NOT enable the extension until the operator
accepts them; no extension SHALL be enabled silently, from any marketplace.
The accepted grants SHALL be recorded against the package's digest.

> **Unresolved review findings** (design.md): S4, enablement scope, consent wording.

#### Scenario: Install asks before enabling
- **WHEN** the operator installs a plugin whose extension wants `exec:openspec` and `clipboard`
- **THEN** UZE lists both grants and the extension stays disabled until they are accepted

#### Scenario: Non-interactive install does not enable
- **WHEN** `uze install` runs without a terminal and a plugin carries an extension with unaccepted grants
- **THEN** the plugin's other capabilities install, the extension stays disabled, and the command reports that it needs consent

### Requirement: An update that widens grants waits for consent
An update whose manifest asks for a grant not previously accepted SHALL leave
the extension disabled, still at its accepted grants, until the operator
accepts the wider set; an update that only narrows SHALL apply without asking.

> **Unresolved review findings** (design.md): S5.

#### Scenario: A widened update is held
- **WHEN** an update adds `net` to an extension whose accepted grants did not include it
- **THEN** the extension is disabled with a notice naming `net` as the new grant, and no program of the new version runs before it is accepted

#### Scenario: A narrowed update applies
- **WHEN** an update drops `net` from an extension whose accepted grants included it
- **THEN** the update applies without asking the operator

### Requirement: The host executes only what was declared, from where it was declared
The host SHALL run only argv arrays that appear in the accepted manifest, with
template values substituted as whole argv elements and never re-parsed. A
relative program path SHALL resolve inside the extension's directory in the
Store after following links, and SHALL be refused if it resolves outside it.
A bare program name SHALL run only under its `exec:` grant.

> **Unresolved review findings** (design.md): S7, S8, S9, argv rebuilt by the runner.

#### Scenario: A value cannot inject arguments or commands
- **WHEN** a selected item is named `x; rm -rf ~` and an action runs `[openspec, archive, "{name}"]`
- **THEN** `openspec` receives the single argument `x; rm -rf ~` and no shell ever sees it

#### Scenario: A path cannot escape the package
- **WHEN** a manifest declares `run: [../../bin/tool]` or a link inside the extension points outside it
- **THEN** the check and the host refuse the program as resolving outside the extension

### Requirement: Programs run with a scrubbed environment and confined where possible
A program SHALL start with an environment built from an allowlist (locale,
terminal, `PATH`, `HOME`, and the contract's own variables) and SHALL NOT
inherit tokens, credentials or the operator's other variables. Where
confinement is unavailable, the consent screen and `uze doctor` SHALL say the
grants are declared but not enforced on this machine.

> **Unresolved review findings** (design.md): enabling where confinement is missing.

#### Scenario: A token in the environment does not leak
- **WHEN** the operator's environment has `GITHUB_TOKEN` set and a source prints its environment
- **THEN** `GITHUB_TOKEN` is absent from what the source saw

#### Scenario: Missing confinement is disclosed
- **WHEN** the platform offers no confinement
- **THEN** the consent screen and `uze doctor` state that grants are not enforced for extensions on this machine

### Requirement: A confined program reaches only what it was granted
Where the platform offers confinement, a program SHALL be confined to reading
the checkout and its own extension directory, writing nowhere unless granted
`write:checkout` (then only inside the checkout), and reaching no network
unless granted `net`.

> **Unresolved review findings** (design.md): S1, S6, S10, S13, S17, S18.

#### Scenario: Confinement denies an ungranted write
- **WHEN** on a platform with confinement an extension without `write:checkout` tries to write `~/.bashrc`
- **THEN** the write fails and the program's run is reported as it exited

### Requirement: Nothing an extension produces reaches the terminal unsanitized
Every string that originates in an extension — manifest labels, source
answers, action replies, standard error shown as a reason — SHALL be stripped
of control characters, escape sequences and bidirectional overrides before it
is drawn or copied, and SHALL be bounded in length. An extension SHALL NOT be
able to move the cursor, change the terminal title, write the clipboard
through an escape sequence, or emit a hyperlink.

> **Unresolved review findings** (design.md): S11, S14.

#### Scenario: An escape sequence is drawn inert
- **WHEN** a source answers an item named `"\u001b]52;c;ZXZpbA==\u0007build"`
- **THEN** the navigator shows `build` with the sequence removed and the clipboard is unchanged

### Requirement: Host chrome is never drawable by an extension
Confirmations, toasts, consent screens and the frame's title SHALL be drawn by
the host, and each SHALL name the extension it speaks for. An extension SHALL
NOT draw outside the surface it was given nor produce anything that looks like
host chrome. A confirmation's body and button text MAY come from the manifest
and SHALL be shown under the extension's name.

> **Unresolved review findings** (design.md): S15.

#### Scenario: A confirmation is attributed
- **WHEN** an extension's action asks for confirmation
- **THEN** the dialog names the extension, so it cannot pass for a question UZE itself is asking

### Requirement: Clipboard and opening paths happen only on a gesture
A write to the clipboard or an open of a path SHALL happen only as the result
of an action the operator invoked, never from a source's answer or an event.
A path to open SHALL resolve inside the checkout.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** S16 (the label shown can differ from the text copied: show the exact text, and confirm multi-line copies).

#### Scenario: A source cannot write the clipboard
- **WHEN** a source's answer contains a field named `clipboard`
- **THEN** it is ignored as data and the clipboard is unchanged

### Requirement: An extension reaches none of UZE's own state
No extension program or declaration SHALL be given a path under `$UZE_HOME`
other than its own extension directory in the Store, read-only, nor any
record, receipt or cache of UZE's. Confinement, where available, SHALL deny
the rest of `$UZE_HOME`.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** S1 and S10 (a confinement with socket and path gaps does not deliver this requirement).

#### Scenario: The Store is not handed over
- **WHEN** a source runs
- **THEN** its context names the checkout and its own extension directory and nothing else under `$UZE_HOME`
