## D1 — Refuse a builtin flag the portable shell does not implement at parse, instead of letting it fail at run time

- status:   pending
- task:     1.4
- commit:
- trigger:  The design accepted a loud run-time failure on Linux and macOS
            for `head -1`, `cp -a` and the like (BREAKING, measured by E2).
            Reading each builtin's argument parsing showed the set of flags
            it implements is small and fixed per pinned version.
- decided:  `FLAGS` in `portable_shell.rs` lists each builtin's short letters
            and long names, read from 0.33.3's sources, with `set`'s
            `-e`/`-o errexit|pipefail` handled on their own. A literal flag
            outside the list makes the line not portable
            (`Divergence::UnsupportedFlag`), so it keeps running in `sh` on
            Linux and macOS exactly as before. An expanded word (`$FLAG`)
            is still refused by the builtin at run time.
- rejected: Failing at run time. It breaks a working line on the
            author's own machine for no gain, since the same line could not
            run on Windows either.
- blast:    The table must follow the pinned version. A bump that adds a
            flag leaves a portable line refused until the table learns it.
            One that drops a flag is caught by the equivalence suite's
            builtin rows.

## D2 — Keep brace alternatives (`{a,b}`) portable

- status:   pending
- task:     1.5
- commit:
- trigger:  The portable shell expands `{a,b}` as bash does. `sh` is dash
            on Debian and Ubuntu, which leaves it as text, and bash on
            macOS, which expands it.
- decided:  Not a divergence. The portable shell settles a line whose
            meaning already depended on which `sh` ran it the way bash
            does. E2 counts such lines: 2 of 1,224 in the corpus, both
            written to be expanded (`rimraf {packages,dev-packages}/*/*.tgz`).
- rejected: Refusing it. That would keep both lines broken on dash-based
            Linux, where they already do not do what their author meant.
- blast:    A line that relied on dash leaving `{a,b}` literal would now
            expand. The ratchet on E2 shows any such line entering the
            corpus.

## D3 — Refuse `[`, function definitions, `{ …; }` groups and `&`

- status:   pending
- task:     1.4
- commit:
- trigger:  The probe showed the portable shell reads all four but runs
            them differently from `sh`:
            - `[` is not a builtin: `/usr/bin/[` on Linux, 127 on Windows;
            - `name() { …; }` and `{ …; }` run `{` as a program;
            - a background `&` is waited for.
- decided:  Each one is a `Divergence`, with a row in the refusal table. The
            line keeps its POSIX-only meaning. `[` points at `test`.
- rejected: Running them and documenting the difference. A line that runs
            differently without saying so is exactly what D6b exists to
            prevent.
- blast:    None beyond the lines involved: they run in `sh` as they do
            today.

## D4 — An interrupt of the workspace leaves a portable line exactly as it leaves a `sh` line

- status:   pending
- task:     3.5
- commit:
- trigger:  Task 3.5 asked that an interrupt of the workspace end the
            `uze shell -c` child. Both runners seat their child in its own
            process group (`Seat::OwnGroup`), so a Ctrl+C at the workspace
            reaches neither. A `sh` setup step today runs to its deadline.
- decided:  Parity: the portable line keeps the bounds the `sh` line has
            (timeout and process tree, proven by E5) and no more. Ending a
            project command on interrupt would be a change to both runners,
            not to this one.
- rejected: Adding an interrupt path for the portable child only. Two
            runners with different lifetimes for the same `setup` line is
            the divergence this change exists to remove.
- blast:    None: it records today's behaviour.

## D5 — Name the `uze` a portable line starts from in the composition, and prove placement and delivery through it

- status:   pending
- task:     4.4
- commit:
- trigger:  Task 4.4 asked for acceptance tests in which a portable setup
            and gate place a checkout and deliver. No CLI verb places an
            agent or delivers its work: the TUI does. An in-process
            application test started the test binary as `uze shell -c`,
            because the runner resolved the process's own executable
            itself.
- decided:  `run_bounded` takes a `subprocess::Uze`: `ThisProcess`, resolved
            when the line runs (a workspace open across an upgrade runs
            the new one), or `At(path)`. `UzeApplication` holds one,
            `ThisProcess` in every composition, and `with_uze` names
            another for a client whose own process is not `uze`.
            Placement and delivery pass it down. So:
            - `tests/acceptance/portable_commands.rs` places a checkout
              with a portable setup and delivers it through a portable
              gate, from the built binary, and reads `uze status` for what
              does not run here;
            - `tests/cli/portable_shell.rs` proves the line itself (E3–E5).

            The flow through the TUI is still task 8.1's journey.
            Application unit tests have no `uze` binary to name, so the
            ones that execute a line keep a pair, with a comment saying
            why.
- rejected: Resolving the executable inside the runner, as first written:
            nothing outside the CLI could run a portable line, and a test
            through the application ran the test binary. Resolving it once
            at composition: a long-lived workspace would keep a binary an
            upgrade replaced.
- blast:    Every caller of placement and delivery reaches the runner
            through the application, which owns the choice; `uze-workspace`
            takes it as an argument and chooses nothing.

## D6 — Refuse a pattern that would match a name in either case

- status:   pending
- task:     1.4
- commit:
- trigger:  The portable shell matches a pattern with `case_sensitive: false`,
            fixed in 0.33.3's `execute.rs` with no option to turn it off.
            `*.txt` takes `README.TXT`, and `rm -rf /tmp/repo-*` takes
            `/tmp/Repo-x`; `sh` matches the case written.
- decided:  `Divergence::CaselessGlob`. A word is refused when a part of
            its path that is a pattern also holds an ASCII letter (glob
            folds no other), or an expansion that may hold one. Braces
            count with what they expand to, and a quoted wildcard makes
            its part a pattern, as the shell matches it. A part that is no
            pattern is looked up as written, so `build/*` and `$OUT/*`
            stay portable. E1: 9 of 1,224 lines, now run in `sh` as
            before.

            The same reading found that the shell also expands a pattern
            in an assignment or a redirect, which `sh` reads as written,
            letter or not: `Divergence::ExpandedPattern`, refused with
            "quote it". No corpus line has one.
- rejected: Accepting it as a known difference: it is the silent change of
            meaning D3 refuses, and it reaches `rm`. Expanding the patterns
            in UZE before the shell does: a second glob beside the shell's.
            A fork of the crate: a dependency of a different provenance for
            one flag.
- blast:    A wildcard inside a variable's value (`ls $PATTERN`) is
            matched without case and not refused: the value is known only
            when the line runs.
            On Windows these lines leave the portable shell and run only
            where `sh` exists. An upstream option for case-sensitive
            matching (bash's `nocaseglob`, off) would make this entry leave
            the list, its row in `tests/cli/portable_shell.rs` failing
            first.
