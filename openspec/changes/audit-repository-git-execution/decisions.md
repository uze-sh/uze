## D1 — Hooks never run in UZE's own Git operations; they are not folded into the approval

- status:   pending
- task:     1.2
- commit:   1c95c06c
- trigger:  the brief allowed either folding operator-intended hooks
            (`pre-push`, `commit-msg`) into the ADR-055 approval digest, or
            not running them.
- decided:  `core.hooksPath` is the null device on every invocation, read
            or write. What should run before a delivery is the approved
            `workspace.gate`.
- rejected: folding hooks into the digest: a hook is a file any agent
            writes, so approving it means snapshotting its bytes into a
            directory UZE owns and pointing Git there, and every Git LFS or
            hook-manager rewrite would ask again. Large machinery for a
            behaviour the gate already offers.
- blast:    Git LFS's `pre-push` upload no longer runs in UZE's delivery
            push (the smudge filter, global by `git lfs install`, still
            does). Reverting restores the WKS-2 attack.

## D2 — Only the operator's scopes name a program; the repository's are overridden key by key

- status:   pending
- task:     1.2
- commit:   1c95c06c
- trigger:  filters and merge drivers are named by the repository under
            names it picks, so they cannot be switched off as a class, and
            `core.sshCommand`/credential helpers are legitimately set by
            operators.
- decided:  for a command that can reach a program, ask Git
            (`config -z --show-scope --get-regexp`) which program keys are
            set and where; override each `local`/`worktree` key with the
            operator's value for the same key, or an inert one. A
            repository-local `core.sshCommand` no longer applies; the
            operator's place for one is global config under
            `includeIf "gitdir:"`.
- rejected: parsing config files in UZE (a parser differential is a
            bypass); a cache keyed on file times (conditional includes make
            the key incomplete); refusing to run in a repository that sets
            any such key (blocks every repository with `git lfs install
            --local` or a local deploy key).
- blast:    reverting reopens filter, merge-driver, credential and
            transport execution in UZE's own process.

## D3 — Inert values chosen to fail closed

- status:   pending
- task:     1.2
- commit:   1c95c06c
- trigger:  each overridden key needs a value under which Git runs nothing
            of the repository's choosing.
- decided:  an empty filter with `required=false`; a merge driver of
            `false` (the merge reports a conflict rather than silently
            keeping one side, which an empty command would do); `ssh` for
            `core.sshCommand`; empty askpass and alternate refs command;
            the default signing programs; `saveCookies=false`. First-wins
            keys get the command line: `--upload-pack`/`--receive-pack`;
            `core.gitProxy` cannot be outranked, so UZE's own transport
            commands do not speak `git://` (`protocol.git.allow=never`).
- rejected: dropping local transports outright (breaks operators and
            every test that uses a local remote).
- blast:    a rebase across a file whose repository-local merge driver the
            operator relied on now stops on a conflict; an operator fetching
            over `git://` inside UZE's sync no longer can.

## D4 — The configuration probe runs only where a program can be reached, and is counted

- status:   pending
- task:     1.2
- commit:   1c95c06c
- trigger:  the probe is one more Git process; the TUI reads constantly and
            the process budgets count every start.
- decided:  probe every write except the known-inert ones (`update-ref`,
            `symbolic-ref`, `config`, `branch`, `init`, `worktree
            prune|lock|unlock|list|repair`), every network subcommand, and
            the reads that hash work tree content (`status`, `diff*`,
            `ls-files`, `hash-object`, `blame`, or `--filters`/`--textconv`).
            The probe is counted like any other process; the per-thread
            counting test's expected number moved from 3 to 5.
- rejected: hiding the probe from the counter (budgets would lie).
- blast:    none beyond cost.

## D5 — No end-of-options marker before a session id; the type is the guard

- status:   pending
- task:     2.2
- commit:   a7bacb2c
- trigger:  the brief asked for `--` before positional ids where each
            harness's CLI supports it.
- decided:  none added. Codex's `resume <id>` is followed by the operator's
            overrides, which a `--` would make positionals (`resume-help.txt`:
            `codex resume [OPTIONS] [SESSION_ID] [PROMPT]`); Antigravity's Go
            flags consume the next word as the value; Claude Code's
            `--resume` takes an optional value, where the attached
            `--resume=<id>` form would be the guard, but the journeys and the
            Lab read the two-word form. `SessionId::parse` refuses a leading
            `-` and each integration requires its harness's shape.
- rejected: `--resume=<id>` for Claude Code (changes an argv the journeys
            assert, unverifiable here against the real binary).
- blast:    none.

## D6 — `SessionId::new` is removed rather than kept beside `parse`

- status:   pending
- task:     2.2
- commit:   a7bacb2c
- trigger:  ~50 test sites built ids with `new`.
- decided:  `new` is gone; tests call `SessionId::parse(..).unwrap()`.
            Serde goes through `parse` too, so a stored record holding an
            option-shaped id reads as no record.
- rejected: keeping an unchecked `new` for tests (public, so production
            could use it again).
- blast:    none.

## D7 — Fixed-name keys are overridden on every command, not only when the probe saw them; the close budget moves from 36 to 42

- status:   pending
- task:     1.2
- commit:   f4d5f503
- trigger:  a fixed-name key needs no probe to be overridden, and the
            slot-close process budget failed (41 > 36).
- decided:  every key whose name is fixed (`core.sshCommand`,
            `core.askPass`, `core.alternateRefsCommand`, the credential
            helpers, `protocol.git.allow`, `--upload-pack`/`--receive-pack`,
            the signing programs) is given on every command that can reach
            it, from the operator's scopes, so no answer of the probe is
            trusted for them. Only filter and merge-driver names, which the
            repository picks, depend on the probe. Plumbing that cannot run
            a filter (`diff-index --cached`, `ls-files` without `-m`,
            `hash-object --stdin`, `commit-tree --no-gpg-sign`,
            `rev-parse`, …) skips the probe. What remains is one process per
            command that can hash work tree content: six in a close, so the
            budget is 42, with the reason written beside it.
- rejected: caching the probe across a critical section (it would answer
            the name-chosen keys from an older reading); hiding the probe
            from the counter.
- blast:    the budget; reverting the always-on part lets the probe's
            answer decide the transport keys again.
