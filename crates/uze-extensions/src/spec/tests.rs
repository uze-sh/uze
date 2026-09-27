use std::path::{Path, PathBuf};

use uze_testkit::temp::TempDir;

use super::{
    Found, SpecAnswer, SpecOutcome, SpecPlace, SpecView, Subject,
    catalog::Unit,
    dialect::{Dialect, OPENSPEC, Role, SPEC_KIT},
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
fn a_nested_capability_is_one_spec_named_by_its_path() {
    let dir = TempDir::new("spec-nested-capabilities");
    write(dir.path(), "openspec/specs/billing/spec.md", "# billing\n");
    write(
        dir.path(),
        "openspec/specs/identity/user-auth/spec.md",
        "# user auth\n",
    );
    write(
        dir.path(),
        "openspec/specs/identity/session-expiry/spec.md",
        "# expiry\n",
    );
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC]);

    assert_eq!(
        names(units(&answer), Subject::Specs),
        ["billing", "identity/session-expiry", "identity/user-auth"],
        "`identity` holds no spec of its own, so it is no unit"
    );
    let expiry = units(&answer)
        .iter()
        .find(|unit| unit.name == "identity/session-expiry")
        .unwrap();
    assert_eq!(expiry.relative, "openspec/specs/identity/session-expiry");
    assert_eq!(expiry.artifacts.len(), 1);
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

/// A Spec Kit project as its own scripts and templates lay it out
/// (github/spec-kit `c00dc05`, 2026-09-25): `.specify/` from `init`,
/// `specs/NNN-name/` from `specify`, the companions `plan` writes beside
/// the plan, and task lines in the tasks template's shape.
fn spec_kit_checkout(label: &str) -> TempDir {
    let dir = TempDir::new(label);
    let root = dir.path();
    write(
        root,
        ".specify/memory/constitution.md",
        "# Project Constitution\n",
    );
    let feature = "specs/001-user-auth";
    write(
        root,
        &format!("{feature}/spec.md"),
        "# Feature Specification: User auth\n\n## User Scenarios & Testing\n",
    );
    write(
        root,
        &format!("{feature}/plan.md"),
        "# Implementation Plan: User auth\n",
    );
    write(root, &format!("{feature}/research.md"), "# Research\n");
    write(root, &format!("{feature}/data-model.md"), "# Data Model\n");
    write(root, &format!("{feature}/quickstart.md"), "# Quickstart\n");
    write(
        root,
        &format!("{feature}/contracts/auth-api.md"),
        "# Auth API\n",
    );
    write(
        root,
        &format!("{feature}/checklists/requirements.md"),
        "- [x] CHK001 No implementation details\n- [ ] CHK002 Requirements are testable\n",
    );
    write(
        root,
        &format!("{feature}/tasks.md"),
        "## Phase 1: Setup\n\n- [x] T001 Create project structure per implementation plan\n\
         - [x] T002 [P] Configure linting and formatting tools\n\n\
         ## Phase 3: User Story 1\n\n- [ ] T003 [P] [US1] Create User model in src/models/user.py\n",
    );
    write(
        root,
        "specs/002-refunds/tasks.md",
        "- [x] T001 Setup\n- [X] T002 Build\n",
    );
    write(
        root,
        &format!("{feature}/contracts/openapi.yaml"),
        "openapi: 3.1.0\npaths: {}\n",
    );
    write(
        root,
        "specs/002-refunds/spec.md",
        "# Feature Specification: Refunds\n",
    );
    dir
}

#[test]
fn a_spec_kit_checkout_lists_its_features_and_its_constitution() {
    let dir = spec_kit_checkout("spec-kit-detect");
    let answer = read_with(&DiskHost, dir.path(), None, &[OPENSPEC, SPEC_KIT]);

    let Found::Units { dialects, .. } = &answer.found else {
        panic!("expected Spec Kit to be detected");
    };
    assert_eq!(dialects, &["Spec Kit"]);
    assert_eq!(
        answer.subjects,
        [Subject::Changes, Subject::Specs],
        "Spec Kit archives nothing"
    );
    assert_eq!(
        names(units(&answer), Subject::Changes),
        ["001-user-auth", "002-refunds"]
    );
    assert_eq!(names(units(&answer), Subject::Specs), ["constitution"]);
    let constitution = units(&answer)
        .iter()
        .find(|unit| unit.subject == Subject::Specs)
        .unwrap();
    assert_eq!(constitution.relative, ".specify/memory/constitution.md");
    assert_eq!(
        constitution
            .artifacts
            .iter()
            .map(|artifact| (artifact.role, artifact.name.as_str()))
            .collect::<Vec<_>>(),
        [(Role::Contract, "constitution")]
    );
}

#[test]
fn a_unit_that_is_one_file_is_headed_by_where_it_lives() {
    let dir = spec_kit_checkout("spec-kit-constitution");
    let state = opened_at(
        read_with(&DiskHost, dir.path(), None, &[SPEC_KIT]),
        SpecPlace {
            subject: Subject::Specs,
            unit: "constitution".to_owned(),
            artifact: Some("constitution.md".to_owned()),
        },
    );
    let Content::Lines { heading, .. } = view(&state, space()).content else {
        panic!("expected the constitution on show");
    };
    assert_eq!(heading, ".specify/memory/constitution.md");
}

#[test]
fn a_contract_that_is_not_markdown_is_listed_and_shown_as_its_source() {
    let dir = spec_kit_checkout("spec-kit-openapi");
    let state = opened_at(
        read_with(&DiskHost, dir.path(), None, &[SPEC_KIT]),
        SpecPlace {
            subject: Subject::Changes,
            unit: "001-user-auth".to_owned(),
            artifact: Some("contracts/openapi.yaml".to_owned()),
        },
    );
    let Content::Lines { heading, lines, .. } = view(&state, space()).content else {
        panic!("expected the contract on show");
    };
    assert_eq!(heading, "001-user-auth/contracts/openapi.yaml");
    assert_eq!(
        lines[0].number, "1",
        "Preview has nothing to render in YAML, so it shows the source"
    );
}

#[test]
fn a_spec_kit_feature_reads_spec_then_plan_and_its_companions_then_tasks() {
    let dir = spec_kit_checkout("spec-kit-roles");
    let answer = read_with(&DiskHost, dir.path(), None, &[SPEC_KIT]);
    let feature = &units(&answer)[0];

    let listed: Vec<(Role, &str)> = feature
        .artifacts
        .iter()
        .map(|artifact| (artifact.role, artifact.name.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (Role::Why, "spec"),
            (Role::How, "plan"),
            (Role::How, "research"),
            (Role::How, "data-model"),
            (Role::How, "quickstart"),
            (Role::Steps, "tasks"),
            (Role::Other, "checklists/requirements"),
            (Role::Other, "contracts/auth-api"),
            (Role::Other, "contracts/openapi.yaml"),
        ]
    );
    assert_eq!(
        feature
            .progress
            .map(|progress| (progress.done, progress.total)),
        Some((2, 3)),
        "counted from tasks.md, never from a checklist"
    );
}

#[test]
fn a_finished_feature_is_done_where_nothing_is_archived_and_done_opens_folded() {
    let dir = spec_kit_checkout("spec-kit-bands");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[SPEC_KIT]));

    assert_eq!(
        row_names(&state),
        ["# in progress (1)", "  001-user-auth", "", "# done (1)"]
    );
}

#[test]
fn done_stays_open_when_it_is_where_the_viewer_lands() {
    let dir = TempDir::new("spec-kit-only-done");
    write(dir.path(), ".specify/memory/constitution.md", "# c\n");
    write(dir.path(), "specs/001-a/tasks.md", "- [x] T001 a\n");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[SPEC_KIT]));

    assert_eq!(row_names(&state), ["# done (1)", "  001-a"]);
}

#[test]
fn nothing_in_flight_points_nowhere_when_there_is_nowhere_to_point() {
    let dir = TempDir::new("spec-kit-nothing-in-flight");
    write(dir.path(), ".specify/memory/constitution.md", "# c\n");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[SPEC_KIT]));
    let Content::Message { text, hint, .. } = view(&state, space()).content else {
        panic!("expected a message");
    };
    assert_eq!(text, "Nothing in flight");
    assert_eq!(
        hint, None,
        "Spec Kit archives nothing, so no finished change is anywhere else"
    );
}

#[test]
fn a_checkout_with_two_tools_names_each_units_tool_and_only_then() {
    let dir = openspec_checkout("spec-two-tools");
    let feature = "specs/001-user-auth";
    write(dir.path(), ".specify/memory/constitution.md", "# c\n");
    write(dir.path(), &format!("{feature}/spec.md"), "# s\n");
    let details = |dialects: &[Dialect]| -> Vec<(String, String)> {
        let state = opened(read_with(&DiskHost, dir.path(), None, dialects));
        view(&state, space())
            .navigator
            .expect("a navigator")
            .rows
            .into_iter()
            .filter_map(|row| match row {
                NavigatorRow::Item { name, detail, .. } => Some((name, detail)),
                _ => None,
            })
            .collect()
    };

    let mixed = details(&[OPENSPEC, SPEC_KIT]);
    assert!(mixed.contains(&("b-change".to_owned(), "OpenSpec".to_owned())));
    assert!(mixed.contains(&("001-user-auth".to_owned(), "Spec Kit".to_owned())));
    assert!(
        details(&[OPENSPEC])
            .iter()
            .all(|(_, detail)| detail.is_empty()),
        "one tool alone is never named"
    );
}

// --- the surface --------------------------------------------------------

fn opened(answer: SpecAnswer) -> SpecView {
    let mut state = SpecView::opening("~/project".to_owned());
    state.absorb(answer);
    state
}

fn opened_at(answer: SpecAnswer, place: SpecPlace) -> SpecView {
    let mut state = SpecView::opening("~/project".to_owned()).resuming(place);
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
fn no_layout_lists_every_tool_it_reads_in_columns() {
    let dir = TempDir::new("spec-no-layout-view");
    let state = opened(read_with(&DiskHost, dir.path(), None, &[OPENSPEC]));
    let shown = view(&state, space());

    assert!(shown.navigator.is_none());
    let Content::Message { text, hint, .. } = shown.content else {
        panic!("expected a message");
    };
    assert_eq!(text, "No spec layout found in this checkout");
    let hint = hint.unwrap();
    let tools: Vec<&str> = hint.lines().skip(2).collect();
    assert_eq!(
        tools,
        ["OpenSpec   openspec/", "Spec Kit   .specify/"],
        "one line per shipped tool, whatever the checkout was read for"
    );
    assert!(
        tools.iter().all(|tool| tool.len() == tools[0].len()),
        "padded to one width, so centred lines stay a column"
    );
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
