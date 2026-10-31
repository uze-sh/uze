//! A path a pane's program wrote, found under the pointer: ctrl+click opens
//! it in the code surface, and the pointer resting on it with ctrl held
//! underlines it, as a terminal's own links are.
//!
//! Found in the cells the client already holds, so finding one is a scan
//! of one row, whatever program wrote it: nothing here asks a harness for
//! anything. Whether it names something on disk is asked on a thread
//! (`reads::spawn_path_check`) and remembered for a moment, so a pointer
//! moving along one path asks once.

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};
use uze_terminal::{PaneId, RenderCell};

/// What stands in a row for the cell a wide character spills into.
const SPILL: char = '\0';

/// How long an answer about a path holds: long enough that a pointer
/// moving along it asks once, short enough that a file a program just
/// wrote and named becomes a link without the person doing anything.
const REMEMBERED: Duration = Duration::from_secs(3);

/// Answers kept at most; past it they are all forgotten at once, which
/// costs a few asks again and never a wrong answer.
const REMEMBERED_AT_MOST: usize = 128;

/// A run of one row's cells that reads as a path, `start..end` in columns,
/// with any `:line:column` after it included in `text` and in the run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Written {
    pub(super) row: u16,
    pub(super) start: u16,
    pub(super) end: u16,
    pub(super) text: String,
}

/// A path written in `pane`, resolved to what it names and to the checkout
/// the code surface opens on to show it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PaneLink {
    pub(super) pane: PaneId,
    pub(super) written: Written,
    pub(super) target: PathBuf,
    pub(super) root: PathBuf,
}

impl PaneLink {
    pub(super) fn covers(&self, pane: PaneId, row: u16, column: u16) -> bool {
        self.pane == pane
            && self.written.row == row
            && (self.written.start..self.written.end).contains(&column)
    }
}

/// The links of the panes: the one drawn, the ones being asked about, and
/// what was answered lately.
#[derive(Default)]
pub(super) struct PaneLinks {
    /// The link under the pointer with ctrl held, drawn underlined.
    pub(super) hovered: Option<PaneLink>,
    /// The link under the pointer whose path is still being asked about.
    waiting: Option<PaneLink>,
    /// A ctrl+click on a path still being asked about, held back from the
    /// pane's program until the answer says whether it opens the path or
    /// is the program's after all.
    opening: Option<HeldClick>,
    asked: HashSet<PathBuf>,
    answered: HashMap<PathBuf, (Instant, bool)>,
}

/// What became of a pointer resting on a link, or a click on one.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Pointed {
    /// Its path is there.
    Found,
    /// Its path is not, or not inside a checkout the code surface shows.
    Missing,
    /// Not known yet: `Some` is the path to ask about, `None` when it is
    /// already being asked.
    Asking(Option<PathBuf>),
}

/// A ctrl+click held back while its path is asked about: the release, and
/// the pane's area it was resolved against, to hand to the program when
/// the path is not there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct HeldClick {
    pub(super) link: PaneLink,
    pub(super) release: MouseEvent,
    pub(super) area: Rect,
}

/// What an answer settles about a held click.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Settled {
    /// Its path is there: open it.
    Open(PaneLink),
    /// Its path is not: the click goes to the program it was held from.
    Release(HeldClick),
}

/// What a ctrl+click on a path comes to.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Clicked {
    /// Its path is there: open it.
    Open(PaneLink),
    /// Not known yet: held until the answer settles it. `Some` is the path
    /// to ask about.
    Waiting(Option<PathBuf>),
    /// Its path is not there: the click is the program's.
    NotALink,
}

impl PaneLinks {
    /// The pointer rests on `link` with ctrl held. Answers whether what is
    /// drawn changed, and the path to ask about when one must be.
    pub(super) fn hover(
        &mut self,
        link: Option<PaneLink>,
        now: Instant,
    ) -> (bool, Option<PathBuf>) {
        let before = self.hovered.clone();
        self.waiting = None;
        let ask = match link {
            None => {
                self.hovered = None;
                None
            }
            Some(link) => match self.point(&link, now) {
                Pointed::Found => {
                    self.hovered = Some(link);
                    None
                }
                Pointed::Missing => {
                    self.hovered = None;
                    None
                }
                Pointed::Asking(ask) => {
                    // The underline already drawn stays while the pointer
                    // is still on its run: the same path, asked again only
                    // because what was answered about it aged.
                    if before.as_ref().map(|drawn| &drawn.written) != Some(&link.written) {
                        self.hovered = None;
                    }
                    self.waiting = Some(link);
                    ask
                }
            },
        };
        (before != self.hovered, ask)
    }

    /// A ctrl+click on `click.link`. Only one click is held at a time: the
    /// one before must have been let go of (`let_go`).
    pub(super) fn click(&mut self, click: HeldClick, now: Instant) -> Clicked {
        match self.point(&click.link, now) {
            Pointed::Found => Clicked::Open(click.link),
            Pointed::Missing => Clicked::NotALink,
            Pointed::Asking(ask) => {
                self.opening = Some(click);
                Clicked::Waiting(ask)
            }
        }
    }

    /// The held click, given up on: the person moved on before its answer
    /// came, so it is the program's rather than a surface opening later
    /// over whatever they turned to.
    pub(super) fn let_go(&mut self) -> Option<HeldClick> {
        self.opening.take()
    }

    /// What was found about `target`. Answers whether what is drawn
    /// changed, and what it settles about the click held for it.
    pub(super) fn absorb(
        &mut self,
        target: PathBuf,
        found: bool,
        now: Instant,
    ) -> (bool, Option<Settled>) {
        self.asked.remove(&target);
        if self.answered.len() >= REMEMBERED_AT_MOST {
            self.answered.clear();
        }
        self.answered.insert(target.clone(), (now, found));
        let mut drawn = false;
        if self
            .waiting
            .as_ref()
            .is_some_and(|link| link.target == target)
        {
            let link = self.waiting.take();
            if found {
                drawn = self.hovered != link;
                self.hovered = link;
            } else {
                drawn = self.hovered.take().is_some();
            }
        }
        let settled = match self.opening.take() {
            Some(click) if click.link.target == target => Some(if found {
                Settled::Open(click.link)
            } else {
                Settled::Release(click)
            }),
            other => {
                self.opening = other;
                None
            }
        };
        (drawn, settled)
    }

    /// Forgets the link drawn and the ones asked about for the pointer,
    /// when it leaves the pane or ctrl is let go. Answers whether anything
    /// drawn went.
    pub(super) fn leave(&mut self) -> bool {
        self.waiting = None;
        self.hovered.take().is_some()
    }

    /// Whether any path is still being asked about.
    #[cfg(test)]
    pub(super) fn is_asking(&self) -> bool {
        !self.asked.is_empty()
    }

    fn point(&mut self, link: &PaneLink, now: Instant) -> Pointed {
        match self.answered.get(&link.target) {
            Some((at, found)) if now.duration_since(*at) < REMEMBERED => {
                if *found {
                    Pointed::Found
                } else {
                    Pointed::Missing
                }
            }
            _ if self.asked.insert(link.target.clone()) => {
                Pointed::Asking(Some(link.target.clone()))
            }
            _ => Pointed::Asking(None),
        }
    }
}

/// The path written at `column` of `row`, in a grid `columns` wide.
pub(super) fn written_at(
    cells: &[RenderCell],
    columns: u16,
    row: u16,
    column: u16,
) -> Option<Written> {
    let width = usize::from(columns);
    let from = usize::from(row) * width;
    let line = cells.get(from..from + width)?;
    let at = usize::from(column);
    if !line.get(at).is_some_and(|cell| in_path(cell.character)) {
        return None;
    }
    let mut start = at;
    while start > 0 && in_path(line[start - 1].character) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < line.len() && in_path(line[end].character) {
        end += 1;
    }
    // A run against the row's edge may go on in the row beside it, and a
    // half of a path names some other path: rather no link than the
    // wrong file.
    let runs_on_from_above = start == 0
        && row > 0
        && cells
            .get(from - 1)
            .is_some_and(|cell| in_path(cell.character));
    let runs_on_below = end == width
        && cells
            .get(from + width)
            .is_some_and(|cell| in_path(cell.character));
    if runs_on_from_above || runs_on_below {
        return None;
    }
    // Punctuation a sentence ends with, or a label's colon, is the
    // sentence's: `see src/main.rs.` names `src/main.rs`.
    while end > start && matches!(line[end - 1].character, '.' | ':' | SPILL) {
        end -= 1;
    }
    while start < end && line[start].character == ':' {
        start += 1;
    }
    end = start + cells_holding(&line[start..end], located_length(&line[start..end]));
    if !(start..end).contains(&at) {
        return None;
    }
    let text: String = line[start..end]
        .iter()
        .map(|cell| cell.character)
        .filter(|character| *character != SPILL)
        .collect();
    if !reads_as_path(&text) {
        return None;
    }
    Some(Written {
        row,
        start: u16::try_from(start).ok()?,
        end: u16::try_from(end).ok()?,
        text,
    })
}

/// How many characters of a run its path and location take: a search
/// prints `src/main.rs:12:fn main`, whose match after the line is not the
/// path's, though nothing but the colon parts them.
fn located_length(run: &[RenderCell]) -> usize {
    let text: Vec<char> = run
        .iter()
        .map(|cell| cell.character)
        .filter(|character| *character != SPILL)
        .collect();
    let digits_from = |from: usize| {
        from + text[from.min(text.len())..]
            .iter()
            .take_while(|character| character.is_ascii_digit())
            .count()
    };
    for (colon, _) in text.iter().enumerate().filter(|(_, c)| **c == ':') {
        let line_end = digits_from(colon + 1);
        if line_end == colon + 1 {
            continue;
        }
        if line_end == text.len() {
            return line_end;
        }
        if text[line_end] == ':' {
            let column_end = digits_from(line_end + 1);
            return if column_end > line_end + 1 {
                column_end
            } else {
                line_end
            };
        }
    }
    text.len()
}

/// The cells of `run` that hold its first `characters` characters, a
/// wide character's spill included.
fn cells_holding(run: &[RenderCell], characters: usize) -> usize {
    let mut seen = 0;
    run.iter()
        .take_while(|cell| {
            if cell.character != SPILL {
                seen += 1;
            }
            seen <= characters
        })
        .count()
}

/// What `text` names, from a program standing in `cwd`: a `:line:column`
/// after it is where in the file, not part of its name, and `~` is the
/// person's home.
pub(super) fn resolve(text: &str, cwd: &Path, home: &Path) -> PathBuf {
    let named = without_location(text);
    let path = match named.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) if rest.starts_with(['/', '\\']) => home.join(&rest[1..]),
        _ => cwd.join(named),
    };
    normalized(&path)
}

/// The first of `roots` that holds `target`: the code surface shows a
/// checkout, and a path outside every one on screen is not a link.
pub(super) fn root_of(target: &Path, roots: &[&Path]) -> Option<PathBuf> {
    roots
        .iter()
        .find(|root| target.starts_with(root))
        .map(|root| root.to_path_buf())
}

/// Whether `character` can stand in a path a program wrote. Brackets,
/// quotes and spaces cannot: they are what a path is written between.
fn in_path(character: char) -> bool {
    character.is_alphanumeric()
        || matches!(
            character,
            '/' | '\\' | '.' | '-' | '_' | '~' | ':' | '@' | '+' | '%' | SPILL
        )
}

/// Whether a run is worth asking the disk about: a word with no separator
/// and no extension is a word, and a URL is not a path.
fn reads_as_path(text: &str) -> bool {
    let named = without_location(text);
    let separated = named.contains(['/', '\\']);
    let has_extension = named
        .rsplit_once('.')
        .is_some_and(|(stem, extension)| !stem.is_empty() && !extension.is_empty());
    !text.contains("://")
        && named.chars().any(char::is_alphanumeric)
        && (separated || has_extension)
}

/// `text` without a `:line` or `:line:column` after it. A drive's colon
/// (`C:\`) is followed by a separator, never by digits alone, so it stays.
fn without_location(text: &str) -> &str {
    let mut named = text;
    for _ in 0..2 {
        match named.rsplit_once(':') {
            Some((head, digits))
                if !head.is_empty()
                    && !digits.is_empty()
                    && digits.chars().all(|character| character.is_ascii_digit()) =>
            {
                named = head;
            }
            _ => break,
        }
    }
    named
}

/// `path` with its `.` and `..` worked out on the names alone: asking the
/// disk would follow links to somewhere outside the checkout it was
/// written in.
fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(text: &str, columns: u16) -> Vec<RenderCell> {
        let mut cells = vec![crate::ui::orchestrator::render::blank_cell(); usize::from(columns)];
        for (cell, character) in cells.iter_mut().zip(text.chars()) {
            cell.character = character;
        }
        cells
    }

    fn found(text: &str, column: u16) -> Option<String> {
        written_at(&row(text, 80), 80, 0, column).map(|written| written.text)
    }

    #[test]
    fn a_path_in_a_sentence_is_the_path_alone() {
        let line = "filtro em crates/uze-platform/src/seccomp.rs.";
        let at = line.find("seccomp").unwrap() as u16;
        assert_eq!(
            found(line, at).as_deref(),
            Some("crates/uze-platform/src/seccomp.rs")
        );
        let written = written_at(&row(line, 80), 80, 0, at).unwrap();
        assert_eq!(written.start, line.find("crates").unwrap() as u16);
        assert_eq!(
            usize::from(written.end),
            line.len() - 1,
            "the full stop is the sentence's"
        );
    }

    #[test]
    fn a_path_between_brackets_or_quotes_is_inside_them() {
        assert_eq!(found("(src/main.rs)", 3).as_deref(), Some("src/main.rs"));
        assert_eq!(found("`src/ui.rs:12`", 3).as_deref(), Some("src/ui.rs:12"));
    }

    #[test]
    fn a_match_a_search_printed_is_its_path_and_line() {
        let line = "src/main.rs:12:fn main() {}";
        assert_eq!(found(line, 2).as_deref(), Some("src/main.rs:12"));
        assert_eq!(found(line, 16), None, "the matched text is not the path");
        assert_eq!(
            found("src/a.rs:3:7:let x", 2).as_deref(),
            Some("src/a.rs:3:7")
        );
    }

    /// A path wider than what is left of its row runs on to the next one,
    /// and either half alone names something else: `src/lib.rs` at the
    /// head of the second row is not the file the whole names.
    #[test]
    fn a_path_running_on_to_the_next_row_is_not_read_in_halves() {
        let mut cells = row("see /repo/crates", 16);
        cells.extend(row("/src/lib.rs now", 16));
        assert_eq!(written_at(&cells, 16, 1, 3), None);
        assert_eq!(written_at(&cells, 16, 0, 8), None);
        let mut cells = row("see /repo/a.rs  ", 16);
        cells.extend(row("/src/lib.rs now", 16));
        assert_eq!(
            written_at(&cells, 16, 1, 3)
                .map(|written| written.text)
                .as_deref(),
            Some("/src/lib.rs"),
            "a row that ends in a space ends what it wrote"
        );
    }

    #[test]
    fn words_and_urls_are_not_paths() {
        assert_eq!(found("hello world", 2), None);
        assert_eq!(found("see https://uze.sh/docs now", 8), None);
        assert_eq!(found("a ... b", 3), None);
    }

    #[test]
    fn a_file_name_with_an_extension_is_asked_about() {
        assert_eq!(found("edit README.md", 7).as_deref(), Some("README.md"));
    }

    #[test]
    fn the_spill_of_a_wide_character_is_part_of_the_run_not_of_the_text() {
        let mut cells = row("docs/日?.md", 80);
        cells[6].character = SPILL;
        cells[7].character = '.';
        cells[8].character = 'm';
        cells[9].character = 'd';
        cells[10].character = ' ';
        assert_eq!(
            written_at(&cells, 80, 0, 0)
                .map(|written| written.text)
                .as_deref(),
            Some("docs/日.md")
        );
    }

    #[test]
    fn a_location_after_the_path_is_not_its_name() {
        let cwd = Path::new("/repo");
        let home = Path::new("/home/me");
        assert_eq!(
            resolve("src/a.rs:12:4", cwd, home),
            PathBuf::from("/repo/src/a.rs")
        );
        assert_eq!(
            resolve("src/a.rs:12", cwd, home),
            PathBuf::from("/repo/src/a.rs")
        );
        assert_eq!(
            resolve("./src/../b.rs", cwd, home),
            PathBuf::from("/repo/b.rs")
        );
        assert_eq!(
            resolve("/etc/hosts", cwd, home),
            PathBuf::from("/etc/hosts")
        );
        assert_eq!(
            resolve("~/notes.md", cwd, home),
            PathBuf::from("/home/me/notes.md")
        );
        assert_eq!(without_location(r"C:\repo\a.rs:3"), r"C:\repo\a.rs");
    }

    #[test]
    fn a_path_outside_every_checkout_on_screen_has_no_root() {
        let agent = Path::new("/repo/.worktrees/a");
        let shell = Path::new("/repo");
        assert_eq!(
            root_of(Path::new("/repo/.worktrees/a/src/x.rs"), &[agent, shell]),
            Some(agent.to_path_buf())
        );
        assert_eq!(
            root_of(Path::new("/repo/src/x.rs"), &[agent, shell]),
            Some(shell.to_path_buf())
        );
        assert_eq!(root_of(Path::new("/etc/hosts"), &[agent, shell]), None);
    }

    fn link(target: &str, start: u16) -> PaneLink {
        PaneLink {
            pane: PaneId(1),
            written: Written {
                row: 0,
                start,
                end: start + 4,
                text: target.to_owned(),
            },
            target: PathBuf::from(target),
            root: PathBuf::from("/"),
        }
    }

    #[test]
    fn a_path_is_asked_about_once_and_drawn_when_it_is_there() {
        let now = Instant::now();
        let mut links = PaneLinks::default();
        assert_eq!(
            links.hover(Some(link("/a", 0)), now),
            (false, Some(PathBuf::from("/a")))
        );
        assert_eq!(
            links.hover(Some(link("/a", 0)), now),
            (false, None),
            "asked once"
        );
        assert_eq!(links.absorb(PathBuf::from("/a"), true, now), (true, None));
        assert!(links.hovered.is_some());
        assert_eq!(
            links.hover(Some(link("/a", 0)), now),
            (false, None),
            "remembered"
        );
        assert!(links.leave());
        assert_eq!(
            links.hover(Some(link("/missing", 0)), now).1,
            Some(PathBuf::from("/missing"))
        );
        assert_eq!(
            links.absorb(PathBuf::from("/missing"), false, now),
            (false, None)
        );
        assert!(links.hovered.is_none());
    }

    fn held(target: &str) -> HeldClick {
        HeldClick {
            link: link(target, 0),
            release: MouseEvent {
                kind: crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: crossterm::event::KeyModifiers::CONTROL,
            },
            area: Rect::new(0, 0, 80, 24),
        }
    }

    #[test]
    fn a_click_on_a_path_being_asked_about_opens_it_once_it_is_there() {
        let now = Instant::now();
        let mut links = PaneLinks::default();
        assert_eq!(
            links.click(held("/a"), now),
            Clicked::Waiting(Some(PathBuf::from("/a")))
        );
        let (_, settled) = links.absorb(PathBuf::from("/a"), true, now);
        assert_eq!(settled, Some(Settled::Open(link("/a", 0))));
        assert_eq!(
            links.click(held("/a"), now),
            Clicked::Open(link("/a", 0)),
            "remembered"
        );
        let _ = links.absorb(PathBuf::from("/b"), false, now);
        assert_eq!(links.click(held("/b"), now), Clicked::NotALink);
    }

    #[test]
    fn a_click_held_for_a_path_that_is_not_there_is_handed_back() {
        let now = Instant::now();
        let mut links = PaneLinks::default();
        let _ = links.click(held("/gone"), now);
        let (_, settled) = links.absorb(PathBuf::from("/gone"), false, now);
        assert_eq!(settled, Some(Settled::Release(held("/gone"))));
    }

    #[test]
    fn a_click_let_go_of_opens_nothing_when_its_answer_comes() {
        let now = Instant::now();
        let mut links = PaneLinks::default();
        let _ = links.click(held("/a"), now);
        assert_eq!(links.let_go(), Some(held("/a")));
        assert_eq!(links.absorb(PathBuf::from("/a"), true, now).1, None);
    }

    #[test]
    fn an_answer_ages() {
        let now = Instant::now();
        let mut links = PaneLinks::default();
        let _ = links.hover(Some(link("/a", 0)), now);
        let _ = links.absorb(PathBuf::from("/a"), false, now);
        assert_eq!(
            links.hover(Some(link("/a", 0)), now + REMEMBERED).1,
            Some(PathBuf::from("/a")),
            "a file written since becomes a link"
        );
    }
}
