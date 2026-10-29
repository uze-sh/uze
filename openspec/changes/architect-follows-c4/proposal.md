## Why

The architect surface reads C4 views as a ladder of levels taken from each
diagram's keyword alone, and joins one view to the next on a bare alias.
C4 is a tree of zooms instead: each container view expands one software
system, each component view one container, and several views may share a
parent. Two component views of the repository's own containers turned the
C4 area into a flat list, a system box led straight to a component view
over the container level, and supplementary views (dynamic, deployment)
were listed as levels. An audit against c4model.com put the engine at
about 62% faithful, the repository's diagrams at about 45%. The operator
asked for 99%.

## What Changes

- C4 views form a zoom tree: a view's scope is read from its outermost
  boundary (a `System_Boundary` for a container view, a
  `Container_Boundary` for a component view, the one internal system for a
  context view), and its parent is the view one level up that draws an
  element of the matching kind with that alias. A `$link` to another
  catalogued `.mmd` declares the zoom explicitly.
- Dynamic and deployment views, and a context view with more than one
  system in scope (a landscape), are supplementary: they sit beside the
  views of the scope they are drawn in, never as a level.
- The menu shows the path of ancestors to the view on show; the last step
  opens the views that share its parent. Going up selects the box that was
  zoomed into.
- Every diagram gets a key drawn under it, listing the notation it uses.
- No relation is lost: a label with no room on its line becomes a numbered
  marker, listed with its text in the key.
- The C4 macros are read as Mermaid specifies them: named parameters,
  relation descriptions, boundary tags, `BiRel` with two heads,
  `Rel_Back` reversed, and dynamic steps numbered by statement order.
- Element types say what C4 says they are, external included, and a queue
  is drawn apart from a person.
- `$link` refuses a URL, accepts a directory, and ignores a `#` fragment.
- `uze agent artifacts check` reports C4 notation notes (no description,
  no technology, an unlabelled line, a view that zooms into nothing, an
  alias that names two kinds of element) beside each artifact.
- BREAKING (surface behaviour only): the C4 area is no longer ordered by
  keyword level, and the trail of entered boxes gives way to the path of
  ancestors.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `architect-surface`: C4 navigation, drawing and the check.

## Impact

- `crates/uze-extensions/src/architect*` (catalog, parser, model, paint,
  navigation, check), `crates/uze-extensions/src/view.rs` (`TrailStep`
  gains the choices its step opens, defaulted empty), and the host's
  trail renderer in `src/ui/extension_view/navigator.rs`.
- `src/cli` report of `uze agent artifacts check` (the notes).
- The repository's C4 views in `docs/architecture/`, the `uze:architect`
  Skill and ADR-050, rewritten to the same rules in the same change.
