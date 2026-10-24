## 1. Contract

- [x] 1.1 `uze-contract` catalog: `Rule`, `Details`, `Entry`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd`, `Grid`, with their arguments, properties, children and docs; containers accept them; tests that each round-trips and is documented

## 2. Engine

- [x] 2.1 `uze-extensions` `view.rs`: a `NodeKind` per node; `declared/bind.rs` binds each from its arguments and properties (a chart's numbers from a list, a gauge's role from its thresholds); engine tests per node

## 3. Renderers

- [x] 3.1 `uze-theme`: `Symbol::ChartLevel` with eight frames, in every bundled theme
- [x] 3.2 `Rule`, `Kbd`, `Stat` and `Gauge`: widths, heights and drawing, TestBackend tests in both themes
- [x] 3.3 `Chart` (`sparkline` and `bar`) and `Details`: drawing and tests, including a scale of all zeros and a narrow width
- [x] 3.4 `Markdown` and `Code` through the code surface's rendering, clamped to 200 rows; tests for wrapping, a fence and the cap
- [x] 3.5 `Grid`: columns from the width, rows from the tallest tile; tests at two widths

## 4. Proof

- [x] 4.1 A reference extension in the canonical `gsd` fixture plugin's sibling, `showcase`, with one view per family and tests with snapshots in both themes
- [ ] 4.2 The `uze:author` Skill's node list, and the catalog page's map
- [ ] 4.3 `make check`, `openspec validate --all --strict`
