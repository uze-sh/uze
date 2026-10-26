## ADDED Requirements

### Requirement: Extension actions are keymap actions in a table of their own
An extension action SHALL be a keymap action named
`ext.<plugin>@<marketplace>/<id>.<action>`, live only while that
extension's surface is in front, in a table of that extension's own
resolved after the host's bindings, so two extensions never collide and
the host always wins. A suggested `key` SHALL be bound only where the
chord is free.

#### Scenario: A suggestion the host already uses is not bound
- **WHEN** an extension suggests `key "ctrl-q"` for an action
- **THEN** the action is offered by pointer and in the action index without that key, and `uze doctor` names the collision

#### Scenario: The operator rebinds an extension action
- **WHEN** `keys.json` binds `r` to `ext.gsd@luizfbalves/gsd-band.refresh`
- **THEN** `r` runs that action while the `gsd-band` surface is in front, and the footer shows `r`

### Requirement: A conflict drops one binding, never the keymap
A conflict inside one extension's table SHALL drop that one binding with
a problem reported, never the load. A destructive action, one that
declares `confirm`, SHALL refuse a bare-letter suggestion. A name of an
extension that is not installed SHALL be a warning.

#### Scenario: A bare letter on a confirmed action is refused
- **WHEN** an action with `confirm` suggests `key "d"`
- **THEN** the check fails saying a confirmed action needs a modifier
