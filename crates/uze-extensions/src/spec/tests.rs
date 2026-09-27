use std::path::{Path, PathBuf};

use uze_testkit::temp::TempDir;

use super::{
    Found, SpecAnswer, SpecOutcome, SpecView, Subject,
    catalog::Unit,
    dialect::{Collection, Dialect, OPENSPEC, Order, Role},
    handle_command, handle_mouse, read_with, view,
};
use crate::{
    DirEntry, Host,
    shared::highlight::FALLBACK_SYNTAX_THEME,
    view::{Command, Content, NavigatorRow, Size, ViewHit},
};

/// The whole grant, read-only, against the real disk and the real Git.
/// The surface's reads are what these tests are about, so nothing is faked.
pub(super) struct DiskHost;

impl Host for DiskHost {
    fn git(&self, root: &Path, args: &[&str], answers: &[i32]) -> Result<String, String> {
        uze_git::read(root, args)
            .map_err(|error| error.to_string())?
            .or_exit(answers)
    }

    fn repository_root(&self, path: &Path) -> Result<PathBuf, String> {
        uze_git::repository::root(path)
    }

    fn read_file(&self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|error| error.to_string())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<DirEntry>, String> {
        let mut entries: Vec<DirEntry> = std::fs::read_dir(path)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| DirEntry {
                directory: entry.path().is_dir(),
                name: entry.file_name().to_string_lossy().into_owned(),
            })
            .collect();
        entries.sort();
        Ok(entries)
    }

    fn write_file(&self, _path: &Path, _contents: &str) -> Result<(), String> {
        unreachable!("the spec surface writes nothing")
    }

    fn delete_file(&self, _path: &Path) -> Result<(), String> {
        unreachable!("the spec surface deletes nothing")
    }

    fn restore_to_head(&self, _root: &Path, _paths: &[PathBuf]) -> Result<(), String> {
        unreachable!("the spec surface restores nothing")
    }

    fn syntax_theme(&self) -> String {
        FALLBACK_SYNTAX_THEME.to_owned()
    }
}

/// Kiro's layout, as a catalog entry the build does not ship: the proof
/// that the shape is not OpenSpec's shape. The farthest tool from OpenSpec
/// that still fits — no specs, no archive, its own file names.
const KIRO: Dialect = Dialect {
    name: "Kiro",
    marker: ".kiro/specs",
    collections: &[Collection {
        subject: Subject::Changes,
        path: ".kiro/specs",
        skip: &[],
        order: Order::Ascending,
    }],
    roles: &[
        ("requirements.md", Role::Why),
        ("design.md", Role::How),
        ("tasks.md", Role::Steps),
    ],
};

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn units(answer: &SpecAnswer) -> &[Unit] {
    match &answer.found {
        Found::Units { units, .. } => units,
        Found::NoLayout => panic!("expected a layout"),
    }
}

fn names(units: &[Unit], subject: Subject) -> Vec<&str> {
    units
        .iter()
        .filter(|unit| unit.subject == subject)
        .map(|unit| unit.name.as_str())
        .collect()
}

fn openspec_checkout(label: &str) -> TempDir {
    let dir = TempDir::new(label);
    let root = dir.path();
    write(root, "openspec/config.yaml", "schema: spec-driven\n");
    write(
        root,
        "openspec/changes/b-change/tasks.md",
        "- [x] one\n- [ ] two\n",
    );
    write(
        root,
        "openspec/changes/b-change/proposal.md",
        "## Why\n\nBecause.\n",
    );
    write(root, "openspec/changes/b-change/design.md", "## Context\n");
    write(
        root,
        "openspec/changes/b-change/specs/spec-surface/spec.md",
        "## ADDED Requirements\n",
    );
    write(root, "openspec/changes/b-change/notes.md", "scratch\n");
    write(root, "openspec/changes/a-done/tasks.md", "- [x] one\n");
    write(root, "openspec/changes/a-done/proposal.md", "## Why\n");
    write(root, "openspec/changes/c-open/tasks.md", "- [ ] one\n");
    write(
        root,
        "openspec/specs/architect-surface/spec.md",
        "# architect\n",
    );
    write(
        root,
        "openspec/changes/archive/2026-09-01-old/proposal.md",
        "old\n",
    );
    write(
        root,
        "openspec/changes/archive/2026-09-20-newer/proposal.md",
        "newer\n",
    );
    dir
}

fn space() -> Size {
    Size {
        width: 80,
        height: 20,
    }
}

// --- reading ------------------------------------------------------------

#[test]
fn an_openspec_checkout_lists_its_changes_specs_and_archive() {
    let dir = openspec_checkout("spec-reads-openspec");
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC]);
    let units = units(&answer);

    assert_eq!(
        names(units, Subject::Changes),
        ["a-done", "b-change", "c-open"]
    );
    assert_eq!(names(units, Subject::Specs), ["architect-surface"]);
    assert_eq!(
        names(units, Subject::Archive),
        ["2026-09-20-newer", "2026-09-01-old"],
        "the archive is newest first, and `archive` is not a change"
    );
    assert_eq!(
        answer.subjects,
        [Subject::Changes, Subject::Specs, Subject::Archive]
    );
}

#[test]
fn a_changes_artifacts_come_in_role_order_with_unnamed_files_last() {
    let dir = openspec_checkout("spec-role-order");
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC]);
    let change = units(&answer)
        .iter()
        .find(|unit| unit.name == "b-change")
        .unwrap();

    let listed: Vec<(Role, &str)> = change
        .artifacts
        .iter()
        .map(|artifact| (artifact.role, artifact.name.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (Role::Why, "proposal"),
            (Role::How, "design"),
            (Role::Steps, "tasks"),
            (Role::Contract, "spec-surface"),
            (Role::Other, "notes"),
        ]
    );
}

#[test]
fn progress_is_counted_for_changes_in_flight_only() {
    let dir = openspec_checkout("spec-progress");
    write(
        dir.path(),
        "openspec/changes/archive/2026-09-01-old/tasks.md",
        "- [x] a\n",
    );
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC]);
    let progress = |name: &str| {
        units(&answer)
            .iter()
            .find(|unit| unit.name == name)
            .unwrap()
            .progress
            .map(|progress| (progress.done, progress.total))
    };

    assert_eq!(progress("b-change"), Some((1, 2)));
    assert_eq!(progress("a-done"), Some((1, 1)));
    assert_eq!(progress("2026-09-01-old"), None);
}

#[test]
fn a_schema_that_names_none_of_the_files_still_lists_every_document() {
    let dir = TempDir::new("spec-custom-schema");
    write(dir.path(), "openspec/changes/x/brief.md", "a\n");
    write(dir.path(), "openspec/changes/x/plan.md", "b\n");
    write(
        dir.path(),
        "openspec/changes/x/.openspec.yaml",
        "schema: other\n",
    );
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC]);
    let change = &units(&answer)[0];

    assert_eq!(change.name, "x");
    assert!(
        change
            .artifacts
            .iter()
            .all(|artifact| artifact.role == Role::Other)
    );
    assert_eq!(change.artifacts.len(), 2, "only Markdown is a document");
}

#[test]
fn a_checkout_with_no_marker_has_no_layout() {
    let dir = TempDir::new("spec-no-layout");
    write(dir.path(), "README.md", "hello\n");

    assert_eq!(
        read_with(&DiskHost, dir.path(), None, &[OPENSPEC]).found,
        Found::NoLayout
    );
}

#[test]
fn a_second_dialect_is_a_table_entry_and_nothing_else() {
    let dir = TempDir::new("spec-kiro");
    write(dir.path(), ".kiro/specs/login/requirements.md", "# login\n");
    write(dir.path(), ".kiro/specs/login/design.md", "# design\n");
    write(
        dir.path(),
        ".kiro/specs/login/tasks.md",
        "- [x] a\n- [ ] b\n",
    );
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC, KIRO]);

    let Found::Units { dialects, units } = &answer.found else {
        panic!("expected Kiro to be detected");
    };
    assert_eq!(dialects, &["Kiro"]);
    assert_eq!(
        answer.subjects,
        [Subject::Changes],
        "Kiro has no specs or archive"
    );
    let login = &units[0];
    assert_eq!(
        login
            .artifacts
            .iter()
            .map(|artifact| artifact.role)
            .collect::<Vec<_>>(),
        [Role::Why, Role::How, Role::Steps]
    );
    assert_eq!(
        login
            .progress
            .map(|progress| (progress.done, progress.total)),
        Some((1, 2))
    );
}

// --- the surface --------------------------------------------------------

fn opened(answer: SpecAnswer) -> SpecView {
    let mut state = SpecView::opening("~/project".to_owned());
    state.absorb(answer);
    state
}

fn row_names(state: &SpecView) -> Vec<String> {
    let navigator = view(state, space()).navigator.expect("a navigator");
    navigator
        .rows
        .iter()
        .map(|row| match row {
            NavigatorRow::Band { name, count, .. } => format!("# {name} ({count})"),
            NavigatorRow::Gap => String::new(),
            NavigatorRow::Group { name, .. } => format!("# {name}"),
            NavigatorRow::Item { name, depth, .. } => format!("{}{name}", "  ".repeat(*depth)),
        })
        .collect()
}

#[test]
fn changes_are_banded_by_where_they_stand() {
    let dir = openspec_checkout("spec-bands");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));

    assert_eq!(
        row_names(&state),
        [
            "# in progress (2)",
            "  b-change",
            "  c-open",
            "",
            "# ready to archive (1)",
            "  a-done",
        ]
    );
}

#[test]
fn the_change_this_checkout_touched_comes_first_opened_on_its_first_document() {
    let repository = uze_testkit::git::Repository::new("spec-own-change");
    let root = repository.root().to_path_buf();
    repository.commit_file("openspec/changes/a/proposal.md", "## Why\n");
    repository.commit_file("openspec/changes/b/proposal.md", "## Why\n\nMine.\n");
    write(&root, "openspec/changes/b/tasks.md", "- [ ] one\n");
    let state = opened(read_with(&DiskHost, &root, None, &[OPENSPEC]));

    assert_eq!(
        row_names(&state),
        [
            "# this checkout (1)",
            "  b",
            "    proposal",
            "    tasks",
            "",
            "# in progress (1)",
            "  a",
        ]
    );
    let Content::Lines { heading, .. } = view(&state, space()).content else {
        panic!("expected the proposal on show");
    };
    assert_eq!(heading, "b/proposal.md");
}

#[test]
fn with_nothing_of_its_own_the_first_change_is_selected_and_shown() {
    let dir = openspec_checkout("spec-first-change");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    let Content::Lines { heading, .. } = view(&state, space()).content else {
        panic!("expected a document on show");
    };
    assert_eq!(heading, "b-change/proposal.md");
}

#[test]
fn activating_a_document_hands_it_to_the_code_surface() {
    let dir = openspec_checkout("spec-activate");
    let mut state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));

    assert_eq!(
        handle_command(&mut state, Command::Activate, space()),
        SpecOutcome::Stay,
        "on a change, activating opens it"
    );
    handle_command(&mut state, Command::SelectNext, space());
    let SpecOutcome::OpenPath { target, .. } =
        handle_command(&mut state, Command::Activate, space())
    else {
        panic!("a document opens in the code surface");
    };
    assert!(target.ends_with("openspec/changes/b-change/proposal.md"));
}

#[test]
fn switching_to_the_archive_lists_it_newest_first() {
    let dir = openspec_checkout("spec-archive");
    let mut state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    handle_mouse(&mut state, Some(ViewHit::SelectSubject(2)), space());

    assert_eq!(row_names(&state), ["2026-09-20-newer", "2026-09-01-old"]);
}

#[test]
fn a_band_folds_from_its_heading() {
    let dir = openspec_checkout("spec-fold");
    let mut state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    handle_mouse(&mut state, Some(ViewHit::ToggleGroup(4)), space());

    assert_eq!(
        row_names(&state),
        [
            "# in progress (2)",
            "  b-change",
            "  c-open",
            "",
            "# ready to archive (1)",
        ],
        "folded, a band still says how many it holds"
    );
}

#[test]
fn nothing_in_flight_points_at_the_archive_and_the_specs() {
    let dir = TempDir::new("spec-nothing-in-flight");
    write(dir.path(), "openspec/specs/x/spec.md", "# x\n");
    write(
        dir.path(),
        "openspec/changes/archive/2026-01-01-x/proposal.md",
        "x\n",
    );
    let state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    let Content::Message { text, hint, .. } = view(&state, space()).content else {
        panic!("expected a message");
    };
    assert_eq!(text, "Nothing in flight");
    assert!(hint.unwrap().contains("Archive"));
}

#[test]
fn no_layout_names_the_layouts_it_reads() {
    let dir = TempDir::new("spec-no-layout-view");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    let shown = view(&state, space());

    assert!(shown.navigator.is_none());
    let Content::Message { text, hint, .. } = shown.content else {
        panic!("expected a message");
    };
    assert_eq!(text, "No spec layout found in this checkout");
    assert!(hint.unwrap().contains("OpenSpec (openspec/)"));
}

#[test]
fn a_place_is_restored_by_name_and_an_archived_one_is_not() {
    let dir = openspec_checkout("spec-place");
    let mut state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    handle_command(&mut state, Command::Expand, space());
    handle_command(&mut state, Command::SelectNext, space());
    handle_command(&mut state, Command::SelectNext, space());
    let place = state.place().unwrap();

    let back = {
        let mut back = SpecView::opening("~/project".to_owned()).resuming(place.clone());
        back.absorb(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
        back
    };
    let Content::Lines { heading, .. } = view(&back, space()).content else {
        panic!("expected a document");
    };
    assert_eq!(heading, "b-change/design.md");

    std::fs::rename(
        dir.join("openspec/changes/b-change"),
        dir.join("openspec/changes/archive/2026-09-27-b-change"),
    )
    .unwrap();
    let mut gone = SpecView::opening("~/project".to_owned()).resuming(place);
    gone.absorb(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    let Content::Lines { heading, .. } = view(&gone, space()).content else {
        panic!("expected a document");
    };
    assert_eq!(
        heading, "c-open/tasks.md",
        "starts over, on the first change"
    );
}

#[test]
fn the_source_mode_shows_the_markup_numbered() {
    let dir = openspec_checkout("spec-source");
    let mut state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    handle_command(&mut state, Command::TogglePreview, space());
    let Content::Lines { lines, total, .. } = view(&state, space()).content else {
        panic!("expected lines");
    };
    assert_eq!(total, 3);
    assert_eq!(lines[0].number, "1");
    let text: String = lines[0]
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect();
    assert_eq!(text, "## Why");
}

#[test]
fn each_subject_is_marked_by_what_it_holds() {
    let dir = openspec_checkout("spec-subject-icons");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    let icons: Vec<crate::view::RowIcon> = view(&state, space())
        .subjects
        .into_iter()
        .map(|subject| subject.icon)
        .collect();

    assert_eq!(
        icons,
        [
            crate::view::RowIcon::InFlight,
            crate::view::RowIcon::Contract,
            crate::view::RowIcon::Finished,
        ]
    );
}
