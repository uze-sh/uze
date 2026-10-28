## 1. Reading

- [ ] 1.1 `uze-core`: a block reader for `.uze` — tags alone on their lines, each block at most once, nothing but blank and `//` lines outside them, every block's offset kept
- [ ] 1.2 The view compile reads a `.uze`'s `<view>`; the script compile its `<script>`; diagnostics report against the whole file

## 2. Manifest and calls

- [ ] 2.1 `view`, `script` and `resident` name `.uze` files holding the block they need; a `.kdl` view or `.rhai` script is refused, naming the `.uze` to write
- [ ] 2.2 `Does::Call(function)` decoded from `call`; the check refuses a `call` on a surface whose file has no `<script>` or no such function of two parameters
- [ ] 2.3 `uze-script` calls a named function; the runner answers a `call` from the script of the surface the action is on

## 3. Tools

- [ ] 3.1 UZE's highlighter reads `.uze`, each block in its language
- [ ] 3.2 The language server completes and hovers inside `<view>` as in a view, and reports the check's diagnostics on `.uze` files
- [ ] 3.3 `uze agent plugin create` scaffolds `.uze` files

## 4. Migration and proof

- [ ] 4.1 Every gallery fixture, journey, the sandbox and the official plugin's examples move to `.uze`
- [ ] 4.2 Tests: the reader's rules, offsets in diagnostics, refusal of the old suffixes, `call`
- [ ] 4.3 `docs/extensions/`, the `uze:author` extension reference
