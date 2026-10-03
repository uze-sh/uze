//! What this checkout is working on, at a glance: the sidebar section
//! that says how far its changes have got without opening the surface.
//!
//! Contextual, like the timeline beside it: it is about the checkout in
//! front and nothing else. A checkout that touched no change has no
//! section, and the project's other changes are the surface's to list.
//! Read every few seconds, so Git is asked first and only the changes it
//! names are opened.

use std::path::Path;

use super::{Found, Progress, Subject, Unit, catalog, dialect, ownership};
use crate::{
    Host,
    view::{Role, RowMark, Section, SectionRow, Span},
};

/// One change this checkout is working on, as the section lists it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeSummary {
    pub name: String,
    pub progress: Option<Progress>,
}

/// The changes this checkout is working on, by name, and the checkboxes
/// of all of them together.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Summary {
    pub changes: Vec<ChangeSummary>,
    pub done: usize,
    pub total: usize,
}

/// The changes `checkout` is working on, or `None` where it is working on
/// none — including where it has no spec layout at all.
pub fn summary(host: &dyn Host, checkout: &Path, target: Option<&str>) -> Option<Summary> {
    let root = host
        .repository_root(checkout)
        .unwrap_or_else(|_| checkout.to_path_buf());
    let touched = ownership::touched(host, &root, target);
    let keep = |unit: &str| touched.iter().any(|path| ownership::lies_in(path, unit));
    let Found::Units { units, .. } =
        catalog::read_subjects(host, &root, dialect::SHIPPED, &[Subject::Changes], &keep)
    else {
        return None;
    };
    (!units.is_empty()).then(|| summarise(units))
}

fn summarise(units: Vec<Unit>) -> Summary {
    let (done, total) = units
        .iter()
        .filter_map(|unit| unit.progress)
        .fold((0, 0), |(done, total), progress| {
            (done + progress.done, total + progress.total)
        });
    Summary {
        changes: units
            .into_iter()
            .map(|unit| ChangeSummary {
                name: unit.name,
                progress: unit.progress,
            })
            .collect(),
        done,
        total,
    }
}

/// The section the host draws in the sidebar.
///
/// The caption is the macro: every checkbox of the checkout's changes,
/// done out of all, which stays on the header when the section is folded.
/// Each row is one change, its step mark filled once all its boxes are.
pub fn summary_section(summary: &Summary, collapsed: bool, scroll: usize) -> Section {
    Section {
        // Named for what the number counts: the checkboxes of the work,
        // not the specs they are written against.
        title: "tasks".to_owned(),
        caption: Span::new(
            if summary.total == 0 {
                String::new()
            } else {
                format!("{}/{}", summary.done, summary.total)
            },
            Role::Muted,
        ),
        collapsed,
        resizable: false,
        scroll,
        rows: summary
            .changes
            .iter()
            .map(|change| {
                let complete = change.progress.is_some_and(Progress::complete);
                SectionRow {
                    mark: RowMark::Step { done: complete },
                    mark_role: if complete {
                        Role::Success
                    } else {
                        Role::Accent
                    },
                    // The ink the timeline beneath writes its commits in,
                    // so lifting a row to bright is the one thing that
                    // stands out in the column.
                    name: Span::new(change.name.clone(), Role::Inactive),
                    trailing: Span::new(
                        change.progress.map_or_else(String::new, |progress| {
                            format!("{}/{}", progress.done, progress.total)
                        }),
                        Role::Faint,
                    ),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::tests::DiskHost;

    #[test]
    fn only_the_changes_this_checkout_touched_are_summarised() {
        let repository = uze_testkit::git::Repository::new("spec-summary");
        let root = repository.root().to_path_buf();
        let target = repository.branch();
        repository.commit_file("openspec/changes/theirs/tasks.md", "- [x] a\n- [ ] b\n");
        repository.git(&["checkout", "--quiet", "-b", "work"]);
        repository.commit_file(
            "openspec/changes/mine/tasks.md",
            "- [x] a\n- [ ] b\n- [ ] c\n",
        );
        std::fs::create_dir_all(root.join("openspec/changes/drafting")).unwrap();
        std::fs::write(root.join("openspec/changes/drafting/tasks.md"), "- [x] a\n").unwrap();

        let summary = summary(&DiskHost, &root, Some(&target)).expect("work in flight here");
        let section = summary_section(&summary, false, 0);
        assert_eq!(section.title, "tasks");
        assert_eq!(section.caption.text, "2/4", "the checkout's own boxes only");
        let rows: Vec<(&str, &str)> = section
            .rows
            .iter()
            .map(|row| (row.name.text.as_str(), row.trailing.text.as_str()))
            .collect();
        assert_eq!(rows, [("drafting", "1/1"), ("mine", "1/3")]);
        assert_eq!(section.rows[0].mark, RowMark::Step { done: true });
        assert_eq!(
            section.rows[0].name.role,
            Role::Inactive,
            "the timeline's ink"
        );
    }

    fn summarised(summary: &Summary) -> (String, Vec<(String, String)>) {
        let section = summary_section(summary, false, 0);
        (
            section.caption.text,
            section
                .rows
                .into_iter()
                .map(|row| (row.name.text, row.trailing.text))
                .collect(),
        )
    }

    #[test]
    fn a_superpowers_plan_this_checkout_touched_is_summarised_by_its_steps() {
        let repository = uze_testkit::git::Repository::new("spec-summary-superpowers");
        let root = repository.root().to_path_buf();
        let target = repository.branch();
        repository.commit_file("docs/superpowers/plans/2026-09-01-theirs.md", "- [ ] a\n");
        repository.git(&["checkout", "--quiet", "-b", "work"]);
        repository.commit_file(
            "docs/superpowers/plans/2026-09-29-mine.md",
            "- [x] **Step 1: a**\n- [ ] **Step 2: b**\n",
        );

        let summary = summary(&DiskHost, &root, Some(&target)).expect("a plan in flight here");
        assert_eq!(
            summarised(&summary),
            (
                "1/2".to_owned(),
                vec![("2026-09-29-mine".to_owned(), "1/2".to_owned())]
            )
        );
    }

    #[test]
    fn a_gsd_phase_this_checkout_touched_is_summarised_by_its_executed_plans() {
        let repository = uze_testkit::git::Repository::new("spec-summary-gsd");
        let root = repository.root().to_path_buf();
        let target = repository.branch();
        repository.commit_file(".planning/ROADMAP.md", "# Roadmap\n");
        repository.commit_file(".planning/phases/02-import/02-01-PLAN.md", "<tasks/>\n");
        repository.git(&["checkout", "--quiet", "-b", "work"]);
        repository.commit_file(".planning/phases/03-sync/03-01-PLAN.md", "<tasks/>\n");
        repository.commit_file(".planning/phases/03-sync/03-01-SUMMARY.md", "# Summary\n");
        std::fs::write(
            root.join(".planning/phases/03-sync/03-02-PLAN.md"),
            "<tasks/>\n",
        )
        .unwrap();

        let summary = summary(&DiskHost, &root, Some(&target)).expect("a phase in flight here");
        assert_eq!(
            summarised(&summary),
            (
                "1/2".to_owned(),
                vec![("03-sync".to_owned(), "1/2".to_owned())]
            )
        );
    }

    #[test]
    fn a_checkout_working_on_no_change_has_no_section() {
        let repository = uze_testkit::git::Repository::new("spec-summary-idle");
        let root = repository.root().to_path_buf();
        repository.commit_file("openspec/changes/theirs/tasks.md", "- [ ] a\n");
        let target = repository.branch();

        assert_eq!(summary(&DiskHost, &root, Some(&target)), None);
    }

    #[test]
    fn a_checkout_with_no_layout_has_no_section() {
        let dir = uze_testkit::temp::TempDir::new("spec-summary-none");
        assert_eq!(summary(&DiskHost, dir.path(), None), None);
    }
}
