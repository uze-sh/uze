//! The review surface: what an agent's branch adds against its target,
//! read file by file, said back to the agent line by line.
//!
//! # Why apart from the code surface
//!
//! The changes used to be the code surface's first half, on the argument
//! that "I see it in the diff, let me fix that line" must not cross two
//! surfaces. Two things moved that argument. A surface can be opened on a
//! path and a line from outside, so the crossing is one gesture
//! ([`crate::contract::Outcome::OpenPath`]). And a review is not another
//! way of reading the same tree: it keeps what the operator wrote across
//! sessions, it speaks to the agent, and it diffs against the target
//! rather than against the last commit. Kept in the code surface, every
//! one of those would be a branch on the mode deciding what the state
//! means, which that surface's own rule forbids.
//!
//! # What it keeps, and where
//!
//! The comments and the viewed marks are the operator's work, so they
//! outlive the session: the host keeps them per agent
//! ([`crate::Host::update_review_record`]) and the agent answers into the
//! same record from its own process. The surface never writes the record
//! whole; it hands the host each [`change::Change`] it made, applied under
//! the record's lock, so an answer that landed in between is kept.

pub(crate) mod change;
pub(crate) mod compose;
pub(crate) mod diff;
mod extension;
pub(crate) mod git;
pub mod record;
mod render;
mod surface;
#[cfg(test)]
mod tests;

pub use extension::{DOOR, ReviewExtension, SURFACE};
pub use record::{Comment, CommentStatus, ReviewRecord, Row, RowKind};
pub use render::view;
pub use surface::{Filter, ReviewView};

use crate::contract::Outcome;
use crate::view::{Command, ScrollDirection, Size, Span, ViewHit};
use render::Act;
use surface::{Discarding, Focus, Line};

/// This extension's registry entry, registered once in
/// `ExtensionRegistry::builtin`.
pub const CATALOG: crate::registry::BuiltinExtension = crate::registry::BuiltinExtension {
    id: "review",
    name: "Review",
    description: "The selected agent's branch against its target, commented line by line and sent back to the agent.",
    surface: "Workspace TUI",
    usage: "Open it with Alt+G or the +N −M door on the tab strip. Mark lines by their numbers or with shift and an arrow, press c to comment, v when a file is read, s to tell the agent its review waits, o to open a line in the code surface.",
};

/// What a gesture asked of the host beyond the surface itself.
pub(crate) enum Asked {
    Stay,
    Close,
    Send,
    OpenInCode,
}

pub(crate) fn handle_command(view: &mut ReviewView, command: Command, space: Size) -> Asked {
    if let Some(discarding) = view.discarding.as_mut() {
        match command {
            Command::Activate => {
                let confirmed = discarding.on_confirm;
                answer_discard(view, confirmed);
            }
            Command::Close => answer_discard(view, false),
            Command::FocusNext | Command::Collapse | Command::Expand => {
                discarding.on_confirm = !discarding.on_confirm;
            }
            _ => {}
        }
        return Asked::Stay;
    }
    if let Some(composer) = view.composer.as_mut() {
        match command {
            Command::Type(character) => composer.type_char(character),
            Command::Newline => composer.newline(),
            Command::EraseBack => composer.erase_back(),
            Command::EraseForward => composer.erase_forward(),
            Command::CaretLeft => composer.left(),
            Command::CaretRight => composer.right(),
            Command::CaretLineStart => composer.line_start(),
            Command::CaretLineEnd => composer.line_end(),
            Command::SelectPrevious => composer.up(),
            Command::SelectNext => composer.down(),
            Command::AddComment | Command::Save => view.add_comment(),
            Command::Close => view.cancel_composer(),
            _ => {}
        }
        keep_composer_in_view(view, space);
        return Asked::Stay;
    }
    view.notice = None;
    match command {
        Command::Close => {
            if view.mark.is_some() {
                view.mark = None;
                view.excerpt.clear();
            } else {
                return Asked::Close;
            }
        }
        Command::FocusNext => {
            view.focus = match view.focus {
                Focus::Navigator => Focus::Content,
                Focus::Content => Focus::Navigator,
            };
        }
        Command::SelectNext | Command::SelectPrevious => {
            let direction = match command {
                Command::SelectNext => ScrollDirection::Down,
                _ => ScrollDirection::Up,
            };
            match view.focus {
                Focus::Navigator => view.step_file(direction),
                Focus::Content => {
                    view.move_cursor(direction, false);
                    keep_cursor_in_view(view, space);
                }
            }
        }
        Command::ExtendMark(direction) => {
            view.focus = Focus::Content;
            view.move_cursor(direction, true);
            keep_cursor_in_view(view, space);
        }
        Command::Activate => {
            if view.focus == Focus::Navigator {
                view.focus = Focus::Content;
            } else {
                view.open_composer();
            }
        }
        Command::Comment => {
            view.open_composer();
            keep_composer_in_view(view, space);
        }
        Command::MarkViewed => view.toggle_viewed(true),
        Command::Send => return Asked::Send,
        Command::OpenInCode => return Asked::OpenInCode,
        Command::Resolve => resolve_near_cursor(view),
        Command::OpenMenu => ask_discard(view),
        Command::ScrollPageDown => {
            view.scroll = view.scroll.saturating_add(usize::from(space.height).max(1));
            clamp_scroll(view);
        }
        Command::ScrollPageUp => {
            view.scroll = view.scroll.saturating_sub(usize::from(space.height).max(1));
        }
        _ => {}
    }
    Asked::Stay
}

pub(crate) fn handle_hit(view: &mut ReviewView, hit: ViewHit, space: Size) -> Asked {
    if view.discarding.is_some() {
        if let ViewHit::Answer(yes) = hit {
            answer_discard(view, yes);
        }
        return Asked::Stay;
    }
    match hit {
        ViewHit::Close => return Asked::Close,
        ViewHit::SelectItem(index) => {
            view.select_file(index);
            view.focus = Focus::Navigator;
        }
        ViewHit::OpenMenu(index) => {
            view.select_file(index);
            ask_discard(view);
        }
        ViewHit::SelectSubject(index) => {
            if let Some((filter, _)) = render::filters().get(index) {
                view.set_filter(*filter);
            }
        }
        ViewHit::SelectMode(_) => view.toggle_viewed(false),
        ViewHit::LineNumber { line, extend } => {
            if view.composer.is_some() {
                return Asked::Stay;
            }
            if let (Some(Line::Cell(cell)), _) = render::act_at(view, line, 0) {
                view.mark_cell(cell, extend);
            }
        }
        ViewHit::PlaceCaret { line, cell } => {
            let (what, act) = render::act_at(view, line, cell);
            match (what, act) {
                (_, Some(act)) => perform(view, act, space),
                (Some(Line::Composer(index)), None) => {
                    let prefix = render_prefix();
                    if let Some(composer) = view.composer.as_mut() {
                        composer.place(index, cell.saturating_sub(prefix));
                    }
                }
                (Some(Line::Cell(index)), None) if view.composer.is_none() => {
                    view.focus = Focus::Content;
                    view.cursor = index;
                    view.mark = None;
                }
                _ => {}
            }
        }
        _ => {}
    }
    Asked::Stay
}

/// The width of what leads every composer line, which a click's cell
/// counts and the composer's own columns do not.
fn render_prefix() -> usize {
    4
}

fn perform(view: &mut ReviewView, act: Act, space: Size) {
    match act {
        Act::Edit(id) => view.edit_comment(id),
        Act::Delete(id) => view.change(change::Change::Delete(id)),
        Act::Resolve(id) => view.change(change::Change::Resolve(id)),
        Act::Comment => {
            view.open_composer();
            keep_composer_in_view(view, space);
        }
        Act::CodeBlock => {
            if let Some(composer) = view.composer.as_mut() {
                composer.insert_block();
            }
        }
        Act::InlineCode => {
            if let Some(composer) = view.composer.as_mut() {
                composer.insert_inline();
            }
        }
    }
}

/// Resolves the answered comment the cursor stands on or above, else the
/// first answered one in the file.
fn resolve_near_cursor(view: &mut ReviewView) {
    let (placed, gone) = view.placed();
    let answered = |comments: &[&record::Comment]| {
        comments
            .iter()
            .find(|comment| comment.status == CommentStatus::Answered)
            .map(|comment| comment.id)
    };
    let mut ends: Vec<(&usize, &Vec<&record::Comment>)> = placed.iter().collect();
    ends.sort_by_key(|(end, _)| **end);
    let id = ends
        .iter()
        .find(|(end, _)| **end >= view.cursor)
        .and_then(|(_, comments)| answered(comments))
        .or_else(|| ends.iter().find_map(|(_, comments)| answered(comments)))
        .or_else(|| answered(&gone));
    match id {
        Some(id) => view.change(change::Change::Resolve(id)),
        None => {
            view.notice = Some(Span::new(
                "no answered comment in this file",
                crate::view::Role::Muted,
            ));
        }
    }
}

fn ask_discard(view: &mut ReviewView) {
    let Some(file) = view.selected_file() else {
        return;
    };
    if !surface::discardable(view, file) {
        view.notice = Some(Span::new(
            "only what is not committed can be discarded",
            crate::view::Role::Muted,
        ));
        return;
    }
    view.discarding = Some(Discarding {
        path: file.path.clone(),
        on_confirm: false,
    });
}

fn answer_discard(view: &mut ReviewView, yes: bool) {
    let Some(discarding) = view.discarding.take() else {
        return;
    };
    if yes {
        // A rename is two paths, and throwing it away brings the old name
        // back as well as taking the new one off.
        let partner = view.partners.get(&discarding.path).cloned();
        view.restore = Some(std::iter::once(discarding.path).chain(partner).collect());
    }
}

fn content_line_of(view: &ReviewView, wanted: Line) -> Option<usize> {
    render::lines(view)
        .iter()
        .position(|drawn| drawn.line == wanted)
}

fn keep_cursor_in_view(view: &mut ReviewView, space: Size) {
    if let Some(line) = content_line_of(view, Line::Cell(view.cursor)) {
        view.follow(line, space);
    }
}

fn keep_composer_in_view(view: &mut ReviewView, space: Size) {
    let Some(caret) = view.composer.as_ref().map(|composer| composer.caret.line) else {
        return;
    };
    if let Some(line) = content_line_of(view, Line::Composer(caret)) {
        view.follow(line, space);
    }
}

fn clamp_scroll(view: &mut ReviewView) {
    let total = render::lines(view).len();
    view.scroll = view.scroll.min(total.saturating_sub(1));
}

pub(crate) fn handle_scroll(view: &mut ReviewView, direction: ScrollDirection, at_end: bool) {
    match direction {
        ScrollDirection::Down if !at_end => view.scroll = view.scroll.saturating_add(3),
        ScrollDirection::Up => view.scroll = view.scroll.saturating_sub(3),
        ScrollDirection::Down => {}
    }
    clamp_scroll(view);
}

/// The text of content lines `lines`, for copying a selection.
pub(crate) fn text(view: &ReviewView, lines: std::ops::Range<usize>) -> Vec<String> {
    render::lines(view)
        .into_iter()
        .skip(lines.start)
        .take(lines.len())
        .map(|drawn| drawn.content.text())
        .collect()
}

/// A text selection settled over `lines`: the diff lines it covered are
/// marked, and what it covered is the excerpt the comment quotes.
pub(crate) fn selected(view: &mut ReviewView, lines: std::ops::Range<usize>, text: &str) {
    if view.composer.is_some() {
        return;
    }
    let drawn = render::lines(view);
    let cells: Vec<usize> = drawn
        .iter()
        .skip(lines.start)
        .take(lines.len().max(1))
        .filter_map(|drawn| match drawn.line {
            Line::Cell(cell) => Some(cell),
            _ => None,
        })
        .collect();
    let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
        return;
    };
    view.mark = Some((*first, *last));
    view.cursor = *last;
    view.focus = Focus::Content;
    let excerpt = compose::clean(text.trim());
    view.excerpt = match excerpt.chars().count() > 160 {
        true => format!("{}…", excerpt.chars().take(160).collect::<String>()),
        false => excerpt,
    };
}

/// What a gesture asked, as the host hears it.
pub(crate) fn outcome(asked: Asked) -> Outcome {
    match asked {
        Asked::Close => Outcome::Close,
        Asked::Stay | Asked::Send | Asked::OpenInCode => Outcome::Stay,
    }
}
