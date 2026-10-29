## Why

A declared extension can answer from a Rhai script, which the engine
confines on every platform. It cannot yet read a GPU, ask `gh` about a pull
request or list what `openspec` knows, because each of those needs a
program, and a program runs with everything its user can reach. This is
change 5 of `extension-platform`'s split (*Proposed split*, D4, D6,
D18, D27). It lets an extension run a program the operator consented to, by
an argv rebuilt from the manifest they accepted, at a path pinned when they
enabled it. On Linux and macOS the operating system confines that program
to reading the checkout and its own files, writing only a scratch directory,
and reaching no network. Where the machine cannot confine, UZE says the
program "runs as you", and the operator has to agree to that on its own
step. The change also brings the store, the first state an extension keeps
between answers.

## What Changes

- `run` sources and `run` actions: an argv array, its program a relative
  path inside the extension or a bare name granted by `exec:<name>`. The
  check refuses a shell string, a path out of the extension, a shebang whose
  interpreter is not granted, `exec:uze` and a harness's own name. Declared
  fields go in as whole arguments, `--` is required before a substituted
  value, and fields can also arrive as `UZE_IN_<NAME>`.
- The program contract: one JSON document on stdin, an answer in `json` or
  `lines` mode (`output "lines"` with `fields`), the envelope of change 4
  plus `store`, exit codes 0/3/other as hooks use them, bounds in time and
  output that a manifest can lower but never raise, and a scrubbed
  environment.
- Confinement is a concept of `uze-platform`, with one implementation per
  platform:
  - **Linux**: Landlock and a seccomp filter, applied by a re-executed
    helper (`uze __confine`) right before `execve`.
  - **macOS**: `sandbox-exec` with a generated profile.
  - **Elsewhere**: "unavailable".

  On Linux and macOS, every run checks that the confinement applied. A run
  that should be confined and is not is refused.
- `exec:` is pinned when the extension is enabled. The record keeps the
  resolved realpath (`gh → /usr/bin/gh`) and whether confinement was
  enforced when the operator accepted. Where it is not enforced, a separate
  step asks for "runs as you" consent and states what a general-purpose
  runner can do. The Extensions screen can resolve a pinned path again.
- `run_program(name, args)` is a host function for scripts. It runs under
  the same `exec:` grant and the same confinement.
- The store: one JSON document per (identity, project), at most 1 MiB.
  Answers patch it through their envelope, a source declaring `store`
  receives it, and scripts read it with `store()`. `store "global"` needs
  its own consent. The store is removed with the plugin.
- What an extension can do is now visible per right:
  - `uze doctor`, `uze status` and the Extensions screen show whether this
    machine confines programs and which grant each program was pinned to.
  - `uze agent plugin check --run` runs a program only where confinement is
    enforced.
- **BREAKING (authors):** `run` stops being refused by the check. A
  manifest that declared `run` to be refused now needs its `exec:` grants.

## Capabilities

### New Capabilities
- `extension-programs`: what a program is, how the host runs it, its input
  and output, its bounds and environment, and `run_program`.
- `extension-confinement`: what a confined program can reach, where
  confinement exists, and the disclosure where it does not.
- `extension-store`: the state an extension keeps between answers.

### Modified Capabilities
- None in `openspec/specs/`. `extension-scripts` and `extension-consent` are
  still in change 4, which is not archived. Their scenario "A program is
  refused for now" is superseded here, and `extension-confinement` carries
  the consent steps `exec:` adds. The delta against them is written when
  change 4 is archived (decisions.md).

## Impact

- `uze-platform`: a `confine` module, with Linux (`landlock` crate and a
  `libc` seccomp filter), macOS (a `sandbox-exec` profile) and an
  "unavailable" implementation.
- `uze-core`: the program runner (`capability::extension::program`), the
  manifest's `run`, `exec:`, `output`, `fields` and `store`, and the check.
- `uze-contract`: `Grant::Exec`, `Grant::Store`, `Does::Run`, the output
  mode, and the envelope's `store`.
- `uze-workspace`: the record's `exec_paths` and `enforced_at_accept` are
  filled, and the store lives under `UzeHome`.
- `uze-application`: the runner answers programs, consent pins and
  discloses, and the store is patched.
- `src/`: the hidden `uze __confine` command, the Extensions screen's
  confinement and "Resolve again", and `doctor` and `status` lines.
- `uze-script`: `run_program` and `store`.
- Docs: the extensions rule, invariants, diagrams, the `uze:extension`
  skill and a `gpu-meter` fixture plugin.
