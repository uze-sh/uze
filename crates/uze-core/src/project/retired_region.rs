//! The regions earlier releases wrote into a project's `AGENTS.md`, named
//! so what they left behind can be recognised and taken back out.
//!
//! Nothing is written under them any more. The workspace's instructions
//! reach an agent through its launch context, the plugin authoring verbs
//! through the `uze:author` Skill, and what a plugin tells every agent of a
//! declaring project through an `always_on` rule: UZE writes nothing into a
//! file the operator authors.

/// The workspace's isolation policy, projected until the launch carried it.
pub const WORKTREE_POLICY: &str = "project:worktree-policy";

/// The plugin authoring verbs, projected until the `uze:author` Skill
/// documented them for every agent.
pub const PLUGIN_AUTHORING: &str = "project:plugin-authoring";

const RETIRED: [&str; 2] = [WORKTREE_POLICY, PLUGIN_AUTHORING];

/// Whether `identity` is a region no release writes any more, written with
/// a digest of its bytes. Each one ended in that digest, so what it held
/// can be told from an edit made since.
pub fn is_retired(identity: &str) -> bool {
    RETIRED.iter().any(|prefix| {
        identity
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether `identity` is a package's instructions region,
/// `package:<plugin>:<market>:instructions`, composed from a plugin's own
/// `AGENTS.md` until plugins spoke through `always_on` rules. Its identity
/// carries no digest and no renderer is left to say what it held, so it is
/// recognised by shape alone and removed structurally.
pub fn is_retired_package(identity: &str) -> bool {
    identity
        .strip_prefix("package:")
        .and_then(|rest| rest.strip_suffix(":instructions"))
        .is_some_and(|middle| {
            middle
                .split_once(':')
                .is_some_and(|(plugin, market)| !plugin.is_empty() && !market.is_empty())
        })
}

/// The region an earlier release wrote into a harness's own instructions
/// file, `CLAUDE.md`, to import `AGENTS.md` while a package contributed to
/// it. It goes with the last package region.
pub const INSTRUCTION_BRIDGE: &str = "instruction-bridge";

/// What [`INSTRUCTION_BRIDGE`] held: the import, and nothing else.
pub const INSTRUCTION_BRIDGE_CONTENT: &str = "@AGENTS.md";

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

    #[test]
    fn a_package_region_is_recognised_by_its_shape() {
        assert!(is_retired_package("package:greet:tools:instructions"));
        assert!(!is_retired_package("package:greet:instructions"));
        assert!(!is_retired_package("package::tools:instructions"));
        assert!(!is_retired_package("package:greet:tools:rules"));
        assert!(!is_retired_package("project:worktree-policy/0123abcd"));
        assert!(!is_retired_package(INSTRUCTION_BRIDGE));
    }
}
