//! What can be done to one changed file, offered on its row.
//!
//! Three things, the ones a reviewer reaches for with a diff open: read
//! the file whole, take its path somewhere else, or throw the change
//! away. Nothing that stages or commits: the checkout belongs to the
//! agent working in it, and the index is what that agent's next commit is
//! made of. Editing and deleting stay in the files half, where the file
//! is the subject rather than the change to it.

use std::path::PathBuf;

use super::{CodeOutcome, CodeView, ContentMode, Focus, changes::FileStatus, request::FileRequest};
use crate::view::{Command, Confirm, RowMenu, ViewHit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    OpenFile,
    CopyPath,
    /// Throwing a change away cannot be undone, so this only asks: see
    /// [`Discarding`].
    Discard,
}

/// A discard waiting on its answer, asked as a dialog rather than as one
/// more row of the menu, which reads as another option instead of a
/// question. The keyboard starts on the way out.
pub(super) struct Discarding {
    path: PathBuf,
    on_confirm: bool,
}

/// The menu open on one changed file. Held by path rather than by row, so
/// a refresh that reorders the list leaves it on the file it was opened on.
pub(super) struct ChangeMenu {
    path: PathBuf,
    actions: Vec<Action>,
    highlighted: usize,
}

impl ChangeMenu {
    /// The entries as the host draws them, on the row `row`.
    pub(super) fn describe(&self, row: usize) -> RowMenu {
        RowMenu {
            row,
            entries: self
                .actions
                .iter()
                .map(|action| self.label(*action))
                .collect(),
            highlighted: self.highlighted,
        }
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn label(&self, action: Action) -> String {
        match action {
            Action::OpenFile => "Open file".to_owned(),
            Action::CopyPath => "Copy path".to_owned(),
            Action::Discard => "Discard changes…".to_owned(),
        }
    }
}

/// Opens the menu on the changed file at `index` in the list, selecting it.
pub(super) fn open(view: &mut CodeView, index: usize) {
    let Some(file) = view.changes.files.get(index) else {
        return;
    };
    let path = file.path.clone();
    // A deleted file has nothing on disk to open.
    let mut actions = match file.status {
        super::changes::FileStatus::Deleted => Vec::new(),
        _ => vec![Action::OpenFile],
    };
    actions.extend([Action::CopyPath, Action::Discard]);
    view.select(path.clone());
    view.menu = Some(ChangeMenu {
        path,
        actions,
        highlighted: 0,
    });
}

/// A command while the menu is open. Every command is the menu's: one that
/// does not move or pick shuts it, the way a click outside it does.
pub(super) fn command(view: &mut CodeView, command: Command) -> CodeOutcome {
    let Some(menu) = view.menu.as_mut() else {
        return CodeOutcome::Stay;
    };
    match command {
        Command::SelectNext => {
            menu.highlighted = (menu.highlighted + 1).min(menu.actions.len().saturating_sub(1));
            CodeOutcome::Stay
        }
        Command::SelectPrevious => {
            menu.highlighted = menu.highlighted.saturating_sub(1);
            CodeOutcome::Stay
        }
        Command::Activate => {
            let picked = menu.highlighted;
            perform(view, picked)
        }
        _ => {
            view.menu = None;
            CodeOutcome::Stay
        }
    }
}

/// A click while the menu is open: an entry is picked, anywhere else
/// shuts it and does nothing more.
pub(super) fn mouse(view: &mut CodeView, hit: Option<ViewHit>) -> CodeOutcome {
    match hit {
        Some(ViewHit::MenuEntry(entry)) => perform(view, entry),
        _ => {
            view.menu = None;
            CodeOutcome::Stay
        }
    }
}

/// The pointer over an entry highlights it, the way the arrows do.
/// Answers whether the highlight moved.
pub(super) fn hover(view: &mut CodeView, hit: Option<ViewHit>) -> bool {
    match (view.menu.as_mut(), hit) {
        (Some(menu), Some(ViewHit::MenuEntry(entry)))
            if entry < menu.actions.len() && entry != menu.highlighted =>
        {
            menu.highlighted = entry;
            true
        }
        _ => false,
    }
}

fn perform(view: &mut CodeView, entry: usize) -> CodeOutcome {
    let Some(menu) = view.menu.take() else {
        return CodeOutcome::Stay;
    };
    let Some(action) = menu.actions.get(entry).copied() else {
        return CodeOutcome::Stay;
    };
    match action {
        Action::OpenFile => {
            view.show(match view.selected_is_markdown() {
                true => ContentMode::Preview,
                false => ContentMode::Contents,
            });
            view.focus = Focus::Content;
        }
        Action::CopyPath => {
            let relative = menu.path.strip_prefix(&view.root).unwrap_or(&menu.path);
            return CodeOutcome::Copy(relative.to_string_lossy().into_owned());
        }
        Action::Discard => {
            view.discarding = Some(Discarding {
                path: menu.path,
                on_confirm: false,
            });
        }
    }
    CodeOutcome::Stay
}

/// The question an open discard asks, for the host to draw.
pub(super) fn confirm(view: &CodeView) -> Option<Confirm> {
    let discarding = view.discarding.as_ref()?;
    let file = view
        .changes
        .position_of(Some(&discarding.path))
        .map(|index| &view.changes.files[index]);
    let body = match file.map(|file| (file.status, file.renamed_from.as_ref())) {
        Some((FileStatus::Untracked | FileStatus::Added, _)) => {
            "Deletes the file, which the last commit does not have. This cannot be undone."
                .to_owned()
        }
        Some((_, Some(from))) => format!(
            "Takes the new name off and brings {} back the way the last commit has it. \
             This cannot be undone.",
            relative(view, from)
        ),
        _ => "Puts the file back the way the last commit has it. This cannot be undone.".to_owned(),
    };
    Some(Confirm {
        title: "Discard changes".to_owned(),
        subject: relative(view, &discarding.path),
        body,
        confirm: "Discard".to_owned(),
        on_confirm: discarding.on_confirm,
    })
}

/// A command while a discard is being asked about. The question is modal:
/// what does not answer it or move between its answers does nothing.
pub(super) fn answer_command(view: &mut CodeView, command: Command) {
    let Some(discarding) = view.discarding.as_mut() else {
        return;
    };
    match command {
        Command::Activate => {
            let yes = discarding.on_confirm;
            answer(view, yes);
        }
        Command::ConfirmDelete => answer(view, true),
        Command::Close => answer(view, false),
        Command::FocusNext | Command::Collapse | Command::Expand => {
            discarding.on_confirm = !discarding.on_confirm;
        }
        _ => {}
    }
}

/// A click while a discard is being asked about: only its answers mean
/// anything.
pub(super) fn answer_mouse(view: &mut CodeView, hit: Option<ViewHit>) {
    if let Some(ViewHit::Answer(yes)) = hit {
        answer(view, yes);
    }
}

fn answer(view: &mut CodeView, yes: bool) {
    let Some(discarding) = view.discarding.take() else {
        return;
    };
    if !yes {
        return;
    }
    let mut paths = vec![discarding.path.clone()];
    // A rename is two paths, and throwing it away brings the old name back
    // as well as taking the new one off.
    if let Some(from) = view
        .changes
        .position_of(Some(&discarding.path))
        .and_then(|index| view.changes.files[index].renamed_from.clone())
    {
        paths.push(from);
    }
    view.queue.push_back(FileRequest::Restore {
        root: view.root.clone(),
        paths,
    });
}

fn relative(view: &CodeView, path: &std::path::Path) -> String {
    path.strip_prefix(&view.root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
