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

#[test]
fn only_a_file_with_uncommitted_work_offers_a_discard() {
    let mut view = reviewing();
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert!(view.menu, "naming.rs is uncommitted: its actions open");
    let drawn = super::view(&view, SPACE);
    let menu = drawn.navigator.unwrap().menu.expect("on its row");
    assert_eq!(menu.entries, ["Discard changes…"]);
    handle_hit(&mut view, ViewHit::MenuEntry(0), SPACE);
    let asked = super::view(&view, SPACE).confirm.expect("asked first");
    assert!(asked.body.contains("Puts the file back"), "{}", asked.body);
    handle_command(&mut view, Command::Close, SPACE);
    view.select_file(1);
    handle_command(&mut view, Command::OpenMenu, SPACE);
    assert!(!view.menu, "tests.rs is committed only");
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
    fn review_record(&self, _: &Path) -> Result<ReviewRecord, String> {
        Ok(self.0.lock().unwrap().clone())
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
    assert!(!view.menu, "nor opens actions on the file being commented");
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
    handle_command(&mut view, Command::OpenMenu, SPACE);
    handle_command(&mut view, Command::Activate, SPACE);
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
fn a_review_is_not_sent_where_two_agents_share_the_checkout() {
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

#[test]
fn the_door_counts_the_branch_and_says_what_waits() {
    use crate::contract::Extension;
    let mut extension = sending_extension();
    assert!(
        badge(&extension).starts_with("review"),
        "nothing counted yet: {}",
        badge(&extension)
    );
    extension.counted(PathBuf::from("/w"), 14_714, 672, 3);
    let drafts = badge(&extension);
    assert!(drafts.starts_with("+14,714 -672"), "{drafts}");
    assert!(drafts.ends_with('1'), "one draft waits: {drafts}");
    let view = extension.review_view_mut().unwrap();
    view.record.comments[0].status = CommentStatus::Answered;
    let answered = extension.door_badge().unwrap();
    let last = answered.spans.last().unwrap();
    assert_eq!(
        last.role,
        crate::view::Role::Success,
        "an answer waits, in the success hue"
    );
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
    handle_command(&mut view, Command::OpenMenu, SPACE);
    handle_command(&mut view, Command::Activate, SPACE);
    assert_eq!(view.restore, None, "asked, not done");
    handle_hit(&mut view, ViewHit::Answer(false), SPACE);
    assert_eq!(view.restore, None, "declined");
    handle_command(&mut view, Command::OpenMenu, SPACE);
    handle_command(&mut view, Command::Activate, SPACE);
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
    handle_command(&mut view, Command::OpenMenu, SPACE);
    handle_command(&mut view, Command::Activate, SPACE);
    let body = super::view(&view, SPACE).confirm.unwrap().body;
    assert!(body.contains("naming.rs returns"), "{body}");
    assert!(body.contains("named.rs goes"), "{body}");
}

#[test]
fn the_door_counts_a_draft_made_just_before_the_review_closed() {
    use crate::contract::Extension;
    let mut extension = sending_extension();
    extension.counted(PathBuf::from("/w"), 3, 1, 1);
    extension.close(SURFACE);
    assert!(
        badge(&extension).ends_with('1'),
        "one draft waits: {}",
        badge(&extension)
    );
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
        .filter(|drawn| drawn.content.tone == LineTone::Card)
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
