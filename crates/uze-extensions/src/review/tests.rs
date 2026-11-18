//! The review surface, driven as the host drives it.

use std::path::{Path, PathBuf};

use super::git::{Base, FileDiff, Listing};
use super::*;
use crate::shared::git_status::{ChangedFile, FileStatus};
use crate::view::{Content, Size};

const SPACE: Size = Size {
    width: 100,
    height: 40,
};

fn diff_of(path: &Path, text: &str) -> FileDiff {
    FileDiff {
        path: path.to_path_buf(),
        cells: diff::read(text, path, crate::shared::highlight::FALLBACK_SYNTAX_THEME),
        fingerprint: git::fingerprint(text),
        binary: false,
        picture: None,
    }
}

const NAMING: &str = "\
diff --git a/naming.rs b/naming.rs
--- a/naming.rs
+++ b/naming.rs
@@ -1,4 +1,5 @@
 use std::fmt;
-fn old() {}
+fn apply() {
+    rename();
+}
 fn branch() {}
";

/// A review open on `/w` with `naming.rs` changed and its diff read.
pub(super) fn reviewing() -> ReviewView {
    let root = PathBuf::from("/w");
    let mut view = ReviewView::opening(root.clone(), "/w".to_owned());
    let path = root.join("naming.rs");
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files: vec![
                ChangedFile {
                    status: FileStatus::Modified,
                    path: path.clone(),
                    renamed_from: None,
                },
                ChangedFile {
                    status: FileStatus::Added,
                    path: root.join("tests.rs"),
                    renamed_from: None,
                },
            ],
            uncommitted: [path.clone()].into(),
            partners: Default::default(),
            stamps: [(path.clone(), 1), (root.join("tests.rs"), 1)].into(),
            head: None,
        }),
    );
    // Asked as the diff job asks it, against what the list read.
    view.asking = view.diff_key(&path).map(|key| (path.clone(), key));
    view.absorb_diff(path.clone(), Ok(diff_of(&path, NAMING)), "x".to_owned());
    view
}

fn line_texts(view: &ReviewView) -> Vec<String> {
    match super::view(view, SPACE).content {
        Content::Lines { lines, .. } => lines.iter().map(|line| line.text()).collect(),
        other => panic!("not lines: {other:?}"),
    }
}

#[test]
fn a_marked_run_offers_a_comment_under_its_last_line() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 1,
            extend: false,
        },
        SPACE,
    );
    // The affordance now stands under line 1, so the frame's fourth line
    // is the diff's third.
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 4,
            extend: true,
        },
        SPACE,
    );
    assert_eq!(view.marked(), Some((1, 3)));
    let lines = line_texts(&view);
    assert!(lines[4].contains("comment on 3 lines"), "{lines:?}");
}

#[test]
fn a_comment_written_on_marked_lines_stands_under_them_as_a_draft() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    assert!(view.typing());
    for character in "check it".chars() {
        handle_command(&mut view, Command::Type(character), SPACE);
    }
    handle_command(&mut view, Command::AddComment, SPACE);
    assert!(!view.typing());
    assert_eq!(view.drafts().len(), 1);
    assert_eq!(view.drafts()[0].path, "naming.rs");
    assert_eq!(view.drafts()[0].rows[0].text, "fn apply() {");
    let lines = line_texts(&view);
    assert!(
        lines[3].is_empty(),
        "the card opens on a row of air: {lines:?}"
    );
    assert!(
        lines[4].contains("L2") && lines[4].contains("draft"),
        "{lines:?}"
    );
    assert!(lines[5].contains("check it"), "{lines:?}");
    assert_eq!(view.unwritten.len(), 1, "kept to be written");
}

#[test]
fn an_empty_comment_is_not_kept() {
    let mut view = reviewing();
    handle_command(&mut view, Command::FocusNext, SPACE);
    handle_command(&mut view, Command::Comment, SPACE);
    handle_command(&mut view, Command::AddComment, SPACE);
    assert!(view.drafts().is_empty());
}

/// The list as the agent left it: naming.rs at content `naming`.
fn relisted(view: &mut ReviewView, naming: u64) {
    let root = PathBuf::from("/w");
    let files = view.files.clone();
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files,
            uncommitted: Default::default(),
            partners: Default::default(),
            stamps: [(root.join("naming.rs"), naming), (root.join("tests.rs"), 1)].into(),
            head: None,
        }),
    );
}

#[test]
fn viewed_marks_the_file_and_moves_on_and_falls_away_when_it_changes() {
    let mut view = reviewing();
    let naming = PathBuf::from("/w/naming.rs");
    handle_command(&mut view, Command::MarkViewed, SPACE);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/tests.rs")));
    assert_eq!(view.viewed(&naming), surface::Viewed::Yes);
    assert_eq!(view.viewed_count(), 1);
    // The agent changes naming.rs while tests.rs is on show.
    relisted(&mut view, 2);
    assert_eq!(
        view.viewed(&naming),
        surface::Viewed::Changed,
        "though not on show"
    );
    assert_eq!(view.viewed_count(), 0);
}

#[test]
fn switching_the_filter_keeps_what_was_viewed() {
    let mut view = reviewing();
    let naming = PathBuf::from("/w/naming.rs");
    handle_command(&mut view, Command::MarkViewed, SPACE);
    handle_hit(&mut view, ViewHit::SelectSubject(1), SPACE);
    handle_hit(&mut view, ViewHit::SelectSubject(0), SPACE);
    relisted(&mut view, 1);
    assert_eq!(view.viewed(&naming), surface::Viewed::Yes);
}

#[test]
fn a_diff_is_read_again_only_once_its_file_moved() {
    let mut view = reviewing();
    assert!(!view.diff_pending, "read once");
    relisted(&mut view, 1);
    assert!(!view.diff_pending, "nothing moved, nothing is read");
    relisted(&mut view, 2);
    assert!(view.diff_pending, "the file moved");
}

#[test]
fn a_comment_whose_lines_are_gone_stands_at_the_end_marked_changed() {
    let mut view = reviewing();
    let naming = PathBuf::from("/w/naming.rs");
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    handle_command(&mut view, Command::Type('x'), SPACE);
    handle_command(&mut view, Command::AddComment, SPACE);
    view.absorb_diff(
        naming.clone(),
        Ok(diff_of(
            &naming,
            &NAMING.replace("fn apply() {", "fn apply() -> R {"),
        )),
        "x".to_owned(),
    );
    let lines = line_texts(&view);
    let at = lines
        .iter()
        .position(|line| line.contains("these lines changed"))
        .expect("marked as changed");
    assert!(at > 5, "after the diff: {lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("fn apply() {")),
        "its old lines are quoted"
    );
}

#[test]
fn a_selection_marks_the_lines_it_covered_and_is_quoted() {
    let mut view = reviewing();
    selected(&mut view, 2..3, ".ok()?");
    assert_eq!(view.marked(), Some((2, 2)));
    handle_command(&mut view, Command::Comment, SPACE);
    assert_eq!(view.composer.as_ref().unwrap().excerpt, ".ok()?");
}

/// Opens the selected file's menu and picks its discard, as a click on
/// the entry does.
fn ask_to_discard(view: &mut ReviewView) {
    handle_command(view, Command::OpenMenu, SPACE);
    let entries = super::view(view, SPACE)
        .navigator
        .unwrap()
        .menu
        .unwrap()
        .entries;
    let discard = entries
        .iter()
        .position(|entry| entry == "Discard changes…")
        .expect("a discard is offered");
    handle_hit(view, ViewHit::MenuEntry(discard), SPACE);
}

fn menu_entries(view: &ReviewView) -> Vec<String> {
    super::view(view, SPACE)
        .navigator
        .unwrap()
        .menu
        .map(|menu| menu.entries)
        .unwrap_or_default()
}

/// A file's row opens what can be done to it, as the code tree's does:
/// open it, take its path, and, only where something is not committed,
/// throw that away.
#[test]
fn a_files_menu_opens_copies_and_offers_a_discard_only_where_it_can() {
    let mut view = reviewing();
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert_eq!(
        menu_entries(&view),
        [
            "Open file",
            "Copy path",
            "Copy relative path",
            "Discard changes…"
        ]
    );
    handle_command(&mut view, Command::Close, SPACE);
    assert!(view.menu.is_none(), "esc puts it away");
    ask_to_discard(&mut view);
    let asked = super::view(&view, SPACE).confirm.expect("asked first");
    assert!(asked.body.contains("Puts the file back"), "{}", asked.body);
    handle_command(&mut view, Command::Close, SPACE);
    view.select_file(1);
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert_eq!(
        menu_entries(&view),
        ["Open file", "Copy path", "Copy relative path"],
        "tests.rs is committed only"
    );
    handle_command(&mut view, Command::SelectNext, SPACE);
    handle_command(&mut view, Command::SelectNext, SPACE);
    assert!(matches!(
        handle_command(&mut view, Command::Activate, SPACE),
        Asked::Copy(path) if path == "tests.rs"
    ));
    assert!(view.menu.is_none(), "picked, the menu goes");
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert!(matches!(
        handle_hit(&mut view, ViewHit::MenuEntry(1), SPACE),
        Asked::Copy(path) if path == PathBuf::from("/w").join("tests.rs").to_string_lossy()
    ));
}

#[test]
fn open_file_from_the_menu_opens_the_code_at_its_top() {
    use crate::contract::{Extension, Outcome};
    let mut extension = sending_extension();
    let view = extension.review_view_mut().unwrap();
    *view = reviewing();
    let asked = extension.command(SURFACE, Command::OpenMenu, SPACE);
    assert_eq!(asked, Outcome::Stay);
    let opened = extension.command(SURFACE, Command::Activate, SPACE);
    assert_eq!(
        opened,
        Outcome::OpenPath {
            project: PathBuf::from("/w"),
            target: PathBuf::from("/w/naming.rs"),
            line: None,
        }
    );
}

#[test]
fn a_changed_image_is_drawn_and_a_binary_without_a_picture_says_it_has_no_lines() {
    let mut view = reviewing();
    let image = PathBuf::from("/w/naming.rs");
    let picture = crate::view::Picture {
        width: 1,
        height: 1,
        pixels: std::sync::Arc::from(vec![[0u8, 0, 0, 255]]),
    };
    view.absorb_diff(
        image.clone(),
        Ok(FileDiff {
            path: image.clone(),
            cells: Vec::new(),
            fingerprint: 1,
            binary: true,
            picture: Some(picture),
        }),
        "x".to_owned(),
    );
    assert!(matches!(
        super::view(&view, SPACE).content,
        Content::Picture { .. }
    ));
    view.absorb_diff(
        image.clone(),
        Ok(FileDiff {
            path: image,
            cells: Vec::new(),
            fingerprint: 2,
            binary: true,
            picture: None,
        }),
        "x".to_owned(),
    );
    match super::view(&view, SPACE).content {
        Content::Message { hint, .. } => {
            assert!(hint.unwrap_or_default().contains("no lines to compare"));
        }
        other => panic!("not a message: {other:?}"),
    }
}

#[test]
fn a_diff_read_for_a_file_no_longer_selected_is_dropped() {
    let mut view = reviewing();
    view.select_file(1);
    let naming = PathBuf::from("/w/naming.rs");
    view.absorb_diff(naming.clone(), Ok(diff_of(&naming, NAMING)), "x".to_owned());
    assert!(view.cells().is_empty(), "the late answer is not drawn");
    assert!(
        view.diff_pending,
        "and the selection's own read is still awaited"
    );
}

#[test]
fn the_filter_stays_on_offer_when_nothing_is_uncommitted() {
    let mut view = reviewing();
    handle_hit(&mut view, ViewHit::SelectSubject(1), SPACE);
    assert_eq!(view.filter, Filter::Uncommitted);
    view.absorb_listing(
        Filter::Uncommitted,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing::default()),
    );
    let drawn = super::view(&view, SPACE);
    assert!(matches!(drawn.content, Content::Message { .. }));
    let offered: Vec<&str> = drawn
        .subjects
        .iter()
        .map(|chip| chip.label.as_str())
        .collect();
    assert_eq!(
        offered,
        ["all", "uncommitted"],
        "the way back is still there"
    );
    handle_hit(&mut view, ViewHit::SelectSubject(0), SPACE);
    assert_eq!(view.filter, Filter::All);
}

#[test]
fn marked_lines_say_so_whatever_their_tone() {
    let mut view = reviewing();
    view.focus = surface::Focus::Content;
    handle_command(
        &mut view,
        Command::ExtendMark(crate::view::ScrollDirection::Down),
        SPACE,
    );
    handle_command(
        &mut view,
        Command::ExtendMark(crate::view::ScrollDirection::Down),
        SPACE,
    );
    let Content::Lines { lines, .. } = super::view(&view, SPACE).content else {
        panic!("lines");
    };
    let marked: Vec<bool> = lines.iter().take(4).map(|line| line.marked).collect();
    assert_eq!(
        marked,
        [true, true, true, false],
        "a removal and an addition among them"
    );
}

/// A host that keeps one review in memory, as the workspace's keeps it on
/// disk.
struct Keeping(std::sync::Mutex<ReviewRecord>);

impl crate::Host for Keeping {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        Ok(path.to_path_buf())
    }
    fn read_file(&self, _: &Path) -> Result<String, crate::Unreadable> {
        Err(crate::Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<crate::DirEntry>, String> {
        Ok(Vec::new())
    }
    fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
        unreachable!()
    }
    fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!()
    }
    fn rename_path(&self, _: &Path, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!()
    }
    fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
        unreachable!()
    }
    fn syntax_theme(&self) -> String {
        crate::shared::highlight::FALLBACK_SYNTAX_THEME.to_owned()
    }
    fn review_record(&self, _: &Path, _: Option<&str>) -> Result<ReviewRecord, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn update_review_record(
        &self,
        _: &Path,
        _: Option<&str>,
        change: &mut dyn FnMut(&mut ReviewRecord),
    ) -> Result<ReviewRecord, String> {
        let mut kept = self.0.lock().unwrap();
        change(&mut kept);
        Ok(kept.clone())
    }
}

#[test]
fn a_draft_kept_just_before_the_review_closes_is_still_written() {
    use crate::contract::{Checkout, Extension};
    let mut extension = ReviewExtension::default();
    let checkout = Checkout {
        root: PathBuf::from("/w"),
        display: "/w".to_owned(),
        target: None,
        agent: None,
    };
    extension.open(SURFACE, &checkout);
    let view = extension.review_view_mut().unwrap();
    view.change(change::Change::Draft {
        id: 1,
        path: "naming.rs".to_owned(),
        rows: Vec::new(),
        excerpt: String::new(),
        text: "kept".to_owned(),
    });
    extension.close(SURFACE);

    let host = Keeping(std::sync::Mutex::new(ReviewRecord::default()));
    for job in extension.jobs() {
        let answer = (job.run)(&host);
        extension.absorb(answer);
    }
    let kept = host.0.lock().unwrap();
    assert_eq!(
        kept.comments.len(),
        1,
        "written though the surface had closed"
    );
    assert_eq!(kept.comments[0].text, "kept");
}

#[test]
fn a_comment_being_written_stays_with_its_file_while_the_list_moves() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    handle_command(&mut view, Command::Type('x'), SPACE);
    handle_hit(&mut view, ViewHit::SelectItem(1), SPACE);
    assert!(view.typing(), "a click on another file keeps the comment");
    handle_hit(&mut view, ViewHit::OpenMenu(1), SPACE);
    assert!(
        view.menu.is_none(),
        "nor opens actions on the file being commented"
    );
    handle_hit(&mut view, ViewHit::SelectSubject(1), SPACE);
    assert_eq!(view.filter, Filter::All, "nor switches the filter under it");
    // The agent commits naming.rs: the list no longer holds it.
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files: vec![ChangedFile {
                status: FileStatus::Added,
                path: PathBuf::from("/w/tests.rs"),
                renamed_from: None,
            }],
            uncommitted: Default::default(),
            partners: Default::default(),
            stamps: Default::default(),
            head: None,
        }),
    );
    assert!(
        view.selected_file().is_some(),
        "the file stays listed while its comment is written"
    );
    handle_command(&mut view, Command::AddComment, SPACE);
    assert_eq!(view.drafts().len(), 1);
    assert_eq!(
        view.drafts()[0].path,
        "naming.rs",
        "kept on the file it was written on"
    );
}

#[test]
fn discarding_a_rename_puts_back_both_of_its_paths() {
    let mut view = reviewing();
    let (new, old) = (PathBuf::from("/w/naming.rs"), PathBuf::from("/w/old.rs"));
    view.partners = [(new.clone(), old.clone()), (old.clone(), new.clone())].into();
    ask_to_discard(&mut view);
    handle_hit(&mut view, ViewHit::Answer(true), SPACE);
    assert_eq!(view.restore, Some(vec![new, old]));
}

/// The review open on /w, with agent a1 working there and one draft.
fn sending_extension() -> ReviewExtension {
    use crate::contract::{Checkout, Extension};
    let mut extension = ReviewExtension::default();
    let checkout = Checkout {
        root: PathBuf::from("/w"),
        display: "/w".to_owned(),
        target: None,
        agent: None,
    };
    extension.focus(Some(&checkout));
    extension.open(SURFACE, &checkout);
    extension.agents(&serde_json::json!([{ "id": "a1", "checkout": "/w" }]));
    extension
        .review_view_mut()
        .unwrap()
        .change(change::Change::Draft {
            id: 1,
            path: "naming.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: "check".to_owned(),
        });
    extension
}

fn badge(extension: &ReviewExtension) -> String {
    use crate::contract::Extension;
    extension
        .door_badge()
        .unwrap()
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect()
}

#[test]
fn a_send_typed_after_the_review_closed_still_sends_its_drafts() {
    use crate::contract::{AgentWritten, Extension};
    let mut extension = sending_extension();
    extension.command(SURFACE, Command::Send, SPACE);
    extension.close(SURFACE);
    extension.agent_write("review-send", AgentWritten::Typed);
    let host = Keeping(std::sync::Mutex::new(ReviewRecord::default()));
    for _ in 0..2 {
        for job in extension.jobs() {
            let answer = (job.run)(&host);
            extension.absorb(answer);
        }
    }
    let kept = host.0.lock().unwrap();
    assert_eq!(kept.comments[0].status, CommentStatus::Sent);
    assert_eq!(kept.comments[0].agent.as_deref(), Some("a1"));
}

/// A list read that began before a write landed is answered after it: the
/// draft that write kept stays on screen.
#[test]
fn a_record_read_before_a_write_landed_never_takes_its_draft_away() {
    use crate::contract::Extension;
    let mut extension = sending_extension();
    let host = Keeping(std::sync::Mutex::new(ReviewRecord::default()));
    let jobs = extension.jobs();
    // door, list, write: the list reads before the write runs.
    let mut answers: Vec<_> = jobs.into_iter().map(|job| (job.run)(&host)).collect();
    let written = answers.pop().unwrap();
    let listed = answers.pop().unwrap();
    // The list read again after the write: the draft is on disk now.
    extension.absorb(written);
    extension.absorb(listed);
    assert_eq!(extension.review_view().unwrap().drafts().len(), 1);
}

#[test]
fn a_draft_laid_again_over_a_record_holding_it_is_not_kept_twice() {
    let mut record = ReviewRecord::default();
    let draft = change::Change::Draft {
        id: 1,
        path: "a.rs".to_owned(),
        rows: Vec::new(),
        excerpt: String::new(),
        text: "once".to_owned(),
    };
    draft.apply(&mut record);
    draft.apply(&mut record);
    assert_eq!(record.comments.len(), 1);
}

#[test]
fn a_review_is_not_sent_where_two_agents_share_the_checkout_and_neither_is_in_front() {
    use crate::contract::{Extension, Outcome};
    let mut extension = sending_extension();
    extension.agents(&serde_json::json!([
        { "id": "a1", "checkout": "/w" },
        { "id": "a2", "checkout": "/w" },
    ]));
    let asked = extension.command(SURFACE, Command::Send, SPACE);
    assert_eq!(asked, Outcome::Stay, "nothing is typed into either");
}

#[test]
fn a_review_in_a_shared_checkout_is_sent_to_the_agent_in_front() {
    use crate::contract::{Checkout, Extension, Outcome};
    let mut extension = ReviewExtension::default();
    let checkout = Checkout {
        root: PathBuf::from("/w"),
        display: "/w".to_owned(),
        target: None,
        agent: Some("a2".to_owned()),
    };
    extension.focus(Some(&checkout));
    extension.open(SURFACE, &checkout);
    extension.agents(&serde_json::json!([
        { "id": "a1", "checkout": "/w" },
        { "id": "a2", "checkout": "/w" },
    ]));
    extension
        .review_view_mut()
        .unwrap()
        .change(change::Change::Draft {
            id: 1,
            path: "naming.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: "check".to_owned(),
        });
    let Outcome::Agent(asked) = extension.command(SURFACE, Command::Send, SPACE) else {
        panic!("the send is asked");
    };
    assert_eq!(asked.agent.as_deref(), Some("a2"));
}

#[test]
fn a_comment_on_a_line_that_repeats_stands_under_the_one_it_was_written_on() {
    let path = PathBuf::from("/w/new.rs");
    let text = "@@ -0,0 +1,5 @@\n+fn a() {\n+}\n+fn b() {\n+}\n+\n";
    let cells = diff_of(&path, text).cells;
    let rows = vec![surface::row_of(&cells[3])];
    assert_eq!(rows[0].text, "}");
    assert_eq!(
        surface::locate(&rows, &cells),
        Some(3),
        "the second brace, not the first"
    );
}

/// A host that refuses to keep any review: no agent works here.
struct Refusing;

impl crate::Host for Refusing {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        Ok(path.to_path_buf())
    }
    fn read_file(&self, _: &Path) -> Result<String, crate::Unreadable> {
        Err(crate::Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<crate::DirEntry>, String> {
        Ok(Vec::new())
    }
    fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
        unreachable!()
    }
    fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!()
    }
    fn rename_path(&self, _: &Path, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!()
    }
    fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
        unreachable!()
    }
    fn syntax_theme(&self) -> String {
        crate::shared::highlight::FALLBACK_SYNTAX_THEME.to_owned()
    }
}

/// A host that keeps a review for each agent, as the workspace's does, and
/// like it refuses one asked with no agent in front of a shared checkout.
#[derive(Default)]
struct KeepingEach(std::sync::Mutex<std::collections::HashMap<String, ReviewRecord>>);

impl KeepingEach {
    fn comments_of(&self, agent: &str) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .get(agent)
            .map(|record| record.comments.iter().map(|c| c.text.clone()).collect())
            .unwrap_or_default()
    }
}

impl crate::Host for KeepingEach {
    fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
        Err("no git here".to_owned())
    }
    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        Ok(path.to_path_buf())
    }
    fn read_file(&self, _: &Path) -> Result<String, crate::Unreadable> {
        Err(crate::Unreadable::NotText)
    }
    fn list_dir(&self, _: &Path) -> Result<Vec<crate::DirEntry>, String> {
        Ok(Vec::new())
    }
    fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
        unreachable!()
    }
    fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!()
    }
    fn rename_path(&self, _: &Path, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!()
    }
    fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
        unreachable!()
    }
    fn syntax_theme(&self) -> String {
        crate::shared::highlight::FALLBACK_SYNTAX_THEME.to_owned()
    }
    fn review_record(&self, _: &Path, in_front: Option<&str>) -> Result<ReviewRecord, String> {
        let agent = in_front.ok_or("none is in front")?;
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(agent)
            .cloned()
            .unwrap_or_default())
    }
    fn update_review_record(
        &self,
        _: &Path,
        in_front: Option<&str>,
        change: &mut dyn FnMut(&mut ReviewRecord),
    ) -> Result<ReviewRecord, String> {
        let agent = in_front.ok_or("none is in front")?;
        let mut kept = self.0.lock().unwrap();
        let record = kept.entry(agent.to_owned()).or_default();
        change(record);
        Ok(record.clone())
    }
}

fn shared_checkout(agent: Option<&str>) -> crate::contract::Checkout {
    crate::contract::Checkout {
        root: PathBuf::from("/w"),
        display: "/w".to_owned(),
        target: None,
        agent: agent.map(str::to_owned),
    }
}

/// `text` written on the review of `agent`, which closes before it is
/// written.
fn drafted_and_closed(extension: &mut ReviewExtension, agent: Option<&str>, text: &str) {
    use crate::contract::Extension;
    extension.open(SURFACE, &shared_checkout(agent));
    extension
        .review_view_mut()
        .unwrap()
        .change(change::Change::Draft {
            id: 1,
            path: "naming.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: text.to_owned(),
        });
    extension.close(SURFACE);
}

/// A review no agent keeps fails for good: what another agent has waiting
/// behind it, a send included, still reaches its record.
#[test]
fn a_write_that_cannot_land_never_holds_up_another_agents() {
    use crate::contract::Extension;
    let mut extension = ReviewExtension::default();
    drafted_and_closed(&mut extension, None, "nobody's");
    drafted_and_closed(&mut extension, Some("a1"), "a1's");
    let host = KeepingEach::default();
    for _ in 0..2 {
        for job in extension.jobs() {
            extension.absorb((job.run)(&host));
        }
        extension.retry_now();
    }
    assert_eq!(host.comments_of("a1"), ["a1's"]);
}

/// What each agent writes in a checkout they share lands in that agent's
/// record, however late it is written.
#[test]
fn what_is_written_lands_in_the_record_of_the_agent_it_was_written_for() {
    use crate::contract::Extension;
    let mut extension = ReviewExtension::default();
    drafted_and_closed(&mut extension, Some("a1"), "a1's");
    drafted_and_closed(&mut extension, Some("a2"), "a2's");
    let host = KeepingEach::default();
    for _ in 0..2 {
        for job in extension.jobs() {
            extension.absorb((job.run)(&host));
        }
    }
    assert_eq!(host.comments_of("a1"), ["a1's"]);
    assert_eq!(host.comments_of("a2"), ["a2's"]);
}

/// The drawer's read of the agent in front is that agent's own.
#[test]
fn the_door_reads_the_review_of_the_agent_in_front() {
    use crate::Host as _;
    use crate::contract::Extension;
    let host = KeepingEach::default();
    for (agent, text) in [("a1", "a1's"), ("a2", "a2's")] {
        host.update_review_record(Path::new("/w"), Some(agent), &mut |record| {
            change::Change::Draft {
                id: 1,
                path: "naming.rs".to_owned(),
                rows: Vec::new(),
                excerpt: String::new(),
                text: text.to_owned(),
            }
            .apply(record);
        })
        .unwrap();
    }
    let mut extension = ReviewExtension::default();
    extension.focus(Some(&shared_checkout(Some("a2"))));
    for job in extension.jobs() {
        extension.absorb((job.run)(&host));
    }
    let drawer = format!("{:?}", extension.view(COMMENTS, SPACE));
    assert!(drawer.contains("a2's"), "{drawer}");
    assert!(!drawer.contains("a1's"), "{drawer}");
}

/// Opening the review on another agent in the checkout hands it nothing of
/// what the one before left to be written.
#[test]
fn a_review_opened_for_an_agent_takes_nothing_another_left_waiting() {
    use crate::contract::Extension;
    let mut extension = ReviewExtension::default();
    drafted_and_closed(&mut extension, Some("a1"), "a1's");
    extension.open(SURFACE, &shared_checkout(Some("a2")));
    let view = extension.review_view().unwrap();
    assert!(view.record.comments.is_empty());
    assert!(view.unwritten.is_empty());
    extension.close(SURFACE);
    extension.open(SURFACE, &shared_checkout(Some("a1")));
    let view = extension.review_view().unwrap();
    assert_eq!(view.record.comments.len(), 1, "a1's, back with a1");
}

/// A list read asked for one agent lands after the review moved to another
/// in the same checkout: Git's answer stands, the record is not theirs.
#[test]
fn a_list_read_of_one_agent_never_lays_its_record_over_anothers() {
    use crate::Host as _;
    use crate::contract::Extension;
    let host = KeepingEach::default();
    host.update_review_record(Path::new("/w"), Some("a1"), &mut |record| {
        change::Change::Draft {
            id: 1,
            path: "naming.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: "a1's".to_owned(),
        }
        .apply(record);
    })
    .unwrap();
    let mut extension = ReviewExtension::default();
    extension.open(SURFACE, &shared_checkout(Some("a1")));
    let asked = extension.jobs();
    extension.close(SURFACE);
    extension.open(SURFACE, &shared_checkout(Some("a2")));
    for job in asked {
        extension.absorb((job.run)(&host));
    }
    assert!(
        extension.review_view().unwrap().record.comments.is_empty(),
        "a2 was shown a1's comment"
    );
}

/// The open review's own read is of the agent it is kept for.
#[test]
fn the_open_review_reads_the_record_of_its_agent() {
    use crate::Host as _;
    use crate::contract::Extension;
    let host = KeepingEach::default();
    host.update_review_record(Path::new("/w"), Some("a1"), &mut |record| {
        change::Change::Draft {
            id: 1,
            path: "naming.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: "a1's".to_owned(),
        }
        .apply(record);
    })
    .unwrap();
    let mut extension = ReviewExtension::default();
    extension.open(SURFACE, &shared_checkout(Some("a1")));
    for job in extension.jobs() {
        extension.absorb((job.run)(&host));
    }
    let view = extension.review_view().unwrap();
    assert_eq!(view.record.comments.len(), 1);
    assert_eq!(view.record_error, None);
}

/// A file one agent's review unmarked by hand stays that review's.
#[test]
fn a_file_unmarked_for_one_agent_is_not_unmarked_for_another() {
    use crate::contract::Extension;
    let mut extension = ReviewExtension::default();
    extension.open(SURFACE, &shared_checkout(Some("a1")));
    extension
        .review_view_mut()
        .unwrap()
        .unmarked
        .insert(PathBuf::from("/w/naming.rs"), 7);
    extension.close(SURFACE);
    extension.open(SURFACE, &shared_checkout(Some("a2")));
    assert!(extension.review_view().unwrap().unmarked.is_empty());
    extension.close(SURFACE);
    extension.open(SURFACE, &shared_checkout(Some("a1")));
    assert_eq!(extension.review_view().unwrap().unmarked.len(), 1);
}

/// A review open that cannot be kept fails for good, and what another agent
/// left waiting still reaches its record.
#[test]
fn an_open_review_that_cannot_be_kept_never_holds_up_another_agents() {
    use crate::contract::Extension;
    let mut extension = ReviewExtension::default();
    drafted_and_closed(&mut extension, Some("a1"), "a1's");
    extension.open(SURFACE, &shared_checkout(None));
    extension
        .review_view_mut()
        .unwrap()
        .change(change::Change::Draft {
            id: 1,
            path: "naming.rs".to_owned(),
            rows: Vec::new(),
            excerpt: String::new(),
            text: "nobody's".to_owned(),
        });
    let host = KeepingEach::default();
    for _ in 0..3 {
        for job in extension.jobs() {
            extension.absorb((job.run)(&host));
        }
        extension.retry_now();
    }
    assert_eq!(host.comments_of("a1"), ["a1's"]);
}

/// A write with no agent to go to is nobody's: the one agent left in the
/// checkout once the others go must not inherit it.
#[test]
fn a_write_for_nobody_never_lands_on_the_agent_left_in_the_checkout() {
    use crate::contract::Extension;
    let mut extension = ReviewExtension::default();
    extension.agents(&serde_json::json!([
        { "id": "a1", "checkout": "/w" },
        { "id": "a2", "checkout": "/w" },
    ]));
    drafted_and_closed(&mut extension, None, "nobody's");
    extension.agents(&serde_json::json!([{ "id": "a1", "checkout": "/w" }]));
    assert!(extension.jobs().is_empty(), "nothing is left to write");
}

/// Another review being written is no news about the one that cannot be
/// kept: that one is still said once.
#[test]
fn a_write_that_keeps_failing_is_said_once_while_another_agents_land() {
    use crate::contract::{Extension, Outcome};
    let mut extension = ReviewExtension::default();
    drafted_and_closed(&mut extension, None, "nobody's");
    let host = KeepingEach::default();
    let mut said = 0;
    for _ in 0..3 {
        drafted_and_closed(&mut extension, Some("a1"), "a1's");
        for _ in 0..2 {
            for job in extension.jobs() {
                if matches!(extension.absorb((job.run)(&host)), Outcome::Failed { .. }) {
                    said += 1;
                }
            }
            extension.retry_now();
        }
    }
    assert_eq!(said, 1);
}

#[test]
fn a_write_that_keeps_failing_is_said_once() {
    use crate::contract::{Extension, Outcome};
    let mut extension = sending_extension();
    extension.close(SURFACE);
    let mut said = 0;
    for _ in 0..3 {
        for job in extension.jobs() {
            if matches!(
                extension.absorb((job.run)(&Refusing)),
                Outcome::Failed { .. }
            ) {
                said += 1;
            }
        }
        extension.retry_now();
    }
    assert_eq!(said, 1, "tried again, said once");
}

#[test]
fn a_comment_that_cannot_be_kept_is_not_offered() {
    let mut view = reviewing();
    view.absorb_record(
        Err("no agent UZE launched works in this checkout".to_owned()),
        &[],
    );
    view.focus = surface::Focus::Content;
    handle_command(&mut view, Command::Comment, SPACE);
    assert!(!view.typing());
    assert!(view.notice.is_some(), "and says why");
}

#[test]
fn a_send_refused_or_declined_leaves_the_drafts_drafts() {
    use crate::contract::{AgentWritten, Extension};
    for written in [
        AgentWritten::Refused("busy".to_owned()),
        AgentWritten::Declined,
    ] {
        let mut extension = sending_extension();
        extension.command(SURFACE, Command::Send, SPACE);
        extension.agent_write("review-send", written);
        let view = extension.review_view().unwrap();
        assert_eq!(view.drafts().len(), 1);
        assert!(view.notice.is_some(), "and says it was not sent");
    }
}

/// The door says how large the branch is against its target, and nothing
/// more: a draft or an answer waiting is the review's to say, inside it.
#[test]
fn the_door_counts_the_branch_and_nothing_else() {
    use crate::contract::Extension;
    let mut extension = sending_extension();
    assert!(extension.door_badge().is_none(), "nothing changed, no door");
    extension.counted(PathBuf::from("/w"), 14_714, 672, 3);
    assert_eq!(
        badge(&extension),
        "+14,714 -672",
        "a draft waits, and is not said"
    );
    let view = extension.review_view_mut().unwrap();
    view.record.comments[0].status = CommentStatus::Answered;
    assert_eq!(badge(&extension), "+14,714 -672", "nor an answer");
}

#[test]
fn resolve_puts_the_answered_comment_away() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    handle_command(&mut view, Command::Type('x'), SPACE);
    handle_command(&mut view, Command::AddComment, SPACE);
    view.record.comments[0].status = CommentStatus::Answered;
    handle_command(&mut view, Command::Resolve, SPACE);
    assert_eq!(view.record.comments[0].status, CommentStatus::Resolved);
    assert!(
        !line_texts(&view)
            .iter()
            .any(|line| line.contains("answered"))
    );
}

#[test]
fn open_in_code_lands_on_a_line_the_new_file_has() {
    use crate::contract::{Extension, Outcome};
    let mut extension = sending_extension();
    let path = PathBuf::from("/w/naming.rs");
    let view = extension.review_view_mut().unwrap();
    view.files = reviewing().files;
    view.selected = Some(path.clone());
    view.absorb_diff(path.clone(), Ok(diff_of(&path, NAMING)), "x".to_owned());
    // The removed `fn old() {}` is cell 1: it has no line in the new file.
    view.mark = Some((1, 1));
    let asked = extension.command(SURFACE, Command::OpenInCode, SPACE);
    assert_eq!(
        asked,
        Outcome::OpenPath {
            project: PathBuf::from("/w"),
            target: path,
            line: Some(2),
        }
    );
}

#[test]
fn a_discard_is_asked_first_and_only_then_handed_to_the_host() {
    let mut view = reviewing();
    ask_to_discard(&mut view);
    assert_eq!(view.restore, None, "asked, not done");
    handle_hit(&mut view, ViewHit::Answer(false), SPACE);
    assert_eq!(view.restore, None, "declined");
    ask_to_discard(&mut view);
    handle_hit(&mut view, ViewHit::Answer(true), SPACE);
    assert_eq!(view.restore, Some(vec![PathBuf::from("/w/naming.rs")]));
}

/// The file moved while its diff was out: the answer is of the content
/// before, and the next list reads it again.
#[test]
fn a_diff_answered_after_its_file_moved_is_read_again() {
    let mut view = reviewing();
    let path = PathBuf::from("/w/naming.rs");
    view.asking = view.diff_key(&path).map(|key| (path.clone(), key));
    relisted(&mut view, 2);
    view.absorb_diff(path.clone(), Ok(diff_of(&path, NAMING)), "x".to_owned());
    relisted(&mut view, 2);
    assert!(
        view.diff_pending,
        "the diff on show is of the content before"
    );
}

#[test]
fn a_diff_is_read_again_once_the_commit_checked_out_moves() {
    let mut view = reviewing();
    view.filter = Filter::Uncommitted;
    let files = view.files.clone();
    let path = PathBuf::from("/w/naming.rs");
    view.diffed = view.diff_key(&path).map(|key| (path.clone(), key));
    view.absorb_listing(
        Filter::Uncommitted,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files,
            uncommitted: Default::default(),
            partners: Default::default(),
            stamps: [(path.clone(), 1)].into(),
            head: Some("c0ffee".to_owned()),
        }),
    );
    assert!(
        view.diff_pending,
        "a hunk committed moves the diff, not the file"
    );
}

#[test]
fn a_file_git_cannot_stamp_reads_changed_and_cannot_be_marked() {
    let mut view = reviewing();
    let path = PathBuf::from("/w/naming.rs");
    handle_command(&mut view, Command::MarkViewed, SPACE);
    view.stamps.remove(&path);
    assert_eq!(view.viewed(&path), surface::Viewed::Changed);
    view.select_file(0);
    handle_command(&mut view, Command::MarkViewed, SPACE);
    assert!(view.notice.is_some(), "says why it is not marked");
}

#[test]
fn discarding_a_rename_from_its_old_name_says_which_side_goes() {
    let mut view = reviewing();
    let (old, new) = (PathBuf::from("/w/naming.rs"), PathBuf::from("/w/named.rs"));
    view.files[0].status = FileStatus::Deleted;
    view.partners = [(new.clone(), old.clone()), (old.clone(), new)].into();
    ask_to_discard(&mut view);
    let body = super::view(&view, SPACE).confirm.unwrap().body;
    assert!(body.contains("naming.rs returns"), "{body}");
    assert!(body.contains("named.rs goes"), "{body}");
}

/// Put away before its write landed, the review opened again shows the
/// draft it held.
#[test]
fn a_draft_made_just_before_the_review_closed_is_there_when_it_opens_again() {
    use crate::contract::{Checkout, Extension};
    let mut extension = sending_extension();
    extension.counted(PathBuf::from("/w"), 3, 1, 1);
    extension.close(SURFACE);
    let checkout = Checkout {
        root: PathBuf::from("/w"),
        display: "/w".to_owned(),
        target: None,
        agent: None,
    };
    extension.open(SURFACE, &checkout);
    assert_eq!(extension.review_view().unwrap().drafts().len(), 1);
}

/// The list's record arrives while the write of a new draft is still out:
/// the draft stays on screen.
#[test]
fn a_record_read_while_a_write_is_out_keeps_what_it_writes() {
    use crate::contract::Extension;
    let mut extension = sending_extension();
    let host = Keeping(std::sync::Mutex::new(ReviewRecord::default()));
    let mut jobs = extension.jobs();
    let write = jobs.pop().unwrap();
    let answers: Vec<_> = jobs.into_iter().map(|job| (job.run)(&host)).collect();
    for answer in answers {
        extension.absorb(answer);
    }
    assert_eq!(extension.review_view().unwrap().drafts().len(), 1);
    extension.absorb((write.run)(&host));
    assert_eq!(
        extension.review_view().unwrap().drafts().len(),
        1,
        "and once only"
    );
}

/// A draft's `edit` and `delete` are words that act: pressable, so the
/// host lifts them under the pointer, and a press on `delete` deletes.
#[test]
fn a_drafts_actions_are_pressed_as_buttons_are() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    handle_command(&mut view, Command::Type('x'), SPACE);
    handle_command(&mut view, Command::AddComment, SPACE);
    let drawn = render::lines(&view);
    let (line, head) = drawn
        .iter()
        .enumerate()
        .find(|(_, drawn)| matches!(drawn.line, surface::Line::CardHeader(_)))
        .expect("the draft stands under its lines");
    let pressable: Vec<&str> = head
        .content
        .spans
        .iter()
        .filter(|span| span.pressable)
        .map(|span| span.text.as_str())
        .collect();
    assert_eq!(pressable, ["edit", "delete"]);
    let delete = head
        .content
        .spans
        .iter()
        .position(|span| span.text == "delete")
        .unwrap();
    handle_hit(&mut view, ViewHit::Press { line, span: delete }, SPACE);
    assert!(view.drafts().is_empty(), "pressed, it deleted");
}

/// The comment being written is a card: every line of it on the card's
/// ground, room for a few lines before anything is typed, and its own
/// save and discard, which do what ctrl+s and esc do.
#[test]
fn the_comment_being_written_is_a_card_with_room_and_its_own_buttons() {
    use crate::view::LineTone;
    let open = || {
        let mut view = reviewing();
        handle_hit(
            &mut view,
            ViewHit::LineNumber {
                line: 2,
                extend: false,
            },
            SPACE,
        );
        handle_command(&mut view, Command::Comment, SPACE);
        handle_command(&mut view, Command::Type('x'), SPACE);
        view
    };
    let view = open();
    let drawn = render::lines(&view);
    let card: Vec<&render::Drawn> = drawn
        .iter()
        .filter(|drawn| matches!(drawn.content.tone, LineTone::Card(_)))
        .collect();
    let writing = card
        .iter()
        .filter(|drawn| {
            matches!(
                drawn.line,
                surface::Line::Composer(_) | surface::Line::ComposerPad
            )
        })
        .count();
    assert!(writing >= 4, "room for a few lines: {writing}");
    let button = |word: &str| {
        drawn
            .iter()
            .enumerate()
            .find_map(|(line, drawn)| {
                let span = drawn
                    .content
                    .spans
                    .iter()
                    .position(|span| span.text == word)?;
                drawn.content.spans[span]
                    .pressable
                    .then_some(ViewHit::Press { line, span })
            })
            .unwrap_or_else(|| panic!("a {word} button"))
    };
    let mut saved = open();
    handle_hit(&mut saved, button("save"), SPACE);
    assert!(!saved.typing());
    assert_eq!(saved.drafts().len(), 1, "save keeps it as a draft");
    let mut discarded = open();
    handle_hit(&mut discarded, button("discard"), SPACE);
    assert!(!discarded.typing());
    assert!(discarded.drafts().is_empty(), "discard lets it go");
}

/// A right-click on a file's row reaches the review as that row's menu:
/// the file is selected and its actions open on it.
#[test]
fn a_right_click_on_a_row_opens_that_files_menu() {
    let mut view = reviewing();
    handle_hit(&mut view, ViewHit::OpenMenu(1), SPACE);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/tests.rs")));
    let drawn = super::view(&view, SPACE).navigator.unwrap();
    assert_eq!(drawn.menu.map(|menu| menu.row), drawn.anchor, "on its row");
    assert_eq!(menu_entries(&view)[0], "Open file");
}

/// The pointer over a menu entry lights it, as the arrows do and as the
/// code tree's menu does; Enter then takes the entry lit.
#[test]
fn a_menu_entry_lights_under_the_pointer() {
    use crate::contract::{Extension, Outcome};
    let mut extension = sending_extension();
    *extension.review_view_mut().unwrap() = reviewing();
    extension.command(SURFACE, Command::OpenMenu, SPACE);
    assert!(
        extension.hover(SURFACE, Some(ViewHit::MenuEntry(2))),
        "drawn again"
    );
    let lit = super::view(extension.review_view().unwrap(), SPACE)
        .navigator
        .unwrap()
        .menu
        .unwrap()
        .highlighted;
    assert_eq!(lit, 2);
    assert!(
        !extension.hover(SURFACE, Some(ViewHit::MenuEntry(2))),
        "already lit"
    );
    assert_eq!(
        extension.command(SURFACE, Command::Activate, SPACE),
        Outcome::Copy("naming.rs".to_owned())
    );
}

/// A kept comment stands on a card, every line of it, its rule in the hue
/// of what waits on it: a draft the operator's gold, an answer green.
#[test]
fn a_kept_comment_stands_on_a_card_ruled_in_its_states_hue() {
    use crate::view::{LineTone, Role};
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    handle_command(&mut view, Command::Type('x'), SPACE);
    handle_command(&mut view, Command::AddComment, SPACE);
    let tones = |view: &ReviewView| -> Vec<LineTone> {
        render::lines(view)
            .iter()
            .filter(|drawn| {
                matches!(
                    drawn.line,
                    surface::Line::CardHeader(_) | surface::Line::CardBody(_)
                )
            })
            .map(|drawn| drawn.content.tone)
            .collect()
    };
    let draft = tones(&view);
    assert!(
        draft.len() >= 3,
        "its header, its text and its air: {draft:?}"
    );
    assert!(
        draft
            .iter()
            .all(|tone| *tone == LineTone::Card(Role::Warning))
    );
    view.record.comments[0].status = CommentStatus::Answered;
    assert!(
        tones(&view)
            .iter()
            .all(|tone| *tone == LineTone::Card(Role::Success))
    );
}

/// A kept draft on `path` about the row `text` on new line `line`.
fn thread_on(view: &mut ReviewView, id: u64, path: &str, line: u32, text: &str) {
    view.change(change::Change::Draft {
        id,
        path: path.to_owned(),
        rows: vec![Row {
            kind: crate::review::RowKind::Added,
            line,
            text: text.to_owned(),
        }],
        excerpt: String::new(),
        text: format!("thread {id}"),
    });
}

const TESTS: &str = "\
diff --git a/tests.rs b/tests.rs
--- /dev/null
+++ b/tests.rs
@@ -0,0 +1,2 @@
+fn a_test() {
+}
";

/// The open threads are walked as GitLab walks them: a counter beside
/// the viewed chip, a step on and back, across files, round the ends; a
/// thread in another file is shown once that file's diff is read, its
/// lines marked.
#[test]
fn the_threads_are_walked_across_files_from_the_chips_and_the_keys() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "tests.rs", 1, "fn a_test() {");
    thread_on(&mut view, 2, "naming.rs", 3, "    rename();");
    let chips = |view: &ReviewView| -> Vec<String> {
        super::view(view, SPACE)
            .modes
            .iter()
            .map(|mode| mode.label.clone())
            .collect()
    };
    assert_eq!(
        chips(&view)[1],
        "1/2",
        "current of total before any is gone to"
    );
    // naming.rs is listed first: its thread is the first.
    handle_command(&mut view, Command::NextThread, SPACE);
    assert_eq!(view.thread, Some(2));
    assert_eq!(chips(&view)[1], "1/2");
    assert_eq!(
        view.marked(),
        Some((3, 3)),
        "its line marked: {:?}",
        view.marked()
    );
    // On to tests.rs, from the chip: its diff is not read yet.
    handle_hit(&mut view, ViewHit::SelectMode(2), SPACE);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/tests.rs")));
    assert_eq!(view.jump, Some(1), "waiting for the file's diff");
    let tests = PathBuf::from("/w/tests.rs");
    view.absorb_diff(tests.clone(), Ok(diff_of(&tests, TESTS)), "x".to_owned());
    assert_eq!(view.jump, None);
    assert_eq!(view.marked(), Some((0, 0)), "shown once read");
    assert_eq!(chips(&view)[1], "2/2");
    // Round the end, and back again from the keys.
    handle_command(&mut view, Command::NextThread, SPACE);
    assert_eq!(view.thread, Some(2));
    handle_command(&mut view, Command::PreviousThread, SPACE);
    assert_eq!(view.thread, Some(1));
    // The last chip is still the viewed one.
    assert_eq!(chips(&view).last().map(String::as_str), Some("viewed"));
}

#[test]
fn with_no_open_thread_the_walk_is_not_offered() {
    let mut view = reviewing();
    assert_eq!(
        super::view(&view, SPACE).modes.len(),
        1,
        "the viewed chip alone"
    );
    handle_hit(&mut view, ViewHit::SelectMode(0), SPACE);
    assert_eq!(view.viewed(Path::new("/w/naming.rs")), surface::Viewed::Yes);
    handle_command(&mut view, Command::NextThread, SPACE);
    assert!(view.notice.is_some(), "says there is none");
}

/// The node and row of the drawer's tree a press lands on, by what the
/// row's comment says.
fn drawer_tree(extension: &ReviewExtension) -> crate::view::Node {
    use crate::contract::Extension;
    match extension
        .view(super::COMMENTS, SPACE)
        .expect("the drawer")
        .content
    {
        Content::Tree { root, .. } => root,
        other => panic!("a tree: {other:?}"),
    }
}

fn walk<'a>(node: &'a crate::view::Node, out: &mut Vec<&'a crate::view::Node>) {
    out.push(node);
    for child in &node.children {
        walk(child, out);
    }
}

/// The bar's item counts what waits on the operator and names it, in its
/// hue: drafts in gold, then answers in green, then the sent ones waiting
/// on the agent, held back; nothing while nothing is open.
#[test]
fn the_bars_item_counts_what_waits_in_its_hue() {
    use crate::contract::Extension;
    use crate::view::{NodeKind, Role};
    let count = |extension: &ReviewExtension| {
        let view = extension.view(super::ITEM, SPACE)?;
        let Content::Tree { root, .. } = view.content else {
            panic!("a tree")
        };
        let mut nodes = Vec::new();
        walk(&root, &mut nodes);
        nodes.into_iter().find_map(|node| match &node.kind {
            NodeKind::Text { spans, .. } => Some((spans[0].text.clone(), spans[0].role)),
            _ => None,
        })
    };
    let mut extension = sending_extension();
    assert_eq!(
        count(&extension),
        Some(("1 draft".to_owned(), Role::Warning))
    );
    let view = extension.review_view_mut().unwrap();
    view.record.comments[0].status = CommentStatus::Answered;
    assert_eq!(
        count(&extension),
        Some(("1 answered".to_owned(), Role::Success))
    );
    let view = extension.review_view_mut().unwrap();
    view.record.comments[0].status = CommentStatus::Resolved;
    assert_eq!(count(&extension), None, "nothing open, no item");
}

/// The drawer lists the comments by file and sends the drafts from its
/// button, with the review put away: the line is asked of the agent, and
/// once typed the drafts are sent, on the door's record at once.
#[test]
fn the_drawer_sends_the_drafts_with_the_review_put_away() {
    use crate::contract::{AgentWritten, Extension, Outcome};
    use crate::view::{NodeAct, NodeKind};
    let mut extension = sending_extension();
    extension.counted(PathBuf::from("/w"), 3, 1, 1);
    extension.close(SURFACE);
    let root = drawer_tree(&extension);
    let mut nodes = Vec::new();
    walk(&root, &mut nodes);
    assert!(
        nodes.iter().any(|node| matches!(&node.kind, NodeKind::Rule { label: Some(path), .. } if path == "naming.rs")),
        "its file names its group"
    );
    assert!(
        nodes
            .iter()
            .any(|node| matches!(&node.kind, NodeKind::Markdown { text } if text == "check"))
    );
    let send = nodes
        .iter()
        .find(|node| matches!(&node.kind, NodeKind::Button { primary: true, .. }))
        .expect("a send while drafts wait");
    let asked = extension.hit(
        super::COMMENTS,
        ViewHit::Node {
            node: send.id,
            act: NodeAct::Press,
        },
        SPACE,
    );
    assert!(matches!(asked, Outcome::Agent(_)), "{asked:?}");
    extension.agent_write("review-send", AgentWritten::Typed);
    let root = drawer_tree(&extension);
    let mut nodes = Vec::new();
    walk(&root, &mut nodes);
    assert!(
        !nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Button { .. })),
        "sent, no draft is left to send"
    );
}

/// A press on a comment in the drawer opens the review on it: its file
/// selected once the list is read, its thread the one gone to.
#[test]
fn a_comment_pressed_in_the_drawer_opens_the_review_on_it() {
    use crate::contract::{Checkout, Extension, Outcome};
    use crate::view::{NodeAct, NodeKind};
    let mut extension = sending_extension();
    extension.counted(PathBuf::from("/w"), 3, 1, 1);
    extension.close(SURFACE);
    let root = drawer_tree(&extension);
    let mut nodes = Vec::new();
    walk(&root, &mut nodes);
    let list = nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::List { .. }))
        .expect("a file's comments");
    let asked = extension.hit(
        super::COMMENTS,
        ViewHit::Node {
            node: list.id,
            act: NodeAct::Row(0),
        },
        SPACE,
    );
    assert_eq!(asked, Outcome::Show(SURFACE.to_owned()));
    extension.open(
        SURFACE,
        &Checkout {
            root: PathBuf::from("/w"),
            display: "/w".to_owned(),
            target: None,
            agent: None,
        },
    );
    let view = extension.review_view_mut().unwrap();
    let files = reviewing().files;
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files,
            uncommitted: Default::default(),
            partners: Default::default(),
            stamps: Default::default(),
            head: None,
        }),
    );
    assert_eq!(view.thread, Some(1), "on the comment pressed");
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/naming.rs")));
}

/// The comment mark stands where comments are, the lines they are on and
/// their cards' first line, and nowhere else: not on the threads' walk,
/// the list, the affordance under a marked run, or the foot.
#[test]
fn the_comment_mark_stands_only_on_commented_lines_and_their_cards() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 3, "    rename();");
    view.mark = Some((0, 0));
    let drawn = super::view(&view, SPACE);
    let mark = crate::view::COMMENT_MARK.to_string();
    assert!(
        drawn.modes.iter().all(|mode| !mode.label.contains(&mark)),
        "the walk"
    );
    let navigator = drawn.navigator.unwrap();
    assert!(
        navigator
            .rows
            .iter()
            .all(|row| !format!("{row:?}").contains(&mark)),
        "the list"
    );
    assert!(drawn.notice.is_none(), "the foot says nothing of drafts");
    for line in render::lines(&view) {
        let text = line.content.text();
        let allowed = matches!(line.line, surface::Line::CardHeader(_))
            || line.content.gutter.starts_with(&mark);
        assert!(allowed || !text.contains(&mark), "{:?}: {text}", line.line);
    }
}

/// A comment pressed in the drawer on a file the `uncommitted` list does
/// not hold opens the whole branch's list and goes to it there; one whose
/// file no longer differs at all says so; a list that failed keeps the
/// comment waiting.
#[test]
fn a_comment_off_the_list_is_gone_to_on_the_whole_branch_or_said_to_be_gone() {
    let mut view = reviewing();
    thread_on(&mut view, 9, "naming.rs", 3, "    rename();");
    let listed = |view: &mut ReviewView, filter, listing: Result<Listing, String>| {
        view.absorb_listing(filter, Base::default(), "fix/naming".to_owned(), listing);
    };
    let only_tests = || Listing {
        files: vec![ChangedFile {
            status: FileStatus::Added,
            path: PathBuf::from("/w/tests.rs"),
            renamed_from: None,
        }],
        uncommitted: Default::default(),
        partners: Default::default(),
        stamps: Default::default(),
        head: None,
    };
    view.set_filter(Filter::Uncommitted);
    view.open_on = Some(9);
    listed(&mut view, Filter::Uncommitted, Err("git failed".to_owned()));
    assert_eq!(view.open_on, Some(9), "a failed list keeps it waiting");
    listed(&mut view, Filter::Uncommitted, Ok(only_tests()));
    assert_eq!(view.filter, Filter::All, "the whole branch is read for it");
    assert_eq!(view.open_on, Some(9));
    let files = reviewing().files;
    listed(
        &mut view,
        Filter::All,
        Ok(Listing {
            files,
            ..only_tests()
        }),
    );
    assert_eq!(view.thread, Some(9), "and gone to there");
    view.thread = None;
    view.open_on = Some(9);
    listed(&mut view, Filter::All, Ok(only_tests()));
    assert!(
        view.notice.is_some(),
        "a file no longer changed is said, not skipped"
    );
}

/// While a comment is written, the threads' walk waits, as the list does:
/// nothing scrolls the comment away from under the keys.
#[test]
fn the_threads_walk_waits_while_a_comment_is_written() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 3, "    rename();");
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 0,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    let (mark, scroll) = (view.mark, view.scroll);
    handle_hit(&mut view, ViewHit::SelectMode(2), SPACE);
    handle_command(&mut view, Command::NextThread, SPACE);
    assert!(view.typing());
    assert_eq!((view.mark, view.scroll, view.thread), (mark, scroll, None));
    assert!(view.notice.is_some(), "and says why");
}

/// What a click away from a comment being written says names the
/// buttons the comment carries, save and discard.
#[test]
fn a_comment_being_written_names_its_own_buttons_when_it_holds_its_place() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    handle_hit(&mut view, ViewHit::SelectItem(1), SPACE);
    let said = view
        .notice
        .as_ref()
        .map(|span| span.text.clone())
        .unwrap_or_default();
    assert!(said.contains("save") && said.contains("discard"), "{said}");
}

/// A comment pressed while the list is not read yet, or failed, waits for
/// a list that is: nothing is said of a file no list has spoken about.
#[test]
fn a_comment_pressed_before_the_list_is_read_waits_for_it() {
    let mut view = reviewing();
    thread_on(&mut view, 9, "elsewhere.rs", 1, "x");
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Err("git failed".to_owned()),
    );
    view.go_to_thread_id(9);
    assert_eq!(view.open_on, Some(9));
    assert!(
        view.notice.is_none(),
        "nothing said of a list that was not read"
    );
}

/// `reviewing()` with its diff shown long enough ago to have been read.
fn read_for_a_while() -> ReviewView {
    let mut view = reviewing();
    view.reading.shown_at = Some(std::time::Instant::now() - surface::READ_FOR);
    view
}

#[test]
fn a_diff_read_to_its_end_is_marked_viewed_as_it_is_left() {
    let mut view = read_for_a_while();
    let naming = PathBuf::from("/w/naming.rs");
    view.note_seen(SPACE);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/tests.rs")));
    assert_eq!(view.viewed(&naming), surface::Viewed::Yes);
    assert_eq!(view.viewed_count(), 1);
    assert!(
        view.unwritten
            .iter()
            .any(|change| matches!(change, super::change::Change::Viewed { .. })),
        "kept to be written"
    );
}

#[test]
fn a_diff_only_glanced_at_or_not_read_to_its_end_stays_not_viewed() {
    let naming = PathBuf::from("/w/naming.rs");
    let mut glanced = reviewing();
    glanced.note_seen(SPACE);
    glanced.step_file(ScrollDirection::Down);
    assert_eq!(
        glanced.viewed(&naming),
        surface::Viewed::No,
        "shown too briefly"
    );

    let mut short_of_the_end = read_for_a_while();
    short_of_the_end.note_seen(Size {
        width: 100,
        height: 2,
    });
    short_of_the_end.step_file(ScrollDirection::Down);
    assert_eq!(
        short_of_the_end.viewed(&naming),
        surface::Viewed::No,
        "its last line was never on screen"
    );
}

#[test]
fn turned_off_a_file_is_marked_only_by_hand() {
    let mut view = read_for_a_while();
    view.marks_by_hand_only = true;
    view.note_seen(SPACE);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(Path::new("/w/naming.rs")), surface::Viewed::No);
}

#[test]
fn a_file_unmarked_by_hand_is_not_marked_again_until_it_changes() {
    let mut view = read_for_a_while();
    let naming = PathBuf::from("/w/naming.rs");
    view.toggle_viewed(false);
    view.toggle_viewed(false);
    assert_eq!(
        view.viewed(&naming),
        surface::Viewed::No,
        "unmarked by hand"
    );
    view.note_seen(SPACE);
    view.step_file(ScrollDirection::Down);
    assert_eq!(
        view.viewed(&naming),
        surface::Viewed::No,
        "the unmark holds"
    );

    // The agent changes it, and it is read again to its end.
    relisted(&mut view, 2);
    view.step_file(ScrollDirection::Up);
    view.asking = view.diff_key(&naming).map(|key| (naming.clone(), key));
    view.absorb_diff(
        naming.clone(),
        Ok(diff_of(&naming, &NAMING.replace("rename", "renamed"))),
        "x".to_owned(),
    );
    view.reading.shown_at = Some(std::time::Instant::now() - surface::READ_FOR);
    view.note_seen(SPACE);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(&naming), surface::Viewed::Yes);
}

#[test]
fn a_file_that_moved_since_its_diff_was_read_is_not_marked_on_the_way_out() {
    let mut view = read_for_a_while();
    let naming = PathBuf::from("/w/naming.rs");
    view.note_seen(SPACE);
    // The agent changes it; its new diff is still being read.
    relisted(&mut view, 2);
    assert!(view.diff_pending);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(&naming), surface::Viewed::No);
}

#[test]
fn a_file_put_on_show_by_the_list_is_not_marked_before_it_was_read() {
    let mut view = read_for_a_while();
    view.note_seen(SPACE);
    // The list drops naming.rs: tests.rs is put on show, its diff unread.
    let root = PathBuf::from("/w");
    let tests = root.join("tests.rs");
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files: vec![
                ChangedFile {
                    status: FileStatus::Added,
                    path: tests.clone(),
                    renamed_from: None,
                },
                ChangedFile {
                    status: FileStatus::Added,
                    path: root.join("other.rs"),
                    renamed_from: None,
                },
            ],
            uncommitted: Default::default(),
            partners: Default::default(),
            stamps: [(tests.clone(), 1), (root.join("other.rs"), 1)].into(),
            head: None,
        }),
    );
    assert_eq!(view.selected.as_deref(), Some(tests.as_path()));
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(&tests), surface::Viewed::No);
}

#[test]
fn the_end_is_seen_only_once_the_last_line_is_inside_the_text_rows() {
    let mut view = reviewing();
    let total = render::lines(&view).len();
    let as_tall = |height: usize| Size {
        width: 100,
        height: u16::try_from(height).unwrap(),
    };
    // As many rows as lines, but the heading and its padding take three.
    view.note_seen(as_tall(total));
    assert!(!view.reading.end_seen, "the last lines were below the fold");
    let chrome = total - usize::from(crate::view::text_height(as_tall(total)));
    view.note_seen(as_tall(total + chrome));
    assert!(view.reading.end_seen);
}

/// The review open on `/w` as `reviewing()` leaves it, read for a while.
fn open_and_read() -> ReviewExtension {
    use crate::contract::{Checkout, Extension};
    let mut extension = ReviewExtension::default();
    extension.open(
        SURFACE,
        &Checkout {
            root: PathBuf::from("/w"),
            display: "/w".to_owned(),
            target: None,
            agent: None,
        },
    );
    *extension.review_view_mut().unwrap() = read_for_a_while();
    extension
}

fn reopen(extension: &mut ReviewExtension) {
    use crate::contract::{Checkout, Extension};
    extension.close(SURFACE);
    extension.open(
        SURFACE,
        &Checkout {
            root: PathBuf::from("/w"),
            display: "/w".to_owned(),
            target: None,
            agent: None,
        },
    );
}

#[test]
fn closing_the_review_marks_the_file_read_on_show() {
    let mut extension = open_and_read();
    extension.review_view_mut().unwrap().note_seen(SPACE);
    reopen(&mut extension);
    let view = extension.review_view().unwrap();
    assert_eq!(
        view.record.viewed.iter().collect::<Vec<_>>(),
        vec![(&"naming.rs".to_owned(), &1)],
        "the file on show, at the content read"
    );
}

#[test]
fn an_unmark_by_hand_outlives_the_review_closing() {
    let mut extension = open_and_read();
    let naming = PathBuf::from("/w/naming.rs");
    let view = extension.review_view_mut().unwrap();
    view.toggle_viewed(false);
    view.toggle_viewed(false);
    reopen(&mut extension);
    let view = extension.review_view_mut().unwrap();
    *view = ReviewView {
        record: view.record.clone(),
        unmarked: std::mem::take(&mut view.unmarked),
        ..read_for_a_while()
    };
    view.note_seen(SPACE);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(&naming), surface::Viewed::No);
}

#[test]
fn a_diff_read_for_older_content_does_not_mark_the_newer_one() {
    let mut view = reviewing();
    let naming = PathBuf::from("/w/naming.rs");
    // The diff of the content stamped 1 is asked; the file moves to 2
    // while the read is out, and the old diff lands after.
    let asked = view.diff_key(&naming).unwrap();
    relisted(&mut view, 2);
    view.asking = Some((naming.clone(), asked));
    view.absorb_diff(
        naming.clone(),
        Ok(diff_of(&naming, &NAMING.replace("rename", "renamed"))),
        "x".to_owned(),
    );
    view.reading.shown_at = Some(std::time::Instant::now() - surface::READ_FOR);
    view.note_seen(SPACE);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(&naming), surface::Viewed::No);
}

#[test]
fn a_file_read_while_only_the_commit_moved_is_still_marked() {
    let mut view = read_for_a_while();
    let naming = PathBuf::from("/w/naming.rs");
    view.note_seen(SPACE);
    // The agent commits another file: the diff is read again, the
    // content on show is the same.
    let files = view.files.clone();
    let stamps = view.stamps.clone();
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing {
            files,
            uncommitted: Default::default(),
            partners: Default::default(),
            stamps,
            head: Some("next".to_owned()),
        }),
    );
    assert!(view.diff_pending);
    view.step_file(ScrollDirection::Down);
    assert_eq!(view.viewed(&naming), surface::Viewed::Yes);
}

fn scrolled_to_an_answered_comment(space: Size) -> ReviewView {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        space,
    );
    handle_command(&mut view, Command::Comment, space);
    handle_command(&mut view, Command::Type('x'), space);
    handle_command(&mut view, Command::AddComment, space);
    view.record.comments[0].status = CommentStatus::Answered;
    view.scroll = render::lines(&view).len() - 1;
    view
}

/// A step to a thread scrolls to it, so a short file's whole diff can stand
/// above the screen. The comment going must bring it back.
#[test]
fn resolving_the_comment_on_show_leaves_the_diff_on_screen() {
    let space = Size {
        width: 100,
        height: 8,
    };
    let mut view = scrolled_to_an_answered_comment(space);
    handle_command(&mut view, Command::Resolve, space);
    let total = render::lines(&view).len();
    let visible = usize::from(crate::view::text_height(space));
    assert_eq!(view.scroll, total.saturating_sub(visible));
    assert!(line_texts(&view).iter().any(|line| !line.trim().is_empty()));
}

#[test]
fn deleting_the_comment_on_show_leaves_the_diff_on_screen() {
    let space = Size {
        width: 100,
        height: 8,
    };
    let mut view = scrolled_to_an_answered_comment(space);
    view.record.comments[0].status = CommentStatus::Draft;
    let id = view.record.comments[0].id;
    perform(&mut view, Act::Delete(id), space);
    let total = render::lines(&view).len();
    let visible = usize::from(crate::view::text_height(space));
    assert_eq!(view.scroll, total.saturating_sub(visible));
}

/// Taking a comment away never moves a diff that still fills the screen.
#[test]
fn resolving_a_comment_keeps_the_scroll_while_lines_fill_the_screen() {
    let space = Size {
        width: 100,
        height: 4,
    };
    let mut view = scrolled_to_an_answered_comment(space);
    view.scroll = 0;
    handle_command(&mut view, Command::Resolve, space);
    assert_eq!(view.scroll, 0);
}

/// `r` with nothing to resolve takes no line away, so it moves nothing.
#[test]
fn resolving_nothing_keeps_the_scroll() {
    let space = Size {
        width: 100,
        height: 8,
    };
    let mut view = reviewing();
    view.scroll = render::lines(&view).len() - 1;
    let scroll = view.scroll;
    handle_command(&mut view, Command::Resolve, space);
    assert_eq!(view.scroll, scroll);
}

fn scrolled_to_a_composer(space: Size) -> ReviewView {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        space,
    );
    handle_command(&mut view, Command::Comment, space);
    view.scroll = render::lines(&view).len() - 1;
    view
}

/// A composer put away takes as many lines with it as a comment does.
#[test]
fn closing_the_composer_leaves_the_diff_on_screen() {
    let space = Size {
        width: 100,
        height: 8,
    };
    let mut view = scrolled_to_a_composer(space);
    handle_command(&mut view, Command::Close, space);
    let total = render::lines(&view).len();
    assert_eq!(
        view.scroll,
        total.saturating_sub(usize::from(crate::view::text_height(space)))
    );
}

#[test]
fn discarding_the_composer_leaves_the_diff_on_screen() {
    let space = Size {
        width: 100,
        height: 8,
    };
    let mut view = scrolled_to_a_composer(space);
    perform(&mut view, Act::Discard, space);
    let total = render::lines(&view).len();
    assert_eq!(
        view.scroll,
        total.saturating_sub(usize::from(crate::view::text_height(space)))
    );
}

/// A press arrives in cells and the composer's caret counts characters: a
/// wide glyph is two of one and one of the other.
#[test]
fn a_press_in_the_composer_past_wide_glyphs_lands_on_its_character() {
    let mut view = reviewing();
    handle_hit(
        &mut view,
        ViewHit::LineNumber {
            line: 2,
            extend: false,
        },
        SPACE,
    );
    handle_command(&mut view, Command::Comment, SPACE);
    for character in "日本語 ok".chars() {
        handle_command(&mut view, Command::Type(character), SPACE);
    }
    let line = render::lines(&view)
        .iter()
        .position(|drawn| drawn.line == surface::Line::Composer(0))
        .expect("the composer is drawn");
    let cell = render::RULE.chars().count()
        + "日本語 "
            .chars()
            .map(|c| if c == ' ' { 1 } else { 2 })
            .sum::<usize>();
    handle_hit(&mut view, ViewHit::PlaceCaret { line, cell }, SPACE);
    assert_eq!(view.composer.as_ref().unwrap().caret.column, 4, "on the o");
}

/// The operator's own preference: a file read to its end is marked viewed
/// unless they turn it off, so it is declared on by default.
#[test]
fn the_review_declares_marking_viewed_once_read_on_by_default() {
    let declared = preferences();
    assert_eq!(declared.len(), 1);
    assert_eq!(declared[0].name, MARK_VIEWED_AUTOMATICALLY);
    assert_eq!(declared[0].kind, uze_contract::PreferenceKind::Toggle);
    assert_eq!(declared[0].default, serde_json::Value::from(true));
}

fn caret(view: &ReviewView) -> (usize, usize) {
    let caret = view.composer.as_ref().expect("a composer").caret;
    (caret.line, caret.column)
}

fn composer_lines(view: &ReviewView) -> Vec<String> {
    view.composer.as_ref().expect("a composer").lines.clone()
}

/// While a comment is written the keys edit it, as a text field's do.
#[test]
fn the_comment_being_written_takes_the_editing_keys() {
    let mut view = reviewing();
    view.mark_cell(2, false);
    handle_command(&mut view, Command::Comment, SPACE);
    for character in "ab".chars() {
        handle_command(&mut view, Command::Type(character), SPACE);
    }
    handle_command(&mut view, Command::Newline, SPACE);
    assert_eq!(caret(&view), (1, 0));
    for character in "cd".chars() {
        handle_command(&mut view, Command::Type(character), SPACE);
    }
    assert_eq!(composer_lines(&view), ["ab", "cd"]);
    handle_command(&mut view, Command::SelectPrevious, SPACE);
    assert_eq!(caret(&view), (0, 2), "up a line");
    handle_command(&mut view, Command::CaretLineStart, SPACE);
    assert_eq!(caret(&view), (0, 0));
    handle_command(&mut view, Command::CaretRight, SPACE);
    assert_eq!(caret(&view), (0, 1));
    handle_command(&mut view, Command::CaretLineEnd, SPACE);
    assert_eq!(caret(&view), (0, 2));
    handle_command(&mut view, Command::CaretLeft, SPACE);
    assert_eq!(caret(&view), (0, 1));
    handle_command(&mut view, Command::EraseForward, SPACE);
    assert_eq!(composer_lines(&view), ["a", "cd"]);
    handle_command(&mut view, Command::EraseBack, SPACE);
    assert_eq!(composer_lines(&view), ["", "cd"]);
    handle_command(&mut view, Command::SelectNext, SPACE);
    assert_eq!(caret(&view).0, 1, "down a line");
    assert!(view.typing(), "no key put the composer away");
}

/// The discard question starts on its safe answer; the keys that move
/// between answers swap it, and enter gives the one standing.
#[test]
fn the_discard_question_moves_between_its_answers_and_enter_gives_one() {
    let mut view = reviewing();
    ask_to_discard(&mut view);
    let on_confirm = |view: &ReviewView| view.discarding.as_ref().unwrap().on_confirm;
    assert!(!on_confirm(&view));
    handle_command(&mut view, Command::FocusNext, SPACE);
    assert!(on_confirm(&view));
    handle_command(&mut view, Command::Collapse, SPACE);
    assert!(!on_confirm(&view));
    handle_command(&mut view, Command::Expand, SPACE);
    assert!(on_confirm(&view));
    handle_command(&mut view, Command::Activate, SPACE);
    assert!(view.discarding.is_none(), "answered");
    assert_eq!(
        view.restore,
        Some(vec![PathBuf::from("/w").join("naming.rs")])
    );
}

#[test]
fn the_files_menu_highlight_steps_back_and_stays_open() {
    let mut view = reviewing();
    handle_command(&mut view, Command::OpenMenu, SPACE);
    handle_command(&mut view, Command::SelectNext, SPACE);
    handle_command(&mut view, Command::SelectNext, SPACE);
    handle_command(&mut view, Command::SelectPrevious, SPACE);
    assert_eq!(view.menu.as_ref().map(|menu| menu.highlighted), Some(1));
}

/// Hovering past the menu's last entry lights nothing.
#[test]
fn the_pointer_past_the_menus_last_entry_lights_nothing() {
    let mut view = reviewing();
    handle_command(&mut view, Command::OpenMenu, SPACE);
    let entries = view.menu.as_ref().unwrap().acts.len();
    assert!(!hover(&mut view, Some(ViewHit::MenuEntry(entries))));
    assert_eq!(view.menu.as_ref().unwrap().highlighted, 0);
}

/// Esc lets go of a mark first, and only with nothing marked closes the
/// review.
#[test]
fn esc_lets_go_of_the_mark_before_it_closes_the_review() {
    let mut view = reviewing();
    selected(&mut view, 2..4, "fn apply() {");
    assert!(view.marked().is_some());
    assert!(matches!(
        handle_command(&mut view, Command::Close, SPACE),
        Asked::Stay
    ));
    assert_eq!(view.marked(), None);
    assert!(view.excerpt.is_empty(), "the quote goes with the mark");
    assert!(matches!(
        handle_command(&mut view, Command::Close, SPACE),
        Asked::Close
    ));
}

/// The arrows walk the list where the list has the focus and the diff's
/// lines where the diff has it; tab hands the focus over and enter on
/// the list hands it to the diff, where enter starts a comment.
#[test]
fn the_arrows_walk_whichever_side_has_the_focus() {
    let mut view = reviewing();
    view.focus = Focus::Content;
    view.cursor = 0;
    handle_command(&mut view, Command::SelectNext, SPACE);
    handle_command(&mut view, Command::SelectNext, SPACE);
    assert_eq!(view.cursor, 2);
    handle_command(&mut view, Command::SelectPrevious, SPACE);
    assert_eq!(view.cursor, 1);
    handle_command(&mut view, Command::FocusNext, SPACE);
    assert_eq!(view.focus, Focus::Navigator);
    handle_command(&mut view, Command::SelectNext, SPACE);
    assert_eq!(view.selected_index(), Some(1), "the next file");
    handle_command(&mut view, Command::FocusNext, SPACE);
    assert_eq!(view.focus, Focus::Content);
    handle_command(&mut view, Command::FocusNext, SPACE);
    handle_command(&mut view, Command::Activate, SPACE);
    assert_eq!(view.focus, Focus::Content);
    assert!(!view.typing(), "enter on the list only crosses over");
}

#[test]
fn enter_on_the_diff_starts_a_comment_on_the_cursors_line() {
    let mut view = reviewing();
    view.focus = Focus::Content;
    view.cursor = 2;
    handle_command(&mut view, Command::Activate, SPACE);
    assert!(view.typing());
}

/// A space four rows tall shows one text row.
const ONE_ROW: Size = Size {
    width: 100,
    height: 4,
};

/// Paging moves the diff by the space's height and stops on its last
/// line; the wheel moves it by three, and not down past the end.
#[test]
fn paging_and_the_wheel_move_the_diff_within_its_lines() {
    let mut view = reviewing();
    let last = render::lines(&view).len() - 1;
    assert!(last >= 5, "a diff long enough to scroll");
    let space = Size {
        width: 100,
        height: 2,
    };
    handle_command(&mut view, Command::ScrollPageDown, space);
    assert_eq!(view.scroll, 2);
    handle_command(&mut view, Command::ScrollPageUp, space);
    assert_eq!(view.scroll, 0);
    for _ in 0..last {
        handle_command(&mut view, Command::ScrollPageDown, space);
    }
    assert_eq!(view.scroll, last, "never past the last line");
    view.scroll = 0;
    handle_scroll(&mut view, ScrollDirection::Down, false);
    assert_eq!(view.scroll, 3);
    handle_scroll(&mut view, ScrollDirection::Down, true);
    assert_eq!(view.scroll, 3, "at the end the wheel down stays");
    handle_scroll(&mut view, ScrollDirection::Up, false);
    assert_eq!(view.scroll, 0);
    for _ in 0..last {
        handle_scroll(&mut view, ScrollDirection::Down, false);
    }
    assert_eq!(view.scroll, last);
}

fn line_of(view: &ReviewView, wanted: Line) -> usize {
    render::lines(view)
        .iter()
        .position(|drawn| drawn.line == wanted)
        .expect("drawn")
}

/// The cursor moved off the screen brings the diff with it.
#[test]
fn the_diff_follows_the_cursor_off_the_screen() {
    let mut view = reviewing();
    view.focus = Focus::Content;
    view.cursor = 0;
    for _ in 0..5 {
        handle_command(&mut view, Command::SelectNext, ONE_ROW);
    }
    assert_eq!(view.cursor, 5);
    assert_eq!(view.scroll, line_of(&view, Line::Cell(5)));
    handle_command(&mut view, Command::ExtendMark(ScrollDirection::Up), ONE_ROW);
    assert_eq!(view.scroll, line_of(&view, Line::Cell(4)));
}

/// The line being written stays on the screen as the comment grows.
#[test]
fn the_diff_follows_the_line_being_written() {
    let mut view = reviewing();
    view.mark_cell(1, false);
    handle_command(&mut view, Command::Comment, ONE_ROW);
    for _ in 0..6 {
        handle_command(&mut view, Command::Newline, ONE_ROW);
    }
    assert_eq!(view.scroll, line_of(&view, Line::Composer(6)));
}

/// `r` puts away the answered comment at or below the cursor before one
/// above it.
#[test]
fn resolve_takes_the_answered_comment_at_or_below_the_cursor() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 2, "fn apply() {");
    thread_on(&mut view, 2, "naming.rs", 4, "}");
    for comment in &mut view.record.comments {
        comment.status = CommentStatus::Answered;
    }
    view.cursor = 3;
    handle_command(&mut view, Command::Resolve, SPACE);
    let status = |view: &ReviewView, id: u64| {
        view.record
            .comments
            .iter()
            .find(|comment| comment.id == id)
            .unwrap()
            .status
    };
    assert_eq!(status(&view, 2), CommentStatus::Resolved);
    assert_eq!(status(&view, 1), CommentStatus::Answered);
}

/// A comment taken away while the diff stands scrolled to its very end
/// brings the last line back onto the one row on show.
#[test]
fn a_comment_taken_away_at_the_very_end_brings_the_last_line_back() {
    let mut measured = scrolled_to_an_answered_comment(ONE_ROW);
    handle_command(&mut measured, Command::Resolve, ONE_ROW);
    let total = render::lines(&measured).len();
    let mut view = scrolled_to_an_answered_comment(ONE_ROW);
    view.scroll = total;
    handle_command(&mut view, Command::Resolve, ONE_ROW);
    assert_eq!(view.scroll, total - 1);
}

#[test]
fn a_click_on_the_close_or_on_a_file_answers_it() {
    let mut view = reviewing();
    view.focus = Focus::Content;
    handle_hit(&mut view, ViewHit::SelectItem(1), SPACE);
    assert_eq!(view.selected_index(), Some(1));
    assert_eq!(view.focus, Focus::Navigator);
    assert!(matches!(
        handle_hit(&mut view, ViewHit::Close, SPACE),
        Asked::Close
    ));
}

/// A click on a comment's button does what the button says, wherever on
/// it the click lands.
#[test]
fn a_click_placed_on_a_drafts_button_does_what_it_says() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 2, "fn apply() {");
    let drawn = render::lines(&view);
    let (line, delete) = drawn
        .iter()
        .enumerate()
        .find_map(|(line, drawn)| {
            drawn
                .acts
                .iter()
                .find(|(_, act)| *act == Act::Delete(1))
                .map(|(range, _)| (line, range.start))
        })
        .expect("the draft offers its delete");
    handle_hit(&mut view, ViewHit::PlaceCaret { line, cell: delete }, SPACE);
    assert!(view.drafts().is_empty());
}

/// A click on the composer's own room puts the caret at the end of what
/// is written.
#[test]
fn a_click_on_the_composers_room_puts_the_caret_at_the_end() {
    let mut view = reviewing();
    view.mark_cell(2, false);
    handle_command(&mut view, Command::Comment, SPACE);
    for command in [
        Command::Type('a'),
        Command::Newline,
        Command::Type('b'),
        Command::Type('c'),
        Command::SelectPrevious,
    ] {
        handle_command(&mut view, command, SPACE);
    }
    let pad = render::lines(&view)
        .iter()
        .position(|drawn| drawn.line == Line::ComposerPad && drawn.acts.is_empty())
        .expect("the composer has room");
    handle_hit(&mut view, ViewHit::PlaceCaret { line: pad, cell: 0 }, SPACE);
    assert_eq!(caret(&view), (1, 2));
}

/// A click on a diff line moves the cursor there and lets go of the mark;
/// while a comment is written the cursor stays where the comment is.
#[test]
fn a_click_on_a_diff_line_moves_the_cursor_unless_a_comment_is_written() {
    let mut view = reviewing();
    view.mark_cell(1, false);
    view.focus = Focus::Navigator;
    let line = line_of(&view, Line::Cell(4));
    handle_hit(&mut view, ViewHit::PlaceCaret { line, cell: 40 }, SPACE);
    assert_eq!(view.cursor, 4);
    assert_eq!(view.mark, None);
    assert_eq!(view.focus, Focus::Content);

    view.mark_cell(1, false);
    handle_command(&mut view, Command::Comment, SPACE);
    let line = line_of(&view, Line::Cell(4));
    handle_hit(&mut view, ViewHit::PlaceCaret { line, cell: 40 }, SPACE);
    assert_eq!(view.cursor, 1);
}

/// Copying a selection gives the text of the lines it covered.
#[test]
fn a_copied_selection_is_the_text_of_its_lines() {
    let view = reviewing();
    let all = line_texts(&view);
    assert_eq!(text(&view, 1..3), all[1..3].to_vec());
    assert!(
        text(&view, 1..3)
            .iter()
            .any(|line| line.contains("fn old()"))
    );
}

/// A selection's quote is kept whole up to 160 characters and cut with an
/// ellipsis past them.
#[test]
fn a_selections_quote_is_cut_only_past_160_characters() {
    let mut view = reviewing();
    let whole = "a".repeat(160);
    selected(&mut view, 2..3, &whole);
    assert_eq!(view.excerpt, whole);
    let long = "b".repeat(200);
    selected(&mut view, 2..3, &long);
    assert_eq!(view.excerpt, format!("{}…", "b".repeat(160)));
}

// --- What the surface says (render.rs) ---------------------------------

fn spoken(drawn: &render::Drawn) -> String {
    drawn
        .content
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect()
}

/// What the drawn line standing for `wanted` says.
fn said_on(view: &ReviewView, wanted: Line) -> String {
    let drawn = render::lines(view);
    spoken(&drawn[line_of(view, wanted)])
}

/// A list that holds nothing, or could not be read, is no list.
#[test]
fn a_review_with_nothing_listed_shows_no_list() {
    let mut view = ReviewView::opening(PathBuf::from("/w"), "/w".to_owned());
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Ok(Listing::default()),
    );
    assert!(super::view(&view, SPACE).navigator.is_none());
    view.absorb_listing(
        Filter::All,
        Base::default(),
        "fix/naming".to_owned(),
        Err("unreadable".to_owned()),
    );
    assert!(super::view(&view, SPACE).navigator.is_none());
}

/// The list says whether it has the keys, which file is on show, and
/// down its leading column which files were viewed.
#[test]
fn the_list_says_its_focus_its_selection_and_what_was_viewed() {
    use crate::view::{NavigatorRow, RowIcon};
    let mut view = reviewing();
    view.toggle_viewed(false);
    let navigator = super::view(&view, SPACE).navigator.expect("a list");
    assert!(navigator.focused);
    let rows: Vec<(bool, RowIcon)> = navigator
        .rows
        .iter()
        .map(|row| match row {
            NavigatorRow::Item { selected, icon, .. } => (*selected, *icon),
            other => panic!("a file's row: {other:?}"),
        })
        .collect();
    assert_eq!(rows, [(true, RowIcon::Viewed), (false, RowIcon::NotViewed)]);
    view.focus = Focus::Content;
    assert!(!super::view(&view, SPACE).navigator.unwrap().focused);
}

#[test]
fn the_filter_on_show_is_the_lit_chip() {
    let mut view = reviewing();
    let lit = |view: &ReviewView| -> Vec<String> {
        super::view(view, SPACE)
            .subjects
            .iter()
            .filter(|chip| chip.active)
            .map(|chip| chip.label.clone())
            .collect()
    };
    assert_eq!(lit(&view), ["all"]);
    view.set_filter(Filter::Uncommitted);
    assert_eq!(lit(&view), ["uncommitted"]);
}

/// The walk's first chip steps back: from no thread gone to, to the last.
#[test]
fn the_walks_first_chip_steps_back_among_the_threads() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 2, "fn apply() {");
    thread_on(&mut view, 2, "naming.rs", 3, "    rename();");
    handle_hit(&mut view, ViewHit::SelectMode(0), SPACE);
    assert_eq!(view.thread, Some(2));
    assert_eq!(view.viewed(Path::new("/w/naming.rs")), surface::Viewed::No);
}

/// The diff's heading names the file, and says when what is shown is
/// only what is not committed.
#[test]
fn the_heading_says_when_only_the_uncommitted_is_shown() {
    let mut view = reviewing();
    let heading = |view: &ReviewView| match super::view(view, SPACE).content {
        Content::Lines { heading, .. } => heading,
        other => panic!("lines: {other:?}"),
    };
    assert_eq!(heading(&view), "DIFF · naming.rs · uncommitted");
    view.base.commit = Some("f00d".to_owned());
    assert_eq!(heading(&view), "DIFF · naming.rs");
    view.filter = Filter::Uncommitted;
    assert_eq!(heading(&view), "DIFF · naming.rs · uncommitted");
}

/// The cursor's line is lit only while the diff has the keys and nothing
/// is marked; a mark lights its own lines instead.
#[test]
fn the_cursors_line_is_lit_only_with_the_keys_and_no_mark() {
    let mut view = reviewing();
    let lit = |view: &ReviewView| -> Vec<Line> {
        render::lines(view)
            .iter()
            .filter(|drawn| drawn.content.marked)
            .map(|drawn| drawn.line)
            .collect()
    };
    view.cursor = 2;
    view.focus = Focus::Navigator;
    assert_eq!(lit(&view), []);
    view.focus = Focus::Content;
    assert_eq!(lit(&view), [Line::Cell(2)]);
    view.mark = Some((4, 4));
    assert_eq!(lit(&view), [Line::Cell(4)]);
}

/// With nothing marked, the comment is written under the cursor's line,
/// and only there; the caret stands where the next character lands.
#[test]
fn the_comment_is_written_under_the_cursors_line_with_the_caret_in_it() {
    let mut view = reviewing();
    view.focus = Focus::Content;
    view.cursor = 2;
    handle_command(&mut view, Command::Comment, SPACE);
    for character in "ab".chars() {
        handle_command(&mut view, Command::Type(character), SPACE);
    }
    let drawn = render::lines(&view);
    let headers: Vec<usize> = drawn
        .iter()
        .enumerate()
        .filter(|(_, drawn)| drawn.line == Line::ComposerHeader)
        .map(|(at, _)| at)
        .collect();
    assert_eq!(headers.len(), 1, "one comment is being written");
    assert!(line_of(&view, Line::Cell(2)) < headers[0]);
    assert!(headers[0] < line_of(&view, Line::Cell(3)));
    let Content::Lines { caret, .. } = super::view(&view, SPACE).content else {
        panic!("lines")
    };
    assert_eq!(
        caret,
        Some(crate::view::Caret {
            line: line_of(&view, Line::Composer(0)),
            column: 2 + render::RULE.chars().count(),
        })
    );
}

/// A comment, kept or being written, names the line it is on, or the
/// first and last of its run.
#[test]
fn a_comment_names_its_line_or_its_run() {
    let mut view = reviewing();
    view.change(change::Change::Draft {
        id: 1,
        path: "naming.rs".to_owned(),
        rows: [(2, "fn apply() {"), (3, "    rename();"), (4, "}")]
            .into_iter()
            .map(|(line, text)| Row {
                kind: crate::review::RowKind::Added,
                line,
                text: text.to_owned(),
            })
            .collect(),
        excerpt: String::new(),
        text: "a run".to_owned(),
    });
    thread_on(&mut view, 2, "naming.rs", 5, "fn branch() {}");
    view.record.comments[1].rows[0].kind = crate::review::RowKind::Context;
    assert!(said_on(&view, Line::CardHeader(1)).contains(" L2–4 · "));
    assert!(said_on(&view, Line::CardHeader(2)).contains(" L5 · "));

    view.mark = Some((2, 4));
    handle_command(&mut view, Command::Comment, SPACE);
    assert!(said_on(&view, Line::ComposerHeader).contains(" L2–4 · new comment"));
    handle_command(&mut view, Command::Close, SPACE);
    view.mark = Some((2, 2));
    handle_command(&mut view, Command::Comment, SPACE);
    assert!(said_on(&view, Line::ComposerHeader).contains(" L2 · new comment"));
}

/// An answered comment offers its resolve on its card; the agent's
/// answer is set apart from the operator's words, its code still code.
#[test]
fn an_answered_comment_offers_its_resolve_and_sets_its_answer_apart() {
    use crate::view::Role;
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 2, "fn apply() {");
    let comment = &mut view.record.comments[0];
    comment.status = CommentStatus::Answered;
    comment.sent = Some((1, 1));
    comment.answers = vec!["renamed with `refreshed`".to_owned()];
    view.recolour();
    let drawn = render::lines(&view);
    assert!(
        drawn
            .iter()
            .any(|drawn| drawn.acts.iter().any(|(_, act)| *act == Act::Resolve(1))),
        "a resolve to press"
    );
    let spans: Vec<(String, Role)> = drawn
        .iter()
        .flat_map(|drawn| &drawn.content.spans)
        .filter(|span| span.text == "renamed with " || span.text == "refreshed")
        .map(|span| (span.text.clone(), span.role))
        .collect();
    assert_eq!(
        spans,
        [
            ("renamed with ".to_owned(), Role::Secondary),
            ("refreshed".to_owned(), Role::Info)
        ]
    );
}

/// The comment being written quotes the selection it is on, when there is
/// one, and offers a placeholder only while nothing is written.
#[test]
fn the_comment_being_written_quotes_its_selection_and_offers_a_placeholder() {
    let placeholder = "what should change here, and why";
    let mut view = reviewing();
    view.mark_cell(2, false);
    handle_command(&mut view, Command::Comment, SPACE);
    let quotes = |view: &ReviewView| {
        render::lines(view)
            .iter()
            .any(|drawn| spoken(drawn).contains("on “"))
    };
    assert!(!quotes(&view), "nothing selected, nothing quoted");
    assert!(said_on(&view, Line::Composer(0)).contains(placeholder));
    handle_command(&mut view, Command::Type('a'), SPACE);
    let first = said_on(&view, Line::Composer(0));
    assert!(
        first.ends_with('a') && !first.contains(placeholder),
        "{first}"
    );
    handle_command(&mut view, Command::Close, SPACE);

    selected(&mut view, 2..3, "apply");
    handle_command(&mut view, Command::Comment, SPACE);
    assert!(quotes(&view), "the selection quoted");
}

/// The comment affordance answers a press anywhere on its line.
#[test]
fn a_press_anywhere_on_the_affordance_starts_the_comment() {
    let mut view = reviewing();
    view.mark_cell(2, false);
    let line = line_of(&view, Line::Affordance);
    handle_hit(&mut view, ViewHit::PlaceCaret { line, cell: 0 }, SPACE);
    assert!(view.typing());
}

/// The footer offers resolve only while an answer waits.
#[test]
fn the_footer_offers_resolve_only_while_an_answer_waits() {
    let mut view = reviewing();
    thread_on(&mut view, 1, "naming.rs", 2, "fn apply() {");
    let footer = super::view(&view, SPACE).footer;
    assert!(footer.contains(&Command::Comment), "{footer:?}");
    assert!(!footer.contains(&Command::Resolve), "{footer:?}");
    view.record.comments[0].status = CommentStatus::Answered;
    assert!(super::view(&view, SPACE).footer.contains(&Command::Resolve));
}

#[test]
fn a_review_that_cannot_be_kept_says_so() {
    let mut view = reviewing();
    view.record_error = Some("the disk is full".to_owned());
    let notice = super::view(&view, SPACE).notice.expect("a notice");
    assert!(notice.text.contains("cannot be kept: the disk is full"));
}

/// Discarding a file the last commit lacks deletes it, and says so.
#[test]
fn discarding_an_added_file_says_it_deletes_it() {
    let mut view = reviewing();
    view.discarding = Some(Discarding {
        path: PathBuf::from("/w").join("tests.rs"),
        on_confirm: false,
    });
    let body = super::view(&view, SPACE).confirm.expect("asked").body;
    assert!(body.starts_with("Deletes the file"), "{body}");
}
