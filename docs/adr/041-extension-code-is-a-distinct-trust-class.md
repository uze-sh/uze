# Extension code is a distinct trust class from plugin bytes

Status: Accepted

Amended 2026-10-23: `uze-extensions` may depend on exactly one UZE crate,
`uze-contract`, the extension contract (manifest and compiled-view types,
the binding grammar, the catalog's names, the sanitizer). It is plain data
with no I/O, no other UZE crate and no rendering library, enforced by the
architecture suite, so an extension handed one of its types is handed
nothing that reaches the machine. It exists so the domain's decoder and the
presentation engine share one definition of an extension without either
reaching the other (`openspec/changes/archive/2026-10-24-extension-view-engine`). Nothing else
about the decision changed.

Amended (extension-manifest-and-scripts): the loading mechanism is chosen.
An extension's logic is a Rhai script run by `uze-script`, a crate
depending on `uze-contract` and `uze-platform` only, whose engine is built
raw and reaches only the host functions the extension's grants register
(`read:checkout` registers `read_checkout` and `list_checkout`, beneath the
checkout and following no link). A program (argv) is not run yet: when it
arrives (change 5 of `add-extension-platform`) it is run by the OS-confined
runner where the platform confines it, and consent says where it does not.
Dynamic libraries and native resident interpreters stay ruled out. The
"second concrete use case" is the gallery of `add-extension-platform` D20.
Extension state (today the grants record, `state/extensions.json`, named in
`UzeHome`; later the store and the preferences) lives under `UzeHome` and is
written by `uze-application` on the extension's behalf: an extension still
touches none of UZE's state directly. The declared engine asks the host by
declaration id, never by argv or script text:
`Host::answer(Question { identity, declaration, asking, input, checkout })`,
which `ExtensionRunner` answers from the compile whose digest the operator
consented to, after judging consent, enabling in the project and the
checkout. Trust (may these bytes be installed) is still never persisted;
consent to run an extension's code is a different question, answered per
grant set, and a later update that widens the set is held until asked again.

## Context

UZE already has a trust boundary. `trust.rs` gates one question — may
these bytes be installed — and the recorded decision is that it is "one
consent boundary, not a permission system". An invariant holds that a
default plugin crossing that boundary is never installed silently.

That boundary was designed for a plugin: bytes UZE stores and delivers to
a harness, which the harness then reads. UZE never executes them. The risk
is transitive and the consent is a single yes.

An extension is not that. An extension is code UZE itself runs, in UZE's
own process, on the operator's machine, while the operator is looking at
something else. Today every extension is compiled into the binary, so the
question has not arisen: the code is the product's own, reviewed with it
and shipped with it.

The question arises the moment an extension is authored by anyone else,
and by then the mechanism for loading it will already have been chosen. A
sandbox is a property of the loading mechanism, not something added
afterwards.

## Decision

Extension code is a distinct trust class from plugin bytes, and the
existing consent boundary does not cover it.

Two consequences bind now, while there is nothing to load:

- An extension never touches UZE's own state — no `UzeHome`, no Store, no
  receipts, no `uze-application`. `uze-extensions` depends on no UZE
  crate but the data-only `uze-contract` (amendment above) and names no
  process, filesystem or environment API; everything
  it needs arrives through the `Host` trait the TUI implements. This is
  enforced by the architecture suite rather than left as a doc comment.
  It is what keeps an extension a pure function of what it is handed.
- Any future mechanism for loading an extension authored elsewhere must
  carry a capability model, not a single install-time yes. This rules out
  dynamic libraries, which offer no boundary at all, and it is the reason
  a sandboxed runtime's weight is a cost worth paying rather than overhead
  to be avoided.

No loading mechanism is chosen here, and none should be until a second
concrete use case exists.

## Consequences

The registry's claim that extensions "never touch the machine" is
corrected to the narrower property that actually holds and is actually
useful: the current extension does run Git and read files, and always
did — through the host, never on its own.

The trust model stays one consent boundary for plugins. Extending it into
a permission system is deferred, but the shape of what would need
permission — UZE's own state — is fixed now, so the answer is not designed
under pressure later.

A capability-scoped application surface stops being only an internal
tidiness argument: it is the thing that would let an extension be handed
read access to packages without being handed the Store (see ADR-042).

Source change: openspec/changes/archive/2026-09-03-enforce-architecture-seams/
