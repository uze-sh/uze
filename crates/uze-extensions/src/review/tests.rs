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

#[test]
fn viewed_marks_the_file_and_moves_on_and_falls_away_when_it_changes() {
    let mut view = reviewing();
    let naming = PathBuf::from("/w/naming.rs");
    handle_command(&mut view, Command::MarkViewed, SPACE);
    assert_eq!(view.selected.as_deref(), Some(Path::new("/w/tests.rs")));
    assert_eq!(view.viewed(&naming), surface::Viewed::Yes);
    view.select_file(0);
    view.absorb_diff(
        naming.clone(),
        Ok(diff_of(&naming, &NAMING.replace("rename();", "rename()?;"))),
        "x".to_owned(),
    );
    assert_eq!(view.viewed(&naming), surface::Viewed::Changed);
    assert_eq!(view.viewed_count(), 0);
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
