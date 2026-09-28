## 1. Model

- [x] 1.1 Subpath on the marketplace source and record (local from
  `--show-toplevel`, remote from `url#sub`); refuse an escaping subpath.
- [x] 1.2 `market link` keeps the subpath; lock entries carry it.

## 2. Readers

- [x] 2.1 One catalogue/plugin path resolver used by mirrored reads, linked
  reads, catalogue refill/adopt and freshness.
- [x] 2.2 Tests: linked and mirrored install from `<repo>/aikit`; escaping
  subpath refused; root marketplace unchanged.

## 3. Report

- [x] 3.1 `market add` prints identity, what is read (checkout + subpath) and
  linked vs mirrored; same in the TUI worker.

## 4. Verification

- [x] 4.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`,
  `openspec validate marketplace-in-a-subdirectory --strict`.
