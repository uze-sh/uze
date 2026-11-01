//! The review, drawn: the files on the left, the diff of the one on show
//! with each comment under its lines, and the comment being written where
//! it will stand.
//!
//! The content is one run of lines. A comment is not a box the host draws
//! but lines of its own between the diff's, set in from the edge and led
//! by a rule, so the reader's eye stays on the code it is about. What a
//! press on one of those lines does is decided here, from where along it
//! the press landed ([`act_at`]).

use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use super::surface::{self, Filter, Focus, Line, ReviewView, Viewed};
use super::{CATALOG, diff};
use crate::shared::checkout;
use crate::shared::git_status::FileStatus;
use crate::view::{
    COMMENT_MARK, Command, Confirm, Content, ContentLine, Layout, LineTone, MarkerSide, Medium,
    Mode, Navigator, NavigatorRow, Role, RowIcon, RowMenu, Size, Span, View,
};

use super::record::{Comment, CommentStatus};

/// How many lines the comment being written stands on before its text
/// needs more.
const COMPOSER_ROWS: usize = 4;

/// What a comment's lines are set in by, after the gutter.
const INDENT: &str = "  ";
/// What sets a comment's body lines, and the comment being written, in
/// under its first line. Air rather than a drawn rule: a rule is chrome,
/// and chrome is the host's to draw.
pub(crate) const RULE: &str = "    ";

/// What a press on a stretch of a comment's line does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Act {
    Edit(u64),
    Delete(u64),
    Resolve(u64),
    CodeBlock,
    Comment,
    /// The comment being written, kept as a draft.
    Save,
    /// The comment being written, let go.
    Discard,
}

/// One line of the content, what it is, and the stretches of it a press
/// acts on, in display cells from the start of its text.
pub(crate) struct Drawn {
    pub(crate) line: Line,
    pub(crate) content: ContentLine,
    pub(crate) acts: Vec<(Range<usize>, Act)>,
}

/// Builds a line from spans, recording where each act's words land.
struct Building {
    spans: Vec<Span>,
    width: usize,
    acts: Vec<(Range<usize>, Act)>,
    tone: LineTone,
}

impl Building {
    fn new() -> Self {
        Self {
            spans: Vec::new(),
            width: 0,
            acts: Vec::new(),
            tone: LineTone::Neutral,
        }
    }

    /// A line of a comment, on its card's ground, the rule down its left
    /// in `hue`.
    fn on_card(mut self, hue: Role) -> Self {
        self.tone = LineTone::Card(hue);
        self
    }

    fn push(mut self, span: Span) -> Self {
        self.width += span.text.width();
        self.spans.push(span);
        self
    }

    fn act(self, words: &str, act: Act, role: Role) -> Self {
        let start = self.width;
        let mut built = self.push(Span::new(words, role).pressable());
        built.acts.push((start..built.width, act));
        built
    }

    fn drawn(self, line: Line) -> Drawn {
        Drawn {
            line,
            content: ContentLine {
                gutter: String::new(),
                number: String::new(),
                tone: self.tone,
                spans: self.spans,
                marked: false,
            },
            acts: self.acts,
        }
    }
}

pub fn view(review: &ReviewView, space: Size) -> View {
    let title = checkout::name(CATALOG.name);
    let caption = checkout::caption(&review.display_root, &review.branch);
    let listing = review.list_error.is_none() && !review.files.is_empty();
    View {
        tabs: Vec::new(),
        title,
        caption,
        navigator: listing.then(|| navigator(review)),
        content: content(review, space),
        footer: footer(review),
        notice: notice(review),
        confirm: discard_confirm(review),
        modes: [thread_modes(review), viewed_mode(review)].concat(),
        subjects: filter_chips(review),
        layout: Layout::Sidebar,
        trail: Vec::new(),
        label: String::new(),
    }
}

/// Which list is on show: the branch against its target, or what is not
/// committed. On the bar's leading slot, where the code surface says files
/// or map, so the way back is there whatever the content shows: an empty
/// list is a message, and a message carries no chips.
pub(crate) fn filters() -> [(Filter, &'static str); 2] {
    [(Filter::All, "all"), (Filter::Uncommitted, "uncommitted")]
}

fn filter_chips(review: &ReviewView) -> Vec<Mode> {
    filters()
        .into_iter()
        .map(|(filter, label)| Mode {
            label: label.to_owned(),
            active: review.filter == filter,
            // What the branch differs by, and the part of it still being
            // worked on: the marks the files and the map stand beside.
            icon: match filter {
                Filter::All => RowIcon::Changes,
                Filter::Uncommitted => RowIcon::InFlight,
            },
        })
        .collect()
}

/// The open threads as a walk, GitLab's way: a step back, where the
/// operator is among them, a step on. Nothing when there are none.
fn thread_modes(review: &ReviewView) -> Vec<Mode> {
    let threads = review.threads();
    if threads.is_empty() {
        return Vec::new();
    }
    let at = review
        .thread
        .and_then(|id| threads.iter().position(|comment| comment.id == id));
    let count = match at {
        Some(at) => format!("{COMMENT_MARK} {}/{}", at + 1, threads.len()),
        None => format!("{COMMENT_MARK} {}", threads.len()),
    };
    let step = |icon| Mode {
        label: String::new(),
        active: false,
        icon,
    };
    vec![
        step(RowIcon::Previous),
        Mode {
            label: count,
            active: false,
            icon: RowIcon::None,
        },
        step(RowIcon::Next),
    ]
}

/// What a press on the content's chip `index` asks: the threads' walk
/// stands before the viewed chip while there are threads.
pub(crate) fn mode_at(review: &ReviewView, index: usize) -> ModeAct {
    match (review.threads().is_empty(), index) {
        (false, 0) => ModeAct::PreviousThread,
        (false, 1 | 2) => ModeAct::NextThread,
        _ => ModeAct::Viewed,
    }
}

pub(crate) enum ModeAct {
    PreviousThread,
    NextThread,
    Viewed,
}

fn viewed_mode(review: &ReviewView) -> Vec<Mode> {
    let Some(file) = review.selected_file() else {
        return Vec::new();
    };
    let viewed = review.viewed(&file.path);
    vec![Mode {
        label: match viewed {
            Viewed::Changed => "changed".to_owned(),
            Viewed::Yes | Viewed::No => "viewed".to_owned(),
        },
        active: viewed == Viewed::Yes,
        icon: viewed_icon(viewed),
    }]
}

fn viewed_icon(viewed: Viewed) -> RowIcon {
    match viewed {
        Viewed::Yes => RowIcon::Viewed,
        Viewed::Changed => RowIcon::ViewedThenChanged,
        Viewed::No => RowIcon::NotViewed,
    }
}

fn navigator(review: &ReviewView) -> Navigator {
    let selected = review.selected_index();
    Navigator {
        field: None,
        heading: "FILES".to_owned(),
        badge: format!("{}/{} viewed", review.viewed_count(), review.files.len()),
        focused: review.focus == Focus::Navigator,
        anchor: selected,
        choosing: None,
        menu: review
            .menu
            .as_ref()
            .zip(selected)
            .map(|(menu, row)| RowMenu {
                row,
                entries: menu.acts.iter().map(|act| act.label().to_owned()).collect(),
                highlighted: menu.highlighted,
            }),
        rows: review
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let relative = file.path.strip_prefix(&review.root).unwrap_or(&file.path);
                let name = relative
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let directory = relative
                    .parent()
                    .map(|parent| parent.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let path = review.relative(&file.path);
                let comments = review
                    .record
                    .comments
                    .iter()
                    .filter(|comment| {
                        comment.path == path && comment.status != CommentStatus::Resolved
                    })
                    .count();
                let detail = match comments {
                    0 => directory,
                    n => format!("{COMMENT_MARK}{n} {directory}"),
                };
                let marker = Span::new(file.status.glyph(), file.status.role());
                // Viewed is what a reviewer working down the list reads:
                // a checklist down the rows' leading column.
                NavigatorRow::Item {
                    id: index,
                    icon: viewed_icon(review.viewed(&file.path)),
                    detail,
                    name,
                    depth: 0,
                    marker,
                    marker_side: MarkerSide::Trailing,
                    selected: selected == Some(index),
                }
            })
            .collect(),
    }
}

fn content(review: &ReviewView, space: Size) -> Content {
    let message = |text: &str, hint: Option<String>, role: Role| Content::Message {
        text: text.to_owned(),
        hint,
        role,
    };
    if let Some(error) = &review.list_error {
        return message(error, None, Role::Danger);
    }
    if !review.listed {
        return message("reading…", None, Role::Muted);
    }
    if review.files.is_empty() {
        return match (&review.filter, &review.base.reason) {
            (Filter::Uncommitted, _) => message(
                "Nothing is uncommitted",
                Some(
                    "Everything on the branch is committed. Choose all to read the whole branch."
                        .to_owned(),
                ),
                Role::Muted,
            ),
            (Filter::All, Some(reason)) => message(
                "Nothing is uncommitted",
                Some(format!(
                    "The review shows what is not committed, since {reason}."
                )),
                Role::Muted,
            ),
            (Filter::All, None) => message(
                &format!("Nothing on {} differs from its target", review.branch),
                Some("The review opens here as soon as it does.".to_owned()),
                Role::Muted,
            ),
        };
    }
    let Some(file) = review.selected_file() else {
        return message(
            "No file selected",
            Some("Pick one on the left to read what changed in it.".to_owned()),
            Role::Muted,
        );
    };
    if let Some(error) = &review.diff_error {
        return message(error, None, Role::Danger);
    }
    let Some(diff) = review.diff.as_ref().filter(|diff| diff.path == file.path) else {
        return message("reading…", None, Role::Muted);
    };
    let heading = heading(review);
    if diff.binary {
        return match &diff.picture {
            Some(picture) => Content::Picture {
                heading,
                picture: picture.clone(),
            },
            None => message(
                &format!("{} is not a text file", review.relative(&file.path)),
                Some("Git records that it changed, but has no lines to compare in it.".to_owned()),
                Role::Muted,
            ),
        };
    }
    let drawn = lines(review);
    let total = drawn.len();
    let caret = review.composer.as_ref().and_then(|composer| {
        drawn
            .iter()
            .position(|line| line.line == Line::Composer(composer.caret.line))
            .map(|line| surface::composer_caret(line, composer.caret, RULE.chars().count()))
    });
    Content::Lines {
        heading,
        scroll: review.scroll,
        first: review.scroll,
        lines: drawn
            .into_iter()
            .skip(review.scroll)
            .take(usize::from(space.height).saturating_mul(2))
            .map(|drawn| drawn.content)
            .collect(),
        total,
        caret,
        editable: review.composer.is_some(),
        medium: Medium::Text,
    }
}

fn heading(review: &ReviewView) -> String {
    let path = review
        .selected
        .as_ref()
        .map(|path| review.relative(path))
        .unwrap_or_default();
    let against = match (&review.filter, &review.base.commit) {
        (Filter::All, Some(_)) => "",
        _ => " · uncommitted",
    };
    format!("DIFF · {path}{against}")
}

/// Every line of the content, in order: the diff's cells, each followed by
/// what stands under it, and the comments whose lines are gone at the end.
pub(crate) fn lines(review: &ReviewView) -> Vec<Drawn> {
    let cells = review.cells();
    let (placed, gone) = review.placed();
    let commented = review.commented_cells();
    let marked = review.marked();
    let mut drawn = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let mut content = diff::content_line(cell);
        if commented.contains(&index) {
            content.gutter = format!("{COMMENT_MARK}{}", content.gutter);
        }
        let in_mark = marked.is_some_and(|(from, to)| (from..=to).contains(&index));
        let at_cursor = review.focus == Focus::Content && index == review.cursor;
        content.marked = in_mark || (at_cursor && marked.is_none());
        drawn.push(Drawn {
            line: Line::Cell(index),
            content,
            acts: Vec::new(),
        });
        if let Some(comments) = placed.get(&index) {
            for comment in comments {
                card(review, comment, false, &mut drawn);
            }
        }
        let composing_here = review
            .composer
            .as_ref()
            .is_some_and(|_| marked.map_or(review.cursor, |(_, to)| to) == index);
        if composing_here {
            composer(review, &mut drawn);
        } else if review.composer.is_none() && marked.is_some_and(|(_, to)| to == index) {
            let (from, to) = marked.unwrap_or_default();
            let count = to - from + 1;
            let lines = if count == 1 {
                "1 line".to_owned()
            } else {
                format!("{count} lines")
            };
            drawn.push(
                Building::new()
                    .push(Span::new(INDENT, Role::Default))
                    .act(
                        &format!("{COMMENT_MARK} comment on {lines}"),
                        Act::Comment,
                        Role::Warning,
                    )
                    .drawn(Line::Affordance),
            );
        }
    }
    if !gone.is_empty() {
        drawn.push(blank());
        drawn.push(
            Building::new()
                .push(Span::new(INDENT, Role::Default))
                .push(Span::new(
                    "the lines these comments were on are no longer in this diff",
                    Role::Warning,
                ))
                .drawn(Line::Blank),
        );
        for comment in gone {
            card(review, comment, true, &mut drawn);
        }
    }
    drawn
}

fn blank() -> Drawn {
    Building::new().drawn(Line::Blank)
}

fn label(comment: &Comment) -> String {
    let lines: Vec<u32> = comment.rows.iter().map(|row| row.line).collect();
    match (lines.iter().min(), lines.iter().max()) {
        (Some(from), Some(to)) if from == to => format!("L{from}"),
        (Some(from), Some(to)) => format!("L{from}–{to}"),
        _ => String::new(),
    }
}

/// A kept comment, as lines under the cell its lines end on.
fn card(review: &ReviewView, comment: &Comment, gone: bool, drawn: &mut Vec<Drawn>) {
    let id = comment.id;
    let (status, role) = match comment.status {
        CommentStatus::Draft => ("draft".to_owned(), Role::Muted),
        CommentStatus::Sent => (
            format!("sent {}", comment.reference().unwrap_or_default()),
            Role::Info,
        ),
        CommentStatus::Answered => (
            format!("answered {}", comment.reference().unwrap_or_default()),
            Role::Success,
        ),
        CommentStatus::Resolved => ("resolved".to_owned(), Role::Dim),
    };
    // The rule says what waits on it, in the door's hues: a draft the
    // operator's, an answer the agent's.
    let hue = match comment.status {
        CommentStatus::Draft => Role::Warning,
        CommentStatus::Sent => Role::Info,
        CommentStatus::Answered => Role::Success,
        CommentStatus::Resolved => Role::Dim,
    };
    let card = || Building::new().on_card(hue);
    drawn.push(card().drawn(Line::CardBody(id)));
    let mut header = card()
        .push(Span::new(INDENT, Role::Default))
        .push(Span::new(COMMENT_MARK.to_string(), Role::Warning))
        .push(Span::new(format!(" {} · ", label(comment)), Role::Muted))
        .push(Span::new(status, role));
    if gone {
        header = header.push(Span::new(" · these lines changed", Role::Warning));
    }
    header = match comment.status {
        CommentStatus::Draft => header
            .push(Span::new("   ", Role::Default))
            .act("edit", Act::Edit(id), Role::Muted)
            .push(Span::new("  ", Role::Default))
            .act("delete", Act::Delete(id), Role::Faint),
        CommentStatus::Answered => header.push(Span::new("   ", Role::Default)).act(
            "resolve",
            Act::Resolve(id),
            Role::Muted,
        ),
        _ => header,
    };
    drawn.push(header.drawn(Line::CardHeader(id)));
    if gone {
        for row in &comment.rows {
            let sign = match row.kind {
                super::record::RowKind::Added => "+",
                super::record::RowKind::Removed => "-",
                super::record::RowKind::Context => " ",
            };
            drawn.push(
                card()
                    .push(Span::new(RULE, Role::Faint))
                    .push(Span::new(format!("{sign} {}", row.text), Role::Dim))
                    .drawn(Line::CardBody(id)),
            );
        }
    }
    if !comment.excerpt.is_empty() {
        drawn.push(
            card()
                .push(Span::new(RULE, Role::Faint))
                .push(Span::new(format!("on “{}”", comment.excerpt), Role::Muted))
                .drawn(Line::CardBody(id)),
        );
    }
    let cached = review.rich.get(&id);
    if let Some(cached) = cached {
        for line in &cached.lines {
            let mut building = card().push(Span::new(RULE, Role::Faint));
            for span in line {
                building = building.push(span.clone());
            }
            drawn.push(building.drawn(Line::CardBody(id)));
        }
    }
    for answer in cached
        .map(|cached| cached.answer_lines.as_slice())
        .unwrap_or_default()
    {
        drawn.push(
            card()
                .push(Span::new(RULE, Role::Faint))
                .push(Span::new("agent", Role::Warning))
                .drawn(Line::CardBody(id)),
        );
        for line in answer {
            let mut building = card().push(Span::new(RULE, Role::Faint));
            for span in line {
                building = building.push(Span {
                    role: if span.role == Role::Default {
                        Role::Secondary
                    } else {
                        span.role
                    },
                    ..span.clone()
                });
            }
            drawn.push(building.drawn(Line::CardBody(id)));
        }
    }
    drawn.push(card().drawn(Line::CardBody(id)));
    drawn.push(blank());
}

/// The comment being written, under the lines it is about.
fn composer(review: &ReviewView, drawn: &mut Vec<Drawn>) {
    let Some(composer) = &review.composer else {
        return;
    };
    let rows: Vec<u32> = composer.rows.iter().map(|row| row.line).collect();
    let lines = match (rows.iter().min(), rows.iter().max()) {
        (Some(from), Some(to)) if from == to => format!("L{from}"),
        (Some(from), Some(to)) => format!("L{from}–{to}"),
        _ => String::new(),
    };
    let what = match composer.editing {
        Some(_) => "editing",
        None => "new comment",
    };
    // Accent: the one being written, where the keys are.
    let card = || Building::new().on_card(Role::Accent);
    drawn.push(card().drawn(Line::ComposerPad));
    drawn.push(
        card()
            .push(Span::new(INDENT, Role::Default))
            .push(Span::new(COMMENT_MARK.to_string(), Role::Warning))
            .push(Span::new(format!(" {lines} · {what}   "), Role::Muted))
            .act("{ } code block", Act::CodeBlock, Role::Secondary)
            .drawn(Line::ComposerHeader),
    );
    if !composer.excerpt.is_empty() {
        drawn.push(
            card()
                .push(Span::new(RULE, Role::Faint))
                .push(Span::new(format!("on “{}”", composer.excerpt), Role::Muted))
                .drawn(Line::ComposerPad),
        );
    }
    drawn.push(card().drawn(Line::ComposerPad));
    let empty = composer.lines.len() == 1 && composer.lines[0].is_empty();
    for (index, text) in composer.lines.iter().enumerate() {
        let span = if empty {
            Span::new("what should change here, and why", Role::Faint)
        } else {
            Span::new(text.clone(), Role::Bright)
        };
        drawn.push(
            card()
                .push(Span::new(RULE, Role::Accent))
                .push(span)
                .drawn(Line::Composer(index)),
        );
    }
    // Room to write in from the start: a comment is a few lines, and a
    // field one line tall reads as a title.
    for _ in composer.lines.len()..COMPOSER_ROWS {
        drawn.push(card().drawn(Line::ComposerPad));
    }
    drawn.push(card().drawn(Line::ComposerPad));
    drawn.push(
        card()
            .push(Span::new(RULE, Role::Default))
            .act("save", Act::Save, Role::Accent)
            .push(Span::new("   ", Role::Default))
            .act("discard", Act::Discard, Role::Muted)
            .drawn(Line::ComposerPad),
    );
    drawn.push(card().drawn(Line::ComposerPad));
    drawn.push(blank());
}

/// What a press `cell` cells into the text of content line `line` acts
/// on, when it lands on one of its acts.
pub(crate) fn act_at(review: &ReviewView, line: usize, cell: usize) -> (Option<Line>, Option<Act>) {
    let drawn = lines(review);
    let Some(drawn) = drawn.get(line) else {
        return (None, None);
    };
    let act = drawn
        .acts
        .iter()
        .find(|(range, _)| range.contains(&cell))
        .map(|(_, act)| *act)
        .or(match drawn.line {
            Line::Affordance => Some(Act::Comment),
            _ => None,
        });
    (Some(drawn.line), act)
}

fn footer(review: &ReviewView) -> Vec<Command> {
    if review.discarding.is_some() {
        return Vec::new();
    }
    if review.menu.is_some() {
        return vec![Command::Activate, Command::Close];
    }
    if review.composer.is_some() {
        return vec![Command::AddComment, Command::Close];
    }
    let mut commands = vec![
        Command::SelectNext,
        Command::ExtendMark(crate::view::ScrollDirection::Down),
        Command::Comment,
        Command::MarkViewed,
        Command::OpenInCode,
    ];
    if !review.drafts().is_empty() {
        commands.push(Command::Send);
    }
    if review
        .record
        .comments
        .iter()
        .any(|comment| comment.status == CommentStatus::Answered)
    {
        commands.push(Command::Resolve);
    }
    if review
        .selected_file()
        .is_some_and(|file| surface::discardable(review, file))
    {
        commands.push(Command::OpenMenu);
    }
    commands.push(Command::FocusNext);
    commands.push(Command::Close);
    commands
}

fn notice(review: &ReviewView) -> Option<Span> {
    if let Some(notice) = &review.notice {
        return Some(notice.clone());
    }
    if let Some(error) = &review.record_error {
        return Some(Span::new(
            format!("the review cannot be kept: {error}"),
            Role::Danger,
        ));
    }
    let drafts = review.drafts().len();
    (drafts > 0).then(|| {
        Span::new(
            match drafts {
                1 => "1 draft".to_owned(),
                many => format!("{many} drafts"),
            },
            Role::Warning,
        )
    })
}

/// What discarding does to this file: a file the last commit lacks is
/// deleted, a rename brings its old name back, anything else is put back.
fn discard_body(review: &ReviewView, path: &std::path::Path) -> String {
    let status = review
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.status);
    match (review.partners.get(path), status) {
        (Some(other), _) => {
            // Asked from either side: the side the last commit lacks goes.
            let (old, new) = match status {
                Some(FileStatus::Deleted) => (path, other.as_path()),
                _ => (other.as_path(), path),
            };
            format!(
                "Takes the rename back: {} returns the way the last commit has it, and {} goes. This cannot be undone.",
                review.relative(old),
                review.relative(new)
            )
        }
        (None, Some(FileStatus::Untracked | FileStatus::Added)) => {
            "Deletes the file, which the last commit does not have. This cannot be undone."
                .to_owned()
        }
        _ => "Puts the file back the way the last commit has it. This cannot be undone.".to_owned(),
    }
}

fn discard_confirm(review: &ReviewView) -> Option<Confirm> {
    let discarding = review.discarding.as_ref()?;
    Some(Confirm {
        title: "Discard".to_owned(),
        subject: review.relative(&discarding.path),
        body: discard_body(review, &discarding.path),
        confirm: "Discard".to_owned(),
        on_confirm: discarding.on_confirm,
        field: None,
    })
}
