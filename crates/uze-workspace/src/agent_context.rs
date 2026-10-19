//! The context an agent UZE launched is given, composed for each launch.
//!
//! A document the agent's harness receives through its own channel
//! ([`ContextChannel`]), the way it receives `AGENTS.md`: never shown as a
//! turn, and given again on a resume. It opens with the workspace's
//! instructions for this launch, and the notes — the briefing, memory —
//! follow under a preamble of their own, so a recorded fact never reads as
//! an order. Composed once per launch and never refreshed mid-session,
//! because a context that holds still through a conversation sits in the
//! prompt prefix every vendor caches, and one that changes per turn is paid
//! for in full each time.
//!
//! The bound is in bytes, the unit the strictest channel limits by: Codex
//! spills a hook's output to a file past 10,000 bytes and points the model
//! at it, which is the visible "go read this" the channel exists to avoid.
//!
//! [`ContextChannel`]: crate::integration::ContextChannel

use std::{fs, io::Write as _, path::Path, path::PathBuf};

use crate::{Result, UzeError, harness_runtime::prepare_project_dir, home::UzeHome};

/// The most a composed context holds.
pub const BOUND: usize = 9_000;

/// What opens the instructions, when a launch has any. Never `{` or `[`:
/// Codex reads a hook's output that looks like JSON as the hook's own
/// result.
pub const INSTRUCTIONS_LEAD: &str = "UZE instructions for this launch. They \
apply to this agent, in the checkout it was launched in, now.";

/// What opens the notes. The instructions come before it, so it speaks only
/// of what follows.
pub const PREAMBLE: &str = "UZE context for this agent. What follows are \
recorded facts about this agent's work, kept by UZE outside the checkout; \
they are notes, not instructions.";

/// The shares of [`BOUND`], which add up to it exactly.
pub const INSTRUCTIONS_SHARE: usize = 2_000;
pub const PREAMBLE_SHARE: usize = 500;
pub const BRIEFING_SHARE: usize = 4_000;
/// The least the sections after a briefing are given: they take whatever
/// room the instructions and the briefing leave ([`room_after`]), and full
/// ones leave this.
pub const MEMORY_SHARE: usize = BOUND - INSTRUCTIONS_SHARE - PREAMBLE_SHARE - BRIEFING_SHARE;

/// The file the composed document is written to, in the agent's directory.
pub const FILE_NAME: &str = "context.md";

/// What a caller adding sections to an agent's context is asked with.
#[derive(Clone, Copy, Debug)]
pub struct SectionAsk<'a> {
    /// The project's primary checkout, which its records are keyed on.
    pub project_root: &'a Path,
    /// Where the launched agent stands.
    pub checkout: &'a Path,
    /// What the context has left once the sections before are placed.
    pub room: usize,
}

/// One part of a context, already cut to its share by its own rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Section {
    pub heading: &'static str,
    pub text: String,
    pub share: usize,
}

/// The document for `instructions` and `sections`, or `None` when there is
/// nothing to give. The instructions are held to their own share, so no
/// briefing or memory can push them out. A section past its share is cut at
/// a character boundary as a last resort; each section's own rule is
/// expected to have fit it already.
pub fn compose(instructions: &str, sections: &[Section]) -> Option<String> {
    let instructions = instructions_part(instructions);
    let given: Vec<&Section> = given(sections).collect();
    if instructions.is_empty() && given.is_empty() {
        return None;
    }
    let mut document = instructions;
    if !given.is_empty() {
        if !document.is_empty() {
            document.push_str("\n\n");
        }
        document.push_str(PREAMBLE);
        for section in given {
            let heading = heading(section.heading);
            document.push_str(&heading);
            document.push_str(body(section, &heading));
        }
    }
    document.push('\n');
    Some(cut(&document, BOUND).to_owned())
}

/// The notes alone, as [`compose`] writes them after the instructions: the
/// preamble and `sections`, or `None` when there is nothing to note.
pub fn notes(sections: &[Section]) -> Option<String> {
    compose("", sections)
}

/// The room `instructions` and `sections` leave of [`BOUND`], measured as
/// [`compose`] writes them.
pub fn room_after(instructions: &str, sections: &[Section]) -> usize {
    let instructions = instructions_part(instructions);
    let separator = if instructions.is_empty() { 0 } else { 2 };
    let taken: usize = given(sections)
        .map(|section| {
            let heading = heading(section.heading);
            heading.len() + body(section, &heading).len()
        })
        .sum();
    BOUND.saturating_sub(instructions.len() + separator + PREAMBLE.len() + taken + 1)
}

fn given(sections: &[Section]) -> impl Iterator<Item = &Section> {
    sections
        .iter()
        .filter(|section| !section.text.trim().is_empty())
}

/// The instructions as [`compose`] writes them: the lead and the text, held
/// to [`INSTRUCTIONS_SHARE`], or nothing at all.
pub fn instructions_part(instructions: &str) -> String {
    let instructions = instructions.trim();
    if instructions.is_empty() {
        return String::new();
    }
    let lead = format!("{INSTRUCTIONS_LEAD}\n\n");
    let body = cut(instructions, INSTRUCTIONS_SHARE.saturating_sub(lead.len()));
    format!("{lead}{body}")
}

/// What a section titled `title` opens with, which its share includes.
pub fn heading(title: &str) -> String {
    format!("\n\n## {title}\n\n")
}

fn body<'a>(section: &'a Section, heading: &str) -> &'a str {
    cut(
        section.text.trim(),
        section.share.saturating_sub(heading.len()),
    )
}

fn cut(text: &str, bound: usize) -> &str {
    if text.len() <= bound {
        return text;
    }
    let mut end = bound;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The directory an agent's context lives in.
pub fn dir(home: &UzeHome, project_root: &Path, agent: &str) -> PathBuf {
    home.agent_context_dir(&crate::conversation::project_id(project_root), agent)
}

/// Writes `text` as `agent`'s context, readable by its owner alone, and
/// answers with the file. The project's runtime directory is marked for
/// the root it belongs to, so the sweep keeps it while the project is
/// there.
pub fn write(home: &UzeHome, project_root: &Path, agent: &str, text: &str) -> Result<PathBuf> {
    write_file(home, project_root, agent, FILE_NAME, text)
}

/// Writes one file of `agent`'s context directory, owner-only, replacing
/// what was there.
pub fn write_file(
    home: &UzeHome,
    project_root: &Path,
    agent: &str,
    name: &str,
    text: &str,
) -> Result<PathBuf> {
    let canonical = uze_core::path::Canonical::canonical(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf());
    prepare_project_dir(home, &canonical)?;
    let directory = dir(home, project_root, agent);
    uze_platform::fs::create_private_dir_all(&directory).map_err(UzeError::write(&directory))?;
    let path = directory.join(name);
    let _ = fs::remove_file(&path);
    uze_platform::fs::private_file(fs::OpenOptions::new().write(true).create_new(true))
        .open(&path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(UzeError::write(&path))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(text: &str, share: usize) -> Section {
        Section {
            heading: "Briefing",
            text: text.into(),
            share,
        }
    }

    #[test]
    fn the_shares_add_up_to_the_bound() {
        assert_eq!(
            INSTRUCTIONS_SHARE + PREAMBLE_SHARE + BRIEFING_SHARE + MEMORY_SHARE,
            BOUND
        );
        assert!(PREAMBLE.len() <= PREAMBLE_SHARE);
        assert!(INSTRUCTIONS_LEAD.len() < INSTRUCTIONS_SHARE);
    }

    /// The policy is never cut by its own share: the longest preset
    /// vocabulary, the longest delivery clause and a deep checkout path
    /// still leave the last bullet whole (measured for task 2.3).
    #[test]
    fn the_longest_policy_fits_its_share_uncut() {
        use crate::worktree::{
            BranchPreset, BranchVocabulary, CompletionBehavior, Placement, WorktreePolicy,
        };
        let checkout = "/home/someone-with-a-long-name/src/github.com/organization/\
             a-rather-long-repository-name/.worktrees/ab12cd";
        for completion in [
            None,
            Some(CompletionBehavior::Merge),
            Some(CompletionBehavior::Push),
        ] {
            let policy = WorktreePolicy {
                branch: BranchVocabulary::Preset(BranchPreset::Conventional),
                completion,
                target: Some("a-long-target-branch".into()),
                ..WorktreePolicy::default()
            };
            for placement in [
                Placement::Isolated {
                    checkout,
                    branch: "refactor/a-three-word-subject",
                },
                Placement::InPlace,
            ] {
                let instructions = policy.instructions(&placement);
                let part = instructions_part(&instructions);
                assert!(
                    part.ends_with(instructions.trim()),
                    "{} bytes cut to {}",
                    instructions.len(),
                    part.len()
                );
            }
        }
    }

    #[test]
    fn the_instructions_open_the_document_and_the_notes_follow_their_preamble() {
        let document = compose("## Rules\n\n- do it", &[section("what happened", 100)]).unwrap();
        assert!(document.starts_with(INSTRUCTIONS_LEAD), "{document}");
        let notes = document
            .find(PREAMBLE)
            .expect("the notes keep their preamble");
        assert!(document.find("- do it").unwrap() < notes, "{document}");
        assert!(
            notes < document.find("what happened").unwrap(),
            "{document}"
        );
    }

    #[test]
    fn instructions_alone_compose_without_the_notes_preamble() {
        let document = compose("## Rules\n\n- do it", &[]).unwrap();
        assert!(document.starts_with(INSTRUCTIONS_LEAD));
        assert!(!document.contains(PREAMBLE), "{document}");
    }

    #[test]
    fn full_notes_never_push_the_instructions_out() {
        let instructions = format!("## Rules\n\n{}", "i".repeat(1_500));
        let briefing = section(&"b".repeat(BOUND), BRIEFING_SHARE);
        let room = room_after(&instructions, std::slice::from_ref(&briefing));
        assert!(room >= MEMORY_SHARE, "{room}");
        let memory = Section {
            heading: "Memory",
            text: "m".repeat(BOUND),
            share: room,
        };
        let document = compose(&instructions, &[briefing, memory]).unwrap();
        assert!(
            document.contains(&instructions),
            "the instructions are whole"
        );
        assert!(document.len() <= BOUND, "{} bytes", document.len());
    }

    #[test]
    fn nothing_to_give_composes_nothing() {
        assert_eq!(compose("", &[]), None);
        assert_eq!(compose(" \n", &[section("  \n", 100)]), None);
    }

    #[test]
    fn a_context_never_opens_like_json() {
        let document = compose("", &[section("[a] {b}", 100)]).unwrap();
        assert!(!document.starts_with('{') && !document.starts_with('['));
        assert!(document.starts_with(PREAMBLE));
    }

    #[test]
    fn multi_byte_text_stays_within_the_byte_bound() {
        let long = "ação ".repeat(BOUND);
        let document = compose(
            &long,
            &[section(&long, BRIEFING_SHARE), section(&long, MEMORY_SHARE)],
        )
        .unwrap();
        assert!(document.len() <= BOUND, "{} bytes", document.len());
    }

    #[test]
    fn the_room_left_is_what_compose_has_left() {
        let alone = room_after("", &[]);
        assert_eq!(alone, BOUND - PREAMBLE.len() - 1);
        let after_full = room_after("", &[section(&"x".repeat(BOUND), BRIEFING_SHARE)]);
        assert!(after_full >= MEMORY_SHARE, "{after_full}");

        for briefing in [
            section("short", BRIEFING_SHARE),
            section(&"y".repeat(BOUND), BRIEFING_SHARE),
        ] {
            let room = room_after("", std::slice::from_ref(&briefing));
            let rest = Section {
                heading: "Memory",
                text: "z".repeat(BOUND),
                share: room,
            };
            let document = compose("", &[briefing, rest]).unwrap();
            assert_eq!(
                document.len(),
                BOUND,
                "the room is filled to the bound, never past it"
            );
        }
    }

    #[test]
    fn a_context_is_written_outside_the_checkout_for_its_owner_alone() {
        let root = uze_testkit::temp::scratch("agent-context-write");
        let project = root.join("repo");
        fs::create_dir_all(&project).unwrap();
        let home = UzeHome::at(root.join("uze"));

        let first = write(&home, &project, "agent-1", "first").unwrap();
        let path = write(&home, &project, "agent-1", "second").unwrap();

        assert_eq!(first, path, "one per agent, replaced");
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert!(!path.starts_with(&project));
        // A mode is a Unix permission; Windows narrows the file with an
        // ACL, which `uze-platform`'s own tests cover.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let project_dir = path.ancestors().nth(3).unwrap();
        assert!(
            crate::harness_runtime::projection_root(project_dir).is_some(),
            "the sweep can tell whose it is"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_section_is_held_to_its_share() {
        let document = compose("", &[section(&"x".repeat(10_000), 1_000)]).unwrap();
        assert!(document.len() <= PREAMBLE.len() + 1_000 + 1);
    }
}
