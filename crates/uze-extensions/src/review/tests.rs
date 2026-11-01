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
        }),
    );
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
        lines[3].contains("L2") && lines[3].contains("draft"),
        "{lines:?}"
    );
    assert!(lines[4].contains("check it"), "{lines:?}");
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

#[test]
fn only_a_file_with_uncommitted_work_offers_a_discard() {
    let mut view = reviewing();
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert!(view.discarding.is_some(), "naming.rs is uncommitted");
    handle_command(&mut view, Command::Close, SPACE);
    view.select_file(1);
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert!(view.discarding.is_none(), "tests.rs is committed only");
}

#[test]
fn a_changed_image_is_drawn_and_a_deleted_one_says_it_has_no_lines() {
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
    fn update_review_record(
        &self,
        _: &Path,
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
    };
    extension.open(SURFACE, &checkout);
    let view = extension.review_view_mut().unwrap();
    view.change(change::Change::Draft {
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
        }),
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
    handle_command(&mut view, Command::OpenMenu, SPACE);
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
    };
    extension.focus(Some(&checkout));
    extension.open(SURFACE, &checkout);
    extension.agents(&serde_json::json!([{ "id": "a1", "checkout": "/w" }]));
    extension
        .review_view_mut()
        .unwrap()
        .change(change::Change::Draft {
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
fn the_working_mark_is_the_sent_checkouts_and_goes_with_its_agent() {
    use crate::contract::{AgentWritten, Checkout, Extension, Outcome};
    let mut extension = sending_extension();
    let asked = extension.command(SURFACE, Command::Send, SPACE);
    assert!(matches!(asked, Outcome::Agent(_)), "{asked:?}");
    extension.agent_write("review-send", AgentWritten::Typed);
    let working = badge(&extension);
    let elsewhere = Checkout {
        root: PathBuf::from("/other"),
        display: "/other".to_owned(),
        target: None,
    };
    extension.focus(Some(&elsewhere));
    assert_ne!(
        badge(&extension),
        working,
        "another checkout's door is not the sent one's"
    );
    extension.focus(Some(&Checkout {
        root: PathBuf::from("/w"),
        display: "/w".to_owned(),
        target: None,
    }));
    assert_eq!(badge(&extension), working);
    extension.agents(&serde_json::json!([]));
    assert_ne!(badge(&extension), working, "an agent gone takes its mark");
}
