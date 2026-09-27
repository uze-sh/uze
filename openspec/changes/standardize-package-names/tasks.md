## 1. The rule

- [x] 1.1 One predicate for plugin, marketplace and alias names: lowercase
  kebab-case, at most 64 characters, replacing the `[A-Za-z0-9_-]` rule at
  the package id constructor
- [x] 1.2 The corrected-name suggestion, carried by every refusal
- [x] 1.3 Unit tests: uppercase, `_`, `--`, leading and trailing `-`,
  65 characters, and the suggestion for each

## 2. Marketplace registration

- [x] 2.1 The manifest reader refuses a marketplace name outside the rule,
  before a Git marketplace is mirrored
- [x] 2.2 The registry refuses to record one, whatever supplied the name
- [x] 2.3 Tests for both

## 3. Typed input

- [x] 3.1 Lowercase the `name@marketplace` spec, the `remove`, `update` and
  `inspect` argument, the `market` verbs' name, `--alias`, `--market` and
  the collision prompt's alias
- [x] 3.2 CLI test: install and remove in another case

## 4. Authoring

- [x] 4.1 `market create` and `plugin create` refuse with the suggestion;
  the plugins directory keeps the path rule
- [x] 4.2 `plugin check` reports a `SKILL.md` `name` missing, outside the
  rule, or different from its directory; `market check` reports the
  marketplace and entry names
- [x] 4.3 The `uze:author` Skill states the rule; its own `SKILL.md`
  carries its `name`

