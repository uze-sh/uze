use std::fs;

use uze_core::UzeHome;
use uze_testkit::git::Repository;

use super::*;
use crate::UzeApplication;

const AUTHORED: &str = "\
# The team's agent environment.
workspace:
  worktree: always   # every agent isolated
  # merged locally until CI exists
  delivery: merge
  setup: pnpm install

marketplaces:
  # pinned until the fix lands
  ai: { path: ../ai, plugins: [flow] }
";

fn application(label: &str) -> UzeApplication {
    UzeApplication::new(UzeHome::at(uze_testkit::temp::scratch(label)), Vec::new())
}

fn project(label: &str, manifest: Option<&str>) -> Repository {
    let repository = Repository::new(label);
    repository.commit_file(".gitignore", ".env\n");
    if let Some(text) = manifest {
        fs::write(repository.root().join("agents.yaml"), text).unwrap();
    }
    repository
}

fn set(key: DeclarationKey, value: DeclaredValue) -> Edit {
    Edit::Set { key, value }
}

fn text(value: &str) -> DeclaredValue {
    DeclaredValue::Text(value.to_owned())
}

fn written(repository: &Repository) -> String {
    fs::read_to_string(repository.root().join("agents.yaml")).unwrap()
}

#[test]
fn the_view_lists_every_key_in_order_with_what_is_declared_and_what_is_default() {
    let app = application("declaration-view");
    let repository = project(
        "declaration-view",
        Some(&AUTHORED.replace(
            "  setup: pnpm install\n",
            "  setup: pnpm install\n  actions:\n    on_publish: open a request\n",
        )),
    );
    let view = app.workspace().declaration_view(repository.root()).unwrap();
    assert!(view.exists);
    assert_eq!(view.editable, Ok(()));
    assert!(view.problems.is_empty(), "{:?}", view.problems);
    assert_eq!(
        view.entries
            .iter()
            .map(|entry| entry.key)
            .collect::<Vec<_>>(),
        DeclarationKey::ALL.to_vec()
    );
    let declared = |key| {
        view.entries
            .iter()
            .find(|entry| entry.key == key)
            .unwrap()
            .declared
            .clone()
    };
    assert_eq!(declared(DeclarationKey::Worktree), Some(text("always")));
    assert_eq!(
        declared(DeclarationKey::Setup),
        Some(DeclaredValue::Commands(vec!["pnpm install".into()]))
    );
    assert_eq!(declared(DeclarationKey::Slots), None);
    let declared_actions: Vec<ActionState> = view
        .actions
        .iter()
        .filter(|entry| entry.declared.is_some())
        .map(|entry| entry.state)
        .collect();
    assert_eq!(declared_actions, vec![ActionState::OnPublish]);
    assert_eq!(
        view.actions.len(),
        ActionState::ALL.len(),
        "every moment is listed"
    );
}

#[test]
fn without_a_manifest_every_key_is_its_default_and_the_file_is_editable() {
    let app = application("declaration-view-absent");
    let repository = project("declaration-view-absent", None);
    let view = app.workspace().declaration_view(repository.root()).unwrap();
    assert!(!view.exists);
    assert_eq!(view.editable, Ok(()));
    assert!(view.entries.iter().all(|entry| entry.declared.is_none()));
}

#[test]
fn a_file_using_an_anchor_is_shown_but_not_editable() {
    let app = application("declaration-anchor");
    let repository = project(
        "declaration-anchor",
        Some("workspace:\n  target: &main main\n  slots: 2\n"),
    );
    let view = app.workspace().declaration_view(repository.root()).unwrap();
    let reason = view.editable.unwrap_err();
    assert!(reason.contains("anchor"), "{reason}");
    assert!(
        app.workspace()
            .preview_declaration(
                repository.root(),
                &[set(DeclarationKey::Slots, DeclaredValue::Number(3))]
            )
            .is_err()
    );
}

#[test]
fn every_problem_is_reported_at_once_under_its_key() {
    let app = application("declaration-problems");
    let repository = project("declaration-problems", Some(AUTHORED));
    let preview = app
        .workspace()
        .preview_declaration(
            repository.root(),
            &[
                set(DeclarationKey::Slots, DeclaredValue::Number(0)),
                set(DeclarationKey::Delivery, text("teleport")),
                set(
                    DeclarationKey::Link,
                    DeclaredValue::List(vec!["../outside".to_owned(), "README.md".to_owned()]),
                ),
                set(
                    DeclarationKey::Artifacts,
                    DeclaredValue::List(vec!["/etc".to_owned()]),
                ),
            ],
        )
        .unwrap();
    let keys: Vec<_> = preview
        .problems
        .iter()
        .map(|problem| problem.key.as_deref().unwrap_or("-"))
        .collect();
    for key in ["slots", "delivery", "artifacts"] {
        assert!(
            keys.contains(&key),
            "{key} missing from {:?}",
            preview.problems
        );
    }
    assert_eq!(
        keys.iter().filter(|key| **key == "link").count(),
        2,
        "the misplaced link and the tracked one: {:?}",
        preview.problems
    );
    assert_eq!(written(&repository), AUTHORED, "a preview writes nothing");
}

#[test]
fn an_unknown_key_in_the_file_is_reported_by_name() {
    let app = application("declaration-unknown");
    let repository = project(
        "declaration-unknown",
        Some("workspace:\n  completon: pr\n  slots: 0\n"),
    );
    let view = app.workspace().declaration_view(repository.root()).unwrap();
    let keys: Vec<_> = view
        .problems
        .iter()
        .filter_map(|problem| problem.key.as_deref())
        .collect();
    assert_eq!(keys, vec!["completon", "slots"], "{:?}", view.problems);
}

#[test]
fn the_diff_shows_only_the_changed_lines_under_their_key() {
    let app = application("declaration-diff");
    let repository = project("declaration-diff", Some(AUTHORED));
    let preview = app
        .workspace()
        .preview_declaration(
            repository.root(),
            &[set(DeclarationKey::Delivery, text("push"))],
        )
        .unwrap();
    assert!(preview.problems.is_empty(), "{:?}", preview.problems);
    assert_eq!(
        preview.diff,
        vec![
            DiffLine::Context("workspace:".to_owned()),
            DiffLine::Removed("  delivery: merge".to_owned()),
            DiffLine::Added("  delivery: push".to_owned()),
        ]
    );
    assert_eq!(preview.expected.as_deref(), Some(AUTHORED));
    assert!(!preview.awaits_approval_again);
}

#[test]
fn saving_keeps_every_comment_and_changes_only_the_edited_keys() {
    let app = application("declaration-save");
    let repository = project("declaration-save", Some(AUTHORED));
    let edits = [
        set(DeclarationKey::Delivery, text("push")),
        set(
            DeclarationKey::Gate,
            DeclaredValue::Commands(vec!["cargo test".into(), "cargo clippy".into()]),
        ),
        Edit::Unset {
            key: DeclarationKey::Worktree,
        },
    ];
    let preview = app
        .workspace()
        .preview_declaration(repository.root(), &edits)
        .unwrap();
    let created = app
        .workspace()
        .save_declaration(repository.root(), &edits, preview.expected.as_deref())
        .unwrap();
    assert!(!created);
    let after = written(&repository);
    for comment in [
        "# The team's agent environment.",
        "# merged locally until CI exists",
        "# pinned until the fix lands",
    ] {
        assert!(after.contains(comment), "{comment} lost:\n{after}");
    }
    assert!(after.contains("delivery: push"), "{after}");
    assert!(!after.contains("worktree:"), "{after}");
    let policy = uze_workspace::declaration::declared(repository.root())
        .unwrap()
        .unwrap();
    assert_eq!(policy.gate.len(), 2);
}

#[test]
fn a_file_changed_since_the_preview_is_refused_and_left_as_it_is() {
    let app = application("declaration-changed");
    let repository = project("declaration-changed", Some(AUTHORED));
    let edits = [set(DeclarationKey::Slots, DeclaredValue::Number(4))];
    let preview = app
        .workspace()
        .preview_declaration(repository.root(), &edits)
        .unwrap();
    let theirs = format!("{AUTHORED}# a teammate's note\n");
    fs::write(repository.root().join("agents.yaml"), &theirs).unwrap();
    let refused =
        app.workspace()
            .save_declaration(repository.root(), &edits, preview.expected.as_deref());
    assert!(
        matches!(refused, Err(UzeError::ManifestChangedSince { .. })),
        "{refused:?}"
    );
    assert_eq!(written(&repository), theirs);
}

#[test]
fn a_project_without_a_manifest_gets_one_holding_only_the_keys_set() {
    let app = application("declaration-create");
    let repository = project("declaration-create", None);
    let edits = [
        set(DeclarationKey::Worktree, text("always")),
        set(DeclarationKey::Slots, DeclaredValue::Number(3)),
    ];
    let preview = app
        .workspace()
        .preview_declaration(repository.root(), &edits)
        .unwrap();
    assert_eq!(preview.expected, None);
    assert!(
        preview
            .diff
            .iter()
            .all(|line| matches!(line, DiffLine::Added(_))),
        "{:?}",
        preview.diff
    );
    assert!(
        app.workspace()
            .save_declaration(repository.root(), &edits, None)
            .unwrap(),
        "the file was created"
    );
    assert_eq!(
        written(&repository),
        "workspace:\n  worktree: always\n  slots: 3\n"
    );
}

#[test]
fn changing_setup_says_the_commands_wait_for_approval_again() {
    let app = application("declaration-approval");
    let repository = project("declaration-approval", Some(AUTHORED));
    let awaiting = app
        .workspace()
        .commands_awaiting_approval(repository.root())
        .expect("the setup awaits approval");
    app.workspace().approve_commands(&awaiting).unwrap();

    let unrelated = app
        .workspace()
        .preview_declaration(
            repository.root(),
            &[set(DeclarationKey::Slots, DeclaredValue::Number(2))],
        )
        .unwrap();
    assert!(!unrelated.awaits_approval_again);

    let changed = app
        .workspace()
        .preview_declaration(
            repository.root(),
            &[set(
                DeclarationKey::Setup,
                DeclaredValue::Commands(vec!["pnpm install --frozen-lockfile".into()]),
            )],
        )
        .unwrap();
    assert!(changed.awaits_approval_again);
}

#[test]
fn an_action_is_written_beside_the_others_and_taken_out_with_its_block() {
    let app = application("declaration-actions");
    let repository = project(
        "declaration-actions",
        Some(
            "workspace:\n  delivery: push\n  actions:\n    # what an agent is asked\n    \
             on_change: Commit your work.\n",
        ),
    );
    let save = |edits: &[Edit]| {
        let preview = app
            .workspace()
            .preview_declaration(repository.root(), edits)
            .unwrap();
        assert!(preview.problems.is_empty(), "{:?}", preview.problems);
        app.workspace()
            .save_declaration(repository.root(), edits, preview.expected.as_deref())
            .unwrap();
        written(&repository)
    };

    let after = save(&[Edit::Action {
        state: ActionState::OnFail,
        text: Some("Fix the checks.\nThen commit.".to_owned()),
    }]);
    assert!(after.contains("# what an agent is asked"), "{after}");
    assert!(after.contains("on_change: Commit your work."), "{after}");
    let view = app.workspace().declaration_view(repository.root()).unwrap();
    let on_fail = view
        .actions
        .iter()
        .find(|entry| entry.state == ActionState::OnFail)
        .unwrap();
    assert_eq!(
        on_fail.declared.as_deref(),
        Some("Fix the checks.\nThen commit."),
        "{after}"
    );

    let after = save(&[
        Edit::Action {
            state: ActionState::OnFail,
            text: None,
        },
        Edit::Action {
            state: ActionState::OnChange,
            text: None,
        },
    ]);
    assert!(!after.contains("actions"), "{after}");
    assert!(after.contains("delivery: push"), "{after}");
}

#[test]
fn an_action_starts_its_block_when_the_file_has_none() {
    let app = application("declaration-actions-new");
    let repository = project(
        "declaration-actions-new",
        Some("workspace:\n  delivery: push\n"),
    );
    let edits = [Edit::Action {
        state: ActionState::OnPublish,
        text: Some("Open a pull request.".to_owned()),
    }];
    let preview = app
        .workspace()
        .preview_declaration(repository.root(), &edits)
        .unwrap();
    app.workspace()
        .save_declaration(repository.root(), &edits, preview.expected.as_deref())
        .unwrap();
    let after = written(&repository);
    assert!(
        after.contains("actions:") && after.contains("on_publish: Open a pull request."),
        "{after}"
    );
}
