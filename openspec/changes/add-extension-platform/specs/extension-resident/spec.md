## Purpose

Lets an extension that needs state, raw input or per-frame drawing run as a
WebAssembly component inside UZE, reaching nothing but the host imports its
grants cover, on every platform.

## ADDED Requirements

### Requirement: A resident surface is fed by a packaged WebAssembly component
A surface with `feed="resident"` SHALL be fed by one WebAssembly component the
plugin ships inside the extension's directory, loaded from the Store with the
digest that was consented to. UZE SHALL NOT compile an extension, and SHALL
refuse a component that is not a module of the published world.

#### Scenario: The consented bytes are what runs
- **WHEN** a component in the Store differs from the digest recorded at consent
- **THEN** the surface does not start and the Extensions screen says the component changed since it was accepted

### Requirement: A component reaches only the imports its grants cover
A component SHALL be given no ambient filesystem, network, environment or
process access; it SHALL reach the machine only through host imports, each
under its grant (`fetch`: `net`; `read-checkout`: `read:checkout`;
`agent-prompt`: `agent:prompt`; `run-program`: its program's grants). A
component importing anything its grants do not cover SHALL be refused.

#### Scenario: An ungranted import refuses the component
- **WHEN** a component imports `fetch` and the extension's `wants` has no `net`
- **THEN** the check and enabling refuse the component, naming the import and the grant it needs

#### Scenario: Confinement holds where the OS offers none
- **WHEN** a resident extension that imports no `run-program` runs on a platform without Landlock or Seatbelt
- **THEN** its component still reaches only the imports its grants cover, and the Extensions screen reports it as enforced

#### Scenario: Programs it runs are reported as they are
- **WHEN** a resident extension imports `run-program` on a platform without Landlock or Seatbelt
- **THEN** the Extensions screen reports the component as enforced and the programs it runs as not enforced

#### Scenario: A checkout read cannot leave the checkout
- **WHEN** a component calls `read-checkout` on `docs/key` and `docs` is a link to `~/.ssh`
- **THEN** the read is refused, since no link is followed and no path resolves outside the checkout

#### Scenario: The component cannot prompt on its own
- **WHEN** a component calls `agent-prompt` from a timer and the extension holds `agent:prompt` but not `agent:prompt:auto`
- **THEN** the host raises its confirmation with the exact text, and nothing is written unless the operator accepts

### Requirement: A component is bounded in time, memory and frames
Each call into a component SHALL be bounded in computation and its memory
SHALL be bounded, and the host SHALL ask it to render at most once per frame
and at most thirty times a second. A component exceeding a bound SHALL be
stopped, its surface showing which bound, while every other surface keeps
working.

#### Scenario: A runaway loop is stopped
- **WHEN** a component's input handler never returns
- **THEN** the call is stopped at its bound, the surface says the extension exceeded its computation budget, and keys keep working everywhere else

#### Scenario: A late frame keeps the last one
- **WHEN** a component's render misses the frame it was asked for
- **THEN** the surface keeps drawing its previous frame and the workspace does not wait for it

### Requirement: A component answers the catalog, never terminal bytes
A component's render SHALL answer a catalog tree or a cell grid of
characters and semantic roles, sanitized like any other extension output.
It SHALL NOT emit terminal escape sequences, colour values outside an image,
or anything drawn outside its slot.

#### Scenario: An escape in a cell is inert
- **WHEN** a component answers a cell whose character is ESC
- **THEN** the cell is drawn blank and nothing reaches the terminal but the host's own output
