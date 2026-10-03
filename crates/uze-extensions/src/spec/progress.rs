//! How far a unit's steps got, counted from its checkboxes.
//!
//! Only the checkboxes are read — no section structure, no numbering.
//! Every tool this surface has in view writes its task list as GitHub
//! checkboxes, and a count that tried to understand more would be the
//! first thing to disagree with the tool that wrote the list.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

impl Progress {
    pub fn complete(self) -> bool {
        self.done == self.total
    }
}

/// The checkboxes in `text`, or `None` when it has none: a list with no
/// checkbox has no progress to report, which is not the same as none done.
pub fn count(text: &str) -> Option<Progress> {
    let mut progress = Progress::default();
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => {
                fence = Some(marker);
                continue;
            }
            (Some(open), Some(marker)) if open == marker => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            (None, None) => {}
        }
        if let Some(checked) = checkbox(trimmed) {
            progress.total += 1;
            progress.done += usize::from(checked);
        }
    }
    (progress.total > 0).then_some(progress)
}

/// The files among `relatives` ending in `step`, each done once a file
/// with the same prefix ending in `receipt` is beside it; `None` when
/// there is no step, as for a list with no checkbox.
pub fn receipts(relatives: &[&str], step: &str, receipt: &str) -> Option<Progress> {
    let mut progress = Progress::default();
    for prefix in relatives
        .iter()
        .filter_map(|relative| relative.strip_suffix(step))
    {
        progress.total += 1;
        let written = format!("{prefix}{receipt}");
        progress.done += usize::from(relatives.contains(&written.as_str()));
    }
    (progress.total > 0).then_some(progress)
}

/// Whether `line` is a list item opening with a checkbox, and if so
/// whether it is checked.
fn checkbox(line: &str) -> Option<bool> {
    let rest = ["- ", "* ", "+ "]
        .into_iter()
        .find_map(|bullet| line.strip_prefix(bullet))?;
    let (checked, after) = match rest.strip_prefix("[ ]") {
        Some(after) => (false, after),
        None => (
            true,
            rest.strip_prefix("[x]")
                .or_else(|| rest.strip_prefix("[X]"))?,
        ),
    };
    (after.is_empty() || after.starts_with(char::is_whitespace)).then_some(checked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_checked_and_unchecked_boxes() {
        let text = "## 1. Setup\n\n- [x] 1.1 one\n- [ ] 1.2 two\n- [X] 1.3 three\n";
        assert_eq!(count(text), Some(Progress { done: 2, total: 3 }));
    }

    #[test]
    fn counts_every_bullet_and_any_indentation() {
        let text = "* [x] a\n+ [ ] b\n    - [x] nested\n\t- [ ] tabbed\n";
        assert_eq!(count(text), Some(Progress { done: 2, total: 4 }));
    }

    #[test]
    fn a_list_with_no_checkbox_has_no_progress() {
        assert_eq!(count("- one\n- two\n"), None);
        assert_eq!(count(""), None);
    }

    #[test]
    fn a_box_inside_fenced_code_is_an_example_not_a_step() {
        let text = "- [ ] real\n```md\n- [x] example\n```\n~~~\n- [x] also\n~~~\n";
        assert_eq!(count(text), Some(Progress { done: 0, total: 1 }));
    }

    #[test]
    fn a_link_that_looks_like_a_box_is_not_one() {
        assert_eq!(
            count("- [x](url) a link\n- [ ]\n"),
            Some(Progress { done: 0, total: 1 })
        );
    }

    #[test]
    fn a_step_is_done_once_its_receipt_is_beside_it() {
        let files = [
            "01-CONTEXT.md",
            "01-01-PLAN.md",
            "01-01-SUMMARY.md",
            "01-02-PLAN.md",
            "01-VERIFICATION.md",
        ];
        assert_eq!(
            receipts(&files, "PLAN.md", "SUMMARY.md"),
            Some(Progress { done: 1, total: 2 })
        );
    }

    #[test]
    fn a_receipt_with_no_step_counts_for_nothing() {
        assert_eq!(
            receipts(&["01-01-SUMMARY.md"], "PLAN.md", "SUMMARY.md"),
            None
        );
    }

    #[test]
    fn complete_when_every_box_is_checked() {
        assert!(Progress { done: 3, total: 3 }.complete());
        assert!(!Progress { done: 2, total: 3 }.complete());
    }
}
