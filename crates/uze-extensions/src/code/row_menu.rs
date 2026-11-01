//! What can be done to one row of the tree, file or directory, where the
//! thing itself rather than a change to it is the subject: take its path,
//! rename it, or delete it. Throwing a change away is the review's.

use std::path::{Path, PathBuf};

use super::{CodeOutcome, CodeView, Deleting, request::FileRequest};
use crate::view::{Command, Confirm, Role, RowMenu, Span, ViewHit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    /// The path as the checkout spells it, the way it is pasted into a
    /// review or a prompt.
    CopyRelativePath,
    /// The path as the machine spells it, for a tool outside the checkout.
    CopyAbsolutePath,
    /// Asks for the new name in a dialog: see [`Renaming`].
    Rename,
    /// Asked first, through the files half's own question.
    Delete,
}

/// A new name being typed for `path`, starting from the one it has.
pub(super) struct Renaming {
    path: PathBuf,
    name: String,
}

/// The menu open on one file. Held by path rather than by row, so a
/// refresh that reorders the list leaves it on the file it was opened on.
pub(super) struct FileMenu {
    path: PathBuf,
    directory: bool,
    actions: Vec<Action>,
    highlighted: usize,
}

impl FileMenu {
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

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    fn label(&self, action: Action) -> String {
        match action {
            Action::CopyRelativePath => "Copy relative path".to_owned(),
            Action::CopyAbsolutePath => "Copy path".to_owned(),
            Action::Rename => "Rename…".to_owned(),
            Action::Delete => "Delete…".to_owned(),
        }
    }
}

/// Opens the menu on a row of the tree, selecting it.
pub(super) fn open_in_tree(view: &mut CodeView, path: PathBuf, directory: bool) {
    // Selected without being read: a directory has nothing to read, and
    // a file is read when it is opened, not when its menu is.
    view.selected = Some(path.clone());
    view.menu = Some(FileMenu {
        path,
        directory,
        actions: vec![
            Action::CopyAbsolutePath,
            Action::CopyRelativePath,
            Action::Rename,
            Action::Delete,
        ],
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
        Action::CopyRelativePath => return CodeOutcome::Copy(relative(view, &menu.path)),
        Action::CopyAbsolutePath => {
            return CodeOutcome::Copy(menu.path.to_string_lossy().into_owned());
        }
        Action::Rename => {
            view.renaming = Some(Renaming {
                name: file_name(&menu.path),
                path: menu.path,
            });
        }
        Action::Delete => {
            view.confirming_delete = Some(Deleting {
                path: menu.path,
                directory: menu.directory,
                on_confirm: false,
            });
        }
    }
    CodeOutcome::Stay
}

fn relative(view: &CodeView, path: &Path) -> String {
    path.strip_prefix(&view.root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The rename being typed, as the dialog the host draws.
pub(super) fn rename_confirm(view: &CodeView) -> Option<Confirm> {
    let renaming = view.renaming.as_ref()?;
    Some(Confirm {
        title: "Rename".to_owned(),
        subject: relative(view, &renaming.path),
        body: "A new name in the same folder.".to_owned(),
        confirm: "Rename".to_owned(),
        on_confirm: true,
        field: Some(renaming.name.clone()),
    })
}

/// A command while a new name is being typed. Modal like the other
/// questions: enter renames, escape leaves, and everything else is text.
pub(super) fn rename_command(view: &mut CodeView, command: Command) {
    let Some(renaming) = view.renaming.as_mut() else {
        return;
    };
    match command {
        Command::Type(character) => renaming.name.push(character),
        Command::EraseBack => {
            renaming.name.pop();
        }
        Command::Activate | Command::Newline | Command::Save => rename(view),
        Command::Close => view.renaming = None,
        _ => {}
    }
}

/// A click while a new name is being typed: only its answers mean
/// anything.
pub(super) fn rename_mouse(view: &mut CodeView, hit: Option<ViewHit>) {
    match hit {
        Some(ViewHit::Answer(true)) => rename(view),
        Some(ViewHit::Answer(false)) => view.renaming = None,
        _ => {}
    }
}

/// Asks for the rename typed, or says why it cannot be one. The dialog
/// stays open on a refusal, so the name can be corrected rather than
/// typed again.
fn rename(view: &mut CodeView) {
    let Some(renaming) = view.renaming.as_ref() else {
        return;
    };
    let name = renaming.name.trim();
    let refusal = match name {
        "" => Some("a name is needed"),
        "." | ".." => Some("that name is taken by the directory itself"),
        _ if name.contains(['/', '\\']) => Some("a name cannot hold a path separator"),
        _ => None,
    };
    if let Some(refusal) = refusal {
        view.notice = Some(Span::new(refusal, Role::Warning));
        return;
    }
    let Some(renaming) = view.renaming.take() else {
        return;
    };
    let to = renaming.path.with_file_name(renaming.name.trim());
    if to != renaming.path {
        view.queue.push_back(FileRequest::Rename {
            root: view.root.clone(),
            from: renaming.path,
            to,
        });
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
