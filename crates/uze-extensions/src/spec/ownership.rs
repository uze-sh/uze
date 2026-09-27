//! Which paths this checkout has touched: the question behind "which
//! change is this checkout working on".
//!
//! A fact read from Git rather than a guess from names. A branch and the
//! change it carries are named by different people at different moments,
//! and rarely agree; what the branch touched is not a matter of opinion.

use std::path::Path;

use crate::Host;

/// Every path, relative to the repository root, that differs from `HEAD`
/// in the working tree, or that a commit only this branch has changed.
/// Without a target only the working tree is asked: work on the branch
/// everything is delivered to has no base to be ahead of.
///
/// "Only this branch has" excludes the target *and* the target's
/// upstream. A local target that lags its remote is the ordinary state of
/// an operator's checkout, and a branch rebased onto the remote carries
/// every commit the local target has not caught up with yet — commits
/// that are the project's, not this checkout's.
///
/// A read that fails contributes nothing rather than failing the whole:
/// a mark this surface cannot place is a mark it does not draw, and the
/// artifacts are worth showing either way.
pub fn touched(host: &dyn Host, root: &Path, target: Option<&str>) -> Vec<String> {
    let mut paths = host
        .git(
            root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            &[],
        )
        .map(|output| parse_status(&output))
        .unwrap_or_default();
    if let Some(target) = target {
        let upstream = upstream_of(host, root, target);
        let mut args = vec![
            "log",
            "--format=",
            "--name-only",
            "-z",
            "HEAD",
            "--not",
            target,
        ];
        args.extend(upstream.as_deref());
        if let Ok(output) = host.git(root, &args, &[]) {
            paths.extend(
                output
                    .split(['\0', '\n'])
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// The branch `target` follows, when it follows one.
fn upstream_of(host: &dyn Host, root: &Path, target: &str) -> Option<String> {
    let spec = format!("{target}@{{upstream}}");
    let upstream = host
        .git(root, &["rev-parse", "--symbolic-full-name", &spec], &[])
        .ok()?;
    let upstream = upstream.trim();
    (!upstream.is_empty()).then(|| upstream.to_owned())
}

/// `git status --porcelain=v1 -z`: `XY path`, and after a rename or copy
/// the path it came from as an entry of its own, which is skipped — the
/// path the file has now is the one a unit holds.
fn parse_status(output: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let Some(path) = entry.get(3..) else {
            continue;
        };
        let status = &entry[..2];
        if status.contains(['R', 'C']) {
            entries.next();
        }
        paths.push(path.to_owned());
    }
    paths
}

/// Whether `path` lies inside the unit at `unit`, or is it where the unit
/// is one file, both relative to the repository root.
pub fn lies_in(path: &str, unit: &str) -> bool {
    path.strip_prefix(unit)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::tests::DiskHost as RepositoryHost;

    #[test]
    fn a_rename_keeps_the_new_path_and_drops_the_old() {
        let output = "R  new/place.md\0old/place.md\0?? draft.md\0 M kept.md\0";
        assert_eq!(
            parse_status(output),
            ["new/place.md", "draft.md", "kept.md"]
        );
    }

    #[test]
    fn a_path_lies_in_a_unit_below_it_or_as_it() {
        assert!(lies_in("openspec/changes/x/tasks.md", "openspec/changes/x"));
        assert!(!lies_in(
            "openspec/changes/xy/tasks.md",
            "openspec/changes/x"
        ));
        // Git names files, so a path equal to a unit is a unit that is one
        // file, touched.
        assert!(lies_in(
            ".specify/memory/constitution.md",
            ".specify/memory/constitution.md"
        ));
        assert!(!lies_in(
            ".specify/memory/constitution.md.bak",
            ".specify/memory/constitution.md"
        ));
    }

    #[test]
    fn an_uncommitted_file_is_touched() {
        let repository = uze_testkit::git::Repository::new("spec-ownership-uncommitted");
        let root = repository.root().to_path_buf();
        repository.commit_file("README.md", "hello\n");
        std::fs::create_dir_all(root.join("openspec/changes/x")).unwrap();
        std::fs::write(root.join("openspec/changes/x/proposal.md"), "## Why\n").unwrap();

        assert_eq!(
            touched(&RepositoryHost, &root, None),
            ["openspec/changes/x/proposal.md"]
        );
    }

    #[test]
    fn a_file_committed_since_the_base_is_touched_on_a_clean_tree() {
        let repository = uze_testkit::git::Repository::new("spec-ownership-committed");
        let root = repository.root().to_path_buf();
        repository.commit_file("README.md", "hello\n");
        repository.git(&["branch", "base"]);
        repository.commit_file("openspec/changes/x/tasks.md", "- [ ] one\n");

        assert_eq!(
            touched(&RepositoryHost, &root, Some("base")),
            ["openspec/changes/x/tasks.md"]
        );
        assert!(
            touched(&RepositoryHost, &root, None).is_empty(),
            "without a target only the working tree is asked, and it is clean"
        );
    }

    /// The operator's `main` a commit behind `origin/main`, and this branch
    /// rebased onto `origin/main`: the commit `main` has not caught up with
    /// is the project's, not this checkout's.
    #[test]
    fn a_commit_the_targets_upstream_has_is_not_this_checkouts() {
        let repository = uze_testkit::git::Repository::new("spec-ownership-upstream");
        let root = repository.root().to_path_buf();
        let target = repository.branch();
        repository.git(&["remote", "add", "origin", &root.to_string_lossy()]);
        repository.commit_file("openspec/changes/theirs/tasks.md", "- [ ] one\n");
        repository.git(&[
            "update-ref",
            &format!("refs/remotes/origin/{target}"),
            "HEAD",
        ]);
        repository.git(&["reset", "--quiet", "--hard", "HEAD~1"]);
        repository.git(&["branch", &format!("--set-upstream-to=origin/{target}")]);
        repository.git(&[
            "checkout",
            "--quiet",
            "-b",
            "work",
            &format!("origin/{target}"),
        ]);
        repository.commit_file("openspec/changes/mine/tasks.md", "- [ ] one\n");

        assert_eq!(
            touched(&RepositoryHost, &root, Some(&target)),
            ["openspec/changes/mine/tasks.md"]
        );
    }

    #[test]
    fn the_branch_everything_is_delivered_to_touches_nothing_it_committed() {
        let repository = uze_testkit::git::Repository::new("spec-ownership-target");
        let root = repository.root().to_path_buf();
        repository.commit_file("openspec/changes/x/tasks.md", "- [ ] one\n");
        let branch = crate::shared::checkout::branch_of(&RepositoryHost, &root);

        assert!(touched(&RepositoryHost, &root, Some(&branch)).is_empty());
    }
}
