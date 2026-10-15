## ADDED Requirements

### Requirement: The tool vocabulary is measured from the real harness

Every run SHALL capture, from the harness's own request to the synthetic provider, the tools the harness declares and the input fields of each. Every native tool name and input field that UZE's hook vocabulary binds for that harness SHALL be present in the capture, or the run SHALL fail, naming the binding and the tools that were declared instead. A tool call the provider scripts SHALL name a tool that is present in the capture.

#### Scenario: A vendor renames a tool
- **WHEN** a harness release renames the tool a portable alias is bound to
- **THEN** the next run fails with the alias, the old native name and the declared tools, before any hook check runs

#### Scenario: The provider cannot script a tool the harness lacks
- **WHEN** a scenario asks the provider to call a tool that is absent from the capture
- **THEN** the run fails with that tool named, and no check is evaluated on the turn

### Requirement: The Lab keeps its own expectation of the tool vocabulary

The Lab SHALL keep its own expectation of which native tool serves each alias, independent of UZE's binding table, and SHALL check both against the capture, so a disagreement between the two is a failure rather than a shared mistake.

#### Scenario: The binding table and the Lab disagree
- **WHEN** UZE's binding table and the Lab's expectation name different native tools for one alias
- **THEN** the run fails, naming the alias and both native names, even when one of them is present in the capture

### Requirement: A tool the harness resolves itself is measured by a recorded discovery

A harness whose request carries no tool declaration for some tool (a tool the harness resolves itself) SHALL have that tool measured by a recorded `--discovery` capture, which names the harness version it was taken on.

#### Scenario: A tool absent from the request
- **WHEN** a harness resolves a bound tool itself and its request to the provider declares nothing for it
- **THEN** the tool's name and input fields are checked against a recorded `--discovery` capture that names the harness version it was taken on

### Requirement: Every claimed delivery is exercised

For each harness, every portable hook alias, every event, and every effect that UZE reports as delivered SHALL have a check in which the real harness makes the real tool call, the delivered handler runs, and the `HOOK_*` values it received are relayed back to the run from the harness's own payload. A combination that UZE does not deliver to a harness SHALL be a declared limitation, pinned to a version and carrying its reason. It SHALL NOT be omitted.

#### Scenario: An alias without a check
- **WHEN** a binding table gains an alias for a harness and no check fires it there
- **THEN** the deterministic suite fails, naming the alias and the harness

#### Scenario: Removal and update are exercised
- **WHEN** a package UZE delivered to a harness is updated and then removed
- **THEN** the run asserts, from the harness's own listing and from the filesystem, that the update re-projected what changed and that removal left nothing the harness still loads

### Requirement: Every delivery route has a fixture that takes it

Every delivery route UZE can take for a harness (the package's own envelope, a generated envelope, and capability by capability) SHALL have a fixture package that takes it.

#### Scenario: The explicit route is exercised
- **WHEN** a fixture package ships its own envelope for a harness beside a root `plugin.json` that another manifest format also reads
- **THEN** the run asserts which capabilities the harness loaded and that each was loaded once

### Requirement: A check proves the behaviour it names

A check that asserts an absence (a tool that did not execute, a handler that did not run, a marker that never appeared) SHALL be evaluated only after a paired check has proven, in the same turn, that the subject ran: the handler left its own marker, or the harness relayed its reason. A check's verdict SHALL be computed from what was observed; a constant verdict SHALL be refused.

#### Scenario: No hook ran
- **WHEN** a deny check's turn ends without the handler's marker and without the harness relaying a reason
- **THEN** the absence check fails as unproven, rather than passing because the tool did not execute

### Requirement: A settled turn proves nothing on its own

A turn SHALL count as settled only when its final answer arrived and the surface went quiet, and no verdict SHALL rest on a turn having settled alone: a tool that ran is proven by what it left behind (a file, its output in the next request) and never by a tool result arriving, since an error, an unknown tool and a refused call answer with one too.

#### Scenario: A tool result arrives and nothing was left behind
- **WHEN** a scripted tool call is answered with a tool result but leaves no file and no output in the next request
- **THEN** the check that the tool ran fails as unproven

### Requirement: A presence marker is a value only the subject can produce

A marker that proves presence SHALL be a value only the subject can produce (a nonce the handler or tool derives at run time), never text that appears in the prompt, in the scripted call or in any request the provider received before the subject ran, and never a common word.

#### Scenario: A marker the request already carries
- **WHEN** an allow check looks for a marker that also appears in the scripted command's arguments
- **THEN** the Lab's lint fails, naming the marker and the place it already appears

### Requirement: A check observes what the harness did with what UZE delivered

A check on the terminal interface SHALL assert content UZE delivered, not the harness's own chrome. A check SHALL observe the harness's behaviour, never UZE's own output or configuration, and never a route UZE no longer delivers through.

#### Scenario: A check reads UZE's own report
- **WHEN** a check takes its evidence from UZE's output or configuration, or from text the harness draws as its own chrome
- **THEN** the Lab's lint fails, naming the check

### Requirement: A parity check holds only when the native side is proven

A parity check SHALL hold only when the native side passed its own presence check, so two broken sides never agree into a pass.

#### Scenario: Both sides of a parity check are broken
- **WHEN** neither the native plugin nor UZE's delivery produces the behaviour a parity check compares
- **THEN** the parity check fails as unproven, rather than passing because the two sides agree

### Requirement: The rules a check obeys are enforced by a lint

The rules a check obeys (presence proven before an absence, an observed verdict, a settled turn proving nothing alone, a marker only the subject produces, delivered content and the harness's behaviour as evidence, and parity only on a proven native side) SHALL be enforced on every scenario and contract by a lint in the deterministic suite, not by review.

#### Scenario: A constant verdict is written
- **WHEN** a scenario calls a check with a literal verdict
- **THEN** the Lab's lint fails, naming the file and the line

### Requirement: Declared limitations are measured on every run

Every declared limitation SHALL be one kind of result, whether a vendor scenario or a contract binding records it. It SHALL pass through the same gate, and it SHALL match a registry entry that names the harness versions where it was observed. A wildcard version SHALL be refused. A declaration SHALL be the outcome of a measurement taken in the run, showing that the harness still lacks the control, and never a constant. A declaration whose measurement did not run SHALL fail as unproven.

#### Scenario: A declaration nobody measured
- **WHEN** a binding declares a limitation without a measurement having run in that turn
- **THEN** the gate fails the declaration as unproven

#### Scenario: A contract declares a limitation
- **WHEN** a contract binding declares that a harness cannot deliver part of the contract
- **THEN** the result is adjudicated against the registry exactly as a vendor scenario's declaration is, and counted as declared, not as asserted

### Requirement: A limitation that reproduces is re-pinned, and one that stops is escalated

On a harness version the entry does not name, a limitation measured as still reproducing SHALL pass, reporting the entry to re-pin and publishing the re-pinned registry with the run's evidence. A limitation that stops reproducing SHALL fail as escalated on any version.

#### Scenario: A vendor release lands
- **WHEN** a nightly run probes a harness version that a registry entry does not name, and the limitation reproduces
- **THEN** the run passes, names the entry and the new version, and its evidence carries the registry with that version added

#### Scenario: A limitation stops reproducing
- **WHEN** a limitation's measurement shows the harness now has the control, on a named version or a new one
- **THEN** the run fails the limitation as escalated

### Requirement: The Lab answers no prompt a user would meet

The Lab SHALL NOT pre-seed a configuration, pass a flag or set an environment variable that answers a vendor prompt a user meets after `uze install` (trust of a folder, trust of a hook, enablement of a feature, a permission). Such a prompt SHALL be answered only the way a user answers it: a gesture on the harness's own interface, once the prompt is on screen. It SHALL NOT author anything a user would not have, such as an agent written by the Lab so that a capability becomes reachable.

#### Scenario: A prompt is answered without a decision
- **WHEN** a scenario adds a bypass flag or a pre-seeded trust entry that no recorded decision names
- **THEN** the Lab's lint fails, naming the flag or the entry

### Requirement: The Lab's world matches a user's machine in everything but the provider

The Lab SHALL run the harness against the same UZE home that the packages were installed into. The world SHALL match a user's machine in everything that is not the provider, and a difference that remains SHALL be a recorded decision. The container's declared isolation SHALL be the isolation actually applied.

#### Scenario: The world a run starts in
- **WHEN** a run starts a harness
- **THEN** the harness was installed by the vendor's own route and is started by its own command on the user's `PATH`
- **AND** the project is a Git repository, and every interactive step runs with a controlling terminal that nobody answers on the user's behalf

### Requirement: A prompt outside UZE's scope is answered only with a recorded decision

The Lab MAY answer a prompt outside UZE's scope (signing in to the synthetic provider, first-run onboarding unrelated to delivery), and SHALL do so only with a recorded decision that names the prompt and the reason.

#### Scenario: Signing in to the synthetic provider
- **WHEN** a harness asks to sign in before its first session
- **THEN** the Lab answers it only because a recorded decision names that prompt and the reason

### Requirement: Every harness has a first-session scene

For every harness, a first-session scene SHALL start the harness as a user would after `uze install`. It SHALL assert what UZE reports while each prompt that affects delivery is unanswered. It SHALL then answer the prompt through the harness's own interaction and assert what is delivered afterwards.

#### Scenario: A harness gates hooks behind a review
- **WHEN** the first-session scene opens a harness that will not run hooks until the user trusts them
- **THEN** UZE's report names those hooks as pending with the action, no hook has run, and after the scene answers the review through the harness's interface the hooks run
