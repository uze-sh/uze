//! The review as it stands open on one checkout: the files that differ,
//! the one being read, what is marked in it, the comment being written,
//! and the review kept for the agent.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::change::Change;
use super::compose::{self, Composer, Rich};
use super::diff::{DiffCell, DiffLineKind};
use super::git::{Base, FileDiff, Listing};
use super::record::{Comment, CommentStatus, ReviewRecord, Row, RowKind};
use crate::shared::git_status::ChangedFile;
use crate::view::{Caret, ScrollDirection, Size, Span};

/// What is listed: everything the branch differs from its target by, or
/// only what is not committed yet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Filter {
    #[default]
    All,
    Uncommitted,
}

/// Where the keyboard is.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Focus {
    #[default]
    Navigator,
    Content,
}

/// What one line of the content is, which is what a press on it means.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Line {
    /// A line of the diff, by its index among the diff's cells.
    Cell(usize),
    /// The affordance under a marked run: write a comment on it.
    Affordance,
    /// A comment's first line, which carries its actions.
    CardHeader(u64),
    CardBody(u64),
    ComposerHeader,
    /// A line of the comment being written, by its index there.
    Composer(usize),
    /// The card's own room around the comment being written: a press there
    /// puts the caret at the end of what is written.
    ComposerPad,
    Blank,
}

/// A send the bridge has not yet said came of.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Sending {
    pub(crate) ids: Vec<u64>,
    pub(crate) agent: String,
}

/// A discard asked for and not yet answered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Discarding {
    pub(crate) path: PathBuf,
    pub(crate) on_confirm: bool,
}

/// The review, open on one checkout.
pub struct ReviewView {
    pub(crate) root: PathBuf,
    pub(crate) display_root: String,
    pub(crate) branch: String,
    pub(crate) filter: Filter,
    pub(crate) base: Base,
    pub(crate) files: Vec<ChangedFile>,
    pub(crate) uncommitted: BTreeSet<PathBuf>,
    pub(crate) partners: std::collections::BTreeMap<PathBuf, PathBuf>,
    /// Each listed file's content, as the list last read it.
    pub(crate) stamps: std::collections::BTreeMap<PathBuf, u64>,
    /// The content the diff on show was read for: the diff is read again
    /// only once the file moved.
    pub(crate) diffed: Option<(PathBuf, DiffKey)>,
    /// What the diff being read was asked against, set as it is asked.
    pub(crate) asking: Option<(PathBuf, DiffKey)>,
    /// The commit checked out, as the list last read it.
    pub(crate) head: Option<String>,
    /// The list has been read at least once.
    pub(crate) listed: bool,
    pub(crate) list_error: Option<String>,
    pub(crate) selected: Option<PathBuf>,
    pub(crate) diff: Option<FileDiff>,
    pub(crate) diff_error: Option<String>,
    /// The selection moved and its diff is still being read.
    pub(crate) diff_pending: bool,
    /// Index into the diff's cells.
    pub(crate) cursor: usize,
    /// The marked run of cells, from where it was started to where it was
    /// carried.
    pub(crate) mark: Option<(usize, usize)>,
    pub(crate) excerpt: String,
    pub(crate) composer: Option<Composer>,
    pub(crate) record: ReviewRecord,
    pub(crate) record_error: Option<String>,
    /// Each kept comment as it is drawn, by id: its text and then each
    /// answer, highlighted once, with what they were drawn from.
    pub(crate) rich: HashMap<u64, Drawn>,
    pub(crate) theme: String,
    pub(crate) focus: Focus,
    /// The first content line on screen.
    pub(crate) scroll: usize,
    pub(crate) notice: Option<Span>,
    pub(crate) discarding: Option<Discarding>,
    /// What can be done to the selected file, open on its row.
    pub(crate) menu: Option<FileMenu>,
    /// The open thread the operator last went to, which the threads chip
    /// counts from.
    pub(crate) thread: Option<u64>,
    /// A thread gone to in another file, waiting for that file's diff to
    /// be read before it can be shown.
    pub(crate) jump: Option<u64>,
    /// The comment the review was opened on from the drawer, gone to once
    /// the list is read.
    pub(crate) open_on: Option<u64>,
    /// Changes made here and not yet written to the record.
    pub(crate) unwritten: Vec<Change>,
    /// A discard to hand the host: the paths to put back as the last
    /// commit has them.
    pub(crate) restore: Option<Vec<PathBuf>>,
    /// The list is to be read again now, not on its cadence.
    pub(crate) relist: bool,
}

impl ReviewView {
    pub fn opening(root: PathBuf, display_root: String) -> Self {
        Self {
            root,
            display_root,
            branch: String::new(),
            filter: Filter::All,
            base: Base::default(),
            files: Vec::new(),
            uncommitted: BTreeSet::new(),
            partners: std::collections::BTreeMap::new(),
            stamps: std::collections::BTreeMap::new(),
            diffed: None,
            asking: None,
            head: None,
            listed: false,
            list_error: None,
            selected: None,
            diff: None,
            diff_error: None,
            diff_pending: false,
            cursor: 0,
            mark: None,
            excerpt: String::new(),
            composer: None,
            record: ReviewRecord::default(),
            record_error: None,
            rich: HashMap::new(),
            theme: crate::shared::highlight::FALLBACK_SYNTAX_THEME.to_owned(),
            focus: Focus::Navigator,
            scroll: 0,
            notice: None,
            discarding: None,
            menu: None,
            thread: None,
            jump: None,
            open_on: None,
            unwritten: Vec::new(),
            restore: None,
            relist: true,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn typing(&self) -> bool {
        self.composer.is_some()
    }

    /// The checkout-relative spelling of `path`, as the record keeps it.
    pub(crate) fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    pub(crate) fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.files.iter().position(|file| &file.path == selected)
    }

    pub(crate) fn selected_file(&self) -> Option<&ChangedFile> {
        self.files.get(self.selected_index()?)
    }

    pub(crate) fn cells(&self) -> &[DiffCell] {
        self.diff
            .as_ref()
            .filter(|diff| Some(&diff.path) == self.selected.as_ref())
            .map_or(&[], |diff| diff.cells.as_slice())
    }

    // --- What arrives from the host ----------------------------------------

    /// A read of what differs, for the filter it was asked with.
    pub(crate) fn absorb_listing(
        &mut self,
        filter: Filter,
        base: Base,
        branch: String,
        listing: Result<Listing, String>,
    ) {
        if filter != self.filter {
            return;
        }
        self.base = base;
        self.branch = branch;
        self.listed = true;
        match listing {
            Err(message) => {
                self.list_error = Some(message);
                self.files.clear();
            }
            Ok(listing) => {
                self.list_error = None;
                // The file a comment is being written on stays listed
                // until the comment is added or cancelled, though the
                // agent committed it meanwhile.
                let held = self
                    .composer
                    .as_ref()
                    .and_then(|composer| self.files.iter().find(|file| file.path == composer.path))
                    .filter(|held| !listing.files.iter().any(|file| file.path == held.path))
                    .cloned();
                self.files = listing.files;
                self.files.extend(held);
                self.uncommitted = listing.uncommitted;
                self.partners = listing.partners;
                self.stamps = listing.stamps;
                self.head = listing.head;
            }
        }
        if self.selected_index().is_none() && self.composer.is_none() {
            self.selected = self.files.first().map(|file| file.path.clone());
            self.diff = None;
            self.cursor = 0;
            self.mark = None;
        }
        // A list that could not be read lists nothing to go to: the
        // comment waits for one that does.
        if self.list_error.is_none()
            && let Some(id) = self.open_on.take()
        {
            self.go_to_thread_id(id);
        }
        // Read the diff again only once the file, the commit or the base
        // it is taken against moved.
        let now = self
            .selected
            .clone()
            .and_then(|path| Some((path.clone(), self.diff_key(&path)?)));
        if self.selected.is_some() && (self.diff.is_none() || now.is_none() || now != self.diffed) {
            self.diff_pending = true;
        }
    }

    /// A read of one file's diff.
    pub(crate) fn absorb_diff(
        &mut self,
        path: PathBuf,
        diff: Result<FileDiff, String>,
        theme: String,
    ) {
        if Some(&path) != self.selected.as_ref() {
            return;
        }
        self.diff_pending = false;
        self.theme = theme;
        // What it was asked against, not what stands now: a file that
        // moved while the read was out is read again.
        self.diffed = self.asking.take().filter(|(asked, _)| *asked == path);
        match diff {
            Ok(diff) => {
                let moved = self.diff.as_ref().is_none_or(|shown| {
                    shown.path != path || shown.fingerprint != diff.fingerprint
                });
                self.diff_error = None;
                self.diff = Some(diff);
                if moved {
                    let last = self.cells().len().saturating_sub(1);
                    self.cursor = self.cursor.min(last);
                    if self.mark.is_some_and(|(a, b)| a.max(b) > last) {
                        self.mark = None;
                    }
                }
                self.recolour();
                self.settle_jump();
            }
            Err(message) => {
                self.diff = None;
                self.diff_error = Some(message);
            }
        }
    }

    // --- Threads ----------------------------------------------------------

    /// The open threads (every comment not resolved) on the files listed,
    /// in the list's order and, within a file, by the lines they are on.
    pub(crate) fn threads(&self) -> Vec<&Comment> {
        let mut open: Vec<(usize, u32, &Comment)> = self
            .record
            .comments
            .iter()
            .filter(|comment| comment.status != CommentStatus::Resolved)
            .filter_map(|comment| {
                let path = self.root.join(&comment.path);
                let file = self.files.iter().position(|file| file.path == path)?;
                let line = comment.rows.iter().map(|row| row.line).min().unwrap_or(0);
                Some((file, line, comment))
            })
            .collect();
        open.sort_by_key(|(file, line, comment)| (*file, *line, comment.id));
        open.into_iter().map(|(_, _, comment)| comment).collect()
    }

    /// Goes to the open thread after the one last gone to, or before it,
    /// round the ends, in whichever file it is.
    pub(crate) fn go_to_thread(&mut self, forward: bool) {
        let threads: Vec<(u64, String)> = self
            .threads()
            .iter()
            .map(|comment| (comment.id, comment.path.clone()))
            .collect();
        if threads.is_empty() {
            self.notice = Some(Span::new("no open threads", crate::view::Role::Muted));
            return;
        }
        let at = self
            .thread
            .and_then(|id| threads.iter().position(|(thread, _)| *thread == id));
        let next = match (at, forward) {
            (Some(at), true) => (at + 1) % threads.len(),
            (Some(at), false) => (at + threads.len() - 1) % threads.len(),
            (None, true) => 0,
            (None, false) => threads.len() - 1,
        };
        self.go_to_thread_id(threads[next].0);
    }

    /// Goes to the open thread `id`, in whichever file it is: its file
    /// selected, its lines marked once that file's diff is read.
    pub(crate) fn go_to_thread_id(&mut self, id: u64) {
        let Some(path) = self
            .record
            .comments
            .iter()
            .find(|comment| comment.id == id)
            .map(|comment| self.root.join(&comment.path))
        else {
            return;
        };
        if self.selected.as_ref() != Some(&path) {
            let Some(index) = self.files.iter().position(|file| file.path == path) else {
                self.not_listed(id);
                return;
            };
            self.select_file(index);
            // Refused while a comment is being written: it holds its file.
            if self.selected.as_ref() != Some(&path) {
                return;
            }
        }
        self.thread = Some(id);
        self.focus = Focus::Content;
        self.jump = Some(id);
        if self.diff.as_ref().is_some_and(|diff| diff.path == path) {
            self.settle_jump();
        }
    }

    /// A thread whose file the list does not hold: under `uncommitted`,
    /// the branch's whole list is read and the thread gone to then; on the
    /// whole branch, its file no longer differs from the target, and that
    /// is said rather than nothing happening.
    fn not_listed(&mut self, id: u64) {
        if self.filter == Filter::Uncommitted && self.composer.is_none() {
            self.set_filter(Filter::All);
            self.open_on = Some(id);
            return;
        }
        self.notice = Some(Span::new(
            "that comment's file no longer differs from the target",
            crate::view::Role::Muted,
        ));
    }

    /// Shows the thread gone to, once its file's diff is on show: its
    /// lines marked and on screen, its comment under them.
    fn settle_jump(&mut self) {
        let Some(id) = self.jump.take() else {
            return;
        };
        let (at, _) = self.placed();
        let placed = at.iter().find_map(|(end, comments)| {
            let comment = comments.iter().find(|comment| comment.id == id)?;
            Some((*end + 1 - comment.rows.len().min(*end + 1), *end))
        });
        let wanted = match placed {
            Some((from, to)) => {
                self.mark = Some((from, to));
                self.cursor = to;
                Line::Cell(from)
            }
            // Its lines are gone from the diff: the comment stands at the
            // end, and that is what is shown.
            None => Line::CardHeader(id),
        };
        if let Some(line) = super::render::lines(self)
            .iter()
            .position(|drawn| drawn.line == wanted)
        {
            // A few lines of what comes before, so the thread is read in
            // its place rather than at the very top edge.
            self.scroll = line.saturating_sub(3);
        }
    }

    /// The review as the record keeps it now, with what was changed here
    /// and not yet written laid over it.
    /// `in_flight` is what a write is out for: not yet in what was read,
    /// and laid over it with what is still to be written.
    pub(crate) fn absorb_record(
        &mut self,
        record: Result<ReviewRecord, String>,
        in_flight: &[Change],
    ) {
        match record {
            Ok(mut record) => {
                for change in in_flight.iter().chain(&self.unwritten) {
                    change.apply(&mut record);
                }
                self.record = record;
                self.record_error = None;
                self.recolour();
            }
            Err(message) => self.record_error = Some(message),
        }
    }

    /// Draws again the kept comments whose text changed since they were
    /// last drawn.
    pub(crate) fn recolour(&mut self) {
        let theme = self.theme.clone();
        let mut kept = HashMap::new();
        for comment in &self.record.comments {
            let drawn = match self.rich.remove(&comment.id) {
                Some(drawn) if drawn.text == comment.text && drawn.answers == comment.answers => {
                    drawn
                }
                _ => Drawn::of(comment, &theme),
            };
            kept.insert(comment.id, drawn);
        }
        self.rich = kept;
    }

    /// Makes `change` here at once, and keeps it to be written.
    pub(crate) fn change(&mut self, change: Change) {
        change.apply(&mut self.record);
        self.unwritten.push(change);
        self.recolour();
    }

    // --- Selecting --------------------------------------------------------

    pub(crate) fn select_file(&mut self, index: usize) {
        // A comment being written is about the file on show: moving away
        // would take it with it, so the move waits for it.
        if self.composer.is_some() {
            self.notice = Some(Span::new(
                "add the comment or cancel it first",
                crate::view::Role::Muted,
            ));
            return;
        }
        let Some(file) = self.files.get(index) else {
            return;
        };
        if self.selected.as_ref() == Some(&file.path) {
            return;
        }
        self.selected = Some(file.path.clone());
        self.diff = None;
        self.diff_error = None;
        self.diff_pending = true;
        self.cursor = 0;
        self.mark = None;
        self.excerpt.clear();
        self.scroll = 0;
        self.notice = None;
    }

    pub(crate) fn step_file(&mut self, direction: ScrollDirection) {
        let next = match (self.selected_index(), direction) {
            (None, _) => Some(0),
            (Some(at), ScrollDirection::Down) => Some(at + 1),
            (Some(at), ScrollDirection::Up) => at.checked_sub(1),
        };
        if let Some(next) = next.filter(|next| *next < self.files.len()) {
            self.select_file(next);
        }
    }

    pub(crate) fn set_filter(&mut self, filter: Filter) {
        if self.filter != filter && self.composer.is_none() {
            self.filter = filter;
            self.relist = true;
            self.listed = false;
            // Taken against another base, the diff on show is another diff.
            self.diff = None;
            self.diffed = None;
        }
    }

    /// What the diff of `path` depends on, as the list last read it.
    pub(crate) fn diff_key(&self, path: &Path) -> Option<DiffKey> {
        Some(DiffKey {
            stamp: *self.stamps.get(path)?,
            base: match self.filter {
                Filter::All => self.base.commit.clone(),
                Filter::Uncommitted => None,
            },
            head: self.head.clone(),
        })
    }

    // --- Marking ----------------------------------------------------------

    pub(crate) fn marked(&self) -> Option<(usize, usize)> {
        self.mark.map(|(a, b)| (a.min(b), a.max(b)))
    }

    pub(crate) fn move_cursor(&mut self, direction: ScrollDirection, extend: bool) {
        let last = self.cells().len().saturating_sub(1);
        let next = match direction {
            ScrollDirection::Down => (self.cursor + 1).min(last),
            ScrollDirection::Up => self.cursor.saturating_sub(1),
        };
        if extend {
            let anchor = self.mark.map_or(self.cursor, |(anchor, _)| anchor);
            self.mark = Some((anchor, next));
        } else {
            self.mark = None;
        }
        self.excerpt.clear();
        self.cursor = next;
    }

    pub(crate) fn mark_cell(&mut self, cell: usize, extend: bool) {
        if cell >= self.cells().len() {
            return;
        }
        let anchor = match (extend, self.mark) {
            (true, Some((anchor, _))) => anchor,
            (true, None) => self.cursor,
            (false, _) => cell,
        };
        self.mark = Some((anchor, cell));
        self.cursor = cell;
        self.excerpt.clear();
        self.focus = Focus::Content;
    }

    /// The rows of the marked run, or of the cursor's line.
    pub(crate) fn marked_rows(&self) -> Vec<Row> {
        let cells = self.cells();
        let (from, to) = self.marked().unwrap_or((self.cursor, self.cursor));
        cells
            .get(from..=to.min(cells.len().saturating_sub(1)))
            .unwrap_or_default()
            .iter()
            .map(row_of)
            .collect()
    }

    // --- Writing ----------------------------------------------------------

    pub(crate) fn open_composer(&mut self) {
        if self.composer.is_some() || self.cells().is_empty() {
            return;
        }
        // A comment that cannot be kept is not offered to be written.
        if let Some(error) = &self.record_error {
            self.notice = Some(Span::new(
                format!("the review cannot be kept: {error}"),
                crate::view::Role::Danger,
            ));
            return;
        }
        let rows = self.marked_rows();
        if rows.is_empty() {
            return;
        }
        let (_, to) = self.marked().unwrap_or((self.cursor, self.cursor));
        self.cursor = to;
        let Some(path) = self.selected.clone() else {
            return;
        };
        self.composer = Some(Composer::new(path, rows, std::mem::take(&mut self.excerpt)));
        self.focus = Focus::Content;
        self.notice = None;
    }

    pub(crate) fn edit_comment(&mut self, id: u64) {
        let Some(comment) = self
            .record
            .comments
            .iter()
            .find(|comment| comment.id == id && comment.status == CommentStatus::Draft)
        else {
            return;
        };
        let path = self.root.join(&comment.path);
        self.composer = Some(Composer::editing(
            id,
            path,
            comment.rows.clone(),
            comment.excerpt.clone(),
            &comment.text,
        ));
        self.focus = Focus::Content;
    }

    /// Keeps what is being written as a draft, or as the draft it edits.
    /// A comment with nothing in it is not kept.
    pub(crate) fn add_comment(&mut self) {
        let Some(composer) = self.composer.take() else {
            return;
        };
        if composer.is_blank() {
            return;
        }
        let path = composer.path.clone();
        let text = composer.text().trim_end().to_owned();
        match composer.editing {
            Some(id) => self.change(Change::Edit { id, text }),
            None => self.change(Change::Draft {
                id: self.record.free_id(),
                path: self.relative(&path),
                rows: composer.rows,
                excerpt: composer.excerpt,
                text,
            }),
        }
        self.mark = None;
    }

    pub(crate) fn cancel_composer(&mut self) {
        self.composer = None;
    }

    #[cfg(test)]
    pub(crate) fn drafts(&self) -> Vec<&Comment> {
        self.record
            .comments
            .iter()
            .filter(|comment| comment.status == CommentStatus::Draft)
            .collect()
    }

    // --- Viewed -----------------------------------------------------------

    /// Whether `path` is marked viewed on the diff on show — or, for a file
    /// not on show, marked at all: its diff is not known here until read.
    pub(crate) fn viewed(&self, path: &Path) -> Viewed {
        let Some(mark) = self.record.viewed.get(&self.relative(path)) else {
            return Viewed::No;
        };
        match self.stamps.get(path) {
            Some(stamp) if stamp == mark => Viewed::Yes,
            // Changed, or a content nothing can vouch for any more.
            _ => Viewed::Changed,
        }
    }

    /// Marks the file on show viewed and moves on to the next one not
    /// viewed, or unmarks it.
    pub(crate) fn toggle_viewed(&mut self, advance: bool) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let Some(fingerprint) = self.stamps.get(&path).copied() else {
            self.notice = Some(Span::new(
                "Git cannot say what this file holds, so it cannot be marked viewed",
                crate::view::Role::Muted,
            ));
            return;
        };
        let viewed = self.viewed(&path) == Viewed::Yes;
        self.change(Change::Viewed {
            path: self.relative(&path),
            fingerprint: (!viewed).then_some(fingerprint),
        });
        if viewed || !advance {
            return;
        }
        let at = self.selected_index().unwrap_or(0);
        let next = (1..self.files.len())
            .map(|step| (at + step) % self.files.len())
            .find(|index| self.viewed(&self.files[*index].path) == Viewed::No);
        if let Some(next) = next {
            self.select_file(next);
        }
    }

    pub(crate) fn viewed_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| self.viewed(&file.path) == Viewed::Yes)
            .count()
    }

    // --- The content, line by line ----------------------------------------

    /// Where each comment on the file on show stands: after the cell its
    /// lines end on, or, when those lines are gone from the diff, at the
    /// end.
    pub(crate) fn placed(&self) -> (HashMap<usize, Vec<&Comment>>, Vec<&Comment>) {
        let mut at: HashMap<usize, Vec<&Comment>> = HashMap::new();
        let mut gone = Vec::new();
        let Some(path) = self.selected.as_ref().map(|path| self.relative(path)) else {
            return (at, gone);
        };
        let cells = self.cells();
        for comment in self
            .record
            .comments
            .iter()
            .filter(|comment| comment.path == path && comment.status != CommentStatus::Resolved)
        {
            match locate(&comment.rows, cells) {
                Some(end) => at.entry(end).or_default().push(comment),
                None => gone.push(comment),
            }
        }
        (at, gone)
    }

    /// The cells that carry a comment's mark.
    pub(crate) fn commented_cells(&self) -> BTreeSet<usize> {
        let (at, _) = self.placed();
        let mut cells = BTreeSet::new();
        for (end, comments) in at {
            for comment in comments {
                let first = end + 1 - comment.rows.len().min(end + 1);
                cells.extend(first..=end);
            }
        }
        cells
    }

    pub(crate) fn follow(&mut self, line: usize, space: Size) {
        let visible = usize::from(crate::view::text_height(space).max(1));
        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll + visible {
            self.scroll = line + 1 - visible;
        }
    }
}

/// Whether a file is marked viewed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Viewed {
    No,
    Yes,
    /// Marked, on a diff that has moved since.
    Changed,
}

/// What can be done to one file of the list, from its row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileAct {
    /// The file in the code surface.
    Open,
    /// The path as the machine spells it.
    CopyPath,
    /// The path as the checkout spells it.
    CopyRelativePath,
    /// Asked first: what is not committed in it, thrown away.
    Discard,
}

impl FileAct {
    pub(crate) fn label(self) -> &'static str {
        match self {
            FileAct::Open => "Open file",
            FileAct::CopyPath => "Copy path",
            FileAct::CopyRelativePath => "Copy relative path",
            FileAct::Discard => "Discard changes…",
        }
    }
}

/// The menu open on the selected file's row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileMenu {
    pub(crate) acts: Vec<FileAct>,
    pub(crate) highlighted: usize,
}

/// What a file's diff was read against: its content, the base, the commit
/// checked out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiffKey {
    stamp: u64,
    base: Option<String>,
    head: Option<String>,
}

/// A kept comment's text and answers, highlighted, and what they were
/// highlighted from: a comment is coloured again only once it changed.
pub(crate) struct Drawn {
    text: String,
    answers: Vec<String>,
    pub(crate) lines: Vec<Rich>,
    pub(crate) answer_lines: Vec<Vec<Rich>>,
}

impl Drawn {
    fn of(comment: &Comment, theme: &str) -> Self {
        let path = Path::new(&comment.path);
        Self {
            text: comment.text.clone(),
            answers: comment.answers.clone(),
            lines: compose::rich(&comment.text, path, theme),
            answer_lines: comment
                .answers
                .iter()
                .map(|answer| compose::rich(answer, path, theme))
                .collect(),
        }
    }
}

pub(crate) fn row_of(cell: &DiffCell) -> Row {
    Row {
        kind: match cell.kind {
            DiffLineKind::Context => RowKind::Context,
            DiffLineKind::Added => RowKind::Added,
            DiffLineKind::Removed => RowKind::Removed,
        },
        line: cell.line_no,
        text: cell_text(cell),
    }
}

pub(crate) fn cell_text(cell: &DiffCell) -> String {
    cell.spans.iter().map(|(_, text)| text.as_str()).collect()
}

/// The cell `rows` end on in `cells`, found by their text rather than their
/// numbers: a commit above them moves the numbers and not the lines. Where
/// the same lines stand more than once (a closing brace, a blank line),
/// the run nearest the line they were on wins.
pub(crate) fn locate(rows: &[Row], cells: &[DiffCell]) -> Option<usize> {
    if rows.is_empty() || rows.len() > cells.len() {
        return None;
    }
    (0..=cells.len() - rows.len())
        .filter(|start| {
            rows.iter()
                .zip(&cells[*start..])
                .all(|(row, cell)| row_of(cell).kind == row.kind && cell_text(cell) == row.text)
        })
        .min_by_key(|start| cells[*start].line_no.abs_diff(rows[0].line))
        .map(|start| start + rows.len() - 1)
}

/// The caret in a composer line, in characters of what is drawn: past the
/// prefix every composer line is drawn with.
pub(crate) fn composer_caret(line: usize, caret: Caret, prefix: usize) -> Caret {
    Caret {
        line,
        column: caret.column + prefix,
    }
}

/// Whether `file` has work not yet committed, which is all a discard can
/// put back.
pub(crate) fn discardable(view: &ReviewView, file: &ChangedFile) -> bool {
    view.uncommitted.contains(&file.path)
}
