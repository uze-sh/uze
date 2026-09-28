## 0. Decisions

- [ ] 0.1 The `harness:` block's name and shape, approved.
- [ ] 0.2 Ownership of a UZE-written file inside a repository (committed or
  ignored, and who removes it).

## 1. Dialect table

- [ ] 1.1 Per (harness, artifact kind, fact) table, version-stamped, each
  fact naming its Lab check; delivery derives roots, link-following,
  placeholders and field acceptance from it.
- [ ] 1.2 Lab: one contract check per fact; the nightly fails on a fact that
  stops holding.

## 2. Canonical frontmatter

- [ ] 2.1 `harness:` block for skills and agents, keyed by harness id,
  rendered through the table, never delivered.
- [ ] 2.2 `plugin check` validates it and names the harness that would drop
  a field.

## 3. Standards

- [ ] 3.1 Canonical package conformant with Agent Plugins 1.0, UZE-only
  surfaces under `extensions["sh.uze"]`.

## 4. Project `.agents/`

- [ ] 4.1 Deliver project-scoped plugins into `./.agents/` for the harnesses
  that read it.
