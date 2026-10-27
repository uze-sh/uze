## 1. Manifest and check

- [x] 1.1 `uze-contract`: `Grant::{Exec(ProgramName), Store, StoreGlobal}` (P8), `Does::Run(Program)`, `SourceKind::Run(Program)` with `Program { argv, timeout }`, `Output::{Json, Lines(fields)}` on a source, `ExecDecl { name, reads }`, `Envelope.store`, `input store`; tests
- [x] 1.2 `uze-core` decoder: `run` with `timeout`, `output`/`fields`, `exec "<name>" { reads … }`, `wants "exec:<name>" "store" "store:global"`, `input … store=#true` (parent 22.5)
- [x] 1.3 The check: argv shape (no shell string), a relative program inside the extension, a bare name needing `exec:`, a shebang needing its interpreter's grant, `exec:uze` refused, `{field}` after `--` or warned, `every` under 30 s warned for a program source, `exec` reads only for a wanted program, `timeout` at most 10 s, `run` no longer refused; the scaffold teaches `run` (parent 5.8, 22.5, 25.5)
- [x] 1.4 `uze-application`: a harness's own name refused as an `exec:` by asking the integration registry

## 2. Confinement in `uze-platform`

- [x] 2.1 Dependency review for `landlock` 0.4.7 (publisher, repository, no C, `cargo deny check`, `cargo tree`), recorded in decisions.md (parent 5.3)
- [x] 2.2 `uze_platform::confine`: `Confinement` (read, read-execute, write paths, network, rlimits), `Availability` per right, `availability()`, `enter(&Confinement, argv) -> io::Error`, one implementation per platform behind `mod linux; mod macos; mod unavailable;`
- [x] 2.3 Linux: Landlock ruleset (P5) with `HardRequirement` for what was promised, then the seccomp filter over `libc` (P4: sockets, socketpair non-Unix, network calls, `io_uring_*`, `ptrace`, `process_vm_*`, `setsid`, `setpgid`, signals outside the group below ABI 6, the arch prologue), then rlimits (CPU twice the limit, FSIZE 64 MiB, NOFILE 256, AS 2 GiB), `no_new_privs`, then `execve`; exit 125 when anything fails to apply (parent 5.2, 5.2b, 5.2c)
- [x] 2.4 macOS: the generated `sandbox-exec` profile (`deny default`, reads and writes as on Linux, `deny network*`, `signal (target same-sandbox)`, `process-fork`) executed through `/usr/bin/sandbox-exec`, with unit tests over the profile text; "unavailable" elsewhere, Windows included (P7) (parent 5.4)
- [x] 2.5 The hidden `uze __confine '<json>' -- <argv>` command calling `enter`, classified in `command_performance.rs`

## 3. The program runner in `uze-core`

- [x] 3.1 `capability::extension::program::run`: argv resolved by the caller, `argv[0]` the program's name, cwd the checkout, an allowlisted environment with `TMPDIR` on the scratch, `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_NOSYSTEM=1`, `UZE_IN_<NAME>`, one JSON stdin, stdout 1 MiB and stderr 64 KiB bounded, the time limit with the group killed twice, through the helper when a `Confinement` is given; outcome `Answered(bytes) | Refused(reason) | Failed(reason, tail) | Unenforced` (parent 5.1)
- [x] 3.2 Argv substitution: `{field}` whole, refused after no `--` when the value begins with `-` (parent 22.5)
- [x] 3.3 `exec:` resolution: `harness_search_path()`, `realpath`, refusals under the project, `$UZE_HOME`, tmp and `/mnt/*`; the general runners list; the shebang's interpreter (P12)
- [x] 3.4 The `lines` decoder and the `json` decoder with the envelope

## 4. Pinning and consent

- [x] 4.1 Consent fills `exec_paths` and `enforced_at_accept` from `availability()`; a name not found pinned as missing; `resolve_again(identity)`; an unenforced record re-asked on every digest change (parent 5.5b)
- [x] 4.2 The consent step for programs: asked after the rest, with each pinned path, each device read, the runners' "can run anything", and the fixed "runs as you" text; on the TUI dialog and `uze install`'s terminal question
- [x] 4.3 A run refused when the record says enforced and the platform no longer enforces, or the helper returned 125; the standing shows "needs consent again"

## 5. The runner answers programs

- [x] 5.1 `ExtensionRunner`: `SourceKind::Run` and `Does::Run` answered through the program runner, with the `Confinement` built from the checkout, its Git common dir, the extension's Store directory, the scratch under `UzeHome` (P6), the pinned directories and declared reads
- [x] 5.2 The scratch directory named in `UzeHome`'s runtime tier, removed after each run
- [x] 5.3 Tests over the real kernel (parent 5.6): a token absent, writes into the checkout, `~` and `$UZE_HOME` denied, reads of `~/.ssh` and another `/proc/<pid>` denied, a Unix and a TCP socket denied, a signal to the client refused, a hang killed with its children, a fork bomb gone, an output and a scratch flood cut off, a link escaping refused, a changed `PATH` running the pinned path, a failed confinement refusing the run; skipped and named where Landlock is absent
- [x] 5.4 Architecture rule: the program runner is the only spawn of extension programs and only it starts `__confine`; only `__confine` calls `uze_platform::confine::enter` (parent 5.7)

## 6. The store

- [x] 6.1 `state/extensions/store/` in `UzeHome` (state tier), one file per (identity key, project id) or per identity for `store:global`, 1 MiB, removed with the plugin (parent 5.9)
- [x] 6.2 Envelope patches applied in arrival order in the runner, an oversized patch refused with a notice; `store` in a program's input when declared
- [x] 6.3 `store()` in `uze-script` under `store`/`store:global`

## 7. `run_program`

- [x] 7.1 `run_program(name, args)` in `uze-script` through a reach the runner hands it, under `exec:<name>`, with the script's deadline paused for the call (P10) (parent 5.10)
- [x] 7.2 `uze agent plugin check --run` runs a program only where confinement is enforced, else says so and uses recorded answers (parent 5.10)

## 8. Surfaces

- [x] 8.1 The read model: the machine's availability per right, and per extension its pinned programs, "confined" or "runs as you", missing programs
- [x] 8.2 The Extensions screen: confinement in the drawer, pinned paths, "Resolve again"; the consent dialog's program step (design-system checklist)
- [x] 8.3 `uze doctor` and `uze status` lines for confinement (parent 5.5)

## 9. Proof and docs

- [x] 9.1 The `gpu-meter` fixture plugin: `exec:nvidia-smi` with its device reads, `output "lines"`, a store history drawn as a `Chart`, a `section` and a `view`, tests with recorded answers
- [x] 9.2 Prove `gpu-meter` against the real `nvidia-smi` on this machine, confined, end to end through the runner (an integration test skipped where `nvidia-smi` is absent)
- [x] 9.3 The extensions rule, invariants, `crate-layering.mmd`/`containers.mmd` with the helper, the `uze:extension` skill teaching `run`, `exec:` and the store
- [ ] 9.4 A journey: a packaged program under `exec:sh` installed, consented to on the program step, enabled and drawn
- [ ] 9.5 `make check`, `openspec validate --all --strict`, and the hand proof script for the operator's review
