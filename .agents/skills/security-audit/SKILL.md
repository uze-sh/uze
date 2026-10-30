---
name: security-audit
description: Offensive security audit of UZE - a red team of agents, each in a bubblewrap sandbox with the operator's home hidden, attacking one surface (acquisition, delivery, extension scripts, workspace and agents, terminal runtime, parsers, distribution) with working exploits, then every finding recorded and fixed as an `audit-<subject>` OpenSpec change that ends in proof. Use when someone asks for a security audit, a pentest, a red team, threat modelling, "procura vulnerabilidade", "tenta quebrar o uze", before a release that widens what UZE runs or trusts, after a change to trust/consent/confinement/approval, or to re-verify an earlier audit's fixes. Not for a general code review (that is `code:polish`) or one dependency's provenance (the `dependencies` rule).
compatibility: Linux with bubblewrap (bwrap) for the lab; the OpenSpec CLI for the audit schema.
---

# Security audit

UZE runs on the operator's machine, beside the agents it launches, with the
operator's permissions. Every boundary it draws - plugin trust, extension
consent, project-command approval, the pane/operator split, the Rhai engine's
bounds - is a claim an attacker will test. This skill tests them the way an
attacker would: with working exploits against the real binary, in a sandbox
that cannot reach the operator's real home, and it carries what breaks to a
fix that is proven by re-running the same exploit.

It has two halves. **Attack** ends in a findings report. **Fix** turns each
group of findings into an `audit-<subject>` OpenSpec change on the `audit`
schema (`findings → proposal → specs → design → tasks → evidence`) and works
it until `evidence.md` closes every finding.

## Step 1 - scope and baseline

- Record the commit under audit (`git rev-parse HEAD`) and the date.
- Read `references/surface.md`: the attack surfaces, the threat
  actors, what earlier audits found and the defenses that held. A defense
  that held is re-tested only if its code changed since
  (`git log <last-audit-commit>.. -- <paths>`).
- Read `docs/architecture/invariants.md` and the ADRs of each surface: the
  claims to break.
- Archived `openspec/changes/archive/*-audit-*` are earlier audits; their
  `evidence.md` names the regression tests that must still pass.

## Step 2 - the lab

Never run uze or an exploit against the operator's home: it holds the real
`~/.uze`, harness configs and keys.

```bash
AUDIT=<scratchpad>/audit; mkdir -p "$AUDIT/bin"
cargo build --locked --bin uze && cp target/debug/uze "$AUDIT/bin/uze"
cp .agents/skills/security-audit/scripts/sandbox "$AUDIT/bin/sandbox"
cp .agents/skills/security-audit/references/brief.md "$AUDIT/BRIEF.md"   # fill in AUDIT
SRC=$PWD "$AUDIT/bin/sandbox" smoke -- bash -c 'ls -A /home/* /mnt /run; uze --version'
# your home shows only .cargo/.rustup; /mnt and /run are empty
```

The smoke test must show the real home, the other mounts and the
privileged sockets hidden. One box per auditor
(`$AUDIT/boxes/<box>`), so PoCs never interfere.

## Step 3 - the red team

One agent per surface in `references/surface.md`, in parallel (keep it under ten),
each briefed with `$AUDIT/BRIEF.md` plus the surface's own paragraph: its
code, its ADRs, and the hypotheses to try. Auditors only read the repo and
write in their box. They must not run `uze agent work name` or any git
write: a subagent inherits the parent's agent stamp and would rename the
parent's branch. Say so in the brief.

A subagent cannot write files outside its box and its report can arrive
truncated. Ask for the report inline in the final message, in the brief's
fixed shape. If a report is cut off, ask the agent to resend the rest.

## Step 4 - verify before you believe

An auditor's report is a claim. Before it reaches the operator:

- Re-run every High/Critical PoC yourself in a fresh box
  (`$AUDIT/boxes/verify-<id>`), copying only the PoC's inputs.
- For a PLAUSIBLE finding, read the exact code path it names.
- Drop what you cannot reproduce or locate. Say which you verified
  yourself.

Report to the operator grouped by root cause, not by auditor. A table per
theme, with the structural fix next to the point fixes.

## Step 5 - record: in private first, in the repository only with the fix

This repository is public, and a committed finding is a disclosure that
survives deletion. The `security-findings` rule (`.agents/rules/`) holds the
policy. In short:

- **Every verified finding opens as a private draft GitHub Security
  Advisory.** Make one per root cause, with the finding in the
  `findings.md` shape. The rule gives the `gh api` call. Check that its
  state reads `draft`.
- **A finding enters the repository only on the branch that fixes it.**
  It goes in as `openspec/changes/audit-<subject>/findings.md`, in the same
  delivery as the fix.

Group the findings by the property that does not hold, not by crate. For
example, "consent covers what is declared, not what runs". Each group is
one advisory and, when fixed, one change:

```bash
openspec new change audit-<subject> --schema audit
```

Write `findings.md` from the advisory text. It ends with `## Defenses that
held`. Then write proposal, specs, design and tasks as for any change. Make
the regression test and the exploit re-run explicit tasks.

Specs state the property, never the attack. That makes them safe to commit
before anything else.

## Step 6 - fix, in parallel checkouts

One agent per change, each in its own checkout from
`uze agent work split <topic>`. Brief each with:

- the change path;
- the `openspec-apply-change` contract;
- the rule that every finding gets a test failing on the audited commit;
- the lab path, so it can rebuild the binary and re-run the original PoC.

Join each with `uze agent work join <topic>` when it is green.

## Step 7 - evidence, then archive

`evidence.md` closes each finding with four things:

- the behavior change and where it lives;
- the regression test;
- the exploit re-run against the fixed binary, with its output;
- the residual.

A residual is either an **accepted limit**, which the threat model excludes
by design and is stated without a recipe, or **unfixed**. An unfixed
residual, such as a bypass found while fixing, goes to a new private
advisory, and `evidence.md` names only its id. The finding it belongs to is
then partially closed, and says so.

Then run the gate:

- `cargo test --workspace`
- `cargo clippy --all-targets -- -D warnings`
- `openspec validate --all --strict`

An `audit-*` change is archived only when evidence closes every finding
(`openspec/config.yaml`, archive guidance). Publish its advisories after a
release carries the fix.

Finally, update `references/surface.md`. Add the new changes and advisory ids (ids only) to its index and new
defenses to "held". If an auditor tested a surface that is missing from it,
add that surface.
