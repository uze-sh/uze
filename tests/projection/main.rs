//! Projection tests: how canonical resources are *named*, routed and
//! physically represented per harness — exposure naming, invocation
//! labels/policy, generated packages and skill roots.

mod invocation;
mod naming;
// Every case here drives shebang stand-ins: Unix-only until they dispatch
// through `uze-fake-harness` (windows-support task 9.2).
#[cfg(unix)]
mod skill_roots;
mod worktree_policy;
