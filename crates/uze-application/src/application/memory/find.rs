//! Finding facts by the words a question holds.
//!
//! What an agent reaches when the index named a fact without saying what
//! it holds, or did not name it at all. Words, not patterns: an agent types
//! this into whatever shell its harness gives it, and a pattern means
//! something different in each of them.
//!
//! Every source the index reads is searched, and nothing it leaves out:
//! a proposed machine fact reaches no agent until the operator accepts it,
//! and a document that does not read is counted, never searched.

use std::fs;

use uze_core::memory::{FactKind, MemoryEntry, Scope, UZE_ORIGIN, Unreadable};
use uze_core::skill::split_frontmatter;

use super::{MemoryReport, words::fold};

/// The most facts one answer lists: past it, the words are too broad to
/// be worth reading through.
const MATCH_LIMIT: usize = 20;
/// The most lines of a body one fact quotes.
const BODY_HITS: usize = 3;
/// The most bytes one quoted line keeps.
const HIT_BYTES: usize = 200;

/// What a search found.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Found {
    pub words: Vec<String>,
    /// Best first: the facts holding the most of the words, then the most
    /// of them in the name, then in the description, then by kind and
    /// recency as the index ranks them.
    pub matches: Vec<Match>,
    /// How many facts matched, listed or not.
    pub total: usize,
    pub truncated: bool,
    pub unreadable: Vec<Unreadable>,
}

/// One fact the words were found in.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Match {
    #[serde(flatten)]
    pub entry: MemoryEntry,
    /// Where each word was found, quoted.
    pub hits: Vec<Hit>,
    /// The command that reads the fact in full.
    pub show: String,
}

/// A word found in one field of a fact.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Hit {
    /// `name`, `description` or `body`.
    pub field: &'static str,
    /// The line of the file, for a body.
    pub line: Option<usize>,
    pub text: String,
}

pub(super) fn find(report: &MemoryReport, words: &[String], kind: Option<FactKind>) -> Found {
    let words: Vec<String> = words
        .iter()
        .flat_map(|word| word.split_whitespace())
        .map(str::to_lowercase)
        .collect();
    let mut scored: Vec<(Score, Match)> = report
        .entries
        .iter()
        .filter(|entry| kind.is_none_or(|kind| entry.kind == kind))
        .filter_map(|entry| matched(entry, &words))
        .collect();
    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| uze_core::memory::by_rank(&left.entry, &right.entry))
    });
    let total = scored.len();
    Found {
        words,
        matches: scored
            .into_iter()
            .take(MATCH_LIMIT)
            .map(|(_, found)| found)
            .collect(),
        total,
        truncated: total > MATCH_LIMIT,
        unreadable: report.unreadable.clone(),
    }
}

/// How well a fact holds the words: how many it holds, then how many of
/// those its name holds, then its description. Compared highest first.
type Score = (usize, usize, usize);

/// The fact, its hits and its score, when it holds any of the words. No
/// words lists every fact, with no hits.
fn matched(entry: &MemoryEntry, words: &[String]) -> Option<(Score, Match)> {
    let folded: Vec<String> = words
        .iter()
        .map(|word| fold(word))
        .filter(|word| !word.is_empty())
        .collect();
    let found = |text: &str| -> Vec<usize> {
        let text = fold(text);
        (0..folded.len())
            .filter(|&index| text.contains(folded[index].as_str()))
            .collect()
    };
    let mut held: Vec<usize> = Vec::new();
    let mut in_name: Vec<usize> = Vec::new();
    let mut in_description: Vec<usize> = Vec::new();
    let mut hits = Vec::new();
    if !folded.is_empty() {
        for (field, text) in [("name", &entry.name), ("description", &entry.description)] {
            let in_field = found(text);
            if !in_field.is_empty() {
                hits.push(Hit {
                    field,
                    line: None,
                    text: quoted(text),
                });
                held.extend(&in_field);
                match field {
                    "name" => in_name = in_field,
                    _ => in_description = in_field,
                }
            }
        }
        let text = fs::read_to_string(&entry.path).unwrap_or_default();
        for (line, text) in body_lines(&text) {
            let in_line = found(text);
            if in_line.is_empty() {
                continue;
            }
            held.extend(in_line);
            if hits.iter().filter(|hit| hit.field == "body").count() < BODY_HITS {
                hits.push(Hit {
                    field: "body",
                    line: Some(line),
                    text: quoted(text),
                });
            }
        }
        if held.is_empty() {
            return None;
        }
    }
    held.sort_unstable();
    held.dedup();
    Some((
        (held.len(), in_name.len(), in_description.len()),
        Match {
            entry: entry.clone(),
            hits,
            show: show_command(entry),
        },
    ))
}

/// The lines of a document past its front matter, numbered as the file
/// numbers them.
fn body_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let skipped = split_frontmatter(text)
        .map(|(head, _)| head.lines().count() + 2)
        .unwrap_or(0);
    text.lines()
        .enumerate()
        .skip(skipped)
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
}

fn quoted(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.len() <= HIT_BYTES {
        return line;
    }
    let mut end = HIT_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &line[..end])
}

fn show_command(entry: &MemoryEntry) -> String {
    let machine = entry.origin == UZE_ORIGIN && entry.scope == Scope::Machine;
    format!(
        "uze agent memory show {}{}",
        entry.name,
        if machine { " -m" } else { "" }
    )
}
