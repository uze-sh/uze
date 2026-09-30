//! The region the package manager projects into a project's `AGENTS.md`
//! so any agent there can author a plugin, whether or not the workspace
//! launched it.
//!
//! Keyed on its content, like every region UZE owns there: a change to the
//! text supersedes the region rather than drifting it. The prefix is the
//! package manager's, disjoint from the workspace's `project:worktree-policy`
//! and from `package:*:instructions`, so no owner mistakes another's region
//! for an orphan of its own.

use crate::digest;

/// The prefix of the region's identity.
pub const REGION_PREFIX: &str = "project:plugin-authoring";

/// The bytes the region carries.
pub fn instructions() -> String {
    "## Authoring plugins\n\
     \n\
     - Creating a plugin is agent work, driven with these deterministic verbs, none of which \
     needs anything but `uze` on the machine: `uze agent market create <name> --at <dir> \
     [--description <text>]` scaffolds a marketplace as a Git repository, registers and links \
     it in one step (or skip to the next verb when a marketplace already exists — ask \
     `uze market list` for the names); `uze agent plugin create <name> --market <market> \
     [--hook] [--mcp] [--instructions]` scaffolds a plugin into it; `uze agent plugin check \
     <path>` and `uze agent market check <path>` validate offline — run the check before any \
     install, then `uze install -m <plugin>@<market>` and iterate on the files, which the \
     linked marketplace already reads. The guided script for the whole loop is the \
     `uze:author` skill.\n"
        .to_owned()
}

/// The region's identity for the current text.
pub fn identity() -> String {
    format!(
        "{REGION_PREFIX}/{}",
        digest::short_hex(instructions().as_bytes())
    )
}

/// Whether `identity` is a region this module owns, in any version.
pub fn owns_region(identity: &str) -> bool {
    identity
        .strip_prefix(REGION_PREFIX)
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_follows_the_text() {
        assert!(owns_region(&identity()));
        assert!(!owns_region("project:worktree-policy/0123"));
        assert!(!owns_region(REGION_PREFIX));
    }

    #[test]
    fn nothing_in_it_needs_the_workspace() {
        let text = instructions();
        assert!(!text.contains("uze agent work"), "{text}");
        assert!(text.contains("uze agent plugin create"), "{text}");
    }
}
