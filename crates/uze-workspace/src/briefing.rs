//! What an agent continuing on another harness is told about its own work.
//!
//! A harness cannot read another vendor's conversation, so a switch writes
//! a briefing — Git's account of the work and, where the harness being left
//! can read its own transcript, the substance of the conversation — and the
//! incoming harness's first turn points at it. The briefing is the one
//! place UZE writes conversation text: the continuity record names where it
//! is and nothing more.
//!
//! The first turn is one line rather than the briefing itself, for two
//! reasons: a process's arguments are readable by anyone who can list it,
//! and a briefing the size of a conversation is the context exhaustion the
//! switch was usually made to escape. Hence the bound, and the order things
//! are cut in when a conversation is larger than it: Git last answers what
//! the checkout already holds, the goal and the latest summary answer what
//! nothing else can.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use crate::{
    Result, UzeError,
    harness_runtime::prepare_project_dir,
    home::UzeHome,
    session::{ConversationDigest, SessionId, Speaker},
};

/// The most a briefing holds, so reading it costs the incoming harness a
/// small, predictable part of its context.
pub const BOUND: usize = 12 * 1024;

const GOAL_BOUND: usize = 1536;
const SUMMARY_BOUND: usize = 4096;
const OPERATOR_TURN_BOUND: usize = 400;
const OPERATOR_TURNS_BOUND: usize = 2560;
const AGENT_MESSAGES: usize = 3;
const AGENT_MESSAGES_BOUND: usize = 3072;
const COMMAND_BOUND: usize = 300;
const GIT_BOUND: usize = 1536;
const COMMITS_LISTED: usize = 30;

/// The operator's first message to a harness taking over an agent's work.
pub fn first_turn(briefing: &Path) -> String {
    format!(
        "You are continuing work another agent started in this checkout. \
         Read {} first, then check `git log` and `git status`, and carry on.",
        briefing.display()
    )
}

/// Who is handing over to whom, and since when.
#[derive(Clone, Debug)]
pub struct Handover<'a> {
    pub label: &'a str,
    /// The display names of the harness left and the one taking over.
    pub from: &'a str,
    pub to: &'a str,
    pub session: Option<&'a SessionId>,
    /// When the incoming harness last ran this agent: it resumes its own
    /// conversation, and is told only what happened since.
    pub returning_since: Option<u64>,
}

/// Git's account of the work, read from the agent's own directory.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitFacts {
    pub branch: Option<String>,
    pub base: Option<String>,
    /// Subjects, newest first.
    pub commits: Vec<String>,
    pub diff_stat: String,
    /// `git status --short` lines.
    pub dirty: Vec<String>,
}

impl GitFacts {
    /// Reads them from `directory`. `base` is the commit the agent's branch
    /// was cut from; without one — an agent in the project's root — the
    /// most recent commits stand in. `since_unix` keeps only the commits a
    /// returning harness has not seen. A directory Git cannot answer about
    /// reads as no facts rather than as an error: the briefing is advisory.
    pub fn read(directory: &Path, base: Option<&str>, since_unix: Option<u64>) -> Self {
        let lines = |args: &[&str]| -> Vec<String> {
            crate::git::read(directory, args)
                .ok()
                .filter(uze_git::Output::is_success)
                .map(|output| {
                    output
                        .stdout
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let range = base.map(|base| format!("{base}..HEAD"));
        let since = since_unix.map(|at| format!("--since=@{at}"));
        let limit = format!("-{COMMITS_LISTED}");
        let mut log = vec!["log", "--format=%s", limit.as_str()];
        log.extend(since.as_deref());
        log.extend(range.as_deref());
        let diff_stat = base
            .map(|base| lines(&["diff", "--stat", base]).join("\n"))
            .unwrap_or_default();
        Self {
            branch: lines(&["branch", "--show-current"]).into_iter().next(),
            base: base.map(str::to_owned),
            commits: lines(&log),
            diff_stat,
            dirty: lines(&["status", "--short"]),
        }
    }
}

/// The briefing's text, within [`BOUND`].
pub fn render(
    handover: &Handover<'_>,
    git: &GitFacts,
    digest: Option<&ConversationDigest>,
) -> String {
    // In the order they are cut when the whole does not fit: the last
    // entry goes first.
    let mut sections: Vec<String> = vec![header(handover, digest)];
    if let Some(digest) = digest {
        let goal = digest
            .goal
            .as_deref()
            .filter(|_| handover.returning_since.is_none());
        if let Some(goal) = goal {
            sections.push(section("Goal", &clip(goal, GOAL_BOUND)));
        }
        if let Some(summary) = &digest.summary {
            sections.push(section(
                &format!("Summary {} wrote", handover.from),
                &clip(summary, SUMMARY_BOUND),
            ));
        }
        sections.push(operator_turns(digest, goal));
        sections.push(agent_messages(handover.from, digest));
        if let Some(command) = &digest.last_command {
            let outcome = match command.failed {
                Some(true) => " (failed)",
                Some(false) => " (succeeded)",
                None => "",
            };
            sections.push(section(
                "Last command",
                &format!("`{}`{outcome}", clip(&command.line, COMMAND_BOUND)),
            ));
        }
    }
    sections.push(git_section(git));
    sections.retain(|section| !section.is_empty());

    while sections.len() > 1 && joined_len(&sections) > BOUND {
        sections.pop();
    }
    let mut text = sections.join("\n");
    if text.len() > BOUND {
        text = clip(&text, BOUND);
    }
    text
}

fn header(handover: &Handover<'_>, digest: Option<&ConversationDigest>) -> String {
    let mut lines = vec![format!("# Briefing: {}", handover.label), String::new()];
    lines.push(match handover.returning_since {
        Some(_) => format!(
            "You worked on this agent before. It continued in {} since; below is only what \
             happened there after you left.",
            handover.from
        ),
        None => format!(
            "This agent was started in {} and continues in {}. Below is what it was doing.",
            handover.from, handover.to
        ),
    });
    if let Some(session) = handover.session {
        lines.push(format!(
            "The {} conversation is `{session}`; it is kept, not continued here.",
            handover.from
        ));
    }
    if digest.is_none() {
        lines.push(format!(
            "{}'s conversation could not be read, so this is Git's account alone.",
            handover.from
        ));
    }
    lines.join("\n") + "\n"
}

/// The operator's turns, without the one already shown as the goal: a
/// digest floored at a time keeps turns from after it, where the goal is
/// usually not among them, so it is told apart by what it says.
fn operator_turns(digest: &ConversationDigest, goal: Option<&str>) -> String {
    let mut goal = goal;
    let turns: Vec<&str> = digest
        .turns
        .iter()
        .filter(|turn| turn.speaker == Speaker::Operator)
        .map(|turn| turn.text.as_str())
        .filter(|text| goal.take_if(|goal| goal == text).is_none())
        .collect();
    // Newest kept first: what the operator asked last is what the agent
    // was answering when it stopped.
    let mut kept = Vec::new();
    let mut used = 0;
    for text in turns.iter().rev() {
        let item = format!("- {}", one_line(&clip(text, OPERATOR_TURN_BOUND)));
        if used + item.len() > OPERATOR_TURNS_BOUND {
            break;
        }
        used += item.len() + 1;
        kept.push(item);
    }
    if kept.is_empty() {
        return String::new();
    }
    kept.reverse();
    section("What the operator asked", &kept.join("\n"))
}

fn agent_messages(from: &str, digest: &ConversationDigest) -> String {
    let messages: Vec<&str> = digest
        .turns
        .iter()
        .filter(|turn| turn.speaker == Speaker::Agent)
        .map(|turn| turn.text.as_str())
        .collect();
    let latest = &messages[messages.len().saturating_sub(AGENT_MESSAGES)..];
    if latest.is_empty() {
        return String::new();
    }
    let each = AGENT_MESSAGES_BOUND / latest.len();
    let body = latest
        .iter()
        .map(|text| clip(text, each))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    section(&format!("What {from} said last"), &body)
}

fn git_section(git: &GitFacts) -> String {
    let mut lines = Vec::new();
    if let Some(branch) = &git.branch {
        lines.push(format!("Branch: `{branch}`"));
    }
    if let Some(base) = &git.base {
        lines.push(format!("Base: `{base}`"));
    }
    if !git.commits.is_empty() {
        lines.push("Commits, newest first:".to_owned());
        lines.extend(git.commits.iter().map(|subject| format!("- {subject}")));
    }
    if !git.diff_stat.is_empty() {
        lines.push(format!(
            "Changed since the base:\n```\n{}\n```",
            git.diff_stat
        ));
    }
    if !git.dirty.is_empty() {
        lines.push(format!("Uncommitted:\n```\n{}\n```", git.dirty.join("\n")));
    }
    if lines.is_empty() {
        return String::new();
    }
    section("Git", &clip(&lines.join("\n"), GIT_BOUND))
}

fn section(title: &str, body: &str) -> String {
    format!("## {title}\n\n{body}\n")
}

fn joined_len(sections: &[String]) -> usize {
    sections.iter().map(String::len).sum::<usize>() + sections.len().saturating_sub(1)
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` within `bound` bytes, cut on a character boundary and marked.
fn clip(text: &str, bound: usize) -> String {
    let text = text.trim();
    if text.len() <= bound {
        return text.to_owned();
    }
    const MARK: &str = " […]";
    let mut end = bound.saturating_sub(MARK.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{MARK}", text[..end].trim_end())
}

/// Writes `text` as `agent`'s briefing, readable by its owner alone,
/// replacing the previous one. The project's runtime directory is marked
/// for the root it belongs to, so the sweep keeps it as long as the project
/// is there.
pub fn write(home: &UzeHome, project_root: &Path, agent: &str, text: &str) -> Result<PathBuf> {
    let canonical = uze_core::path::Canonical::canonical(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf());
    prepare_project_dir(home, &canonical)?;
    let path = home.briefing_path(&crate::conversation::project_id(project_root), agent);
    let directory = path.parent().expect("a briefing lives in a directory");
    uze_platform::fs::create_private_dir_all(directory).map_err(UzeError::write(directory))?;
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
    use crate::session::{LastCommand, Turn};

    fn handover() -> Handover<'static> {
        Handover {
            label: "branch naming",
            from: "Claude Code",
            to: "Codex",
            session: None,
            returning_since: None,
        }
    }

    fn turn(speaker: Speaker, text: &str) -> Turn {
        Turn {
            speaker,
            text: text.to_owned(),
            at_unix: None,
        }
    }

    fn git() -> GitFacts {
        GitFacts {
            branch: Some("fix/branch-naming".into()),
            base: Some("abc123".into()),
            commits: vec!["fix: name the branch".into()],
            diff_stat: " src/a.rs | 2 +-".into(),
            dirty: vec![" M src/b.rs".into()],
        }
    }

    #[test]
    fn without_a_transcript_the_briefing_is_git_alone_and_says_so() {
        let text = render(&handover(), &git(), None);
        assert!(text.contains("could not be read"), "{text}");
        assert!(text.contains("fix/branch-naming"), "{text}");
        assert!(text.contains("fix: name the branch"), "{text}");
        assert!(text.contains(" M src/b.rs"), "{text}");
    }

    #[test]
    fn a_digest_carries_the_goal_the_summary_and_what_was_said_last() {
        let digest = ConversationDigest {
            goal: Some("make branch names short".into()),
            summary: Some("renamed the helper".into()),
            turns: vec![
                turn(Speaker::Operator, "also cover the root case"),
                turn(Speaker::Agent, "the root case now names the label"),
            ],
            last_command: Some(LastCommand {
                line: "cargo test -p uze-workspace".into(),
                failed: Some(true),
            }),
        };
        let text = render(&handover(), &git(), Some(&digest));
        for expected in [
            "make branch names short",
            "renamed the helper",
            "also cover the root case",
            "the root case now names the label",
            "`cargo test -p uze-workspace` (failed)",
        ] {
            assert!(text.contains(expected), "{expected} missing from:\n{text}");
        }
    }

    #[test]
    fn a_conversation_far_larger_than_the_bound_keeps_the_goal_the_summary_and_the_latest_turns() {
        let digest = ConversationDigest {
            goal: Some("the goal".into()),
            summary: Some("s".repeat(10_000)),
            turns: (0..500)
                .flat_map(|n| {
                    [
                        turn(Speaker::Operator, &format!("ask {n} {}", "x".repeat(300))),
                        turn(Speaker::Agent, &format!("answer {n} {}", "y".repeat(3000))),
                    ]
                })
                .collect(),
            last_command: None,
        };
        let text = render(&handover(), &git(), Some(&digest));
        assert!(text.len() <= BOUND, "{} bytes", text.len());
        assert!(text.contains("the goal"));
        assert!(text.contains("sss"));
        assert!(text.contains("ask 499"), "the newest operator turn is kept");
        assert!(!text.contains("ask 0 "), "the oldest is the one cut");
        assert!(text.contains("answer 499"));
    }

    #[test]
    fn when_the_whole_does_not_fit_git_is_cut_before_the_goal() {
        let digest = ConversationDigest {
            goal: Some("g".repeat(GOAL_BOUND)),
            summary: Some("s".repeat(SUMMARY_BOUND)),
            turns: (0..3)
                .map(|n| turn(Speaker::Agent, &format!("{n}{}", "y".repeat(2000))))
                .chain((0..20).map(|n| turn(Speaker::Operator, &format!("{n}{}", "o".repeat(390)))))
                .collect(),
            last_command: Some(LastCommand {
                line: "c".repeat(COMMAND_BOUND),
                failed: None,
            }),
        };
        let mut git = git();
        git.commits = (0..COMMITS_LISTED)
            .map(|n| format!("{n} {}", "z".repeat(60)))
            .collect();
        let text = render(&handover(), &git, Some(&digest));
        assert!(text.len() <= BOUND, "{} bytes", text.len());
        assert!(text.contains("## Goal"));
        assert!(!text.contains("## Git"), "{text}");
    }

    #[test]
    fn a_returning_harness_is_told_it_is_returning_and_not_the_goal_again() {
        let mut handover = handover();
        handover.returning_since = Some(100);
        let digest = ConversationDigest {
            goal: Some("the goal".into()),
            ..ConversationDigest::default()
        };
        let text = render(&handover, &git(), Some(&digest));
        assert!(text.contains("You worked on this agent before"), "{text}");
        assert!(!text.contains("## Goal"), "{text}");
    }

    #[test]
    fn the_goal_is_listed_once_and_no_other_turn_is_taken_for_it() {
        let digest = ConversationDigest {
            goal: Some("the goal".into()),
            turns: vec![
                turn(Speaker::Operator, "the goal"),
                turn(Speaker::Operator, "the second ask"),
            ],
            ..ConversationDigest::default()
        };
        let text = render(&handover(), &git(), Some(&digest));
        assert_eq!(text.matches("the goal").count(), 1, "{text}");
        assert!(text.contains("- the second ask"), "{text}");
    }

    #[test]
    fn a_returning_harness_is_told_every_turn_since_it_left() {
        let mut handover = handover();
        handover.returning_since = Some(100);
        let digest = ConversationDigest {
            goal: Some("the goal".into()),
            turns: vec![turn(Speaker::Operator, "asked after you left")],
            ..ConversationDigest::default()
        };
        let text = render(&handover, &git(), Some(&digest));
        assert!(text.contains("- asked after you left"), "{text}");
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        assert_eq!(clip("ééééé", 9), "é […]");
    }

    #[test]
    fn a_briefing_is_written_outside_the_checkout_for_its_owner_alone() {
        let root = uze_testkit::temp::scratch("briefing-write");
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
        assert!(
            crate::harness_runtime::projection_root(path.parent().unwrap().parent().unwrap())
                .is_some(),
            "the sweep can tell whose it is"
        );
        let _ = fs::remove_dir_all(root);
    }
}
