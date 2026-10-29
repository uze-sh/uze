## 1. Contract

- [x] 1.1 `Box` in the catalog with its properties, docs and children; `Box` in every container's children
- [x] 1.2 A `size` property kind (cells or `N%`), checked by the view decoder, in the schema and the language server
- [x] 1.3 `style` on `Progress` (`block`, `line`)

## 2. Binding and drawing

- [x] 2.1 `NodeKind::Box` bound from the view, every value resolved
- [x] 2.2 The Box's width, height and layout in the renderer, `wrap` and percentages included
- [x] 2.3 Ground (solid, soft) with the host's ink, and the border, painted before the children
- [x] 2.4 `Progress style="line"`

## 3. Proof and documentation

- [x] 3.1 Renderer tests: a row of percentages wraps; a ground keeps its text legible; a full line reads as a bar
- [x] 3.2 A colour value refused by the check, naming the roles
- [x] 3.3 The extension reference in `uze:author`, regenerated `docs/extensions/`, and decisions.md
