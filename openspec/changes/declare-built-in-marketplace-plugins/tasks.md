## 1. The official marketplace and its packages

- [x] 1.1 Name the built-in marketplace `uze` (`BUILT_IN_MARKETPLACE`), its tests, journeys and docs with it
- [x] 1.2 Move a package recorded under `uze-official` under `uze` once, at seeding, with a test from an earlier release's Store
- [x] 1.3 Ship `gsd`, `focus` and `contributions` as one plugin, `workspace`, in `marketplace.json` and `plugins/workspace`
- [x] 1.4 Point the tests and journeys that install the official extensions at `workspace@uze`

## 2. The manifest declares built-in plugins

- [ ] 2.1 Accept an entry under the built-in name with plugins and no source in `manifest::validate`; refuse one with `git:`, `path:`, `ref:` or `subdirectory:`, naming it as built in
- [ ] 2.2 Declare a built-in plugin in `project_environment::add` unless `bootstrap::DEFAULT_PLUGIN_IDS` names it, and test both
- [ ] 2.3 Resolve a manifest's built-in plugins from the running binary's snapshot in `plan` and `install`, with nothing registered

## 3. The lock pins the built-in marketplace by version

- [ ] 3.1 Turn `LockedMarketplace` into a Git shape and a built-in shape (`uze: <version>`), untagged in YAML, existing Git entries unchanged on disk
- [ ] 3.2 Record the built-in shape and each plugin's integrity when a built-in plugin resolves
- [ ] 3.3 On install, verify a built-in plugin's integrity under the same version, and resolve it again from this binary, rewriting the lock and reporting it updated, under another
- [ ] 3.4 Test: a lock round-trips both shapes; another version's lock is brought up to this binary; same version with other bytes fails naming both hashes

## 4. The ways to try uze

- [x] 4.1 `/uze:try`, `make try` and the demo base install `workspace@uze` into the project, registering nothing
- [x] 4.2 The playgrounds install their checkout's uze and register no copy of the marketplace; `make examples` gives way to `make pingpong-mcp`
- [ ] 4.3 Run the journeys that install the official examples, and the upgrade chapter, against this build
