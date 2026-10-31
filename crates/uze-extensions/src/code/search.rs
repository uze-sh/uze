//! Searching every file of the checkout: the field typed into, what it
//! found, and the list that shows it — a file per heading, with how many
//! lines it was found on, and each line with what matched set apart.
//!
//! Git does the finding (`git grep`), off the drawing thread like every
//! other read here; this module keeps the question, reads the answer and
//! draws the list. A search runs as it is typed: each keystroke asks
//! again, and an answer to anything but the text in the field now is
//! dropped on arrival.

use std::{
    collections::BTreeSet,
    ops::Range,
    path::{Path, PathBuf},
};

use crate::view::{Field, NavigatorRow, Role, Span};

/// How many lines one search reports before it stops. A search wide
/// enough to pass it is one to narrow, and the list says so.
pub(super) const LIMIT: usize = 2000;

/// The ground a match is set on, in the list and in the file.
const MATCH: Role = Role::Warning;

/// How much of a found line is kept before its first match: enough to
/// read what it is in, without pushing the match off a narrow column.
const LEAD: usize = 16;

/// One line a search found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Hit {
    /// Counted from one, as the file's line numbers are.
    pub(super) line: usize,
    pub(super) text: String,
    /// The line as the list draws it, made once when the answer lands
    /// rather than on every frame.
    spans: Vec<Span>,
}

/// The lines found in one file, in the order the file has them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FoundFile {
    pub(super) path: PathBuf,
    /// Its name and its directory, as the list heads its lines with.
    name: String,
    directory: String,
    pub(super) hits: Vec<Hit>,
}

/// What a search found: every file with a line that matched, and whether
/// it stopped at [`LIMIT`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Findings {
    pub(super) files: Vec<FoundFile>,
    pub(super) truncated: bool,
}

/// Reads `git grep -z -n --column` output: `path\0line\0column\0text`
/// per line, the path relative to `root`.
pub(super) fn parse(root: &Path, output: &str) -> Findings {
    let mut findings = Findings::default();
    let mut count = 0;
    for record in output.lines() {
        let mut fields = record.splitn(4, '\0');
        let (Some(path), Some(line), Some(_column), Some(text)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(line) = line.parse::<usize>() else {
            continue;
        };
        if count == LIMIT {
            findings.truncated = true;
            break;
        }
        count += 1;
        let path = root.join(path);
        let hit = Hit {
            line,
            text: text.trim_end_matches('\r').to_owned(),
            spans: Vec::new(),
        };
        match findings.files.last_mut() {
            Some(file) if file.path == path => file.hits.push(hit),
            _ => findings.files.push(FoundFile {
                name: super::file_name(&path),
                directory: super::relative_dir(root, &path),
                path,
                hits: vec![hit],
            }),
        }
    }
    findings
}

/// The search half's state.
#[derive(Debug, Default)]
pub(super) struct Search {
    /// What is in the field.
    pub(super) query: String,
    /// Whether keys land in the field rather than on the list.
    pub(super) typing: bool,
    /// The query the findings below answer, once one has.
    answered: Option<String>,
    findings: Findings,
    error: Option<String>,
    /// Files whose lines are folded away.
    folded: BTreeSet<PathBuf>,
    /// The line the keyboard is on, as (file, line) indices.
    pub(super) cursor: Option<(usize, usize)>,
}

impl Search {
    /// Whether a search is still to be answered for what is typed.
    pub(super) fn pending(&self) -> bool {
        !self.query.is_empty() && self.answered.as_deref() != Some(self.query.as_str())
    }

    /// Takes the answer to `query`, unless the field has moved on since.
    pub(super) fn absorb(&mut self, query: &str, outcome: Result<Findings, String>) {
        if query != self.query {
            return;
        }
        self.answered = Some(query.to_owned());
        self.folded.clear();
        self.cursor = None;
        match outcome {
            Ok(mut findings) => {
                for hit in findings.files.iter_mut().flat_map(|file| &mut file.hits) {
                    hit.spans = marked(&hit.text, query);
                }
                self.findings = findings;
                self.error = None;
            }
            Err(message) => {
                self.findings = Findings::default();
                self.error = Some(message);
            }
        }
    }

    /// What a new query leaves: nothing found yet, and nothing to show
    /// for an empty one.
    pub(super) fn typed(&mut self) {
        if self.query.is_empty() {
            self.answered = None;
            self.findings = Findings::default();
            self.error = None;
            self.cursor = None;
        }
    }

    pub(super) fn files(&self) -> &[FoundFile] {
        &self.findings.files
    }

    /// The file and line `cursor` is on.
    pub(super) fn at(&self, (file, hit): (usize, usize)) -> Option<(&Path, usize)> {
        let found = self.findings.files.get(file)?;
        let hit = found.hits.get(hit)?;
        Some((found.path.as_path(), hit.line))
    }

    /// Every line on show in the list, in order, as (file, line) indices:
    /// what the arrows walk.
    fn walk(&self) -> Vec<(usize, usize)> {
        self.findings
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| !self.folded.contains(&file.path))
            .flat_map(|(index, file)| (0..file.hits.len()).map(move |hit| (index, hit)))
            .collect()
    }

    /// Moves the cursor one line along the list, from the first line when
    /// it is nowhere yet.
    pub(super) fn step(&mut self, down: bool) {
        let walk = self.walk();
        let next = match self
            .cursor
            .and_then(|at| walk.iter().position(|step| *step == at))
        {
            None => walk.first(),
            Some(index) if down => walk.get(index + 1).or(walk.get(index)),
            Some(index) => walk.get(index.saturating_sub(1)),
        };
        self.cursor = next.copied();
    }

    /// The id a [`NavigatorRow::Match`] hands back, as (file, line).
    pub(super) fn hit_of(&self, id: usize) -> Option<(usize, usize)> {
        let mut first = 0;
        for (index, file) in self.findings.files.iter().enumerate() {
            if id < first + file.hits.len() {
                return Some((index, id - first));
            }
            first += file.hits.len();
        }
        None
    }

    /// Folds `file`'s lines away, or back out.
    pub(super) fn toggle(&mut self, file: usize) {
        let Some(path) = self.findings.files.get(file).map(|file| file.path.clone()) else {
            return;
        };
        if !self.folded.remove(&path) {
            self.folded.insert(path);
            if self.cursor.is_some_and(|(at, _)| at == file) {
                self.cursor = None;
            }
        }
    }

    /// Folds the file the cursor is in, or unfolds it.
    pub(super) fn fold_cursor(&mut self, open: bool) {
        let Some((file, _)) = self.cursor else {
            return;
        };
        let folded = self
            .findings
            .files
            .get(file)
            .is_some_and(|found| self.folded.contains(&found.path));
        if folded == open {
            self.toggle(file);
        }
    }

    /// The field over the list, saying what the text found.
    pub(super) fn field(&self, focused: bool) -> Field {
        let lines: usize = self.findings.files.iter().map(|file| file.hits.len()).sum();
        let summary = if self.query.is_empty() {
            Span::new("", Role::Muted)
        } else if self.pending() {
            Span::new("searching…", Role::Muted)
        } else if let Some(message) = &self.error {
            Span::new(message.clone(), Role::Danger)
        } else if lines == 0 {
            Span::new("no results", Role::Muted)
        } else {
            Span::new(
                format!(
                    "{lines}{} {} in {} {}",
                    if self.findings.truncated { "+" } else { "" },
                    plural(lines, "result", "results"),
                    self.findings.files.len(),
                    plural(self.findings.files.len(), "file", "files"),
                ),
                Role::Muted,
            )
        };
        Field {
            text: self.query.clone(),
            placeholder: "Search".to_owned(),
            focused: focused && self.typing,
            summary,
        }
    }

    /// The list: each file, then each of its lines unless it is folded.
    pub(super) fn rows(&self) -> Vec<NavigatorRow> {
        let mut rows = Vec::new();
        let mut id = 0;
        for (index, file) in self.findings.files.iter().enumerate() {
            let collapsed = self.folded.contains(&file.path);
            rows.push(NavigatorRow::Found {
                id: index,
                icon: super::files::icon_for(&file.name, false, false),
                name: file.name.clone(),
                detail: file.directory.clone(),
                count: file.hits.len(),
                collapsed,
            });
            if !collapsed {
                rows.extend(file.hits.iter().enumerate().map(|(hit_index, hit)| {
                    NavigatorRow::Match {
                        id: id + hit_index,
                        spans: hit.spans.clone(),
                        selected: self.cursor == Some((index, hit_index)),
                    }
                }));
            }
            id += file.hits.len();
        }
        rows
    }

    /// The row the cursor is on, for the list to keep in sight.
    pub(super) fn anchor(&self) -> Option<usize> {
        let (file, hit) = self.cursor?;
        let mut row = 0;
        for (index, found) in self.findings.files.iter().enumerate() {
            row += 1;
            if index == file {
                return Some(row + hit);
            }
            if !self.folded.contains(&found.path) {
                row += found.hits.len();
            }
        }
        None
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    match count {
        1 => one,
        _ => many,
    }
}

/// Where `query` occurs in `text`, ignoring case, as character ranges,
/// none overlapping.
///
/// Compared on one lowercased copy of the text, each of its characters
/// remembering the character of `text` it came from — a few characters
/// lowercase to more than one, and a range has to name `text`'s.
pub(super) fn occurrences(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut lowered = Vec::with_capacity(text.len());
    let mut from = Vec::with_capacity(text.len());
    for (index, character) in text.chars().enumerate() {
        for lower in character.to_lowercase() {
            lowered.push(lower);
            from.push(index);
        }
    }
    let mut found = Vec::new();
    let mut at = 0;
    while at + needle.len() <= lowered.len() {
        // A match starts where a character of `text` starts.
        let starts = at == 0 || from[at] != from[at - 1];
        if starts && lowered[at..at + needle.len()] == needle[..] {
            let end = at + needle.len();
            let last = from[end - 1] + 1;
            found.push(from[at]..last);
            at = end;
        } else {
            at += 1;
        }
    }
    found
}

/// `spans` with the characters in `ranges` — sorted, as [`occurrences`]
/// gives them — set on the ground a match is drawn on, each span cut where
/// a range starts or ends.
pub(super) fn grounded(spans: Vec<Span>, ranges: &[Range<usize>]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut ranges = ranges.iter().peekable();
    let mut at = 0;
    for span in spans {
        let mut piece = String::new();
        let mut piece_in = None;
        for character in span.text.chars() {
            while ranges.next_if(|range| range.end <= at).is_some() {}
            let here = ranges.peek().is_some_and(|range| range.contains(&at));
            if piece_in.is_some_and(|was| was != here) {
                out.push(cut(
                    &span,
                    std::mem::take(&mut piece),
                    piece_in == Some(true),
                ));
            }
            piece_in = Some(here);
            piece.push(character);
            at += 1;
        }
        if !piece.is_empty() {
            out.push(cut(&span, piece, piece_in == Some(true)));
        }
    }
    out
}

fn cut(span: &Span, text: String, found: bool) -> Span {
    Span {
        text,
        ground: match found {
            true => Some(MATCH),
            false => span.ground,
        },
        ..span.clone()
    }
}

/// `text` as spans, every occurrence of `query` on a ground of its own —
/// kept from a little before the first one, so a match far along a long
/// line is still on the row.
pub(super) fn marked(text: &str, query: &str) -> Vec<Span> {
    let indent = text
        .chars()
        .take_while(|character| character.is_whitespace())
        .count();
    let found = occurrences(text, query);
    let from = match found.first() {
        Some(first) if first.start > indent + LEAD => first.start - LEAD,
        _ => indent,
    };
    let shifted: Vec<Range<usize>> = found
        .iter()
        .filter(|range| range.end > from)
        .map(|range| range.start.saturating_sub(from)..range.end - from)
        .collect();
    let kept = vec![Span::new(
        text.chars().skip(from).collect::<String>(),
        Role::Default,
    )];
    let mut spans = Vec::new();
    if from > indent {
        spans.push(Span::new("…", Role::Muted));
    }
    spans.extend(grounded(kept, &shifted));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_greps_records_become_files_of_lines() {
        let output = "src/a.rs\u{0}3\u{0}5\u{0}let foo = 1;\nsrc/a.rs\u{0}9\u{0}1\u{0}foo()\nb.txt\u{0}1\u{0}1\u{0}FOO\r\n";
        let found = parse(Path::new("/w"), output);
        assert_eq!(found.files.len(), 2);
        assert_eq!(found.files[0].path, Path::new("/w/src/a.rs"));
        let hits: Vec<(usize, &str)> = found.files[0]
            .hits
            .iter()
            .map(|hit| (hit.line, hit.text.as_str()))
            .collect();
        assert_eq!(hits, [(3, "let foo = 1;"), (9, "foo()")]);
        assert_eq!(
            (
                found.files[0].name.as_str(),
                found.files[0].directory.as_str()
            ),
            ("a.rs", "src")
        );
        assert_eq!(found.files[1].hits[0].text, "FOO", "a CR is not text");
        assert!(!found.truncated);
    }

    #[test]
    fn a_search_past_the_limit_says_it_stopped() {
        let output = "a\u{0}1\u{0}1\u{0}x\n".repeat(LIMIT + 1);
        let found = parse(Path::new("/w"), &output);
        assert!(found.truncated);
        assert_eq!(found.files[0].hits.len(), LIMIT);
    }

    #[test]
    fn every_occurrence_is_found_whatever_its_case() {
        assert_eq!(occurrences("Fog fog FOG", "fog"), [0..3, 4..7, 8..11]);
        assert_eq!(occurrences("aaa", "aa"), [0..2], "not overlapping");
        assert!(occurrences("abc", "").is_empty());
    }

    #[test]
    fn a_found_line_sets_its_matches_apart_and_drops_its_indent() {
        let spans = marked("    let fog = 1;", "fog");
        let texts: Vec<(&str, Option<Role>)> = spans
            .iter()
            .map(|span| (span.text.as_str(), span.ground))
            .collect();
        assert_eq!(
            texts,
            [
                ("let ", None),
                ("fog", Some(Role::Warning)),
                (" = 1;", None)
            ]
        );
    }

    #[test]
    fn a_match_far_along_a_line_is_brought_onto_the_row() {
        let line = format!("{}needle", "x".repeat(100));
        let spans = marked(&line, "needle");
        assert_eq!(spans[0].text, "…");
        assert_eq!(spans.last().unwrap().text, "needle");
    }

    #[test]
    fn an_answer_to_an_older_query_is_dropped() {
        let mut search = Search {
            query: "fog".to_owned(),
            ..Search::default()
        };
        search.absorb("fo", Ok(Findings::default()));
        assert!(search.pending(), "still waiting for fog");
        search.absorb("fog", Ok(Findings::default()));
        assert!(!search.pending());
    }
}
