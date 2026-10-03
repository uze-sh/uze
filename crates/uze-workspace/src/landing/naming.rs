//! The name a delivered branch carries, derived from its work when nobody gave one.

use super::*;

/// The name a branch is published under when nobody named the work.
///
/// Sourced from the first commit on the branch rather than from the task's
/// label, because the label of an unnamed task is its generated
/// identifier — and a pull request titled `agent/zulqgq` teaches a
/// reviewer nothing. The agent that wrote that commit is the only party
/// that held the intent, and its subject line is the one place that intent
/// was already written down.
///
/// A Conventional Commits subject gives up its type as the branch's own
/// (`feat(ui): one layout file` -> `feat/one-layout-file`); anything else
/// keeps the project's prefix. This is the net under every other
/// mechanism, not the mechanism: a named task never reaches it.
pub(super) fn readable_branch_name(primary: &Path, isolation: &Isolation) -> String {
    // The branch it already has: generated, so it is the prefix and the
    // agent's own identifier, which is exactly what the fallback is.
    let fallback = || isolation.branch.clone();
    let Some((kind, subject)) = commit_derived_halves(primary, isolation) else {
        return fallback();
    };
    match kind {
        Some(kind) => format!("{kind}/{subject}"),
        None => format!("{}{subject}", crate::worktree::BRANCH_PREFIX),
    }
}

/// The name the work would take from its own first commit, judged against
/// what the project accepts — `None` when nothing usable can be derived.
///
/// This is the automatic half of naming, and it is deliberately the
/// *later* half: it runs once the work has a commit, because until then
/// there is nothing to name it after. The agent naming its own work
/// arrives earlier and therefore wins, which is the whole of the
/// precedence rule — no ladder, no overwriting.
///
/// Judged rather than trusted: a derived name that the declared vocabulary
/// would refuse from an agent is not one UZE may write behind its back, so
/// a project whose types the commit does not match keeps the generated
/// name and says nothing.
pub fn derived_name(
    primary: &Path,
    isolation: &Isolation,
    vocabulary: &crate::worktree::BranchVocabulary,
) -> Option<String> {
    let (kind, subject) = commit_derived_halves(primary, isolation)?;
    let proposed = match kind {
        Some(kind) => format!("{kind}/{subject}"),
        None => subject,
    };
    vocabulary.accept(&proposed).ok()
}

/// The type and subject the branch's first commit yields, if any. A
/// Conventional Commits subject gives up its type (`feat(ui): one layout
/// file` -> `feat` + `one-layout-file`); anything else yields a subject
/// alone.
pub(super) fn commit_derived_halves(
    primary: &Path,
    isolation: &Isolation,
) -> Option<(Option<String>, String)> {
    let subject = first_commit_subject(primary, isolation)?;
    let (kind, rest) = match subject.split_once(':') {
        Some((head, rest)) if !head.contains(' ') => {
            let kind = head.split('(').next().unwrap_or(head).trim_end_matches('!');
            (Some(slug(kind)).filter(|kind| !kind.is_empty()), rest)
        }
        _ => (None, subject.as_str()),
    };
    let subject =
        crate::worktree::cut_at_word_boundary(&slug(rest), crate::worktree::SUBJECT_MAX_CHARS);
    (!subject.is_empty()).then_some((kind, subject))
}

/// The subject of the oldest commit the branch carries beyond its base.
pub(super) fn first_commit_subject(primary: &Path, isolation: &Isolation) -> Option<String> {
    let range = format!("{}..{}", isolation.base_commit, isolation.branch);
    let listing = uze_git::read(primary, &["log", "--format=%s", "--reverse", &range, "--"])
        .ok()?
        .successful()
        .ok()?;
    listing
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Lowercase, non-alphanumerics collapsed to single hyphens, trimmed.
pub(super) fn slug(text: &str) -> String {
    let mut slug = String::new();
    let mut pending = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            if pending && !slug.is_empty() {
                slug.push('-');
            }
            pending = false;
            slug.extend(character.to_lowercase());
        } else {
            pending = true;
        }
    }
    slug.trim_matches('-').to_owned()
}
