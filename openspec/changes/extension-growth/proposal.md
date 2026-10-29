## Why

The plan's last change grows the platform only where a gallery entry needs
it (`extension-platform`, *Proposed split*, 8). None of the ten
reference extensions needs the network, a secret or a write to the
checkout, so this change adds one entry that does and the least it takes:

- `ci-status`: the checkout's branch's last CI runs, read from the forge's
  API with the operator's token, in a status row and a section.

It needs `net` (a request to a host the manifest names), `fetch` (the
request itself, over the system's `curl`, never a TLS crate, D19) and a
`secret` preference (the token, kept in the operating system's keychain,
never shown again). The cosmetic `kdl` fence inside a Markdown file open in
the code surface's editor (parent 23.8) rides along.

## What Changes

- **`net:<host>` grant**: an extension names each host it may reach;
  consent lists them; nothing else is reachable.
- **`fetch(url, options)`** in scripts, under `net`: `GET` or `POST`, JSON
  or text, run through the program runner with the system's `curl` by path,
  `-q`, a scrubbed environment, the run's time and output bounds, refused
  for a host the grant does not name (redirects included) and for anything
  but `https`.
- **`secret` preferences**: a typed preference whose value is kept in the
  keychain (macOS Keychain through `security` by path, Windows Credential
  Manager, a `0600` file under `state/extensions/secrets/` on Linux), set on
  the Extensions screen and never redisplayed, logged, cached, piped or sent
  to an agent; `{pref.*}` never reads one; a script reads it only through
  `fetch`'s `secret` option, which places it in a header.
- **Gallery**: `ci-status` as a fixture plugin, its tests answered by
  recorded responses.
- **The `kdl` fence** coloured inside a Markdown file in the editor.

Not done, recorded as decisions: `edit:checkout`, pinning by digest as a
preference, pipes and extension CLI commands: no gallery entry needs them.

## Capabilities

### New Capabilities
- `extension-net`: the hosts an extension may reach and how a request is made.
- `extension-secrets`: secrets an extension holds without ever seeing them drawn.

### Modified Capabilities
- None in `openspec/specs/`.

## Impact

- `uze-contract`: `Grant::Net(host)`, `PreferenceKind::Secret`.
- `uze-core`: the decoder and check; consent lists hosts.
- `uze-platform`: `secrets` (one implementation per platform), `tools::curl`.
- `uze-script`: `fetch`.
- `uze-application`: the runner's fetch through the program runner; secrets.
- `src/ui`: the secret field on the Extensions screen; the fence.
- Fixture `ci-status`, docs, invariants, a journey.
