## D1 — Refuse a key-less record outright; no ladder rung, no grace period

- status:   pending
- task:     5.2
- commit:   squashed on delivery
- trigger:  Two tests (`a_record_issued_no_key_admits_the_launch_that_names_it`, `an_agent_placed_before_launch_keys_still_comes_back_to_its_conversation`) pinned the admit-any behavior the audit exploited, as a continuity courtesy to agents placed before ADR-056.
- decided:  `admits_launch` refuses a record with no digest. Both tests were rewritten to pin the opposite (absent proof is no proof). A pre-key agent's persisted tab now starts its harness as an ordinary invocation until its next placement issues a key. No `uze-document` rung: the field's shape is unchanged, only its reading.
- rejected: A ladder rung minting keys for key-less records (a key nobody holds equals refusing them, with more code); admitting key-less top-level records but not children (a top-level key-less record is as forgeable as a child's).
- blast:    Reverting reopens WKS-5. Keeping it costs a fresh conversation for any agent placed before ADR-056 and never re-placed since.

## D2 — Issue a split subagent a key and drop the secret

- status:   pending
- task:     5.2
- commit:   squashed on delivery
- trigger:  Subagents never prove themselves (they run under the parent's stamp and are refused inside a child's checkout), so the brief's "a subagent's record must carry a credential" has no holder.
- decided:  `split_work` calls `issue_launch_key` on the child and discards the secret. With D1 it is redundant in this build; it protects the record from an older build on the same machine that still admits key-less records.
- rejected: Handing the secret to the parent in the split answer (nothing needs it; a secret printed to an agent's transcript is one more to leak).
- blast:    None beyond the revert.

## D3 — Seal program panes only; the person's shell panes stay unsealed

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  The brief proposed sealing every pane. Landlock needs `no_new_privs` for an unprivileged process, which disables set-uid: `sudo`, `su`, `pkexec` stop working in a sealed pane (verified: `setpriv --no-new-privs sudo -n true` is refused).
- decided:  A pane is sealed when its launch is a program (every agent the workspace places, every program relaunched on restore). A shell the person opens, and the shell an `OpenSpace` opens, are not. Those keep ADR-056's layers (classification and key) at both endpoints.
- rejected: Sealing every pane (takes `sudo` from the person's own terminal); sealing only panes stamped with `UZE_AGENT` (the env is the agent's to strip and a relaunched typed agent carries none).
- blast:    An agent the person types into their own shell pane is unsealed and keeps ADR-056's residual. Reverting to "seal every pane" is a one-line change to `spawn_pane`.

## D4 — Seal from a throwaway thread, not through a helper exec

- status:   pending
- task:     1.1
- commit:   squashed on delivery
- trigger:  The pane is forked by `portable_pty` from the server, with no `pre_exec` hook; Landlock must be applied before the program runs.
- decided:  `uze_platform::seal::sealed` runs the spawn on a scoped thread that first restricts itself (Landlock and `no_new_privs` are per thread), so the child inherits the domain and the server does not.
- rejected: `uze terminal host-pane --seal` as an exec wrapper (an extra image per pane, and the hook only exists where a host binary is configured, not in in-process tests); restricting the whole server (it would seal the person's endpoint's own creator and every shell).
- blast:    None beyond the revert.

## D5 — Do not scope signals

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  `Scope::Signal` would also stop a sealed agent from killing the server and replacing it with one inside its own domain.
- decided:  Only abstract sockets are scoped. The replacement attack is closed on the client instead: a client that does not run confined retires a server (or refuses any endpoint peer) that does (`Replaced::StartedInsidePane`).
- rejected: Scoping signals (an agent could no longer end a process another pane or the person started, which harnesses do; a kill of the server stays a DoS, not a takeover).
- blast:    A sealed agent can still kill the runtime (DoS). Adding `Scope::Signal` later is one flag.

## D6 — `no_new_privs` is the kernel's word on "who asks"

- status:   pending
- task:     4.2
- commit:   squashed on delivery
- trigger:  The brief suggested probing the person's endpoint. That only answers while a server runs, and a sealed agent can kill the server (D5) before running `uze workspace allow` under `script`.
- decided:  `asker()` also refuses a process under `no_new_privs`: every sealed pane has it, it is inherited and never cleared, and Codex, bubblewrap (Claude Code, Flatpak) and Docker's `no-new-privileges` set it too. The refusal says to answer from an unsandboxed terminal.
- rejected: The endpoint probe (absent server = no answer); keying the approval record (the key would live in the same writable home).
- blast:    A person in a sandboxed terminal (Flatpak VS Code, a `no-new-privileges` container) cannot approve or consent there and must use another terminal. Inside the audit sandbox (bubblewrap) every process is refused, which is why the WKS-4 re-run's "operator" path cannot be shown approving there.

## D7 — Pane-opened spaces: per-pane 3, total 8, unpersisted until visited

- status:   pending
- task:     3.2
- commit:   squashed on delivery
- trigger:  ADR-056 lets a pane open a space for a nested `uze`; the finding showed no bound and full persistence.
- decided:  A space `OpenSpace` creates is unvisited until a client selects it (SelectSpace, SelectTab in it, or an attach seated in it); at most 3 per asking pane and 8 in total, the rest `Refused`; unvisited spaces are left out of `workspace.json`. The bound applies to every `OpenSpace`, whatever its origin.
- rejected: A rate limit (a slow loop still fills the workspace); refusing `OpenSpace` from panes (breaks the nested launch).
- blast:    A person who types `uze` in more than 3 projects from one pane without looking at the spaces is refused the fourth until they visit one.

## D8 — Protocol version 24

- status:   pending
- task:     2.2
- commit:   squashed on delivery
- trigger:  The wire did not change, but an attach at the shared socket now means a refusal while the person's endpoint stands, and a client of the previous build would read that refusal as "this server speaks my protocol" and never replace it.
- decided:  Bump to 24, so builds on either side replace each other as for every bump.
- rejected: Keeping 23 (an older client would be stuck refused by a newer server).
- blast:    The first client of this build retires a running server of the previous one; its panes are restored.
