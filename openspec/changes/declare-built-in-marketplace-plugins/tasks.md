## 1. The official marketplace and its packages

- [x] 1.1 Name the built-in marketplace `uze` (`BUILT_IN_MARKETPLACE`), its tests, journeys and docs with it
- [x] 1.2 Move a package recorded under `uze-official` under `uze` once, at seeding, with a test from an earlier release's Store
- [x] 1.3 Ship `gsd`, `focus` and `contributions` as one plugin, `workspace`, in `marketplace.json` and `plugins/workspace`
- [x] 1.4 Point the tests and journeys that install the official extensions at `workspace@uze`

## 2. The manifest declares built-in plugins

- [x] 2.1 Accept an entry under the built-in name with plugins and no source in `manifest::validate`; refuse a source (`git:`, `path:`, `ref:`, `subdirectory:`) and a bundled plugin on it, each by name
- [x] 2.2 Update the scaffolded `agents.yaml` comment that says the built-in marketplace is not declared
- [x] 2.3 Declare a built-in plugin in `project_environment::add` unless `bootstrap::DEFAULT_PLUGIN_IDS` names it, writing a source-less entry, and test both

## 3. The lock and install follow the binary

- [x] 3.1 Add `uze: <version>` to `ProjectLock`, written only while a built-in plugin is locked
- [x] 3.2 Skip `uze` entries in `stale_against`, `resolve_declared`, `reproduce_locked`, `rise_to_lock`, the project `update`, the prefetch and the rules acquisition
- [x] 3.3 An install phase for built-in plugins: install the missing, replace through `replace_with` those whose bytes differ from the binary's, record integrity and version, report the moved as installed
- [x] 3.4 `plan` and `status` say a built-in plugin is behind when the machine's or the lock's bytes differ from the binary's
- [x] 3.5 Test: a manifest-only clone installs a built-in plugin and writes the lock; a lock with other bytes is brought to this binary and reported; an agreeing one moves nothing; the lock round-trips `uze:`

## 4. The ways to try uze

- [x] 4.1 `/uze:try`, `make try` and the demo base install `workspace@uze` into the project, registering nothing
- [x] 4.2 The playgrounds install their checkout's uze and register no copy of the marketplace; `make examples` gives way to `make pingpong-mcp`
- [ ] 4.3 Run the journeys that install the official examples, and the upgrade chapter, against this build

## 5. Close

- [ ] 5.1 Archive only after `project-agent-environment`, whose `agents-manifest` and `agents-lock` specs this change modifies
