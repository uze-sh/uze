//! The shelf against real Git: what it keeps, what it leaves, and how it
//! comes back.

use super::shelf::*;
use super::*;
use uze_testkit::git::Repository;

const TASK: &str = "t1";

/// A repository with a linked checkout on a branch of its own, as a slot
/// has one.
fn slot(label: &str) -> (Repository, PathBuf, String) {
    let repository = Repository::new(label);
    repository.commit_file(".gitignore", "target/\n");
    repository.commit_file("kept.rs", "fn kept() {}\n");
    let branch = format!("agent/{label}");
    let path = repository.root().join(".worktrees").join(label);
    repository.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        &branch,
        path.to_str().unwrap(),
        "HEAD",
    ]);
    (repository, path, branch)
}

fn shelved(repository: &Repository, slot: &Path, branch: &str) -> String {
    match shelve(slot, TASK, "label", branch).unwrap() {
        Shelving::Kept(commit) => {
            assert_eq!(
                shelf_of(repository.root(), TASK).as_deref(),
                Some(commit.as_str())
            );
            commit
        }
        Shelving::Nothing => panic!("expected work to be shelved"),
    }
}

fn show(repository: &Repository, commit: &str, file: &str) -> Option<String> {
    repository
        .try_git_in(repository.root(), &["show", &format!("{commit}:{file}")])
        .ok()
}

#[test]
fn tracked_deleted_and_new_files_are_kept_and_ignored_ones_are_not() {
    let (repository, slot, branch) = slot("shelf-kinds");
    let tip = tip_of(&slot, "HEAD");
    fs::write(slot.join("README.md"), "changed\n").unwrap();
    fs::remove_file(slot.join("kept.rs")).unwrap();
    fs::write(slot.join("new.rs"), "fn new() {}\n").unwrap();
    fs::create_dir_all(slot.join("target")).unwrap();
    fs::write(slot.join("target/out"), "built").unwrap();
    repository.git_in(&slot, &["add", "README.md"]);

    let commit = shelved(&repository, &slot, &branch);

    assert_eq!(
        show(&repository, &commit, "README.md").as_deref(),
        Some("changed")
    );
    assert_eq!(
        show(&repository, &commit, "kept.rs"),
        None,
        "the deletion is kept"
    );
    assert_eq!(
        show(&repository, &commit, "new.rs").as_deref(),
        Some("fn new() {}")
    );
    assert_eq!(
        show(&repository, &commit, "target/out"),
        None,
        "ignored output is not work"
    );
    assert_eq!(tip_of(&slot, &branch), tip, "the branch did not move");
    assert_eq!(
        tip_of(&slot, &format!("{commit}^1")),
        tip,
        "cut from the checkout's HEAD"
    );
    assert!(
        repository.git(&["stash", "list"]).trim().is_empty(),
        "the shared stash was not used"
    );
    assert!(
        repository
            .git_in(&slot, &["diff", "--cached", "--name-only"])
            .contains("README.md"),
        "the checkout's own index is as it was"
    );
}

#[test]
fn a_clean_checkout_on_its_branch_has_nothing_to_shelve() {
    let (_, slot, branch) = slot("shelf-nothing");
    assert_eq!(
        shelve(&slot, TASK, "label", &branch).unwrap(),
        Shelving::Nothing
    );
}

#[test]
fn a_managed_region_alone_is_not_shelved_and_a_hand_edit_is() {
    let (repository, slot, branch) = slot("shelf-derived");
    let committed =
        "# Project\n\n<!-- uze:begin project:context -->\nold\n<!-- uze:end project:context -->\n";
    fs::write(slot.join("AGENTS.md"), committed).unwrap();
    repository.git_in(&slot, &["add", "AGENTS.md"]);
    repository.git_in(&slot, &["commit", "-qm", "instructions"]);
    fs::write(
        slot.join("AGENTS.md"),
        committed.replace("old", "reprojected"),
    )
    .unwrap();

    assert_eq!(
        shelve(&slot, TASK, "label", &branch).unwrap(),
        Shelving::Nothing
    );

    fs::write(
        slot.join("AGENTS.md"),
        committed.replace("# Project", "# Project, by hand"),
    )
    .unwrap();
    let commit = shelved(&repository, &slot, &branch);
    assert!(
        show(&repository, &commit, "AGENTS.md")
            .unwrap()
            .starts_with("# Project, by hand")
    );
}

#[test]
fn a_new_shelf_keeps_the_earlier_one_as_its_second_parent() {
    let (repository, slot, branch) = slot("shelf-chain");
    fs::write(slot.join("first.rs"), "first").unwrap();
    let first = shelved(&repository, &slot, &branch);
    fs::write(slot.join("second.rs"), "second").unwrap();

    let second = shelved(&repository, &slot, &branch);

    assert_eq!(
        tip_of(&slot, &format!("{second}^2")),
        first,
        "nothing was dropped"
    );
}

#[test]
fn a_shelf_comes_back_unstaged_on_the_commit_it_was_cut_from() {
    let (repository, slot, branch) = slot("shelf-restore");
    fs::write(slot.join("README.md"), "changed\n").unwrap();
    fs::write(slot.join("new.rs"), "fn new() {}\n").unwrap();
    fs::remove_file(slot.join("kept.rs")).unwrap();
    repository.git_in(&slot, &["add", "-A"]);
    let commit = shelved(&repository, &slot, &branch);
    repository.git_in(&slot, &["reset", "--quiet", "--hard"]);
    repository.git_in(&slot, &["clean", "-qfd"]);

    assert_eq!(restore(&slot, &commit).unwrap(), Restoring::Restored);

    assert_eq!(
        fs::read_to_string(slot.join("README.md")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        fs::read_to_string(slot.join("new.rs")).unwrap(),
        "fn new() {}\n"
    );
    assert!(!slot.join("kept.rs").exists());
    assert!(
        repository
            .git_in(&slot, &["diff", "--cached", "--name-only"])
            .is_empty(),
        "nothing comes back staged"
    );
}

#[test]
fn a_shelf_applies_onto_a_branch_that_moved_elsewhere() {
    let (repository, slot, branch) = slot("shelf-moved");
    fs::write(slot.join("new.rs"), "fn new() {}\n").unwrap();
    let commit = shelved(&repository, &slot, &branch);
    repository.git_in(&slot, &["clean", "-qfd"]);
    fs::write(slot.join("other.rs"), "moved on").unwrap();
    repository.git_in(&slot, &["add", "other.rs"]);
    repository.git_in(&slot, &["commit", "-qm", "the branch moved"]);

    assert_eq!(restore(&slot, &commit).unwrap(), Restoring::Restored);

    assert_eq!(
        fs::read_to_string(slot.join("new.rs")).unwrap(),
        "fn new() {}\n"
    );
    assert!(slot.join("other.rs").is_file());
}

#[test]
fn a_shelf_that_conflicts_changes_nothing_and_names_the_file() {
    let (repository, slot, branch) = slot("shelf-conflict");
    fs::write(slot.join("kept.rs"), "the shelf's line\n").unwrap();
    let commit = shelved(&repository, &slot, &branch);
    repository.git_in(&slot, &["checkout", "--quiet", "--", "kept.rs"]);
    fs::write(slot.join("kept.rs"), "the branch's line\n").unwrap();
    repository.git_in(&slot, &["commit", "-qam", "the branch moved"]);

    assert_eq!(
        restore(&slot, &commit).unwrap(),
        Restoring::Conflict(vec![PathBuf::from("kept.rs")])
    );
    assert_eq!(
        fs::read_to_string(slot.join("kept.rs")).unwrap(),
        "the branch's line\n",
        "nothing was half applied"
    );
}

#[test]
fn a_nested_repository_cannot_be_shelved() {
    let (repository, slot, branch) = slot("shelf-nested");
    let nested = slot.join("vendor");
    fs::create_dir_all(&nested).unwrap();
    repository.git_in(&nested, &["init", "--quiet"]);
    fs::write(nested.join("lib.rs"), "").unwrap();

    assert!(matches!(
        shelve(&slot, TASK, "label", &branch),
        Err(Unshelvable::Uncapturable(_))
    ));
    assert_eq!(shelf_of(repository.root(), TASK), None);
}

#[test]
fn a_repository_nested_deep_in_a_new_directory_cannot_be_shelved() {
    let (repository, slot, branch) = slot("shelf-nested-deep");
    let nested = slot.join("vendor").join("lib");
    fs::create_dir_all(&nested).unwrap();
    repository.git_in(&nested, &["init", "--quiet"]);
    fs::write(nested.join("lib.rs"), "fn lib() {}\n").unwrap();
    repository.git_in(
        &nested,
        &[
            "-c",
            "user.name=Vendor",
            "-c",
            "user.email=vendor@uze.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "vendored",
        ],
    );
    fs::write(nested.join("edited.rs"), "fn edited() {}\n").unwrap();

    assert!(
        matches!(
            shelve(&slot, TASK, "label", &branch),
            Err(Unshelvable::Uncapturable(what)) if what.contains("vendor/lib")
        ),
        "only its commit id would be kept, and its edits left in the slot"
    );
    assert_eq!(shelf_of(repository.root(), TASK), None);
}

#[test]
fn a_shelf_lists_itself_from_its_own_commit() {
    let (repository, slot, branch) = slot("shelf-list");
    fs::write(slot.join("new.rs"), "").unwrap();
    let commit = shelve(&slot, TASK, "fix/naming", &branch).unwrap();

    assert_eq!(
        list(repository.root()),
        vec![Shelf {
            task: TASK.to_owned(),
            commit: match commit {
                Shelving::Kept(commit) => commit,
                Shelving::Nothing => unreachable!(),
            },
            label: "fix/naming".to_owned(),
            branch: branch.clone(),
        }]
    );
}

#[test]
fn a_shelf_is_in_the_target_only_path_by_path() {
    let (repository, slot, branch) = slot("shelf-in-target");
    fs::write(slot.join("[x].md"), "same\n").unwrap();
    let commit = shelved(&repository, &slot, &branch);

    repository.commit_file("x.md", "same\n");
    assert!(
        !is_in_target(repository.root(), &commit, "main"),
        "a pattern-looking name is only itself"
    );

    repository.commit_file("[x].md", "same\n");
    assert!(is_in_target(repository.root(), &commit, "main"));
}

// The executable bit is a Unix mode: Windows has no such bit to set.
#[cfg(unix)]
#[test]
fn a_file_made_executable_is_not_in_a_target_that_has_it_plain() {
    use std::os::unix::fs::PermissionsExt;

    let (repository, slot, branch) = slot("shelf-mode");
    let script = slot.join("kept.rs");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let commit = shelved(&repository, &slot, &branch);

    assert!(
        !is_in_target(repository.root(), &commit, "main"),
        "the same bytes with another mode are still the agent's change"
    );
}

#[test]
fn a_taken_back_shelf_leaves_the_ref_where_it_stood() {
    let (repository, slot, branch) = slot("shelf-unshelve");
    fs::write(slot.join("first.rs"), "").unwrap();
    let first = shelved(&repository, &slot, &branch);
    fs::write(slot.join("second.rs"), "").unwrap();
    let second = shelved(&repository, &slot, &branch);

    unshelve(repository.root(), TASK, &second, Some(&first)).unwrap();
    assert_eq!(shelf_of(repository.root(), TASK), Some(first.clone()));
    unshelve(repository.root(), TASK, &first, None).unwrap();
    assert_eq!(shelf_of(repository.root(), TASK), None);
}

#[test]
fn a_dropped_shelf_is_dropped_only_while_it_is_the_one_expected() {
    let (repository, slot, branch) = slot("shelf-drop");
    fs::write(slot.join("first.rs"), "").unwrap();
    let first = shelved(&repository, &slot, &branch);
    fs::write(slot.join("second.rs"), "").unwrap();
    let second = shelved(&repository, &slot, &branch);

    assert!(
        drop_shelf(repository.root(), TASK, &first).is_err(),
        "a newer shelf is kept"
    );
    drop_shelf(repository.root(), TASK, &second).unwrap();
    assert_eq!(shelf_of(repository.root(), TASK), None);
}
