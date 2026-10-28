## 1. Reading

- [ ] 1.1 `uze-contract::component`: the block rules — tag lines (trailing whitespace, CRLF and a BOM accepted), each block at most once, only blank and `//` lines outside, a file with no block refused — and a blanked copy of the file per block
- [ ] 1.2 The view compile reads the blanked `<view>`; its own bytes bounded as a view's, the file as a script's; spans and diagnostics are the file's
- [ ] 1.3 `uze-script` compiles the blanked `<script>` without requiring `answer`; positions are the file's

## 2. Manifest and calls

- [ ] 2.1 `view`, `script` and `resident` name `.uze` files; a `.kdl` view or `.rhai` script is refused, naming the `.uze` to write
- [ ] 2.2 `Does::Call { function, file }` decoded from `call "<fn>" in="<file>.uze"`; refused on a surface whose script a source runs `resident`, and for `answer` and the resident entry names
- [ ] 2.3 Every site that matches `Does::Script` handles `Call`: `needs_consent`, `then`, the confirm text, the declared engine's action paths, the runner
- [ ] 2.4 A script's role decides what it must define: `answer` for a source or a `script` action, the entries for a resident, the named function for a `call`

## 3. Check and tools

- [ ] 3.1 The check compiles every reachable script, reports its Rhai errors and a missing function, warns on a `<script>` nothing reaches, and scans only script blocks for host functions — residents and `call` targets included
- [ ] 3.2 UZE's highlighter treats tag lines as fences and highlights each block in its language; a `.uze` sample joins `tests/_fixtures/highlight/`
- [ ] 3.3 The language server: `.uze` as a component; inside `<view>` as a view, inside `<script>` nothing; hover texts name `.uze`
- [ ] 3.4 `uze agent plugin create` scaffolds `.uze` files

## 4. Migration and proof

- [ ] 4.1 The gallery's 15 extensions (28 views, 11 scripts) and `tests/_fixtures/README.md`
- [ ] 4.2 Journeys 15–29 that write extensions, journey 15's live edit included
- [ ] 4.3 Rust tests across `uze-core`, `uze-script` (its fixture suites read gallery scripts), `uze-application`, `uze-extensions`, `src/`, `tests/`; sample names in tests
- [ ] 4.4 `uze-contract/src/docs.rs` (the generated reference and schema), the `uze:author` Skill and its extension reference (editor filetype setup for `.uze`), the sandbox timesheet
- [ ] 4.5 Tests: the block rules and CRLF, positions in a view's and a script's errors, refusal of the old suffixes, `call` (needs consent, refused on a resident, missing function), a `.uze` highlighted
