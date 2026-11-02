<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — `[preferences."<id>"]`, beside the switch rather than under it

- status:   pending
- task:     1.2
- commit:   (this change)
- trigger:  The design said `config.toml`; `[extensions]` already maps each id to a boolean, so `[extensions.review]` cannot also be a table.
- decided:  A section of its own, named by the manifest's word, keyed by the switch's id (`review`, `plugin@marketplace/id`). Only scalars are read; a date, an array or a table reads as unset.
- rejected: Moving the switch to `[extensions.<id>] enabled = …`: churns a section the CLI and the screen already write, for no reader that needs it.
- blast:    `uze-core::config`, `uze-workspace::extensions::preferences`.

## D2 — Whether a secret is set is its own record beside the secrets

- status:   pending
- task:     1.2
- commit:   (this change)
- trigger:  The marks lived among the preferences as `true`; in `config.toml` a hand edit could make one lie, and the file would say something about a secret.
- decided:  `state/extensions/secrets/<plugin>/<id>.json` holds the set names, under a lock beside it, and goes with the plugin's secrets.
- rejected: Asking the keychain each time the screen draws: a prompt on macOS.
- blast:    `UzeHome`, `uze-workspace::extensions::secrets`, the health catalogue.

## D3 — Going to another agent leaves the file too

- status:   pending
- task:     3.1
- commit:   (this change)
- trigger:  The host holds the review and then closes it when another agent comes to the front; `close` cannot tell that from the operator closing it.
- decided:  Both leave the file: a diff read to its end and on screen two seconds was read, whichever way the operator moved on.
- rejected: A new contract method to tell a suspend from a close: a gesture the review alone would read, for a case the rule already answers.
- blast:    `review::extension::close`.

## D4 — The end is noted when the review is asked something

- status:   pending
- task:     3.1
- commit:   (this change)
- trigger:  `view` draws from `&self`; the content's size is known only when a key, a click or the wheel reaches the surface.
- decided:  `note_seen` runs before and after each of those, so the gesture that leaves a short diff already counts its last line on screen.
- rejected: Interior mutability in `view`: drawing would change what the review keeps.
- blast:    `review::surface`, `review::extension`.
