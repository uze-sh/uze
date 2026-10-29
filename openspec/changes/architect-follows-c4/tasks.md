## 1. Reading C4 as Mermaid writes it

- [x] 1.1 Macro arguments matched to each signature, positional then `$name=`; relation description kept; typed boundaries read tags, not a type
- [x] 1.2 Element and boundary kinds kept on the graph (abstraction, external, boundary kind) for the zoom tree and the check
- [x] 1.3 Dynamic relations numbered by statement order; `BiRel` with two heads; `Rel_Back` reversed
- [x] 1.4 Types as C4 names them, external included; a queue drawn apart from a person

## 2. The zoom tree

- [x] 2.1 Scope and parent per C4 view, by boundary and kind; explicit `$link` to a catalogued `.mmd`; supplementary views beside their scope
- [x] 2.2 The C4 area ordered depth first; siblings levels first
- [x] 2.3 A box leads inside only to its children; both marks on a box that leads both ways
- [x] 2.4 The trail is the path of ancestors; the step on show opens its siblings (`TrailStep::choices`, host renderer); going up selects the zoomed box

## 3. Drawing every relation and a key

- [ ] 3.1 A label with no room becomes a numbered marker
- [ ] 3.2 The key under the drawing: shapes, external, boundaries, dotted lines, marks, markers

## 4. Links and the check

- [ ] 4.1 `$link` refuses URLs, accepts directories, drops a fragment
- [ ] 4.2 The check's C4 notes, in text and JSON; the repository's own views held to none

## 5. Content and proof

- [ ] 5.1 The repository's C4 views, the `uze:architect` Skill and ADR-050 rewritten to the same rules
- [ ] 5.2 Tests for each scenario; `openspec validate --all --strict`, `uze agent artifacts check`, the workspace suite
