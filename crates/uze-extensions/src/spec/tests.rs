use std::path::{Path, PathBuf};

use uze_testkit::temp::TempDir;

use super::{
    Found, SpecAnswer, SpecOutcome, SpecPlace, SpecView, Subject,
    catalog::Unit,
    dialect::{Dialect, GSD, OPENSPEC, Role, SHIPPED, SPEC_KIT, SUPERPOWERS},
    handle_command, handle_mouse, read_with, text, view,
};
use crate::{
    ArtifactRoot, ArtifactSource, DirEntry, Host,
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

    fn read_file(&self, path: &Path) -> Result<String, crate::Unreadable> {
        std::fs::read_to_string(path).map_err(|error| crate::Unreadable::Failed(error.to_string()))
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

    fn write_file(&self, _root: &Path, _path: &Path, _contents: &str) -> Result<(), String> {
        unreachable!("the spec surface writes nothing")
    }

    fn delete_file(&self, _root: &Path, _path: &Path) -> Result<(), String> {
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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    );
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
        [Subject::Changes, Subject::Specs],
        "what was put away is a band of the changes, not a tab"
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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    );

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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    );
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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    );
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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    );
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
        read_with(
            &DiskHost,
            dir.path(),
            None,
            &[OPENSPEC],
            &ArtifactSource::Undeclared
        )
        .found,
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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC, SPEC_KIT],
        &ArtifactSource::Undeclared,
    );

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
        read_with(
            &DiskHost,
            dir.path(),
            None,
            &[SPEC_KIT],
            &ArtifactSource::Undeclared,
        ),
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
        read_with(
            &DiskHost,
            dir.path(),
            None,
            &[SPEC_KIT],
            &ArtifactSource::Undeclared,
        ),
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
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[SPEC_KIT],
        &ArtifactSource::Undeclared,
    );
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
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[SPEC_KIT],
        &ArtifactSource::Undeclared,
    ));

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
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[SPEC_KIT],
        &ArtifactSource::Undeclared,
    ));

    assert_eq!(row_names(&state), ["# done (1)", "  001-a"]);
}

#[test]
fn nothing_in_flight_points_nowhere_when_there_is_nowhere_to_point() {
    let dir = TempDir::new("spec-kit-nothing-in-flight");
    write(dir.path(), ".specify/memory/constitution.md", "# c\n");
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[SPEC_KIT],
        &ArtifactSource::Undeclared,
    ));
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
        let state = opened(read_with(
            &DiskHost,
            dir.path(),
            None,
            dialects,
            &ArtifactSource::Undeclared,
        ));
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
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));

    assert_eq!(
        row_names(&state),
        [
            "# in progress (2)",
            "  b-change",
            "  c-open",
            "",
            "# ready to archive (1)",
            "  a-done",
            "",
            "# archived (2)",
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
    let state = opened(read_with(
        &DiskHost,
        &root,
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));

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
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
    let Content::Lines { heading, .. } = view(&state, space()).content else {
        panic!("expected a document on show");
    };
    assert_eq!(heading, "b-change/proposal.md");
}

#[test]
fn activating_a_document_hands_it_to_the_code_surface() {
    let dir = openspec_checkout("spec-activate");
    let mut state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));

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

/// The id of the band a navigator draws under `label`.
fn band_id(state: &SpecView, label: &str) -> usize {
    view(state, space())
        .navigator
        .expect("a navigator")
        .rows
        .iter()
        .find_map(|row| match row {
            NavigatorRow::Band { id, name, .. } if name == label => Some(*id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {label} band"))
}

/// What a tool put away is a change that ended: the last band of the
/// changes, folded, newest first once it is opened, and never a tab of
/// its own.
#[test]
fn archived_changes_are_the_last_band_newest_first() {
    let dir = openspec_checkout("spec-archive");
    let mut state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
    let rows = row_names(&state);
    assert_eq!(rows.last().map(String::as_str), Some("# archived (2)"));
    assert!(
        !rows.iter().any(|row| row.contains("newer")),
        "folded at first"
    );

    let archived = band_id(&state, "archived");
    handle_mouse(&mut state, Some(ViewHit::ToggleGroup(archived)), space());
    let rows = row_names(&state);
    let band = rows.iter().position(|row| row == "# archived (2)").unwrap();
    assert_eq!(rows[band + 1..], ["  2026-09-20-newer", "  2026-09-01-old"]);
}

#[test]
fn a_band_folds_from_its_heading() {
    let dir = openspec_checkout("spec-fold");
    let mut state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
    handle_mouse(&mut state, Some(ViewHit::ToggleGroup(4)), space());

    assert_eq!(
        row_names(&state),
        [
            "# in progress (2)",
            "  b-change",
            "  c-open",
            "",
            "# ready to archive (1)",
            "",
            "# archived (2)",
        ],
        "folded, a band still says how many it holds"
    );
}

/// With nothing in flight, what was put away is still the changes' to
/// show: the surface opens on it rather than on an empty list.
#[test]
fn nothing_in_flight_opens_on_what_was_put_away() {
    let dir = TempDir::new("spec-nothing-in-flight");
    write(dir.path(), "openspec/specs/x/spec.md", "# x\n");
    write(
        dir.path(),
        "openspec/changes/archive/2026-01-01-x/proposal.md",
        "x\n",
    );
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
    assert_eq!(row_names(&state), ["# archived (1)", "  2026-01-01-x"]);
}

#[test]
fn no_layout_lists_every_tool_it_reads_in_columns() {
    let dir = TempDir::new("spec-no-layout-view");
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
    let shown = view(&state, space());

    assert!(shown.navigator.is_none());
    let Content::Message { text, hint, .. } = shown.content else {
        panic!("expected a message");
    };
    assert_eq!(text, "No specs found");
    let hint = hint.unwrap();
    let tools: Vec<&str> = hint.lines().skip(2).collect();
    assert_eq!(
        tools,
        [
            "OpenSpec      openspec/        ",
            "Spec Kit      .specify/        ",
            "Superpowers   docs/superpowers/",
            "GSD           .planning/       ",
        ],
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
    let mut state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
    handle_command(&mut state, Command::Expand, space());
    handle_command(&mut state, Command::SelectNext, space());
    handle_command(&mut state, Command::SelectNext, space());
    let place = state.place().unwrap();

    let back = {
        let mut back = SpecView::opening("~/project".to_owned()).resuming(place.clone());
        back.absorb(read_with(
            &DiskHost,
            dir.path(),
            None,
            &[OPENSPEC],
            &ArtifactSource::Undeclared,
        ));
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
    gone.absorb(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
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
    let mut state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
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
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    ));
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
        ]
    );
}

// --- Superpowers and GSD ---------------------------------------------------

/// A Superpowers project as its skills lay it out (obra/superpowers
/// `8ca22db`, 5.1.3): `brainstorming` saves
/// `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`, `writing-plans`
/// saves `docs/superpowers/plans/YYYY-MM-DD-<feature>.md` with each step a
/// checkbox under a `### Task N` heading and its code in a fence.
fn superpowers_checkout(label: &str) -> TempDir {
    let dir = TempDir::new(label);
    let root = dir.path();
    let plan = |steps: &str| {
        format!(
            "# Share Links Implementation Plan\n\n> **For agentic workers:** REQUIRED SUB-SKILL: \
             Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax \
             for tracking.\n\n**Goal:** A collection opens read-only from a link.\n\n---\n\n\
             ### Task 1: Publish\n\n**Files:**\n- Create: `src/publish.ts`\n\n\
             - [x] **Step 1: Write the failing test**\n\n```ts\n// - [ ] not a step\n```\n\n{steps}"
        )
    };
    write(
        root,
        "docs/superpowers/plans/2026-09-29-share-links.md",
        &plan("- [x] **Step 2: Run it to make sure it fails**\n- [ ] **Step 3: Implement**\n"),
    );
    write(
        root,
        "docs/superpowers/plans/2026-09-15-offline-sync.md",
        &plan("- [x] **Step 2: Run it to make sure it fails**\n- [x] **Step 3: Implement**\n"),
    );
    write(
        root,
        "docs/superpowers/specs/2026-09-28-share-links-design.md",
        "# Share Links Design\n\n## Approach\n",
    );
    write(
        root,
        "docs/superpowers/specs/2026-09-14-offline-sync-design.md",
        "# Offline Sync Design\n\n## Approach\n",
    );
    dir
}

/// A GSD project as its templates lay it out (gsd-build/get-shit-done
/// `bdcaab2`): the project documents at the root of `.planning/`, every
/// file in `phases/XX-name/` prefixed with the phase number, plans as
/// `{phase}-{plan}-PLAN.md` of `<task>` blocks with the `SUMMARY.md` the
/// executor writes beside each, `quick/YYMMDD-xxx-slug/` for ad-hoc work,
/// and `complete-milestone`'s copies and moved phases in `milestones/`.
fn gsd_checkout(label: &str) -> TempDir {
    let dir = TempDir::new(label);
    let root = dir.path();
    let plan = "---\nphase: 03-sync-engine\nplan: 01\ntype: execute\nwave: 1\n---\n\n\
                <objective>\nChange log.\n</objective>\n\n<tasks>\n<task type=\"auto\">\n  \
                <name>Task 1: Record every write</name>\n</task>\n</tasks>\n";
    let summary = "---\nphase: 03-sync-engine\nplan: 01\n---\n\n# Phase 3 Plan 1: Summary\n";
    write(
        root,
        ".planning/config.json",
        "{\"mode\": \"interactive\"}\n",
    );
    write(
        root,
        ".planning/PROJECT.md",
        "# Linkshelf\n\n## Core Value\n",
    );
    write(
        root,
        ".planning/REQUIREMENTS.md",
        "# Requirements: Linkshelf\n\n- [ ] **SYNC-01**: Offline saves sync\n",
    );
    write(
        root,
        ".planning/ROADMAP.md",
        "# Roadmap: Linkshelf\n\n- [x] **Phase 2: Import**\n- [ ] **Phase 3: Sync Engine**\n",
    );
    write(root, ".planning/STATE.md", "# Project State\n");
    write(
        root,
        ".planning/research/SUMMARY.md",
        "# Research Summary\n",
    );
    let phase = ".planning/phases/03-sync-engine";
    write(
        root,
        &format!("{phase}/03-CONTEXT.md"),
        "# Phase 3: Sync Engine - Context\n",
    );
    write(
        root,
        &format!("{phase}/03-RESEARCH.md"),
        "# Phase 3: Sync Engine - Research\n",
    );
    write(root, &format!("{phase}/03-01-PLAN.md"), plan);
    write(root, &format!("{phase}/03-01-SUMMARY.md"), summary);
    write(root, &format!("{phase}/03-02-PLAN.md"), plan);
    let phase = ".planning/phases/02-import";
    write(root, &format!("{phase}/02-01-PLAN.md"), plan);
    write(root, &format!("{phase}/02-01-SUMMARY.md"), summary);
    write(
        root,
        &format!("{phase}/02-VERIFICATION.md"),
        "---\nstatus: passed\n---\n",
    );
    write(
        root,
        &format!("{phase}/02-UAT.md"),
        "---\nstatus: complete\n---\n",
    );
    let phase = ".planning/phases/04-sharing";
    write(
        root,
        &format!("{phase}/04-SPEC.md"),
        "# Phase 4: Sharing - Specification\n",
    );
    write(
        root,
        &format!("{phase}/04-CONTEXT.md"),
        "# Phase 4: Sharing - Context\n",
    );
    write(
        root,
        &format!("{phase}/04-UI-SPEC.md"),
        "# Phase 4 - UI Design Contract\n",
    );
    write(
        root,
        &format!("{phase}/04-DISCUSSION-LOG.md"),
        "# Discussion Log\n",
    );
    let quick = ".planning/quick/260928-k3p-fix-favicon-cache";
    write(root, &format!("{quick}/260928-k3p-PLAN.md"), plan);
    write(root, &format!("{quick}/260928-k3p-SUMMARY.md"), summary);
    write(
        root,
        ".planning/milestones/v1.0-ROADMAP.md",
        "# Milestone v1.0: MVP\n",
    );
    write(
        root,
        ".planning/milestones/v1.0-REQUIREMENTS.md",
        "# Requirements Archive\n",
    );
    let archived = ".planning/milestones/v1.0-phases/01-foundation";
    write(
        root,
        &format!("{archived}/01-CONTEXT.md"),
        "# Phase 1 - Context\n",
    );
    write(root, &format!("{archived}/01-01-PLAN.md"), plan);
    write(root, &format!("{archived}/01-01-SUMMARY.md"), summary);
    write(root, &format!("{archived}/01-02-PLAN.md"), plan);
    dir
}

fn listed(unit: &Unit) -> Vec<(Role, &str)> {
    unit.artifacts
        .iter()
        .map(|artifact| (artifact.role, artifact.name.as_str()))
        .collect()
}

fn unit<'a>(answer: &'a SpecAnswer, subject: Subject, name: &str) -> &'a Unit {
    units(answer)
        .iter()
        .find(|unit| unit.subject == subject && unit.name == name)
        .unwrap_or_else(|| panic!("no {name} among {subject:?}"))
}

fn done_of(unit: &Unit) -> Option<(usize, usize)> {
    unit.progress
        .map(|progress| (progress.done, progress.total))
}

#[test]
fn a_superpowers_checkout_lists_each_plan_as_a_change_and_each_design_as_a_spec() {
    let dir = superpowers_checkout("spec-superpowers");
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        SHIPPED,
        &ArtifactSource::Undeclared,
    );

    let Found::Units { dialects, .. } = &answer.found else {
        panic!("expected Superpowers to be detected");
    };
    assert_eq!(dialects, &["Superpowers"]);
    assert_eq!(answer.subjects, [Subject::Changes, Subject::Specs]);
    assert_eq!(
        names(units(&answer), Subject::Changes),
        ["2026-09-29-share-links", "2026-09-15-offline-sync"],
        "dated, so newest first"
    );
    assert_eq!(
        names(units(&answer), Subject::Specs),
        [
            "2026-09-28-share-links-design",
            "2026-09-14-offline-sync-design"
        ]
    );

    let plan = unit(&answer, Subject::Changes, "2026-09-29-share-links");
    assert_eq!(
        plan.relative,
        "docs/superpowers/plans/2026-09-29-share-links.md"
    );
    assert_eq!(listed(plan), [(Role::Steps, "2026-09-29-share-links")]);
    assert_eq!(
        done_of(plan),
        Some((2, 3)),
        "the steps' checkboxes, never the one in a fenced example"
    );
    let finished = unit(&answer, Subject::Changes, "2026-09-15-offline-sync");
    assert_eq!(done_of(finished), Some((3, 3)));

    let design = unit(&answer, Subject::Specs, "2026-09-28-share-links-design");
    assert_eq!(
        listed(design),
        [(Role::How, "2026-09-28-share-links-design")]
    );
}

#[test]
fn a_finished_superpowers_plan_is_done_where_it_was_written() {
    let dir = superpowers_checkout("spec-superpowers-bands");
    let state = opened(read_with(
        &DiskHost,
        dir.path(),
        None,
        SHIPPED,
        &ArtifactSource::Undeclared,
    ));
    assert_eq!(
        row_names(&state),
        [
            "# in progress (1)",
            "  2026-09-29-share-links",
            "",
            "# done (1)",
        ],
        "Superpowers puts nothing away, so a finished plan is done, folded"
    );
}

#[test]
fn a_gsd_checkout_lists_phases_and_quick_tasks_the_project_documents_and_milestones() {
    let dir = gsd_checkout("spec-gsd");
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        SHIPPED,
        &ArtifactSource::Undeclared,
    );

    let Found::Units { dialects, .. } = &answer.found else {
        panic!("expected GSD to be detected");
    };
    assert_eq!(dialects, &["GSD"]);
    assert_eq!(
        answer.subjects,
        [Subject::Changes, Subject::Specs],
        "what was put away is a band of the changes, not a tab"
    );
    assert_eq!(
        names(units(&answer), Subject::Changes),
        [
            "02-import",
            "03-sync-engine",
            "04-sharing",
            "260928-k3p-fix-favicon-cache"
        ]
    );
    assert_eq!(
        names(units(&answer), Subject::Specs),
        ["PROJECT", "REQUIREMENTS", "ROADMAP", "STATE"],
        "config.json and research/ are the tool's, not documents of intent"
    );
    assert_eq!(
        names(units(&answer), Subject::Archive),
        ["v1.0-phases", "v1.0-ROADMAP", "v1.0-REQUIREMENTS"]
    );
}

#[test]
fn a_gsd_phase_reads_its_intent_then_research_then_plans_then_what_happened() {
    let dir = gsd_checkout("spec-gsd-roles");
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        SHIPPED,
        &ArtifactSource::Undeclared,
    );

    assert_eq!(
        listed(unit(&answer, Subject::Changes, "03-sync-engine")),
        [
            (Role::Why, "03-CONTEXT"),
            (Role::How, "03-RESEARCH"),
            (Role::Steps, "03-01-PLAN"),
            (Role::Steps, "03-02-PLAN"),
            (Role::Other, "03-01-SUMMARY"),
        ]
    );
    assert_eq!(
        listed(unit(&answer, Subject::Changes, "04-sharing")),
        [
            (Role::Why, "04-SPEC"),
            (Role::Why, "04-CONTEXT"),
            (Role::How, "04-UI-SPEC"),
            (Role::Other, "04-DISCUSSION-LOG"),
        ],
        "the UI contract ends in SPEC.md and is still a how"
    );
    assert_eq!(
        listed(unit(&answer, Subject::Archive, "v1.0-phases"))[..3],
        [
            (Role::Why, "01-foundation/01-CONTEXT"),
            (Role::Steps, "01-foundation/01-01-PLAN"),
            (Role::Steps, "01-foundation/01-02-PLAN"),
        ]
    );
    assert_eq!(
        listed(unit(&answer, Subject::Specs, "REQUIREMENTS")),
        [(Role::Contract, "REQUIREMENTS")]
    );
}

#[test]
fn a_gsd_plan_is_done_once_its_summary_is_written() {
    let dir = gsd_checkout("spec-gsd-progress");
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        SHIPPED,
        &ArtifactSource::Undeclared,
    );

    assert_eq!(
        done_of(unit(&answer, Subject::Changes, "03-sync-engine")),
        Some((1, 2))
    );
    assert_eq!(
        done_of(unit(&answer, Subject::Changes, "02-import")),
        Some((1, 1))
    );
    assert_eq!(
        done_of(unit(
            &answer,
            Subject::Changes,
            "260928-k3p-fix-favicon-cache"
        )),
        Some((1, 1))
    );
    assert_eq!(
        done_of(unit(&answer, Subject::Changes, "04-sharing")),
        None,
        "a phase not yet planned has no steps to count"
    );
}

#[test]
fn a_checkout_is_read_by_every_tool_whose_marker_it_holds() {
    let dir = gsd_checkout("spec-gsd-and-superpowers");
    write(
        dir.path(),
        "docs/superpowers/plans/2026-09-29-share-links.md",
        "- [ ] one\n",
    );
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        &[GSD, SUPERPOWERS],
        &ArtifactSource::Undeclared,
    );
    let Found::Units { dialects, .. } = &answer.found else {
        panic!("expected both to be detected");
    };
    assert_eq!(dialects, &["GSD", "Superpowers"]);
}

#[test]
fn the_change_on_show_is_named_only_while_a_change_is_on_show() {
    let dir = openspec_checkout("spec-change-on-show");
    let state = opened_at(
        read_with(
            &DiskHost,
            dir.path(),
            None,
            &[OPENSPEC],
            &ArtifactSource::Undeclared,
        ),
        SpecPlace::change("c-open"),
    );
    assert_eq!(state.change_on_show().as_deref(), Some("c-open"));

    let state = opened_at(
        read_with(
            &DiskHost,
            dir.path(),
            None,
            &[OPENSPEC],
            &ArtifactSource::Undeclared,
        ),
        SpecPlace {
            subject: Subject::Archive,
            unit: "2026-09-01-old".to_owned(),
            artifact: None,
        },
    );
    assert_eq!(
        state.change_on_show(),
        None,
        "an archived change is not in flight"
    );
}

// --- decisions ----------------------------------------------------------

const NYGARD: &str = "# Keep one lock\n\nStatus: Accepted\n\n## Context\n\nx\n\n## Decision\n\ny\n";

fn declared(root: &Path, places: &[&str]) -> ArtifactSource {
    ArtifactSource::Directories {
        roots: places
            .iter()
            .map(|place| ArtifactRoot {
                path: root.join(place),
                declared: (*place).to_owned(),
            })
            .collect(),
        project: root.to_path_buf(),
    }
}

/// Decisions are no tool's, so a checkout with no tool at all still has
/// them, found by their shape wherever the project declared its places.
#[test]
fn decisions_are_found_by_their_shape_in_the_declared_places() {
    let dir = TempDir::new("spec-decisions");
    let root = dir.path();
    write(
        root,
        "docs/adr/001-keep-one-lock.md",
        "# 1. Keep one lock\n\nDate: 2026-01-02\n\n## Status\n\n\
         Superceded by [2. Split the lock](002-split-the-lock.md)\n\n## Context\n\nx\n",
    );
    write(
        root,
        "docs/adr/002-split-the-lock.md",
        "# 2. Split the lock\n\nDate: 2026-02-03\n\n## Status\n\nAccepted\n\n\
         Supercedes [1. Keep one lock](001-keep-one-lock.md)\n\n## Context\n\nz\n",
    );
    write(root, "docs/adr/README.md", "# Decisions\n\nAn index.\n");
    write(root, "docs/guide.md", "# Guide\n\nHow to.\n");

    let answer = read_with(&DiskHost, root, None, SHIPPED, &declared(root, &["docs"]));
    assert_eq!(answer.subjects, vec![Subject::Decisions]);
    assert_eq!(
        names(units(&answer), Subject::Decisions),
        vec!["001 Keep one lock", "002 Split the lock"],
        "a guide and an index are passed over"
    );

    let replaced = unit(&answer, Subject::Decisions, "001 Keep one lock")
        .standing
        .clone()
        .unwrap();
    assert!(replaced.superseded, "a later record took its place");
    assert_eq!(replaced.status.as_deref(), Some("superseded"));
    assert_eq!(replaced.notes, vec!["Superseded by 002 Split the lock"]);
    let replacing = unit(&answer, Subject::Decisions, "002 Split the lock")
        .standing
        .clone()
        .unwrap();
    assert_eq!(
        replacing.notes,
        vec!["Supersedes 001 Keep one lock"],
        "adr-tools' own `Supercedes` link is the same pair, said once"
    );
}

#[test]
fn nothing_declared_offers_no_decisions_even_where_they_exist() {
    let dir = TempDir::new("spec-decisions-undeclared");
    write(dir.path(), "docs/adr/001-keep-one-lock.md", NYGARD);
    let answer = read_with(
        &DiskHost,
        dir.path(),
        None,
        SHIPPED,
        &ArtifactSource::Undeclared,
    );
    assert_eq!(answer.found, Found::NoLayout);
}

#[test]
fn a_change_carrying_its_decision_lists_it_after_the_design() {
    let dir = TempDir::new("spec-decisions-in-a-change");
    let root = dir.path();
    write(root, "openspec/changes/x/proposal.md", "## Why\n");
    write(root, "openspec/changes/x/design.md", "## Context\n");
    write(root, "openspec/changes/x/tasks.md", "- [ ] one\n");
    write(root, "openspec/changes/x/adr/017-lock.md", NYGARD);

    let answer = read_with(
        &DiskHost,
        root,
        None,
        &[OPENSPEC],
        &ArtifactSource::Undeclared,
    );
    assert_eq!(
        listed(unit(&answer, Subject::Changes, "x")),
        vec![
            (Role::Why, "proposal"),
            (Role::How, "design"),
            (Role::Decision, "Keep one lock"),
            (Role::Steps, "tasks"),
        ]
    );
}

#[test]
fn a_decisions_relations_are_said_above_it_and_its_status_beside_it() {
    let dir = TempDir::new("spec-decisions-shown");
    let root = dir.path();
    write(root, "docs/adr/001-keep-one-lock.md", NYGARD);
    write(
        root,
        "docs/adr/002-split-the-lock.md",
        "# Split the lock\n\nStatus: Proposed, see [001](001-keep-one-lock.md)\n\n\
         ## Decision\n\nz\n",
    );
    let mut state = opened(read_with(
        &DiskHost,
        root,
        None,
        SHIPPED,
        &declared(root, &["docs"]),
    ));
    let rendered = view(&state, space());
    let Some(navigator) = rendered.navigator else {
        panic!("decisions are listed");
    };
    let markers: Vec<String> = navigator
        .rows
        .iter()
        .filter_map(|row| match row {
            NavigatorRow::Item { marker, .. } => Some(marker.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        markers,
        vec!["accepted", "proposed"],
        "a link says nothing about which record replaced which"
    );

    handle_command(&mut state, Command::SelectNext, space());
    let shown = text(&state, 0..200).join("\n");
    assert!(shown.contains("Related to 001 Keep one lock"), "{shown}");
}

/// The records this repository keeps are what the reader is for: every one
/// under `docs/adr` reads as a decision, and a partial supersession leaves
/// the older record standing, named by the newer one.
#[test]
fn this_repositorys_own_decisions_are_read() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let answer = read_with(&DiskHost, &root, None, &[], &declared(&root, &["docs/adr"]));
    let decisions: Vec<&Unit> = units(&answer)
        .iter()
        .filter(|unit| unit.subject == Subject::Decisions)
        .collect();
    let records = std::fs::read_dir(root.join("docs/adr"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(char::is_numeric)
        })
        .count();
    assert_eq!(
        decisions.len(),
        records,
        "every numbered record is a decision"
    );
    let without: Vec<&str> = decisions
        .iter()
        .filter(|unit| {
            unit.standing
                .as_ref()
                .is_none_or(|standing| standing.status.as_deref() != Some("Accepted"))
        })
        .map(|unit| unit.name.as_str())
        .collect();
    assert!(
        without.is_empty(),
        "read without an accepted status: {without:?}"
    );

    let older = decisions
        .iter()
        .find(|unit| unit.name.starts_with("019 "))
        .and_then(|unit| unit.standing.clone())
        .unwrap();
    // 054 writes `Supersedes in part: [019](…)` on a line of its own: a
    // field this repository added, which no template defines, so the
    // reader leaves it to the text rather than guessing what it means.
    assert!(!older.superseded);
    assert!(older.notes.is_empty(), "{:?}", older.notes);
}

/// The surface never offers more than three tabs, whatever a checkout
/// carries: every shipped tool's marker at once, and decisions beside them.
#[test]
fn no_checkout_offers_more_than_three_subjects() {
    let dir = TempDir::new("spec-at-most-three");
    let root = dir.path();
    write(root, "openspec/changes/a/proposal.md", "## Why\n");
    write(root, "openspec/specs/x/spec.md", "# x\n");
    write(
        root,
        "openspec/changes/archive/2026-01-01-b/proposal.md",
        "x\n",
    );
    write(root, ".specify/memory/constitution.md", "# c\n");
    write(root, "specs/001-f/spec.md", "# f\n");
    write(root, "docs/superpowers/plans/2026-01-02-p.md", "- [ ] s\n");
    write(
        root,
        "docs/superpowers/specs/2026-01-02-d-design.md",
        "# d\n",
    );
    write(root, ".planning/PROJECT.md", "# p\n");
    write(root, ".planning/phases/01-a/01-01-PLAN.md", "# p\n");
    write(root, ".planning/milestones/v1/01-01-PLAN.md", "# p\n");
    write(root, "docs/adr/0001-keep-one-lock.md", NYGARD);

    let answer = read_with(
        &DiskHost,
        root,
        None,
        SHIPPED,
        &declared(root, &["docs/adr"]),
    );
    assert_eq!(
        answer.subjects,
        [Subject::Changes, Subject::Specs, Subject::Decisions]
    );
    assert!(
        Subject::ALL
            .iter()
            .filter(|s| **s != Subject::Archive)
            .count()
            <= 3
    );
}
