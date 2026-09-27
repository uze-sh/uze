//! Which paths this checkout has touched: the question behind "which
//! change is this checkout working on".
//!
//! A fact read from Git rather than a guess from names. A branch and the
//! change it carries are named by different people at different moments,
//! and rarely agree; what the branch touched is not a matter of opinion.

use std::path::Path;

use crate::Host;

/// Every path, relative to the repository root, that differs from `HEAD`
/// in the working tree, or that a commit on this branch changed since it
/// left `target`. Without a target only the working tree is asked: work
/// on the branch everything is delivered to has no base to be ahead of.
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
    if let Some(base) = target.and_then(|target| merge_base(host, root, target))
        && let Ok(output) = host.git(root, &["diff", "--name-only", "-z", &base, "HEAD"], &[])
    {
        paths.extend(
            output
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(str::to_owned),
        );
    }
    paths.sort();
    paths.dedup();
    paths
}

fn merge_base(host: &dyn Host, root: &Path, target: &str) -> Option<String> {
    let base = host.git(root, &["merge-base", "HEAD", target], &[]).ok()?;
    let base = base.trim();
    (!base.is_empty()).then(|| base.to_owned())
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

/// Whether `path` lies inside the unit at `unit`, both relative to the
/// repository root.
pub fn lies_in(path: &str, unit: &str) -> bool {
    path.strip_prefix(unit)
        .is_some_and(|rest| rest.starts_with('/'))
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
    fn a_path_lies_in_a_unit_only_below_it() {
        assert!(lies_in("openspec/changes/x/tasks.md", "openspec/changes/x"));
        assert!(!lies_in(
            "openspec/changes/xy/tasks.md",
            "openspec/changes/x"
        ));
        assert!(!lies_in("openspec/changes/x", "openspec/changes/x"));
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

    #[test]
    fn the_branch_everything_is_delivered_to_touches_nothing_it_committed() {
        let repository = uze_testkit::git::Repository::new("spec-ownership-target");
        let root = repository.root().to_path_buf();
        repository.commit_file("openspec/changes/x/tasks.md", "- [ ] one\n");
        let branch = crate::shared::checkout::branch_of(&RepositoryHost, &root);

        assert!(touched(&RepositoryHost, &root, Some(&branch)).is_empty());
    }
}
