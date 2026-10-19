---
trigger: glob
globs: "**/crates/uze-terminal/**"
---

`uze-terminal` is the local terminal runtime: a server owning the
pseudoterminals and a versioned client protocol, so a pane survives a
client leaving (ADR-056). It knows nothing of agents, depends on nothing in
the workspace but `uze-platform` and `uze-document`, and obeys
`uze-document`'s rule like everything else that persists. The path to its
own workspace is the only path it computes for itself, the one sanctioned
exception to `UzeHome` naming every path UZE owns.
