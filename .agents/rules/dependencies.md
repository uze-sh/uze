---
trigger: glob
globs: "Cargo.toml, deny.toml"
---

The dependency surface is small on purpose: every addition is a permanent
maintenance liability and part of the binary's supply chain. Choose by
**provenance**, and stop at the first tier that can do the job:

1. **The standard library, or a crate already in the workspace.** Most
   additions lose here.
2. **Foundation**: `rust-lang`, `serde-rs`, `tokio-rs`, dtolnay (`serde`,
   `serde_json`, `thiserror`, `libc`, `tokio`).
3. **A de-facto standard with an organization behind it**: `clap`,
   `ratatui`, `crossterm`, `toml_edit`, `indicatif`/`dialoguer`,
   `anstyle`, `unicode-width`, `alacritty_terminal`, `portable-pty`,
   `rmcp`.
4. **A single maintainer, mature and widely adopted** (`syntect`,
   `schemars`), with the trade-off stated in the PR.

Refused unless there is no alternative and the reason is written down: a
`0.0.x` crate; a crate that has renamed itself; a recently published crate
with no adoption; a crate whose author publishes many similarly-branded
crates with claims disproportionate to their version.

The PR that adds a crate answers each of these:

- **Who publishes it.** `cargo info <crate>` shows `repository`, but the
  publisher sets that field: confirm it is genuinely published from there.
- **Whether it compiles C.** `release.yml` builds eight targets on native
  runners (two musl, two Windows MSVC) and no dependency compiles C on any
  of them today. If a feature flag avoids the C path, take it.
- **`cargo deny check` passes.** A new advisory ignore needs a reason *and*
  the condition that removes it.
- **Transitive weight**: `cargo tree -p <crate>` before, not after.
- `make attributions` regenerates `CREDITS.md`; CI fails when it drifts.

None of this applies to a crate the workspace already depends on.

Written exceptions, each with what removes it:

- **`noyalib`** (`0.0.x`, one author, renamed once from `serde_yml`) parses
  `agents.yaml` and `agents.lock`. Kept because `serde_yaml` is archived and
  `yamlpath`/`yamlpatch` brings `tree-sitter` and a C grammar onto the
  release matrix; its tests, vendored `yaml-test-suite` cases and
  `#![forbid(unsafe_code)]` are the evidence. Contained:
  `project/manifest/edit.rs` is the only file naming `noyalib::cst`, and it
  refuses anchors, aliases, merge keys, tabs and document separators before
  an edit. Track its advisories (`RUSTSEC-2026-0333` was fixed by 0.0.53).
  The exception goes when the crate reaches `0.1`, goes twelve months
  without a release or renames again, or a maintained YAML serde crate with
  an editing model appears.
