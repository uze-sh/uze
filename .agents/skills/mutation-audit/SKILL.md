---
name: mutation-audit
description: Measuring whether UZE's tests would notice the code being wrong, with cargo-mutants - starting and monitoring `mutants.yml`, downloading the evidence, triaging each surviving mutant, writing the test a real gap needs and proving it catches the mutant. Use when someone asks about mutation testing, mutants, survivors, `missed.txt` or a "Missed mutant" annotation, whether tests really prove anything or only pass ("os testes testam mesmo?", "teste que só passa"), to audit a crate's tests, to check tests an agent wrote, or before refactoring code whose tests nobody has measured. Not for reviewing a whole change (that is `code:polish`), not for a coverage figure alone, and not for a security audit (that is `security-audit`).
compatibility: The GitHub CLI for runs; for the local lab, Linux with bubblewrap (bwrap), cargo and this repository's `security-audit` skill, whose sandbox it uses.
---

# Mutation audit

Coverage says a line ran. A mutant asks whether a test would notice that line
being wrong. The output of an audit is a triaged list and the tests it led to,
never a score.

## The rules everything else serves

**A test states behavior, and the mutant only points at it.** Read what the
code should do from its doc comment, `docs/architecture/invariants.md`, the
OpenSpec specs or the rule that owns it. A test copied from what the code does
today locks in whatever bug it has. If nothing says what is right, stop and ask;
that absence is a finding.

**A survivor is a question.** It may be a real gap, a mutant no test could
catch, or something only another crate's tests would catch. Read the code before
concluding anything.

**No target score.** Some mutants are equivalent, so 100% is not the goal.
Compare a crate with its own earlier runs, and never write a test only to move
the number.

**A claim is proven by re-running the mutant.** "This test covers it" counts
once that mutant reads `caught`.

## Before starting: results are public

The repository is public, so a run's logs, annotations, summary and artifacts
are too. A survivor in a check names a check no test protects.

- Before dispatching a crate that holds trust, consent, confinement, approval or
  acquisition, confirm the operator accepts that, or use the lab below, which
  publishes nothing.
- If triage finds an exploitable flaw and not only a missing test, stop and
  follow the `security-findings` rule. It enters the repository with its fix,
  and the commit, test name and PR text do not explain the attack before then.

## The loop

### 1. Measure

```sh
gh workflow run mutants.yml -f package=<crate>                      # that crate's tests
gh workflow run mutants.yml -f package=<crate> -f test_scope=workspace
gh workflow run mutants.yml -f package=<crate> -f mode=diff -f base=<tag> -f ref=<branch>
```

Scope `crate` is a cheap list of candidates and overstates survivors, because a
foundation crate's behavior is often tested by whoever uses it. Scope `workspace`
is the verdict on whether the project as a whole notices. Use `workspace` for a
small crate and `crate` first for a large one, then confirm what matters in the
lab. In scope `crate`, the survivors of `uze-contract/src/docs.rs` mean nothing: it
compiles only with feature `schema`, which that scope leaves off. `mode=diff` suits a branch or a release range. `ref` defaults to `main`, so
name the branch you are auditing, or the run measures something else and succeeds.

Start one crate at a time: GitHub keeps one run waiting, and a third start
replaces it. A run shares the account's 20 concurrent jobs with `ci.yml`, so
cancel it (`gh run cancel <id>`) when a delivery needs them.

Measured so far, for planning a run rather than as a promise:

| Run | Mutants | Median per mutant | Runner-hours | Wall clock |
|---|---|---|---|---|
| `uze-document`, scope `crate` | 69 | 1 s | 0.3 | 20 min |
| `uze-document`, scope `workspace` | 69 | 97 s | 2.6 | 48 min |

A mutant in a foundation crate rebuilds what depends on it, so scope `workspace`
costs most there.

### 2. Collect

```sh
gh run watch <id> --exit-status
gh run download <id> --pattern 'mutants-*' --dir <scratch>
python3 .github/scripts/mutants.py report <scratch> \
  --package <crate> --planned <n> --commit <sha> --missed <scratch>/missed.txt
```

The run's summary has the same report. Annotations are only a sample of the
survivors, so read `missed.txt`. **Incomplete** means shards ran out of time:
raise `-f shards=<n>` before reading the score. A job's success says the run
worked, and survivors do not fail it.

### 3. Triage

Open each survivor at its line and put it in one row. Have someone who did not
write the code judge it (the `code:reviewer` agent), since whoever wrote it reads
what they meant.

| Verdict | How to recognize it | What to do |
|---|---|---|
| **Real gap** | The change would alter something a user or a later check relies on | Step 4 |
| **Caught elsewhere** | Scope was `crate`, and the behavior is exercised by a consumer | Re-run in scope `workspace` |
| **Equivalent** | The mutant cannot change behavior: an unreachable branch, a value equal to what it replaces | Record it in `.cargo/mutants.toml` with the reason |
| **Not worth testing** | A log line, a debug message, cosmetic output | Exclude it with the reason, if it recurs |

Patterns from past audits:

- `-> bool with true` **and** `with false` both surviving: nothing checks the
  answer. It is the strongest signal on the list.
- Only `with true` surviving: every test sees the true case, and nobody asked
  whether the function ever says no.
- `replace > with >=`: no test sits on the boundary. First check whether equality
  can reach that line at all, since an earlier return makes the mutant equivalent.
- `replace f -> Vec<_> with vec![]` on something a check iterates: the check still
  passes, having checked nothing.
- A match guard replaced `with true`: a narrow error handler became a catch-all.
  Failure paths are where this hides, as a test rarely forces a read to fail.

### 4. Write the test

Name the behavior, and say in a comment why it matters only when that is not
obvious. The test belongs with the code it proves, in the crate that owns the
rule, even if a consumer already covers part of it. Don't assert an
implementation detail or duplicate the expression under test:

```rust
assert!(may_vote(18));                // states the rule
assert_eq!(may_vote(18), 18 >= 18);   // copies the code and proves nothing
```

A test for a bug the mutant exposed lands with the fix, in a `fix:` commit. A test
for correct code is a `test:` commit.

### 5. Prove it

Run the lab over the changed files, which leaves a list that should be shorter by
exactly what you meant to catch. `scripts/lab` is relative to this skill, and the
script works from any directory:

```sh
AUDIT=<dir> scripts/lab --package <crate> --file '<path>'
```

`<dir>` sits on a disk, outside the repository: the builds outgrow a memory-backed
`/tmp`, which everything else on the machine shares, so the lab refuses a tmpfs.

Add `--test-workspace=true` to judge against the whole workspace, which first proves
the suite green in the box, as `cargo-mutants`' own baseline would try only the crate, and `--re
'<pattern>'` to re-run one mutant. Read
`$AUDIT/boxes/mutants/work/uze/mutants.out/missed.txt`; the file is what proves it.

The box can mislead or get in the way in four ways:

- **A false `missed`.** Bun is absent, and the OpenCode tests that need it skip
  themselves. Before calling a survivor in that code a gap, confirm it on a CI run,
  which installs Bun.
- **A false `caught`.** A test that fails now and then there reads as catching the
  mutant. When a result surprises you, read that mutant's log under
  `mutants.out/log/` to see which test failed. The timing budgets (`performance`,
  `extension_storm`, `budget`) fail on a busy machine, so a workspace run leaves
  them out; a mutant only they would catch reads as `missed`.
- **The machine runs out.** The box limits neither memory nor CPU. A mutant of
  `uze-script` can take a limit off the engine, and one test reached 33 GB.
  Watch the available memory while that crate runs.
- **No result.** A test that always fails there stops the run at the baseline.
  Find out why it fails: anything in a sandbox is refused an approval, for example,
  so those tests are filtered out for a workspace run in `scripts/lab`. A bare
  `--baseline=skip` would hide it by making every mutant read `caught`.

A `caught` is only as good as the test that failed. If one test catches unrelated
mutants, or a report is too good to be true, read the logs first.

### 6. Deliver

Open an ordinary pull request. Its body says which mutants the tests now catch and
how many remain, with the triage for what remains, and names no attack. Comparing
before and after is the evidence. Say what was not run.

## Recording an equivalent mutant

`.cargo/mutants.toml` is the list of what was read and set aside. Match on the file
and the change, without the line number, which moves:

```toml
# `read` has already returned when the shape equals SHAPE.
'uze-document/src/lib\.rs.*replace > with >= in read$',
```

Check it with `AUDIT=<dir> scripts/lab --list --package <crate> | wc -l`.
An exclusion
without its reason gets deleted, and anyone changing the code it names re-reads
it. It is for equivalents and noise, never for a real gap someone would rather not
fix.

## Never

- Run `cargo mutants` outside the lab on this machine. A mutant can take a test out
  of its isolation, and `uze-platform` and `uze-terminal` hold code that ends
  processes (the `ending-processes` rule).
- Add a mutation run to the `Gate`. It is slow and it reports questions.
- Set a mutant timeout close to the suite's own length. A mutant that outlasts the
  limit counts as detected, so an honest slow run would inflate the score.
- Delete or weaken an assertion to make a report tidier.
