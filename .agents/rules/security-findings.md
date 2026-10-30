---
trigger: glob
globs: "**/openspec/changes/audit-*/**, **/openspec/schemas/audit/**, **/.agents/skills/security-audit/**, **/SECURITY.md"
---

A security finding is committed only with its fix (`AGENTS.md`, Security
findings). Every attack falls into one of three classes, and the class
decides where it is written down:

| Class | What it is | Where it goes |
|---|---|---|
| **Fixed** | the change closes it, and an exploit re-run proves that | `findings.md` and `evidence.md` of the `audit-*` change that fixes it |
| **Accepted limit** | the threat model leaves it out by design, so no fix is owed | `evidence.md` **Residual**, without a recipe |
| **Unfixed** | something else an attacker in scope can still do, including a bypass of a fix and a way around a fix found while making it | a private draft advisory; the repository names only its id |

Examples of an accepted limit:
- a same-uid process that runs outside every sandbox;
- a stolen release key;
- an agent the operator started unsealed on purpose.

A residual that only works on some platforms, or that is "unlikely", is
still unfixed.

**Opening a private advisory.** Draft advisories are visible only to the
repository's maintainers.

```bash
gh api -X POST repos/uze-sh/uze/security-advisories --input advisory.json \
  --jq '.ghsa_id, .state'          # state must read "draft"
```

`advisory.json` holds the following fields:
- `summary`;
- `description`: the finding in the `findings.md` shape;
- `severity`;
- `cwe_ids`;
- `vulnerabilities`:
  `[{"package":{"ecosystem":"other","name":"uze"},"vulnerable_version_range":"<= <current version>"}]`.

Group by root cause, as an `audit-*` change would, because the advisory
becomes that change's `findings.md` once it is fixed. List the open drafts
with `gh api repos/uze-sh/uze/security-advisories?state=draft`.

**Referencing it.** The repository names an open finding only by its id,
for example `tracked privately in GHSA-xxxx-xxxx-xxxx`. A public text never
holds:
- the attack;
- the file and line;
- a PoC;
- a test name that describes the attack.

The lab under the scratchpad is never committed.

**Closing it.** The fix and the `audit-*` change land together, with the
advisory text as `findings.md`. Publish the advisory only after a release
carries the fix (`SECURITY.md`, Disclosure), then set its
`patched_versions`.
