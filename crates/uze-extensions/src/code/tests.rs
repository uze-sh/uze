//! What the code surface must keep doing.
//!
//! Two kinds here, deliberately mixed: fixtures that build a surface in
//! memory and prove what it *describes*, and a handful that drive real
//! `git` in a scratch repository, because the porcelain this parses is a
//! contract with a program nobody here controls.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use super::{
    changes::{ChangedFile, Changes, FileStatus},
    diff,
    render::changes_navigator as navigator,
    *,
};
use crate::{
    DirEntry,
    shared::highlight::FALLBACK_SYNTAX_THEME,
    view::{
        Command, Content, LineTone, MarkerSide, NavigatorRow, Role, RowIcon, RowMark, Size, Span,
        ViewHit,
    },
};

fn space() -> Size {
    Size {
        width: 80,
        height: 20,
    }
}

/// A changed file read whole, the way the changes list offers it: its
/// menu, then the first entry.
fn open_the_file_from_its_menu(view: &mut CodeView) {
    press(view, Command::OpenMenu);
    press(view, Command::Activate);
}

/// One command, the way the host hands one down.
fn press(view: &mut CodeView, command: Command) -> CodeOutcome {
    handle_command(view, command, space())
}

/// Types a string, one character at a time, the way `Resolution::Text`
/// reaches the surface.
fn type_text(view: &mut CodeView, text: &str) {
    for character in text.chars() {
        press(view, Command::Type(character));
    }
}

/// The two halves in memory, with no host and no disk — what most of
/// these are about.
fn surface(root: &Path, files: Vec<ChangedFile>, selected: usize) -> CodeView {
    let selected_path = files.get(selected).map(|file| file.path.clone());
    CodeView {
        branch: "main".to_owned(),
        selected: selected_path,
        changes: Changes {
            files,
            ..Changes::default()
        },
        ..CodeView::opening(
            root.to_path_buf(),
            root.display().to_string(),
            ContentMode::Diff,
        )
    }
}

/// Two changed files, opened on the second, with a diff already read.
fn fixture() -> CodeView {
    let root = PathBuf::from("/repo");
    let mut view = surface(
        &root,
        vec![
            ChangedFile {
                status: FileStatus::Modified,
                path: root.join("src/ui/git_diff.rs"),
                renamed_from: None,
            },
            ChangedFile {
                status: FileStatus::Added,
                path: root.join("src/ui.rs"),
                renamed_from: None,
            },
        ],
        1,
    );
    view.changes.diff = diff::read(
        "@@ -1,3 +1,4 @@\n context\n-removed line\n+added line\n",
        Path::new("/repo/src/ui.rs"),
        FALLBACK_SYNTAX_THEME,
    );
    view.changes.diff_pending = false;
    view
}

/// Git's porcelain for three files under two directories, in the order
/// the index keeps them, opened on the deepest one.
fn flat_fixture() -> CodeView {
    let root = PathBuf::from("/repo");
    let files = changes::parse_porcelain_status(
        " M src/ui/git_diff.rs\0A  src/ui.rs\0?? README.md\0 M src/Main.rs\0",
        &root,
    );
    let deepest = files
        .iter()
        .position(|file| file.path.ends_with("git_diff.rs"))
        .expect("the deepest file is listed");
    let mut view = surface(&root, files, deepest);
    view.changes.diff_pending = false;
    view
}

/// Each row as the reader scans it: the name, then where it sits.
fn flat_rows(view: &CodeView) -> Vec<(String, String)> {
    navigator(view)
        .rows
        .into_iter()
        .filter_map(|row| match row {
            NavigatorRow::Item { name, detail, .. } => Some((name, detail)),
            _ => None,
        })
        .collect()
}

/// The changes are one flat list: no row for a directory, every file at
/// the same depth, named first with its directory after it, grouped by
/// that directory and by name within it — whatever order Git kept them in.
#[test]
fn the_changes_are_a_flat_list_named_first_and_placed_after() {
    let view = flat_fixture();
    let rows = navigator(&view).rows;

    assert!(
        rows.iter().all(|row| matches!(
            row,
            NavigatorRow::Item {
                depth: 0,
                marker_side: MarkerSide::Trailing,
                ..
            }
        )),
        "no directory rows, no indent, the status at the right edge"
    );
    assert_eq!(
        flat_rows(&view),
        [
            ("README.md".to_owned(), String::new()),
            ("Main.rs".to_owned(), "src".to_owned()),
            ("ui.rs".to_owned(), "src".to_owned()),
            ("git_diff.rs".to_owned(), "src/ui".to_owned()),
        ]
    );
    assert_eq!(navigator(&view).anchor, Some(3), "the selection is its row");
}

/// The arrows walk the list as drawn, and hold at either end.
#[test]
fn the_arrows_walk_the_list_as_drawn() {
    let mut view = flat_fixture();

    press(&mut view, Command::SelectNext);
    assert_eq!(view.selected_change(), Some(3), "the last row holds");
    press(&mut view, Command::SelectPrevious);
    assert_eq!(view.selected_change(), Some(2));
    for _ in 0..5 {
        press(&mut view, Command::SelectPrevious);
    }
    assert_eq!(view.selected_change(), Some(0), "the first row holds");
}

/// Nothing in a flat list opens or closes, so neither is offered.
#[test]
fn a_flat_list_offers_no_folding() {
    let mut view = flat_fixture();
    press(&mut view, Command::Collapse);
    assert_eq!(flat_rows(&view).len(), 4);
    assert!(
        !super::view(&view, space())
            .footer
            .iter()
            .any(|command| matches!(command, Command::Collapse | Command::Expand))
    );
}

/// The entries of the menu the view describes, and the row it is on.
fn menu_of(view: &CodeView) -> Option<(usize, Vec<String>, usize)> {
    navigator(view)
        .menu
        .map(|menu| (menu.row, menu.entries, menu.highlighted))
}

/// The secondary button on a row opens that file's actions and selects
/// it; while the menu is open the footer speaks only for the menu.
#[test]
fn a_changed_file_offers_its_actions_on_its_row() {
    let mut view = flat_fixture();

    handle_mouse(&mut view, Some(ViewHit::OpenMenu(1)), space());

    assert_eq!(view.selected_change(), Some(1), "the row it was opened on");
    assert_eq!(
        menu_of(&view),
        Some((
            1,
            vec![
                "Open file".to_owned(),
                "Copy path".to_owned(),
                "Discard changes…".to_owned()
            ],
            0
        ))
    );
    assert_eq!(
        super::view(&view, space()).footer,
        [Command::SelectNext, Command::Activate, Command::Close]
    );
}

/// The keyboard reaches the same menu, and the changes list offers it in
/// place of editing and deleting, which belong to the files half.
#[test]
fn the_changes_list_offers_the_menu_rather_than_edit_and_delete() {
    let mut view = flat_fixture();
    let footer = super::view(&view, space()).footer;
    assert!(footer.contains(&Command::OpenMenu));
    assert!(!footer.contains(&Command::Edit) && !footer.contains(&Command::Delete));

    press(&mut view, Command::Delete);
    assert!(
        view.confirming_delete.is_none(),
        "nothing is deleted from here"
    );

    press(&mut view, Command::OpenMenu);
    assert_eq!(menu_of(&view).map(|(row, ..)| row), Some(3));
}

/// Anything that is not a move or a pick shuts the menu and does nothing
/// else — a key or a click elsewhere alike.
#[test]
fn the_menu_shuts_on_anything_that_is_not_meant_for_it() {
    let mut view = flat_fixture();
    press(&mut view, Command::OpenMenu);
    press(&mut view, Command::Close);
    assert!(menu_of(&view).is_none());
    assert!(matches!(
        press(&mut view, Command::Close),
        CodeOutcome::Close
    ));

    let mut view = flat_fixture();
    press(&mut view, Command::OpenMenu);
    handle_mouse(&mut view, Some(ViewHit::SelectItem(0)), space());
    assert!(menu_of(&view).is_none());
    assert_eq!(view.selected_change(), Some(3), "the click only shut it");
}

/// The pointer over an entry highlights it, the way the arrows do, and
/// only a move onto another entry asks for a new frame.
#[test]
fn the_pointer_highlights_the_entry_it_is_over() {
    let mut view = flat_fixture();
    press(&mut view, Command::OpenMenu);

    assert!(handle_hover(&mut view, Some(ViewHit::MenuEntry(2))));
    assert_eq!(menu_of(&view).map(|(.., highlighted)| highlighted), Some(2));
    assert!(
        !handle_hover(&mut view, Some(ViewHit::MenuEntry(2))),
        "already there"
    );
    assert!(
        !handle_hover(&mut view, Some(ViewHit::SelectItem(0))),
        "off the menu"
    );
    assert_eq!(menu_of(&view).map(|(.., highlighted)| highlighted), Some(2));
}

/// A deleted file has nothing on disk to open.
#[test]
fn a_deleted_file_is_not_offered_to_open() {
    let root = PathBuf::from("/repo");
    let mut view = surface(
        &root,
        changes::parse_porcelain_status(" D gone.rs\0", &root),
        0,
    );
    press(&mut view, Command::OpenMenu);
    let (_, entries, _) = menu_of(&view).expect("the menu opened");
    assert_eq!(entries, ["Copy path", "Discard changes…"]);
}

/// The path goes to the host's clipboard, relative to the checkout, the
/// way a reviewer pastes it anywhere else.
#[test]
fn copying_a_path_hands_the_host_the_checkouts_own_spelling() {
    let mut view = flat_fixture();
    press(&mut view, Command::OpenMenu);
    press(&mut view, Command::SelectNext);
    assert_eq!(
        press(&mut view, Command::Activate),
        CodeOutcome::Copy("src/ui/git_diff.rs".to_owned())
    );
    assert!(menu_of(&view).is_none());
}

/// Picks "Discard changes…" from the selected file's menu.
fn ask_to_discard(view: &mut CodeView) {
    press(view, Command::OpenMenu);
    press(view, Command::SelectNext);
    press(view, Command::SelectNext);
    press(view, Command::Activate);
}

/// Throwing a change away cannot be undone, so it is asked as a dialog,
/// with the keyboard on the way out; only a yes reaches the machine, and
/// the list is read again at once.
#[test]
fn discarding_a_change_asks_first_and_then_restores_it() {
    let machine = FakeMachine::default();
    let mut view = flat_fixture();

    ask_to_discard(&mut view);
    assert!(
        menu_of(&view).is_none(),
        "the menu gave way to the question"
    );
    let asked = super::view(&view, space()).confirm.expect("a question");
    assert_eq!(
        (
            asked.title.as_str(),
            asked.subject.as_str(),
            asked.confirm.as_str()
        ),
        ("Discard changes", "src/ui/git_diff.rs", "Discard")
    );
    assert!(!asked.on_confirm, "the keyboard starts on the way out");

    press(&mut view, Command::SelectNext);
    assert!(view.discarding.is_some(), "the question holds the keyboard");
    press(&mut view, Command::Activate);
    assert!(
        view.discarding.is_none() && view.peek_request().is_none(),
        "no restores nothing"
    );

    ask_to_discard(&mut view);
    press(&mut view, Command::Expand);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    assert_eq!(
        *machine.restored.borrow(),
        [PathBuf::from("/repo/src/ui/git_diff.rs")]
    );
    assert!(view.refresh_due(), "the list is read again now");
    assert!(
        view.diff_pending(),
        "and the diff of a change that is gone with it"
    );
}

/// The dialog's buttons answer it, and nothing else on the surface does.
#[test]
fn a_click_answers_the_question_only_on_its_buttons() {
    let machine = FakeMachine::default();
    let mut view = flat_fixture();
    ask_to_discard(&mut view);

    handle_mouse(&mut view, Some(ViewHit::SelectItem(0)), space());
    assert!(view.discarding.is_some());
    assert_eq!(view.selected_change(), Some(3), "nothing behind it moved");

    handle_mouse(&mut view, Some(ViewHit::Answer(true)), space());
    settle(&mut view, &machine);
    assert_eq!(machine.restored.borrow().len(), 1);
}

/// What agreeing does is said for the kind of change it undoes.
#[test]
fn the_question_says_what_discarding_this_change_does() {
    let root = PathBuf::from("/repo");
    let body = |porcelain: &str| {
        let mut view = surface(&root, changes::parse_porcelain_status(porcelain, &root), 0);
        ask_to_discard(&mut view);
        super::view(&view, space()).confirm.expect("asked").body
    };
    assert!(body("?? new.rs\0").starts_with("Deletes the file"));
    assert!(body(" M kept.rs\0").starts_with("Puts the file back"));
    assert!(body("R  new.rs\0old.rs\0").contains("brings old.rs back"));
}

/// A rename is two paths, and throwing it away puts the old one back.
#[test]
fn discarding_a_rename_restores_both_of_its_paths() {
    let machine = FakeMachine::default();
    let root = PathBuf::from("/repo");
    let mut view = surface(
        &root,
        changes::parse_porcelain_status("R  new.rs\0old.rs\0", &root),
        0,
    );
    ask_to_discard(&mut view);
    press(&mut view, Command::ConfirmDelete);
    settle(&mut view, &machine);

    assert_eq!(
        *machine.restored.borrow(),
        [root.join("new.rs"), root.join("old.rs")]
    );
}

/// The view carries meaning, never appearance: a status mark is a
/// [`Role`], not a colour, so the host's palette stays the only place
/// chrome colour is decided.
#[test]
fn the_view_names_meaning_rather_than_colour() {
    let view = view(&fixture(), space());
    let navigator = view.navigator.expect("files to navigate");
    assert_eq!(navigator.badge, "2");
    assert!(navigator.focused);

    let marker = navigator
        .rows
        .iter()
        .find_map(|row| match row {
            NavigatorRow::Item { name, marker, .. } if name == "ui.rs" => Some(marker),
            _ => None,
        })
        .expect("the added file is listed");
    assert_eq!(marker.role, Role::Success, "added, not a colour");
    assert!(
        marker.color.is_none(),
        "chrome never carries its own colour"
    );
}

/// Syntax colour is the one thing that does travel as data: it comes
/// from a theme the extension ships, and a role would throw it away.
#[test]
fn syntax_colour_survives_as_the_extensions_own_data() {
    let view = view(&fixture(), space());
    let Content::Lines { lines, .. } = view.content else {
        panic!("a selected file has a diff");
    };
    assert!(lines.iter().any(|line| line.tone == LineTone::Added));
    assert!(lines.iter().any(|line| line.tone == LineTone::Removed));
    assert!(
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .any(|span| span.color.is_some()),
        "highlighting reaches the host"
    );
}

/// Outside a git repository the *changes* have nothing to say. The
/// surface does not: the tree still works, which is why that failure is
/// content rather than the whole view.
#[test]
fn a_checkout_that_is_no_repository_still_has_a_tree() {
    let mut view = CodeView::opening(
        PathBuf::from("/nope"),
        "/nope".to_owned(),
        ContentMode::Diff,
    );
    view.changes.error = Some("not a git repository".to_owned());
    view.changes.diff_pending = false;

    let rendered = super::view(&view, space());
    assert!(
        rendered.navigator.is_some(),
        "the file tree is unaffected by git being absent"
    );
    assert!(matches!(
        rendered.content,
        Content::Message {
            role: Role::Danger,
            ..
        }
    ));
}

/// Moving the selection reads nothing — the property that keeps an arrow
/// key off the thread that draws.
#[test]
fn selecting_a_file_asks_for_its_diff_rather_than_reading_it() {
    let mut open = fixture();
    assert!(!open.diff_pending(), "a fresh surface shows what it read");

    let outcome = press(&mut open, Command::SelectPrevious);

    assert!(matches!(outcome, CodeOutcome::Stay));
    assert_eq!(open.selected_change(), Some(0), "the selection moved");
    assert!(open.diff_pending(), "and a read is owed for it");
    assert!(
        open.changes.diff.is_empty(),
        "the previous file's diff is not left under the new name"
    );

    assert!(
        matches!(&view(&open, space()).content, Content::Message { text, .. } if text == "reading…"),
    );
}

/// The wheel over the content scrolls the content and nothing else: the
/// selection is not a scroll position.
#[test]
fn the_wheel_scrolls_the_content_and_leaves_the_selection_alone() {
    let mut view = fixture();

    handle_scroll(&mut view, ScrollDirection::Down, false);

    assert_eq!(view.scroll, 3);
    assert_eq!(view.selected_change(), Some(1));
    assert!(!view.diff_pending());
}

/// The wheel stops once the last line is on screen: past it the content
/// would scroll off into blank rows, and each notch there is one more to
/// take back before anything moves on the way up.
#[test]
fn the_wheel_stops_once_the_last_line_is_on_screen() {
    let mut view = fixture();
    handle_scroll(&mut view, ScrollDirection::Down, false);

    handle_scroll(&mut view, ScrollDirection::Down, true);
    assert_eq!(view.scroll, 3, "held where the end came into view");

    handle_scroll(&mut view, ScrollDirection::Up, true);
    assert_eq!(view.scroll, 0, "and the first notch back moves it");
}

/// The section names meaning, never colour — the same contract the
/// full-frame view is held to.
#[test]
fn the_timeline_section_names_meaning_rather_than_colour() {
    let timeline = Timeline {
        branch: "agent/x".to_owned(),
        commits: vec![
            Commit {
                hash: "a".to_owned(),
                subject: "feat: ahead".to_owned(),
                age: "3h".to_owned(),
                ahead: true,
            },
            Commit {
                hash: "b".to_owned(),
                subject: "chore: landed".to_owned(),
                age: "2d".to_owned(),
                ahead: false,
            },
        ],
    };

    let section = timeline_section(&timeline, false, 0);

    assert_eq!(section.title, "timeline");
    assert_eq!(section.caption.text, "agent/x");
    assert!(section.resizable);
    // HEAD is ringed; standing is the hue, and the hue is a role.
    assert_eq!(section.rows[0].mark, RowMark::Head);
    assert_eq!(section.rows[0].mark_role, Role::Info);
    assert_eq!(section.rows[1].mark, RowMark::Commit);
    assert_eq!(section.rows[1].mark_role, Role::Warning);
    assert_eq!(section.rows[0].trailing.text, "3h");

    let folded = timeline_section(&timeline, true, 0);
    assert!(folded.collapsed);
    assert_eq!(
        folded.rows.len(),
        section.rows.len(),
        "folding is the host's to draw; the section still holds what it holds"
    );
}

// --- the files half, and the switch between them -------------------------

/// The same grant the workspace client makes, for the tests that drive real
/// `git` in a scratch repository. A fake is the right tool for testing *the
/// view*; these test what the view reads.
pub(super) struct RepositoryHost;

impl Host for RepositoryHost {
    fn git(&self, root: &Path, args: &[&str], answers: &[i32]) -> Result<String, String> {
        uze_git::read(root, args)
            .map_err(|error| error.to_string())?
            .or_exit(answers)
    }

    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        uze_git::repository::root(path)
    }

    fn read_file(&self, path: &Path) -> Result<String, crate::Unreadable> {
        std::fs::read_to_string(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::InvalidData => crate::Unreadable::NotText,
            _ => crate::Unreadable::Failed(error.to_string()),
        })
    }

    fn syntax_theme(&self) -> String {
        FALLBACK_SYNTAX_THEME.to_owned()
    }

    fn list_dir(&self, _path: &Path) -> Result<Vec<DirEntry>, String> {
        unreachable!("what the repository tests read, they read through `git`")
    }

    fn write_file(&self, _path: &Path, _contents: &str) -> Result<(), String> {
        unreachable!("reading a repository writes nothing")
    }

    fn delete_file(&self, _path: &Path) -> Result<(), String> {
        unreachable!("reading a repository deletes nothing")
    }

    fn restore_to_head(&self, _root: &Path, _paths: &[PathBuf]) -> Result<(), String> {
        unreachable!("reading a repository restores nothing")
    }
}

/// A repository that answers the same thing every time, and counts how
/// often it was asked to diff.
struct StillRepository {
    diffs: std::cell::Cell<usize>,
}

impl Host for StillRepository {
    fn git(&self, _root: &Path, args: &[&str], _answers: &[i32]) -> Result<String, String> {
        match args.first() {
            Some(&"status") => Ok(" M src/ui.rs\n".to_owned()),
            Some(&"diff") => {
                self.diffs.set(self.diffs.get() + 1);
                Ok("@@ -1,2 +1,2 @@\n context\n-old line\n+new line\n".to_owned())
            }
            _ => Ok("main\n".to_owned()),
        }
    }

    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        Ok(path.to_path_buf())
    }

    fn read_file(&self, _path: &Path) -> Result<String, crate::Unreadable> {
        Err(crate::Unreadable::Failed("nothing is read here".to_owned()))
    }

    fn list_dir(&self, _path: &Path) -> Result<Vec<DirEntry>, String> {
        Ok(Vec::new())
    }

    fn write_file(&self, _path: &Path, _contents: &str) -> Result<(), String> {
        unreachable!("reading a repository writes nothing")
    }

    fn delete_file(&self, _path: &Path) -> Result<(), String> {
        unreachable!("reading a repository deletes nothing")
    }

    fn restore_to_head(&self, _root: &Path, _paths: &[PathBuf]) -> Result<(), String> {
        unreachable!("reading a repository restores nothing")
    }

    fn syntax_theme(&self) -> String {
        FALLBACK_SYNTAX_THEME.to_owned()
    }
}

/// The changes are re-read on a timer, because the checkout is under an
/// agent's hands. Colouring a diff that did not move is the same picture
/// drawn twice — the one unbounded cost this surface pays over and over
/// if nothing notices.
#[test]
fn a_diff_that_has_not_moved_is_not_coloured_again() {
    let host = StillRepository {
        diffs: std::cell::Cell::new(0),
    };
    let root = PathBuf::from("/repo");
    let mut view = CodeView::opening(root.clone(), "/repo".to_owned(), ContentMode::Diff);

    // The first read has no selection yet, so there is no diff to ask
    // about; it is what lands the selection.
    let first = CodeView::refresh(&host, root.clone(), view.placement());
    assert!(!first.changes.diff_unchanged);
    view.absorb_changes(first);
    let second = CodeView::refresh(&host, root.clone(), view.placement());
    assert!(
        !second.changes.diff_unchanged,
        "the selected file's own diff"
    );
    view.absorb_changes(second);
    let coloured = view.changes.diff.len();
    assert!(coloured > 0, "there is a diff on screen");

    let third = CodeView::refresh(&host, root.clone(), view.placement());
    assert!(
        third.changes.diff_unchanged,
        "the same bytes in the same palette are the same picture"
    );
    assert!(third.changes.diff.is_empty(), "so none of it travels back");
    view.absorb_changes(third);
    assert_eq!(
        view.changes.diff.len(),
        coloured,
        "and what is on screen stays on screen"
    );
    assert_eq!(
        host.diffs.get(),
        2,
        "git is still asked every time — only the colouring is skipped"
    );
}

/// Moving the selection costs the one diff it asks for: never a `status`
/// of the whole checkout, which on a large repository is the difference
/// between a click and a wait.
#[test]
fn moving_the_selection_reads_one_diff_and_no_status() {
    let host = StillRepository {
        diffs: std::cell::Cell::new(0),
    };
    let mut open = fixture();
    press(&mut open, Command::SelectPrevious);
    let request = open
        .diff_request()
        .expect("the moved selection is owed a diff");

    let answer = CodeView::read_diff(&host, &PathBuf::from("/repo"), request);
    open.absorb_diff(answer);

    assert_eq!(host.diffs.get(), 1);
    assert!(
        !open.diff_pending(),
        "the diff on screen is the selection's"
    );
    assert!(!open.changes.diff.is_empty());
    assert!(open.diff_request().is_none(), "and nothing more is owed");
}

/// A periodic re-read asked for before the selection moved lands after
/// the selection's own diff did. It brings the list, and leaves the diff
/// on screen alone — wiping it would show the viewer an empty file and
/// ask for the same diff twice.
#[test]
fn a_refresh_from_before_the_selection_moved_keeps_the_new_diff() {
    let host = StillRepository {
        diffs: std::cell::Cell::new(0),
    };
    let mut open = fixture();
    let before = open.placement();
    press(&mut open, Command::SelectPrevious);
    let request = open.diff_request().expect("owed a diff");
    open.absorb_diff(CodeView::read_diff(&host, &PathBuf::from("/repo"), request));
    let shown = open.changes.diff.len();

    let files = open.changes.files.clone();
    open.absorb_changes(RefreshedChanges {
        placement: before,
        branch: "main".to_owned(),
        changes: Changes {
            files,
            ..Changes::default()
        },
    });

    assert_eq!(open.changes.diff.len(), shown, "the new diff stays");
    assert!(!open.diff_pending(), "and is not asked for again");
}

/// A filesystem that only ever existed in memory, so these prove the
/// surface's own behaviour rather than a temp directory's.
#[derive(Default)]
struct FakeMachine {
    directories: BTreeMap<PathBuf, Vec<DirEntry>>,
    files: RefCell<BTreeMap<PathBuf, String>>,
    restored: RefCell<Vec<PathBuf>>,
}

impl FakeMachine {
    fn with_file(mut self, path: &str, contents: &str) -> Self {
        let path = PathBuf::from(path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let parent = path.parent().unwrap().to_path_buf();
        self.directories.entry(parent).or_default().push(DirEntry {
            directory: false,
            name,
        });
        self.files.borrow_mut().insert(path, contents.to_owned());
        self
    }

    fn with_directory(mut self, path: &str) -> Self {
        let path = PathBuf::from(path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let parent = path.parent().unwrap().to_path_buf();
        self.directories.entry(parent).or_default().push(DirEntry {
            directory: true,
            name,
        });
        self.directories.entry(path).or_default();
        self
    }
}

impl Host for FakeMachine {
    fn git(&self, _root: &Path, _args: &[&str], _answers: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }

    fn repository_root(&self, _path: &Path) -> Result<PathBuf, String> {
        Err("no git here".to_owned())
    }

    fn read_file(&self, path: &Path) -> Result<String, crate::Unreadable> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or(crate::Unreadable::NotText)
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, String> {
        self.directories
            .get(path)
            .cloned()
            .ok_or_else(|| format!("no such directory: {}", path.display()))
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), String> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), contents.to_owned());
        Ok(())
    }

    fn delete_file(&self, path: &Path) -> Result<(), String> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }

    fn restore_to_head(&self, _root: &Path, paths: &[PathBuf]) -> Result<(), String> {
        self.restored.borrow_mut().extend_from_slice(paths);
        Ok(())
    }

    fn syntax_theme(&self) -> String {
        FALLBACK_SYNTAX_THEME.to_owned()
    }
}

/// Drains the surface's file requests against `machine` until it stops
/// asking — what the host's background thread does, minus the thread.
fn settle(view: &mut CodeView, machine: &FakeMachine) {
    let mut guard = 0;
    while let Some(request) = view.take_request() {
        view.absorb(fulfill(machine, request));
        guard += 1;
        assert!(guard < 64, "the surface never stopped asking for things");
    }
}

fn files_at(root: &str) -> CodeView {
    CodeView::opening(PathBuf::from(root), root.to_owned(), ContentMode::Contents)
}

/// Opening a directory that holds only a directory carries on down the
/// chain in the one press, and the selection follows onto the row the
/// chain is drawn as.
#[test]
fn opening_a_chain_of_only_children_opens_all_of_it() {
    let machine = FakeMachine::default()
        .with_directory("/w/src")
        .with_directory("/w/src/main")
        .with_directory("/w/src/main/java")
        .with_file("/w/src/main/java/App.java", "class App {}\n")
        .with_file("/w/README.md", "# hi\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/src")));

    press(&mut view, Command::Activate);
    // One answer brings the whole chain: the tree draws between answers,
    // and a chain read a level per answer is seen folding into its row.
    let request = view.take_request().expect("the opened directory is read");
    view.absorb(fulfill(&machine, request));
    assert!(
        view.take_request().is_none(),
        "nothing further down the chain is left to read"
    );

    let rows = view.files.rows(&view.root);
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["src/main/java", "App.java", "README.md"]);
    assert_eq!(
        view.selected.as_deref(),
        Some(Path::new("/w/src/main/java")),
        "the selection is on a row that is drawn"
    );

    // Left on a file inside the chain steps out to the chain's row.
    press(&mut view, Command::SelectNext);
    press(&mut view, Command::Collapse);
    assert_eq!(
        view.selected.as_deref(),
        Some(Path::new("/w/src/main/java"))
    );
}

#[test]
fn a_directory_is_read_only_when_it_is_opened() {
    let machine = FakeMachine::default()
        .with_directory("/w/src")
        .with_file("/w/src/main.rs", "fn main() {}\n")
        .with_file("/w/README.md", "# hi\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    assert_eq!(item_names_of_tree(&view), ["src", "README.md"]);

    press(&mut view, Command::Expand);
    settle(&mut view, &machine);
    assert_eq!(item_names_of_tree(&view), ["src", "main.rs", "README.md"]);
}

/// `.git` is the repository's own database, not the project — and in a
/// linked worktree it is a file rather than a directory, so both spellings
/// have to go. Every other dotfile is content and stays.
#[test]
fn the_tree_shows_every_dotfile_but_the_repository_itself() {
    let machine = FakeMachine::default()
        .with_directory("/w/.git")
        .with_file("/w/.git/HEAD", "ref: refs/heads/main\n")
        .with_directory("/w/.github")
        .with_file("/w/.gitignore", "target\n")
        .with_file("/w/README.md", "# hi\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    assert_eq!(
        item_names_of_tree(&view),
        [".github", ".gitignore", "README.md"]
    );

    let worktree = FakeMachine::default()
        .with_file("/w/.git", "gitdir: /repo/.git/worktrees/one\n")
        .with_file("/w/main.rs", "fn main() {}\n");
    let mut view = files_at("/w");
    settle(&mut view, &worktree);
    assert_eq!(item_names_of_tree(&view), ["main.rs"]);
}

fn item_names_of_tree(view: &CodeView) -> Vec<String> {
    view.files
        .rows(&view.root)
        .into_iter()
        .map(|row| row.name)
        .collect()
}

/// A file is on screen before all of it is coloured, and colouring the
/// rest never changes a character of it.
///
/// The whole point of the two passes: colouring costs per line, a screen
/// does not, and a file that has to be coloured to the end before it can
/// be read is a file that opens in half a second.
#[test]
fn a_long_file_is_readable_before_all_of_it_is_coloured() {
    let long: String = (1..=900).map(|n| format!("fn line_{n}() {{}}\n")).collect();
    let machine = FakeMachine::default().with_file("/w/long.rs", &long);
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);

    // Everything up to, but not including, the pass that colours the
    // rest: this is the state the first frame draws.
    let mut asked = Vec::new();
    while let Some(request) = view.take_request() {
        asked.push(request.clone());
        if matches!(request, FileRequest::Colour(_)) {
            break;
        }
        view.absorb(fulfill(&machine, request));
    }
    assert_eq!(
        asked.last(),
        Some(&FileRequest::Colour(PathBuf::from("/w/long.rs"))),
        "the rest of the colour is asked for once the file is shown"
    );

    let open = view.open.as_ref().expect("the first row is read");
    assert_eq!(open.lines.len(), 900, "every line is there to read");
    assert!(
        !open.highlighted[0].is_empty(),
        "what the viewer is looking at is coloured"
    );
    assert!(
        open.highlighted[899].is_empty(),
        "the far end of the file is not, yet"
    );
    let Content::Lines { lines, .. } = super::view(&view, space()).content else {
        panic!("an open file is lines");
    };
    assert!(
        lines.iter().all(|line| !line.spans.is_empty()),
        "an uncoloured line is still drawn as the text it is"
    );

    view.absorb(fulfill(
        &machine,
        FileRequest::Colour(PathBuf::from("/w/long.rs")),
    ));
    let open = view.open.as_ref().expect("still open");
    assert!(
        open.highlighted.iter().all(|spans| !spans.is_empty()),
        "the second pass colours the whole file"
    );
    assert_eq!(open.lines.len(), 900, "and changes none of its text");
    assert_eq!(open.lines[899], "fn line_900() {}");
}

/// A document opens as the document it is, and its markup is never
/// coloured for a screen nobody asked for — until somebody asks for the
/// source, which is when that work becomes worth doing.
#[test]
fn a_documents_markup_is_coloured_only_once_the_source_is_asked_for() {
    let machine = FakeMachine::default().with_file("/w/README.md", "# hi\n\n`code`\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    assert_eq!(
        view.content,
        ContentMode::Preview,
        "a document reads as one"
    );
    let open = view.open.as_ref().expect("the document is read");
    assert!(
        open.highlighted.iter().all(|spans| spans.is_empty()),
        "nothing coloured the markup"
    );

    view.show(ContentMode::Contents);
    settle(&mut view, &machine);
    let open = view.open.as_ref().expect("still open");
    assert!(
        open.highlighted.iter().any(|spans| !spans.is_empty()),
        "asking for the source is what pays for colouring it"
    );
}

/// The content the host is handed is a window that starts where the
/// viewer is, and says where that is — so drawing the end of a long file
/// costs the end of it rather than all of it.
#[test]
fn the_content_window_starts_where_the_viewer_is() {
    let long: String = (1..=900).map(|n| format!("line {n}\n")).collect();
    let machine = FakeMachine::default().with_file("/w/long.rs", &long);
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    scroll_to(&mut view, 500);

    let Content::Lines {
        first,
        lines,
        total,
        scroll,
        ..
    } = super::view(&view, space()).content
    else {
        panic!("an open file is lines");
    };
    assert_eq!(total, 900, "the whole file is still what it is");
    assert_eq!(first, 500);
    assert_eq!(scroll, 500, "and the host still applies it");
    assert_eq!(lines.first().map(|line| line.number.as_str()), Some("501"));
    assert!(
        lines.len() <= usize::from(space().height) * 2,
        "a window, not a file: {} lines",
        lines.len()
    );
}

#[test]
fn editing_a_file_and_saving_it_writes_what_was_typed() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    press(&mut view, Command::CaretLineEnd);
    type_text(&mut view, "!");
    assert!(
        view.open.as_ref().is_some_and(|open| open.modified),
        "typing marks the buffer unsaved"
    );

    press(&mut view, Command::Save);
    settle(&mut view, &machine);

    assert_eq!(machine.files.borrow()[Path::new("/w/notes.txt")], "one!\n");
    assert!(
        view.open.as_ref().is_some_and(|open| !open.modified),
        "a saved buffer is no longer unsaved"
    );
}

/// Opens `path` from `machine` and starts typing into it.
fn editing(machine: &FakeMachine, path: &str) -> CodeView {
    let mut view = files_at("/w");
    settle(&mut view, machine);
    view.select(PathBuf::from(path));
    view.load_selection(None);
    settle(&mut view, machine);
    press(&mut view, Command::Edit);
    assert!(view.editing(), "{path} opened for typing");
    view
}

fn buffer(view: &CodeView) -> String {
    view.open.as_ref().expect("a file is open").contents()
}

#[test]
fn indenting_uses_the_files_own_kind_of_indentation() {
    let machine = FakeMachine::default()
        .with_file("/w/Makefile", "all:\n\techo hi\n")
        .with_file("/w/app.yaml", "a:\n  b: 1\n");

    let mut makefile = editing(&machine, "/w/Makefile");
    press(&mut makefile, Command::Indent);
    assert_eq!(buffer(&makefile), "\tall:\n\techo hi\n");

    let mut yaml = editing(&machine, "/w/app.yaml");
    type_text(&mut yaml, "x");
    press(&mut yaml, Command::Indent);
    assert_eq!(
        buffer(&yaml),
        "x a:\n  b: 1\n",
        "a two-space file indents to its next two-space step"
    );
}

#[test]
fn a_paste_lands_at_the_caret_as_one_edit() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "head tail\n");
    let mut view = editing(&machine, "/w/notes.txt");
    for _ in 0.."head ".len() {
        press(&mut view, Command::CaretRight);
    }

    view.paste("one\r\ntwo ", space());

    assert_eq!(buffer(&view), "head one\ntwo tail\n");
    let open = view.open.as_ref().expect("open");
    assert_eq!((open.caret.line, open.caret.column), (1, 4));
    assert_eq!(open.highlighted.len(), open.lines.len());
    assert!(open.modified);
}

#[test]
fn a_paste_reaches_nothing_that_is_only_being_read() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "x\n");
    let mut view = editing(&machine, "/w/notes.txt");
    press(&mut view, Command::Close);

    view.paste("pasted", space());

    assert_eq!(buffer(&view), "x\n");
}

#[test]
fn page_keys_move_the_caret_a_screen_at_a_time() {
    let text: String = (1..=100).map(|n| format!("{n}\n")).collect();
    let machine = FakeMachine::default().with_file("/w/long.txt", &text);
    let mut view = editing(&machine, "/w/long.txt");

    press(&mut view, Command::ScrollPageDown);
    press(&mut view, Command::ScrollPageDown);
    let line = view.open.as_ref().expect("open").caret.line;
    assert_eq!(line, usize::from(space().height) * 2);
    assert!(
        (view.scroll..view.scroll + space().height).contains(&u16::try_from(line).unwrap()),
        "the caret stays on screen"
    );

    press(&mut view, Command::ScrollPageUp);
    assert_eq!(
        view.open.as_ref().expect("open").caret.line,
        usize::from(space().height)
    );
}

/// One sample per language the editor colours, from
/// `tests/_fixtures/highlight/`, with the grammar it must be read as.
/// Compiled in rather than read, since this crate reaches no filesystem.
macro_rules! highlight_fixtures {
    ($(($name:literal, $grammar:literal),)*) => {
        [$(($name, $grammar, include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/_fixtures/highlight/",
            $name
        )))),*]
    };
}

/// Each fixture is read as its own language, and coloured.
///
/// The grammar is asserted, not only the colour: a file read as the wrong
/// language still gets a number or a string coloured here and there, and
/// that is how `.fs` passed as a GLSL shader while an F# file drew plain.
#[test]
fn every_language_fixture_is_read_as_its_own_language() {
    use crate::shared::highlight;
    let wrong: Vec<String> = highlight_fixtures![
        ("batch.bat", "Batch File"),
        ("c.c", "C"),
        ("Cargo.lock", "TOML"),
        ("clojure.clj", "Clojure"),
        ("CMakeLists.txt", "CMake"),
        ("composer.lock", "JSON"),
        ("cpp.cpp", "C++"),
        ("crystal.cr", "Crystal"),
        ("csharp.cs", "C#"),
        ("css.css", "CSS"),
        ("d.d", "D"),
        ("dart.dart", "Dart"),
        ("diff.diff", "Diff"),
        ("Dockerfile", "Dockerfile"),
        ("elisp.el", "Lisp"),
        ("elixir.ex", "Elixir"),
        ("elm.elm", "Elm"),
        ("erlang.erl", "Erlang"),
        ("fish.fish", "Fish"),
        ("flake.lock", "JSON"),
        ("fsharp.fs", "F#"),
        ("glsl.glsl", "GLSL"),
        ("go.go", "Go"),
        ("graphql.graphql", "GraphQL"),
        ("groovy.gradle", "Groovy"),
        ("haskell.hs", "Haskell"),
        ("header.h", "C++"),
        ("html.html", "HTML"),
        ("ini.ini", "INI"),
        ("java.java", "Java"),
        ("javascript-commonjs.cjs", "JavaScript"),
        ("javascript-module.mjs", "JavaScript"),
        ("javascript-react.jsx", "TypeScriptReact"),
        ("javascript.js", "JavaScript"),
        ("json-with-comments.jsonc", "JSON"),
        ("json.json", "JSON"),
        ("julia.jl", "Julia"),
        ("kotlin.kt", "Kotlin"),
        ("latex.tex", "LaTeX"),
        ("less.less", "Less"),
        ("lua.lua", "Lua"),
        ("Makefile", "Makefile"),
        ("markdown-jsx.mdx", "Markdown"),
        ("markdown.md", "Markdown"),
        ("mix.lock", "Elixir"),
        ("nim.nim", "Nim"),
        ("nix.nix", "Nix"),
        ("ocaml.ml", "OCaml"),
        ("perl.pl", "Perl"),
        ("php.php", "PHP"),
        ("protobuf.proto", "Protocol Buffer"),
        ("python.py", "Python"),
        ("r.r", "R"),
        ("ruby.rb", "Ruby"),
        ("rust.rs", "Rust"),
        ("scala.scala", "Scala"),
        ("scss.scss", "SCSS"),
        ("shell-shebang", "Bourne Again Shell (bash)"),
        ("shell.sh", "Bourne Again Shell (bash)"),
        ("solidity.sol", "Solidity"),
        ("sql.sql", "SQL"),
        ("svelte.svelte", "Svelte"),
        ("swift.swift", "Swift"),
        ("terraform.tf", "Terraform"),
        ("toml.toml", "TOML"),
        ("typescript-module.mts", "TypeScript"),
        ("typescript-react.tsx", "TypeScriptReact"),
        ("typescript.ts", "TypeScript"),
        ("unknown-json.lock", "JSON"),
        ("unknown-toml.lock", "TOML"),
        ("vim.vim", "VimL"),
        ("vue.vue", "Vue Component"),
        ("xml.xml", "XML"),
        ("yaml.yaml", "YAML"),
        ("yarn.lock", "YAML"),
        ("zig.zig", "Zig"),
    ]
    .into_iter()
    .filter_map(|(name, grammar, text)| {
        let path = Path::new(name);
        let read_as = highlight::syntax_for(path, text.lines().next())
            .name
            .as_str();
        let lines = highlight::lines(text, path, FALLBACK_SYNTAX_THEME, usize::MAX);
        let coloured = lines.concat().windows(2).any(|pair| pair[0].0 != pair[1].0);
        (read_as != grammar || !coloured)
            .then(|| format!("{name}: read as {read_as}, coloured: {coloured}"))
    })
    .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The one irreversible gesture, so it is the one that must never happen
/// on a single keystroke.
#[test]
fn deleting_asks_before_it_deletes() {
    let machine = FakeMachine::default().with_file("/w/scratch.txt", "x\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    press(&mut view, Command::Delete);
    assert!(view.queue.is_empty(), "asking is not deleting");
    press(&mut view, Command::Close);
    settle(&mut view, &machine);
    assert!(
        machine
            .files
            .borrow()
            .contains_key(Path::new("/w/scratch.txt"))
    );

    press(&mut view, Command::Delete);
    press(&mut view, Command::ConfirmDelete);
    settle(&mut view, &machine);
    assert!(
        !machine
            .files
            .borrow()
            .contains_key(Path::new("/w/scratch.txt"))
    );
}

/// The question is asked in the dialog every other one is, and answered
/// the way the discard question is: by its buttons, or by moving the
/// keyboard between them and activating one.
#[test]
fn deleting_is_asked_in_the_confirm_dialog() {
    let machine = FakeMachine::default().with_file("/w/scratch.txt", "x\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    press(&mut view, Command::Delete);
    let confirm = super::view(&view, space())
        .confirm
        .expect("deleting asks first");
    assert_eq!(confirm.subject, "scratch.txt");
    assert!(
        !confirm.on_confirm,
        "the way out is where the keyboard starts"
    );

    handle_mouse(&mut view, Some(ViewHit::SelectItem(0)), space());
    assert!(
        super::view(&view, space()).confirm.is_some(),
        "only an answer closes it"
    );
    press(&mut view, Command::Activate);
    assert!(
        view.queue
            .iter()
            .all(|request| !matches!(request, FileRequest::Delete(_)))
    );
    assert!(super::view(&view, space()).confirm.is_none());

    press(&mut view, Command::Delete);
    press(&mut view, Command::FocusNext);
    assert!(
        super::view(&view, space())
            .confirm
            .is_some_and(|confirm| confirm.on_confirm)
    );
    press(&mut view, Command::Close);

    press(&mut view, Command::Delete);
    handle_mouse(&mut view, Some(ViewHit::Answer(true)), space());
    settle(&mut view, &machine);
    assert!(
        !machine
            .files
            .borrow()
            .contains_key(Path::new("/w/scratch.txt"))
    );
}

#[test]
fn a_directory_is_never_deletable() {
    let machine = FakeMachine::default().with_directory("/w/src");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    press(&mut view, Command::Delete);
    assert!(view.confirming_delete.is_none());
    assert_eq!(
        view.notice.as_ref().map(|notice| notice.text.as_str()),
        Some("only files are deletable")
    );
}

#[test]
fn closing_with_unsaved_changes_asks_once() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    type_text(&mut view, "x");
    press(&mut view, Command::Close);

    assert!(
        matches!(press(&mut view, Command::Close), CodeOutcome::Stay),
        "the first esc asks rather than closing"
    );
    assert!(
        matches!(press(&mut view, Command::Close), CodeOutcome::Close),
        "the second is the answer"
    );
}

/// The question a second Esc answers is asked on screen, with the one key
/// that answers it — and any other gesture withdraws it.
#[test]
fn closing_with_unsaved_changes_says_what_the_second_press_does() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    type_text(&mut view, "x");
    press(&mut view, Command::Close);
    press(&mut view, Command::Close);

    let asked = super::view(&view, space());
    assert_eq!(
        asked.notice.map(|notice| notice.text),
        Some("notes.txt has unsaved changes · close again to discard them".to_owned())
    );
    assert_eq!(asked.footer, [Command::Close]);

    press(&mut view, Command::FocusNext);
    assert_eq!(super::view(&view, space()).notice, None, "withdrawn");
}

/// A file with no trailing newline gains one on the way through
/// `str::lines`; one that had it must not gain a second.
#[test]
fn saving_a_file_nobody_edited_a_newline_into_keeps_its_ending() {
    let machine = FakeMachine::default().with_file("/w/a.txt", "one\ntwo\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    assert_eq!(
        view.open.as_ref().expect("a file is open").contents(),
        "one\ntwo\n"
    );
}

/// Opening a file and saving it unedited writes back the bytes it was
/// given — whatever those bytes were. The buffer is not the place a
/// project's line endings get an opinion held about them.
#[test]
fn a_file_opened_and_saved_unedited_is_byte_for_byte_what_it_was() {
    for original in [
        "one\r\ntwo\r\n",
        "one\r\ntwo",
        "one\ntwo",
        "one\ntwo\n",
        "",
        "\n",
        "one",
    ] {
        let machine = FakeMachine::default().with_file("/w/a.txt", original);
        let mut view = files_at("/w");
        settle(&mut view, &machine);
        press(&mut view, Command::Activate);
        settle(&mut view, &machine);

        assert_eq!(
            view.open.as_ref().expect("a file is open").contents(),
            original,
            "opening and saving rewrote {original:?}"
        );
    }
}

/// Editing a CRLF file leaves the lines nobody touched as they were: a
/// one-character change must not arrive as a whole-file diff.
#[test]
fn editing_a_crlf_file_keeps_every_other_line_crlf() {
    let machine = FakeMachine::default().with_file("/w/a.txt", "one\r\ntwo\r\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    type_text(&mut view, "x");

    assert_eq!(
        view.open.as_ref().expect("a file is open").contents(),
        "xone\r\ntwo\r\n"
    );
}

/// An image or an archive is a file the tree is right to list and the
/// text view has nothing to draw for. That is a state of the file, not a
/// failure, so it reads as an empty state rather than in red.
#[test]
fn a_file_that_is_not_text_is_an_empty_state_where_its_contents_would_be() {
    let mut machine = FakeMachine::default();
    machine.directories.insert(
        PathBuf::from("/w"),
        vec![DirEntry {
            directory: false,
            name: "logo.png".to_owned(),
        }],
    );
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    let Content::Message { text, hint, role } = super::view(&view, space()).content else {
        panic!("there are no lines to show");
    };
    assert_eq!(role, Role::Muted, "nothing failed");
    assert!(text.contains("logo.png"), "it names the file: {text}");
    assert!(hint.is_some(), "and says why there is nothing to read");
}

/// Git answers a binary change with a note rather than hunks. Drawn as
/// lines, that is an empty pane under the file's name, which reads as
/// "nothing changed" about a file the list says did.
#[test]
fn a_binary_change_is_an_empty_state_rather_than_an_empty_diff() {
    let output = "diff --git a/logo.png b/logo.png\nindex 1..2 100644\nBinary files a/logo.png and b/logo.png differ\n";
    assert!(diff::is_binary(output));
    assert!(!diff::is_binary(
        "@@ -1 +1 @@\n-Binary files are fun\n+text\n"
    ));

    let mut view = fixture();
    view.changes.diff = diff::read(output, Path::new("/repo/src/ui.rs"), FALLBACK_SYNTAX_THEME);
    view.changes.diff_binary = true;

    let Content::Message { text, role, .. } = super::view(&view, space()).content else {
        panic!("a binary change has no lines to compare");
    };
    assert_eq!(role, Role::Muted, "nothing failed");
    assert!(text.contains("ui.rs"), "it names the file: {text}");
}

/// A click says "this many cells into that line"; the caret has to end
/// up on the character those cells actually reach. A double-width glyph
/// is where the two counts come apart.
#[test]
fn a_click_lands_on_the_character_its_cells_reach() {
    let machine = FakeMachine::default().with_file("/w/wide.txt", "a漢b\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    for (cell, expected, why) in [
        (0, 0, "the first cell is the first character"),
        (1, 1, "the wide glyph's first cell is the wide glyph"),
        (2, 1, "and so is its second"),
        (
            3,
            2,
            "the character after it starts a cell later than its index",
        ),
        (99, 3, "past the end is the end"),
    ] {
        handle_mouse(
            &mut view,
            Some(ViewHit::PlaceCaret { line: 0, cell }),
            space(),
        );
        assert_eq!(
            view.open.as_ref().expect("a file is open").caret.column,
            expected,
            "{why}"
        );
    }
}

/// The save's own re-read arrives a beat later. Anything typed in that
/// beat must survive it.
#[test]
fn a_reread_never_overwrites_keystrokes_typed_while_it_was_out() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    press(&mut view, Command::CaretLineEnd);
    type_text(&mut view, "!");

    view.absorb(FileAnswer::Read {
        path: PathBuf::from("/w/notes.txt"),
        file: Ok(LoadedFile::of("one\n")),
    });
    assert_eq!(
        view.open.as_ref().map(|open| open.lines.clone()),
        Some(vec!["one!".to_owned()]),
        "what was typed is still what is on screen"
    );
}

/// A save is answered after the write lands, and typing does not wait
/// for it. What was typed in between is not what was written, so the
/// answer may neither call it saved nor re-read the file over it.
#[test]
fn keystrokes_typed_while_a_save_is_out_survive_its_answer() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    press(&mut view, Command::CaretLineEnd);
    type_text(&mut view, "!");
    press(&mut view, Command::Save);

    let save = view.take_request().expect("the save is asked for");
    type_text(&mut view, "?");
    view.absorb(fulfill(&machine, save));
    settle(&mut view, &machine);

    assert_eq!(machine.files.borrow()[Path::new("/w/notes.txt")], "one!\n");
    let open = view.open.as_ref().expect("still open");
    assert_eq!(open.lines, ["one!?"], "what was typed after is still there");
    assert!(open.modified, "and is still unsaved");

    press(&mut view, Command::Save);
    settle(&mut view, &machine);
    assert_eq!(machine.files.borrow()[Path::new("/w/notes.txt")], "one!?\n");
    assert!(!view.open.as_ref().expect("still open").modified);
}

/// Moving the cursor is not a request to throw away what was typed: an
/// unsaved buffer keeps the surface on its file, by the arrows or by a
/// click, until it is saved.
#[test]
fn moving_away_from_unsaved_work_keeps_it() {
    let machine = FakeMachine::default()
        .with_file("/w/a.txt", "aaa\n")
        .with_file("/w/b.txt", "bbb\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    type_text(&mut view, "x");
    press(&mut view, Command::Close);
    press(&mut view, Command::FocusNext);

    press(&mut view, Command::SelectNext);
    handle_mouse(&mut view, Some(ViewHit::SelectItem(1)), space());
    settle(&mut view, &machine);

    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/a.txt")));
    let open = view.open.as_ref().expect("the buffer is still open");
    assert_eq!(open.lines, ["xaaa"]);
    assert!(open.modified);
    let said = super::view(&view, space()).notice;
    assert_eq!(
        said.as_ref()
            .map(|notice| (notice.text.as_str(), notice.role)),
        Some(("a.txt has unsaved changes", Role::Warning)),
        "the refusal is said where the viewer can read it"
    );

    press(&mut view, Command::Edit);
    press(&mut view, Command::Save);
    settle(&mut view, &machine);
    press(&mut view, Command::Close);
    handle_mouse(&mut view, Some(ViewHit::SelectItem(1)), space());
    assert_eq!(
        view.selected.as_deref(),
        Some(Path::new("/w/b.txt")),
        "once it is saved, the cursor moves"
    );
}

/// A caret or a diff row past what a `u16` counts is clamped to the last
/// row a scroll can name, rather than wrapped to one near the top.
#[test]
fn a_position_past_the_scroll_range_is_clamped_rather_than_wrapped() {
    let mut view = fixture();
    let far = usize::from(u16::MAX) + 10;
    view.changes.diff = (0..=far)
        .map(|line| diff::DiffCell {
            kind: DiffLineKind::Context,
            line_no: line as u32,
            spans: Vec::new(),
        })
        .collect();
    assert_eq!(view.diff_row_of(far), Some(u16::MAX));

    let mut open = OpenFile::opening(PathBuf::from("/repo/long.rs"));
    open.editing = true;
    open.caret.line = far;
    view.open = Some(open);
    view.scroll = u16::MAX - 1;
    view.follow_caret(20);
    assert_eq!(view.scroll, u16::MAX - 19);
}

/// Moving to another file in the tree points every mode at it: the diff
/// read for the file left behind is not left standing under the new one,
/// by the arrows or by a click.
#[test]
fn moving_to_another_file_in_the_tree_asks_for_its_diff() {
    let machine = FakeMachine::default()
        .with_file("/w/a.txt", "aaa\n")
        .with_file("/w/b.txt", "bbb\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    view.changes.diff_pending = false;

    press(&mut view, Command::SelectNext);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/b.txt")));
    assert!(view.diff_pending(), "the arrows moved to another file");

    view.changes.diff_pending = false;
    handle_mouse(&mut view, Some(ViewHit::SelectItem(0)), space());
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/a.txt")));
    assert!(view.diff_pending(), "and so did the click");
}

/// The second colouring pass runs beside the reads rather than behind
/// them, so it can land after a save and its re-read of the same file.
/// Colour read from the text before is colour for text nobody holds.
#[test]
fn colour_read_from_an_older_text_is_not_installed() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "two\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    view.absorb(FileAnswer::Coloured {
        path: PathBuf::from("/w/notes.txt"),
        file: Ok(LoadedFile::of("one\n")),
    });
    assert_eq!(
        view.open.as_ref().map(|open| open.lines.clone()),
        Some(vec!["two".to_owned()]),
        "the text on screen is the newer one"
    );

    view.absorb(FileAnswer::Coloured {
        path: PathBuf::from("/w/notes.txt"),
        file: Ok(LoadedFile::of("two\n")),
    });
    assert!(
        view.open.as_ref().is_some_and(|open| !open.partial),
        "colour for the same text is taken"
    );
}

/// An answer names the file it is about, so one that lands after the
/// viewer opened something else is dropped.
#[test]
fn a_read_that_arrives_late_does_not_replace_a_different_file() {
    let machine = FakeMachine::default()
        .with_file("/w/a.txt", "aaa\n")
        .with_file("/w/b.txt", "bbb\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::SelectNext);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);

    view.absorb(FileAnswer::Read {
        path: PathBuf::from("/w/a.txt"),
        file: Ok(LoadedFile::of("aaa\n")),
    });
    assert_eq!(
        view.open.as_ref().map(|open| open.lines.clone()),
        Some(vec!["bbb".to_owned()]),
        "the file on screen is still the one that was asked for"
    );
}

// --- the switch this whole surface exists for ---------------------------

/// The move the merge was for: from a line of the diff into that line of
/// the file. Carrying only the file would leave the reader at the top of
/// something they were reading the middle of.
#[test]
fn switching_from_a_diff_to_the_contents_keeps_the_file_and_the_line() {
    let machine = FakeMachine::default().with_file("/w/a.rs", "one\ntwo\nthree\nfour\n");
    let mut view = CodeView::opening(PathBuf::from("/w"), "/w".to_owned(), ContentMode::Diff);
    view.selected = Some(PathBuf::from("/w/a.rs"));
    view.changes.files = vec![ChangedFile {
        status: FileStatus::Modified,
        path: PathBuf::from("/w/a.rs"),
        renamed_from: None,
    }];
    view.changes.diff = diff::read(
        "@@ -1,4 +1,4 @@\n one\n two\n-old\n+three\n four\n",
        Path::new("/w/a.rs"),
        FALLBACK_SYNTAX_THEME,
    );
    view.changes.diff_pending = false;
    // Scrolled to the replacement, which is line three of the new file.
    view.scroll = 3;

    open_the_file_from_its_menu(&mut view);
    settle(&mut view, &machine);

    let open = view.open.as_ref().expect("the file opened");
    assert_eq!(open.path, PathBuf::from("/w/a.rs"), "the same file");
    assert_eq!(
        open.lines[open.caret.line], "three",
        "and the line that was being read"
    );
    assert_eq!(view.content, ContentMode::Contents);
    assert_eq!(
        view.navigator(),
        NavigatorMode::Files,
        "the navigator follows the content"
    );
}

/// A replacement of several lines is drawn in Git's order — every removal,
/// then every addition — and the line carried across the switch is still
/// the one on screen, both ways: an addition's number is the new file's,
/// and a removal that shares it is not where the reader comes back to.
#[test]
fn a_multi_line_replacement_carries_the_line_both_ways() {
    let machine = FakeMachine::default().with_file("/w/a.rs", "one\nc\nd\nfour\n");
    let mut view = CodeView::opening(PathBuf::from("/w"), "/w".to_owned(), ContentMode::Diff);
    view.selected = Some(PathBuf::from("/w/a.rs"));
    view.changes.files = vec![ChangedFile {
        status: FileStatus::Modified,
        path: PathBuf::from("/w/a.rs"),
        renamed_from: None,
    }];
    view.changes.diff = diff::read(
        "@@ -1,4 +1,4 @@\n one\n-a\n-b\n+c\n+d\n four\n",
        Path::new("/w/a.rs"),
        FALLBACK_SYNTAX_THEME,
    );
    view.changes.diff_pending = false;
    let Content::Lines { lines, .. } = super::view(&view, space()).content else {
        panic!("a selected file has a diff");
    };
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.gutter.as_str(), line.number.as_str()))
            .collect::<Vec<_>>(),
        [
            (" ", "1"),
            ("-", "2"),
            ("-", "3"),
            ("+", "2"),
            ("+", "3"),
            (" ", "4")
        ]
    );
    // On `+d`, line three of the new file.
    view.scroll = 4;

    open_the_file_from_its_menu(&mut view);
    settle(&mut view, &machine);
    let open = view.open.as_ref().expect("the file opened");
    assert_eq!(open.lines[open.caret.line], "d");

    press(&mut view, Command::Close);
    view.show(ContentMode::Diff);
    assert_eq!(
        view.scroll, 4,
        "back on `+d`, not on `-b` which shares its number"
    );
}

/// The selection is authoritative and the tree catches up to it: a file
/// that changed may sit under directories nobody has opened.
#[test]
fn switching_to_a_file_the_tree_has_not_listed_opens_its_ancestors() {
    let machine = FakeMachine::default()
        .with_directory("/w/src")
        .with_directory("/w/src/ui")
        .with_file("/w/src/ui/deep.rs", "fn deep() {}\n");
    let mut view = CodeView::opening(PathBuf::from("/w"), "/w".to_owned(), ContentMode::Diff);
    view.selected = Some(PathBuf::from("/w/src/ui/deep.rs"));
    view.changes.files = vec![ChangedFile {
        status: FileStatus::Modified,
        path: PathBuf::from("/w/src/ui/deep.rs"),
        renamed_from: None,
    }];

    open_the_file_from_its_menu(&mut view);
    settle(&mut view, &machine);

    assert!(
        item_names_of_tree(&view).contains(&"deep.rs".to_owned()),
        "the tree opened down to the file: {:?}",
        item_names_of_tree(&view)
    );
}

/// The doors are shortcuts once the surface is open: the same two the
/// host uses to choose where to land also choose what to show.
#[test]
fn the_doors_switch_modes_once_the_surface_is_open() {
    let mut view = fixture();
    assert_eq!(view.content, ContentMode::Diff);

    view.show(ContentMode::Contents);
    assert_eq!(view.content, ContentMode::Contents);
    assert_eq!(
        view.navigator(),
        NavigatorMode::Files,
        "the navigator follows the content"
    );

    view.show(ContentMode::Diff);
    assert_eq!(view.content, ContentMode::Diff);
    assert_eq!(view.navigator(), NavigatorMode::Changes);
}

/// The frame's two edges: what the surface is on top, where it is open
/// at the foot — and each part of the second told apart by weight, since
/// one run of text gives them all the same and that is how a line stops
/// being read.
#[test]
fn the_frame_says_what_this_is_on_top_and_where_it_is_at_the_foot() {
    let mut view = fixture();
    view.branch = "feat/thing".to_owned();
    view.display_root = "~/uze/.worktrees/joipv0".to_owned();

    let drawn = super::view(&view, space());
    let said = |spans: &[Span]| {
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
    };
    assert_eq!(
        said(&drawn.title),
        CATALOG.name,
        "the name it is registered under"
    );
    assert_eq!(said(&drawn.caption), "~/uze/.worktrees/joipv0 · feat/thing");

    let weight = |spans: &[Span], text: &str| {
        spans
            .iter()
            .find(|span| span.text == text)
            .map(|span| (span.role, span.bold))
    };
    assert_eq!(
        weight(&drawn.title, CATALOG.name),
        Some((Role::Muted, false)),
        "the surface's name is a label, said once and quietly"
    );
    assert_eq!(
        weight(&drawn.caption, "~/uze/.worktrees/"),
        Some((Role::Dim, false)),
        "the directories leading to it are context"
    );
    assert_eq!(
        weight(&drawn.caption, "joipv0"),
        Some((Role::Bright, true)),
        "the checkout's own name identifies it"
    );
    assert_eq!(
        weight(&drawn.caption, "feat/thing"),
        Some((Role::Accent, true)),
        "and so does the branch, which is what changes"
    );
}

/// The control is offered only where it means something, and clicking a
/// segment shows that mode — the pointer's way to the same place the key
/// reaches.
#[test]
fn a_document_offers_its_two_modes_and_a_click_picks_one() {
    let machine = FakeMachine::default()
        .with_file("/w/README.md", "# Title\n")
        .with_file("/w/main.rs", "fn main() {}\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    // The tree lists directories first, then files by name: README.md
    // before main.rs.
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    let offered = super::view(&view, space()).modes;
    assert_eq!(
        offered
            .iter()
            .map(|mode| mode.label.as_str())
            .collect::<Vec<_>>(),
        ["Preview", "Source"],
        "a document offers both ways of reading it"
    );
    assert!(
        offered[0].active,
        "and opens as the document it is, not as the markup describing it"
    );

    handle_mouse(&mut view, Some(ViewHit::SelectMode(1)), space());
    assert_eq!(view.content, ContentMode::Contents);
    assert!(super::view(&view, space()).modes[1].active);

    // A file that is not a document has one way of being read. Picked by
    // pointing at it: the focus is on the content now, so an arrow would
    // scroll rather than move the selection.
    handle_mouse(&mut view, Some(ViewHit::SelectItem(1)), space());
    settle(&mut view, &machine);
    assert!(
        super::view(&view, space()).modes.is_empty(),
        "nothing to choose between for a file that is not a document"
    );
}

/// Every way into a document lands on the document. The rule is the
/// selection's, not one entry point's, so it holds for the row the tree
/// opens on, for the next row, and after the reader has asked to see the
/// markup of the file before.
#[test]
fn a_document_is_read_as_one_however_it_was_reached() {
    let machine = FakeMachine::default()
        .with_file("/w/README.md", "# Title\n")
        .with_file("/w/main.rs", "fn main() {}\n")
        .with_file("/w/NOTES.markdown", "# Notes\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    assert_eq!(
        view.selected,
        Some(PathBuf::from("/w/README.md")),
        "the tree opens on its first row"
    );
    assert_eq!(
        view.content,
        ContentMode::Preview,
        "and a README is a document before it is a file"
    );

    // Asking for the markup lasts as long as the file it was asked about.
    press(&mut view, Command::TogglePreview);
    assert_eq!(view.content, ContentMode::Contents);

    press(&mut view, Command::SelectNext);
    settle(&mut view, &machine);
    assert_eq!(view.selected, Some(PathBuf::from("/w/main.rs")));
    assert_eq!(
        view.content,
        ContentMode::Contents,
        "a file that is not a document is read as a file"
    );

    press(&mut view, Command::SelectNext);
    settle(&mut view, &machine);
    assert_eq!(view.selected, Some(PathBuf::from("/w/NOTES.markdown")));
    assert_eq!(
        view.content,
        ContentMode::Preview,
        "`.markdown` is the same document `.md` is, and the source asked \
         for on another file was about that file"
    );
}

/// Closing the surface and opening it again is returning to it: the file
/// being read, the directories opened to reach it, and how far down it.
/// Without this the commonest gesture in the product — glance at the diff,
/// close, come back — costs the whole walk down the tree again.
#[test]
fn reopening_a_checkout_returns_to_where_the_viewer_was() {
    let machine = FakeMachine::default()
        .with_directory("/w/src")
        .with_file("/w/src/deep.rs", "one\ntwo\nthree\n")
        .with_file("/w/README.md", "# hi\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);

    press(&mut view, Command::Expand);
    settle(&mut view, &machine);
    press(&mut view, Command::SelectNext);
    settle(&mut view, &machine);
    assert_eq!(view.selected, Some(PathBuf::from("/w/src/deep.rs")));
    view.scroll = 2;

    // What the host keeps when the surface closes.
    let place = view.place();
    drop(view);

    let mut view = files_at("/w").resuming(place);
    settle(&mut view, &machine);

    assert_eq!(
        view.selected,
        Some(PathBuf::from("/w/src/deep.rs")),
        "the file being read is the file being read"
    );
    assert_eq!(
        item_names_of_tree(&view),
        ["src", "deep.rs", "README.md"],
        "and the directory opened to reach it is still open"
    );
    assert_eq!(view.scroll, 2, "at the line it was left at");
    assert_eq!(
        view.open.as_ref().map(|open| open.lines.len()),
        Some(3),
        "the contents were read again rather than assumed"
    );
}

/// The door still decides what the surface is showing. A place that
/// carried the mode would make `Alt+G` mean "wherever I was last time",
/// which is the one thing a door must not mean.
#[test]
fn a_restored_place_does_not_outrank_the_door_it_came_through() {
    let machine = FakeMachine::default().with_file("/w/a.rs", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    assert_eq!(view.content, ContentMode::Contents);

    let place = view.place();
    let view =
        CodeView::opening(PathBuf::from("/w"), "/w".to_owned(), ContentMode::Diff).resuming(place);

    assert_eq!(view.content, ContentMode::Diff);
    assert_eq!(
        view.selected,
        Some(PathBuf::from("/w/a.rs")),
        "the file came with it, which is what makes the switch worth having"
    );
    assert!(
        view.changes.diff_pending,
        "and its diff is what the surface is now waiting for"
    );
}

/// Reviewing a document is still reviewing. The preview reads one file as
/// it is on disk and a diff is about two of them, so the half that answers
/// for changes keeps its mode whatever the selection is.
#[test]
fn a_documents_diff_is_still_a_diff() {
    let machine = FakeMachine::default().with_file("/w/README.md", "# Title\n");
    let mut view = CodeView::opening(PathBuf::from("/w"), "/w".to_owned(), ContentMode::Diff);
    view.changes.files = vec![
        ChangedFile {
            status: FileStatus::Modified,
            path: PathBuf::from("/w/a.rs"),
            renamed_from: None,
        },
        ChangedFile {
            status: FileStatus::Modified,
            path: PathBuf::from("/w/README.md"),
            renamed_from: None,
        },
    ];
    view.selected = Some(PathBuf::from("/w/a.rs"));

    view.select(PathBuf::from("/w/README.md"));
    settle(&mut view, &machine);

    assert_eq!(view.selected, Some(PathBuf::from("/w/README.md")));
    assert_eq!(
        view.content,
        ContentMode::Diff,
        "a document selected in the changes is still being reviewed"
    );
}

/// A refresh answers about the changes and nothing else. The same struct
/// holds a buffer, and a refresh that could reach it would be one that
/// eats what was typed.
#[test]
fn a_changes_refresh_leaves_an_unsaved_buffer_alone() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    press(&mut view, Command::Edit);
    press(&mut view, Command::CaretLineEnd);
    type_text(&mut view, "!");

    view.absorb_changes(RefreshedChanges {
        placement: view.placement(),
        branch: "main".to_owned(),
        changes: Changes {
            files: vec![ChangedFile {
                status: FileStatus::Modified,
                path: PathBuf::from("/w/notes.txt"),
                renamed_from: None,
            }],
            ..Changes::default()
        },
    });

    assert_eq!(
        view.open.as_ref().map(|open| open.lines.clone()),
        Some(vec!["one!".to_owned()]),
        "the buffer is untouched by a read of what changed"
    );
    assert!(view.open.as_ref().is_some_and(|open| open.modified));
}

/// A file tree's rows say what they *are*, so the host can mark them.
/// Classified by whole name as well as by extension, because the files at
/// the root of a repository carry their meaning without one.
#[test]
fn a_row_is_classified_by_what_it_is_rather_than_by_its_language() {
    use crate::code::files::icon_for;
    use crate::view::RowIcon;

    assert_eq!(icon_for("src", true, false), RowIcon::Directory);
    assert_eq!(icon_for("src", true, true), RowIcon::DirectoryOpen);

    // A whole name, where an extension would say nothing.
    assert_eq!(icon_for("Makefile", false, false), RowIcon::Code);
    assert_eq!(icon_for("LICENSE", false, false), RowIcon::Legal);
    assert_eq!(icon_for(".gitignore", false, false), RowIcon::Git);

    // And by extension, across the kinds this repository's own tree has.
    assert_eq!(icon_for("main.rs", false, false), RowIcon::Code);
    assert_eq!(icon_for("AGENTS.md", false, false), RowIcon::Markup);
    assert_eq!(icon_for("Cargo.toml", false, false), RowIcon::Config);
    assert_eq!(icon_for("Cargo.lock", false, false), RowIcon::Lock);
    assert_eq!(icon_for("logo.svg", false, false), RowIcon::Image);
    assert_eq!(icon_for("bundle.tar", false, false), RowIcon::Archive);

    // Case is not a classification, and an unknown extension is a file.
    assert_eq!(icon_for("README.MD", false, false), RowIcon::Markup);
    assert_eq!(icon_for("notes.qqq", false, false), RowIcon::File);
    assert_eq!(icon_for("noextension", false, false), RowIcon::File);
}

/// The changes list marks status, so it takes no file icon: two marks per
/// row is one too many, and the status is the reason that list exists.
#[test]
fn the_changes_list_marks_status_rather_than_kind() {
    let view = view(&fixture(), space());
    let rows = &view.navigator.as_ref().expect("a navigator").rows;
    assert!(
        rows.iter().all(|row| match row {
            crate::view::NavigatorRow::Group { icon, .. }
            | crate::view::NavigatorRow::Item { icon, .. } => *icon == crate::view::RowIcon::None,
            crate::view::NavigatorRow::Band { .. } | crate::view::NavigatorRow::Gap => true,
        }),
        "the changes list asked for a file icon: {rows:?}"
    );
}

#[test]
fn a_place_somebody_was_sent_to_opens_every_directory_on_the_way() {
    let root = Path::new("/project");
    let file = CodePlace::at(root, Path::new("/project/crates/core/src/store.rs"), false);
    assert_eq!(
        file.selected.as_deref(),
        Some(Path::new("/project/crates/core/src/store.rs"))
    );
    let opened: Vec<&Path> = file.expanded.iter().map(PathBuf::as_path).collect();
    assert_eq!(
        opened,
        [
            Path::new("/project"),
            Path::new("/project/crates"),
            Path::new("/project/crates/core"),
            Path::new("/project/crates/core/src"),
        ],
        "and nothing above the project"
    );

    let directory = CodePlace::at(root, Path::new("/project/crates"), true);
    assert_eq!(directory.selected, None, "a directory is opened, not read");
    assert!(directory.expanded.contains(Path::new("/project/crates")));
}

/// A checkout measured into two directories and a file at the root.
fn measured() -> map::Measure {
    let file = |path: &str, lines: u32, commits: u32| map::FileMeasure {
        path: path.to_owned(),
        lines,
        commits,
        changed: false,
    };
    map::Measure {
        root: PathBuf::from("/repo"),
        files: vec![
            file("src/ui/render.rs", 900, 12),
            file("src/ui/input.rs", 700, 3),
            file("src/main.rs", 600, 4),
            file("docs/guide.md", 800, 1),
        ],
    }
}

/// A directory is a place the cursor can be, and no file to read.
///
/// Coming back to one asked the host to read it: the answer is "not
/// readable as text", drawn in the colour of a failure, where what is
/// true is that nothing is open. The same surface says exactly that
/// when it opens on an empty selection, and it is what a reader who has
/// selected nothing needs to be told.
#[test]
fn coming_back_to_a_directory_row_reads_nothing_and_says_nothing_is_open() {
    let machine = FakeMachine::default()
        .with_directory("/w/src")
        .with_file("/w/src/main.rs", "fn main() {}\n")
        .with_file("/w/notes.txt", "one\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    assert_eq!(
        view.selected.as_deref(),
        Some(Path::new("/w/src")),
        "the tree opens on its first row, which is a directory"
    );

    let mut again = files_at("/w").resuming(view.place());
    settle(&mut again, &machine);

    assert!(again.open.is_none(), "a directory was never a file to open");
    let Content::Message { role, hint, .. } = super::view(&again, space()).content else {
        panic!("nothing is open, so the content is what to do about it");
    };
    assert_eq!(role, Role::Muted, "nothing failed");
    assert!(hint.is_some(), "and there is something to do about it");

    // The other way to the same read: leave the files half with the
    // cursor on a directory, and come back to it.
    again.show(ContentMode::Diff);
    again.show(ContentMode::Contents);
    settle(&mut again, &machine);
    assert!(
        again.open.is_none(),
        "the half is the same one, and so is the row it was left on"
    );
}

/// The map is a fourth way of looking at the same checkout, and the
/// selection is what survives the switch: a tile followed is the file
/// selected, read by whichever half was on show before the map.
/// Selecting a tile marks it without recolouring it.
///
/// Every tile on the map is coloured by how hot its file is, so a
/// selection that repainted one answered "which is selected" by erasing
/// the only thing the map is drawn to show. The mark is the ground under
/// it — the tile's own hue — and the border and the name go on saying
/// what they said.
#[test]
fn a_selected_tile_keeps_the_colour_that_says_how_hot_it_is() {
    let mut view = surface(Path::new("/repo"), Vec::new(), 0);
    view.absorb_measure(measured());
    press(&mut view, Command::ToggleMap);
    assert_eq!(view.showing(), ContentMode::Map);

    let space = (80, 20);
    let roles = |view: &CodeView| -> Vec<Role> {
        view.map_view()
            .expect("measured")
            .paint(space, crate::shared::canvas::Glyphs::Unicode)
            .lines()
            .into_iter()
            .flat_map(|line| line.spans)
            .map(|span| span.role)
            .collect()
    };
    let before = roles(&view);

    let frame = view
        .map_view()
        .expect("measured")
        .tiles(space)
        .into_iter()
        .find(|tile| tile.path == "src")
        .expect("a tile for src")
        .frame;
    let (x, y) = frame.center();
    handle_mouse(
        &mut view,
        Some(ViewHit::PlaceCaret {
            line: y as usize,
            cell: x as usize,
        }),
        Size {
            width: space.0 as u16,
            height: space.1 as u16,
        },
    );

    let painted = view
        .map_view()
        .expect("measured")
        .paint(space, crate::shared::canvas::Glyphs::Unicode);
    let spans: Vec<_> = painted
        .lines()
        .into_iter()
        .flat_map(|line| line.spans)
        .collect();

    // The ground is the tile's *own* hue, so what marks a cold file and
    // what marks a hot one are different colours — the map goes on being
    // readable while one of its tiles is picked.
    let picked_heat = view
        .map_view()
        .expect("measured")
        .tiles(space)
        .into_iter()
        .find(|tile| tile.path == "src")
        .expect("a tile for src")
        .heat;
    let grounds: Vec<Role> = spans.iter().filter_map(|span| span.ground).collect();
    assert!(!grounds.is_empty(), "the selection is a ground");
    assert!(
        grounds.iter().all(|role| *role == picked_heat.role()),
        "and it is the tile's own hue, not one of its own: {grounds:?}"
    );
    assert!(
        spans.iter().all(|span| span.role != Role::Accent),
        "and never a colour of its own: {:?}",
        spans
            .iter()
            .filter(|span| span.role == Role::Accent)
            .map(|span| &span.text)
            .collect::<Vec<_>>()
    );
    let named = |roles: &[Role]| {
        let mut seen: Vec<String> = roles.iter().map(|role| format!("{role:?}")).collect();
        seen.sort();
        seen.dedup();
        seen
    };
    let after: Vec<Role> = spans.iter().map(|span| span.role).collect();
    assert_eq!(
        named(&before),
        named(&after),
        "selecting changes no tile's colour"
    );
}

#[test]
fn a_tile_followed_on_the_map_is_the_file_the_surface_goes_back_to() {
    let mut view = surface(Path::new("/repo"), Vec::new(), 0);
    view.show(ContentMode::Contents);
    assert!(!view.has_map(), "nothing measured yet");
    press(&mut view, Command::ToggleMap);
    assert_eq!(view.showing(), ContentMode::Map, "entered all the same");
    assert!(
        view.wants_measure(),
        "which is what asks for the measurement"
    );

    view.absorb_measure(measured());

    let tile = |view: &CodeView, path: &str| {
        view.map_view()
            .expect("measured")
            .tiles((80, 20))
            .into_iter()
            .find(|tile| tile.path == path)
            .unwrap_or_else(|| panic!("no tile for {path}"))
            .frame
    };
    // Into `src`, then onto the file: a click selects, a second follows.
    for path in ["src", "src", "src/main.rs", "src/main.rs"] {
        let (x, y) = tile(&view, path).center();
        handle_mouse(
            &mut view,
            Some(ViewHit::PlaceCaret {
                line: y as usize,
                cell: x as usize,
            }),
            space(),
        );
    }
    assert_eq!(
        view.selected.as_deref(),
        Some(Path::new("/repo/src/main.rs")),
        "the tile is the selection every other half answers about"
    );
    assert_eq!(
        view.showing(),
        ContentMode::Contents,
        "and the map hands back to whatever it was opened over"
    );
}

fn chips(modes: &[crate::view::Mode]) -> Vec<(&str, bool)> {
    modes
        .iter()
        .map(|mode| (mode.label.as_str(), mode.active))
        .collect()
}

/// The two ends of the header row answer two questions: what the surface
/// is about, over the half that does the finding, and how what was found
/// is shown, over the half that shows it.
#[test]
fn the_header_offers_the_halves_on_one_side_and_the_renderings_on_the_other() {
    let machine = FakeMachine::default()
        .with_file("/w/README.md", "# hi\n")
        .with_file("/w/main.rs", "fn main() {}\n");
    let mut view = files_at("/w");
    settle(&mut view, &machine);
    view.absorb_measure(crate::code::Measure {
        root: PathBuf::from("/w"),
        files: vec![crate::code::FileMeasure {
            path: "main.rs".to_owned(),
            lines: 1,
            commits: 1,
            changed: false,
        }],
    });

    // On a document: the two ways of reading it, and the two halves.
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    let drawn = render::view(&view, space());
    assert_eq!(
        chips(&drawn.subjects),
        [("Files", true), ("Map", false), ("Changes", false)]
    );
    assert_eq!(chips(&drawn.modes), [("Preview", true), ("Source", false)]);
    // Each half is a thing, and says which. A way of *drawing* one is
    // not a thing, and carries no mark at all.
    assert_eq!(
        drawn
            .subjects
            .iter()
            .map(|mode| mode.icon)
            .collect::<Vec<_>>(),
        [RowIcon::Directory, RowIcon::Map, RowIcon::Changes]
    );
    assert!(
        drawn.modes.iter().all(|mode| mode.icon == RowIcon::None),
        "a rendering has nothing to be a picture of"
    );

    // On anything else there is one way to read it, and a control with
    // one option is a label that can be clicked. Back to the tree first:
    // activating a file puts the keyboard in what it opened.
    press(&mut view, Command::FocusNext);
    press(&mut view, Command::SelectNext);
    press(&mut view, Command::Activate);
    settle(&mut view, &machine);
    let drawn = render::view(&view, space());
    assert_eq!(
        chips(&drawn.subjects),
        [("Files", true), ("Map", false), ("Changes", false)],
        "the halves never come and go — the row that says where you are \
         is the one thing that may not move when it is used"
    );
    assert!(drawn.modes.is_empty(), "nothing to choose between");
}

/// The map takes the frame, and its levels are the breadcrumb: an
/// either/or with the tree, not a column beside it.
#[test]
fn the_map_takes_the_frame_and_says_which_level_it_is_on() {
    let mut view = surface(Path::new("/repo"), Vec::new(), 0);
    view.absorb_measure(measured());

    // One of the three halves, so it is reachable from each of the
    // others — including the changes, where going to the map is also
    // leaving them, which is what the press said.
    assert!(
        render::view(&view, space())
            .footer
            .contains(&Command::ToggleMap),
        "a measured checkout offers it wherever you are in it"
    );
    press(&mut view, Command::ToggleMap);
    assert_eq!(view.showing(), ContentMode::Map, "from the changes too");
    press(&mut view, Command::ToggleMap);
    assert_eq!(
        view.showing(),
        ContentMode::Diff,
        "and back to the half it was opened over"
    );

    view.show(ContentMode::Contents);
    press(&mut view, Command::ToggleMap);

    let drawn = render::view(&view, space());
    assert!(drawn.navigator.is_none(), "the map is the navigator");
    assert_eq!(drawn.layout, crate::view::Layout::Board);
    let steps: Vec<&str> = drawn.trail.iter().map(|step| step.name.as_str()).collect();
    assert_eq!(steps, ["repo"], "the checkout, and nothing entered yet");
    assert_eq!(
        chips(&drawn.subjects),
        [("Files", false), ("Map", true), ("Changes", false)],
        "which half you are in, and the way to the others as chips \
         somebody can point at rather than keys they have to know"
    );
    assert_eq!(
        chips(&drawn.modes),
        [("Unicode", true), ("ASCII", false), ("Ranking", false)],
        "and beside the content, only the ways of drawing it — named as \
         what they are, since the nav above already says Map"
    );

    // Down a level, and the key that closes comes back up before it
    // leaves the map. Entered the way a viewer does: a click selects the
    // tile, a second one goes in.
    let frame = view
        .map_view()
        .expect("measured")
        .tiles((80, 20))
        .into_iter()
        .find(|tile| tile.path == "src")
        .expect("src is a tile")
        .frame;
    let (x, y) = frame.center();
    for _ in 0..2 {
        handle_mouse(
            &mut view,
            Some(ViewHit::PlaceCaret {
                line: y as usize,
                cell: x as usize,
            }),
            space(),
        );
    }
    let steps: Vec<String> = render::view(&view, space())
        .trail
        .into_iter()
        .map(|step| step.name)
        .collect();
    assert_eq!(steps, ["repo", "src"]);

    press(&mut view, Command::Close);
    let steps: Vec<String> = render::view(&view, space())
        .trail
        .into_iter()
        .map(|step| step.name)
        .collect();
    assert_eq!(steps, ["repo"], "back up a level, not out of the map");
    // Coming back selects the directory that was entered, which is a
    // level of its own: the next press lets go of it.
    press(&mut view, Command::Close);
    assert_eq!(view.showing(), ContentMode::Map, "still on the map");
    press(&mut view, Command::Close);
    assert_eq!(
        view.showing(),
        ContentMode::Contents,
        "and only then out of it"
    );
}

/// The changes list opened on a file that is not among them — one the
/// tree was on, or one the last commit took off the list — lands on the
/// first change rather than beside an empty diff with nothing highlighted.
#[test]
fn the_changes_open_on_the_first_change_when_nothing_of_theirs_is_selected() {
    let mut open = fixture();
    open.selected = Some(PathBuf::from("/repo/README.md"));
    let files = open.changes.files.clone();
    open.absorb_changes(RefreshedChanges {
        placement: open.placement(),
        branch: "main".to_owned(),
        changes: Changes {
            files,
            ..Changes::default()
        },
    });

    assert_eq!(
        open.selected(),
        Some(Path::new("/repo/src/ui/git_diff.rs")),
        "the first row of the list is the selection"
    );
    assert!(open.diff_request().is_some(), "and its diff is asked for");
}

/// The same arriving from the tree: the file it was on has no diff, so
/// the list it switches to is read from its top.
#[test]
fn switching_to_the_changes_from_an_unchanged_file_selects_the_first_change() {
    let mut view = fixture();
    view.content = ContentMode::Contents;
    view.selected = Some(PathBuf::from("/repo/README.md"));

    view.show(ContentMode::Diff);

    assert_eq!(view.selected(), Some(Path::new("/repo/src/ui/git_diff.rs")));
}

/// A changed file already selected is left where it is.
#[test]
fn switching_to_the_changes_keeps_a_changed_file_selected() {
    let mut view = fixture();
    view.content = ContentMode::Contents;

    view.show(ContentMode::Diff);

    assert_eq!(view.selected(), Some(Path::new("/repo/src/ui.rs")));
}

/// What the host copies from a diff is the lines' text, without the
/// gutter that says how they changed.
#[test]
fn a_diff_reads_as_its_lines_without_the_gutter() {
    let view = fixture();

    assert_eq!(
        text(&view, 1..3),
        ["removed line".to_owned(), "added line".to_owned()]
    );
}

/// A range that runs past the end gives back the lines there are.
#[test]
fn text_past_the_end_is_only_the_lines_there_are() {
    let view = fixture();

    assert_eq!(text(&view, 2..9), ["added line".to_owned()]);
}

/// A file being read or edited reads as it is in the file; the map is a
/// drawing and has no text.
#[test]
fn a_file_reads_as_it_is_and_the_map_as_nothing() {
    let machine = FakeMachine::default().with_file("/w/notes.txt", "one\ttwo\nthree\n");
    let mut view = editing(&machine, "/w/notes.txt");

    assert_eq!(
        text(&view, 0..2),
        ["one\ttwo".to_owned(), "three".to_owned()]
    );
    view.content = ContentMode::Map;
    assert!(text(&view, 0..2).is_empty());
}
