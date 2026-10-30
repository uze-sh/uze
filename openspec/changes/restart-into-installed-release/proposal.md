## Why

Once the updater has replaced the binary, the sidebar says "restart to use
it", and the operator has to quit the workspace and start it again by hand.
The panes would survive that: the terminal runtime owns them and keeps them
when a client leaves, and it already retires itself when a newer client
cannot speak its protocol. So a restart is only the client's, and it can be
one click.

## What Changes

- The release notice's second row, "restart to use it", restarts the
  workspace client into the installed release; its first row still opens
  the release's notes. The action index offers the same restart
  (`restart-uze`) while the notice stands.
- The client leaves the terminal as it found it, then runs the installed
  binary in its place: `exec` on Linux and macOS, and on Windows a child on
  the same console that this process waits for and whose exit it returns.
- The restarted client lands where the operator was, not in a space for
  the directory the first one was started in.

## Capabilities

### Modified Capabilities

- `binary-updates`: the workspace can restart into an installed release.

## Impact

- `uze-platform`: `process::replace_self`.
- `src/self_update.rs`: what a restart runs.
- `src/ui/`: the notice's restart row, the action, the client's exit.
- `src/main.rs` and the CLI grammar: running the restart, a hidden flag
  saying where the client lands.
