## Context

Declared extensions declare preferences (`PreferenceDecl`); the operator
edits them in the Extensions drawer; they reach the extension through
`Extension::configure`. Values are a record under
`state/extensions/preferences/<plugin>/<id>.json`, which also says which
secrets are set. Built-ins have no preferences at all. `config.toml`
already holds `[extensions]`, the switch of every extension by id.

## Decisions

- **`config.toml`, `[preferences."<id>"]`.** A preference is a choice
  only the operator makes: the authored tier. The section is apart from
  `[extensions]` because that one maps an id to a boolean, so
  `[extensions.review]` cannot also be a table; the id is the switch's,
  so one extension has one key in the file. The name is the manifest's
  own word for what the author declares. Only scalars are kept (string,
  integer, float, boolean); anything else reads as unset.
- **Preferences outlive their plugin.** Reinstalling finds them; a
  section for an extension no longer installed is the operator's text,
  never deleted (the persistence tiers: unknown authored entries are
  reported, not removed).
- **Secrets keep their own record.** Whether a secret is set moves to
  `state/extensions/secrets/<plugin>/<id>.json`, beside the files a
  machine with no keychain keeps them in, and leaves with the plugin as
  the secrets do. `config.toml` never says anything about a secret.
- **The client hands the declarations of a built-in to the service.**
  `uze-application` does not know the built-in set (it lives in
  `uze-extensions`, which only `src/` names), so the service takes the
  extension's id and its declarations and answers the admitted values and
  the drawer's rows; a declared extension's come from its manifest as
  before. A built-in declaring a `secret` is refused when set.
- **Viewed on leaving, not on reaching the end.** Reaching the end then
  scrolling back to reread must not have marked already; a file opened and
  dropped must not be marked. The diff's last line must have been on
  screen (tracked whenever a key, a click or the wheel reaches the review,
  which is when its size is known) and the diff shown for two seconds, the
  threshold a GitHub userscript and Basil use, which keeps a short diff
  that fits the screen from being marked by a glance. An explicit unmark
  holds for that content.

## Risks

- A hand-edited `config.toml` that does not parse: the drawer and the
  workspace read the defaults, and every write is refused, as for any
  other section.
