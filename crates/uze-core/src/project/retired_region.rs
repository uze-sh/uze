//! The regions earlier releases wrote into a project's `AGENTS.md`, named
//! so what they left behind can be recognised and taken back out.
//!
//! Nothing is written under them any more. The workspace's instructions
//! reach an agent through its launch context, and the plugin authoring
//! verbs through the `uze:author` Skill: UZE writes nothing into a file the
//! operator authors.

/// The workspace's isolation policy, projected until the launch carried it.
pub const WORKTREE_POLICY: &str = "project:worktree-policy";

/// The plugin authoring verbs, projected until the `uze:author` Skill
/// documented them for every agent.
pub const PLUGIN_AUTHORING: &str = "project:plugin-authoring";

const RETIRED: [&str; 2] = [WORKTREE_POLICY, PLUGIN_AUTHORING];

/// Whether `identity` is a region no release writes any more. Each one
/// ended in a digest of the bytes it was written with, so what it held can
/// be told from an edit made since.
pub fn is_retired(identity: &str) -> bool {
    RETIRED.iter().any(|prefix| {
        identity
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_retired_prefixes_are_retired() {
        assert!(is_retired("project:worktree-policy/0123abcd"));
        assert!(is_retired("project:plugin-authoring/0123abcd"));
        assert!(!is_retired("project:worktree-policy"));
        assert!(!is_retired("package:uze:official:instructions"));
        assert!(!is_retired("instruction-bridge"));
    }
}
