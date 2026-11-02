## Why

Marking every file of an agent's branch viewed by hand is tedious when one
piece of work touches dozens of files. Gerrit and Crucible mark a file
reviewed as it is read, with a preference to turn that off; the review
should too. That needs a preference on a built-in extension, and built-ins
have none: only declared extensions declare preferences, kept as a record
under `state/` that leaves with the plugin. A preference is the operator's
own choice, which the persistence tiers place in `config.toml`, beside the
theme and the notifications, where it can be read, edited and carried in a
dotfiles repository.

## What Changes

- **One preference mechanism for every extension**: a built-in declares
  preferences the way a manifest does, they are edited in the same drawer
  of the Extensions screen and reach the extension through the same
  `configure`.
- **BREAKING**: preferences are kept in `config.toml`, under
  `[preferences."<extension id>"]`, keyed by the id the `[extensions]`
  switch already uses (`review`, `plugin@marketplace/id`). They stay when
  the plugin leaves, as an editor keeps an uninstalled extension's
  settings. What was kept under `state/extensions/preferences/` is no
  longer read (beta: no carry-across).
- Secrets stay out of `config.toml`: the value in the keychain or its
  `0600` file, and whether one is set in a record beside those files.
- **The review marks a file viewed as it is left**, once its diff was read
  to the end and stayed on screen a moment, unless the operator turned
  that off (`mark-viewed-automatically`, on by default) or unmarked that
  file by hand.

## Capabilities

### New Capabilities

### Modified Capabilities

- `extension-preferences`: kept in `config.toml`, declared by built-ins too.
- `review-surface`: a file read to the end is marked viewed as it is left.

## Impact

`uze-core::config` (scalar values in a table), `uze-workspace::extensions`
(`preferences` over `config.toml`, the secrets' ledger), `UzeHome` (the
preference paths go, the ledger's come), `uze-application`'s extensions
service, `uze-extensions` (`BuiltinExtension::preferences`, the review),
the TUI's Extensions drawer and the workspace's discovery.
