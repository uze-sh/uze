//! The checkout an extension may read: a Git worktree root, canonical,
//! and never a directory whose reading would reach far more than one
//! project (`extension-platform` D6).
//!
//! A dotfiles repository in `~` is a Git worktree, and so is a repository
//! someone made at `/`; `read:checkout` over either would be a grant over
//! the operator's whole machine. So the answer is refused for what it
//! would reach, not only for what Git says, and asked again on every
//! answer an extension gives, since a checkout can be moved or replaced by
//! a link while the workspace runs.

use std::path::{Path, PathBuf};

use crate::application::services::Extensions;

/// A directory an extension may read as its checkout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionCheckout {
    /// The worktree's root, canonical.
    pub root: PathBuf,
    /// The repository's Git common directory, canonical: what
    /// `read:checkout` opens beside the root.
    pub common_dir: PathBuf,
}

/// Why a directory is not a checkout an extension may read, said as the
/// sentence the operator reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotACheckout(String);

impl NotACheckout {
    pub fn reason(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NotACheckout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotACheckout {}

/// The places a checkout must keep its distance from, as this machine
/// has them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Surroundings {
    pub(crate) home: Option<PathBuf>,
    pub(crate) uze_home: PathBuf,
    pub(crate) temp: Vec<PathBuf>,
    /// Roots of volumes that are not the system's own: a Windows drive
    /// mounted into WSL at `/mnt/c`.
    pub(crate) volume_roots: Vec<PathBuf>,
}

impl Surroundings {
    fn of_this_machine(uze_home: &Path) -> Self {
        Self {
            home: uze_platform::home::user_home(),
            uze_home: uze_home.to_path_buf(),
            temp: vec![std::env::temp_dir()],
            volume_roots: uze_platform::mounts::network_mount_points().to_vec(),
        }
    }
}

impl Extensions<'_> {
    /// The checkout an extension may read for `path`: the root of the
    /// worktree it sits in, refused when that root is `/`, a volume root,
    /// the home directory or above it, anywhere above, at or below UZE's
    /// own directory, inside a temporary directory, or a bare repository.
    pub fn checkout(&self, path: &Path) -> Result<ExtensionCheckout, NotACheckout> {
        extension_checkout(path, &Surroundings::of_this_machine(self.0.home.root()))
    }
}

pub(crate) fn extension_checkout(
    path: &Path,
    around: &Surroundings,
) -> Result<ExtensionCheckout, NotACheckout> {
    let refuse = |reason: String| Err(NotACheckout(reason));
    let shown = path.display();
    let root = match uze_git::repository::root(path) {
        Ok(root) => root,
        Err(_) if is_bare(path) => {
            return refuse(format!(
                "{shown} is a bare repository, which has no files to read"
            ));
        }
        Err(_) => return refuse(format!("{shown} is not inside a Git checkout")),
    };
    let Ok(root) = uze_platform::path::canonical(&root) else {
        return refuse(format!("{} cannot be resolved", root.display()));
    };
    let within = uze_platform::path::is_within;
    let canonical =
        |place: &Path| uze_platform::path::canonical(place).unwrap_or_else(|_| place.to_path_buf());
    if root.parent().is_none()
        || around
            .volume_roots
            .iter()
            .any(|volume| uze_platform::path::same_path(&root, &canonical(volume)))
    {
        return refuse(format!(
            "{} is the root of a volume, not a checkout",
            root.display()
        ));
    }
    if let Some(home) = around.home.as_deref().map(canonical)
        && within(&home, &root)
    {
        return refuse(if uze_platform::path::same_path(&home, &root) {
            "your home directory is not a checkout".to_owned()
        } else {
            format!(
                "{} holds your home directory, and is not a checkout",
                root.display()
            )
        });
    }
    let uze_home = canonical(&around.uze_home);
    if within(&root, &uze_home) || within(&uze_home, &root) {
        return refuse(format!(
            "{} is UZE's own directory or holds it, and is not a checkout",
            root.display()
        ));
    }
    if around
        .temp
        .iter()
        .map(|temp| canonical(temp))
        .any(|temp| within(&root, &temp))
    {
        return refuse(format!(
            "{} is in a temporary directory, and is not a checkout",
            root.display()
        ));
    }
    if !root.join(".git").exists() {
        return refuse(format!(
            "{}'s repository lives outside it, as a bare repository's worktree does",
            root.display()
        ));
    }
    let common_dir = uze_git::repository::common_dir(&root)
        .ok()
        .and_then(|common| uze_platform::path::canonical(&common).ok());
    let Some(common_dir) = common_dir else {
        return refuse(format!(
            "{}'s Git directory cannot be found",
            root.display()
        ));
    };
    Ok(ExtensionCheckout { root, common_dir })
}

/// Whether `path` is, or sits inside, a repository with no worktree:
/// Git's own files laid out at its top rather than under a `.git`. Read
/// from the files, because a read through `uze-git` never discovers a
/// bare repository it was not named, and only the reason depends on it.
fn is_bare(path: &Path) -> bool {
    path.ancestors().any(|dir| {
        dir.join("HEAD").is_file() && dir.join("objects").is_dir() && dir.join("refs").is_dir()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A world whose temporary directory is elsewhere, so a scratch
    /// repository can stand for an ordinary checkout.
    fn world(label: &str) -> (PathBuf, Surroundings) {
        let root = uze_platform::path::canonical(&uze_testkit::temp::scratch(label)).unwrap();
        let around = Surroundings {
            home: Some(root.join("home")),
            uze_home: root.join("home/.uze"),
            temp: vec![root.join("tmp")],
            volume_roots: vec![root.join("mnt/c")],
        };
        for dir in ["home/.uze", "tmp", "mnt/c"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        (root, around)
    }

    fn repository(at: &Path) {
        fs::create_dir_all(at).unwrap();
        git(at, &["init", "-q"]);
    }

    fn git(at: &Path, args: &[&str]) {
        uze_testkit::git::isolated_git_in(at, args);
    }

    fn refused(path: &Path, around: &Surroundings) -> String {
        extension_checkout(path, around)
            .expect_err("refused")
            .reason()
            .to_owned()
    }

    #[test]
    fn a_repository_in_the_home_directory_is_a_checkout() {
        let (root, around) = world("checkout-ordinary");
        let project = root.join("home/src/project");
        repository(&project);
        fs::create_dir_all(project.join("docs")).unwrap();

        let checkout = extension_checkout(&project.join("docs"), &around).expect("a checkout");
        assert_eq!(checkout.root, project);
        assert_eq!(checkout.common_dir, project.join(".git"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_linked_worktree_reads_its_repository_s_common_directory() {
        let (root, around) = world("checkout-worktree");
        let project = root.join("home/project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("a.txt"), "a\n").unwrap();
        uze_testkit::git::commit_everything_in(&project);
        let linked = root.join("home/linked");
        git(
            &project,
            &["worktree", "add", "-q", linked.to_str().unwrap()],
        );

        let checkout = extension_checkout(&linked, &around).expect("a checkout");
        assert_eq!(checkout.root, linked);
        assert_eq!(checkout.common_dir, project.join(".git"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_home_directory_is_not_a_checkout() {
        let (root, around) = world("checkout-home");
        repository(&root.join("home"));
        assert_eq!(
            refused(&root.join("home"), &around),
            "your home directory is not a checkout"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_directory_above_the_home_directory_is_not_a_checkout() {
        let (root, mut around) = world("checkout-above-home");
        around.home = Some(root.join("users/me"));
        fs::create_dir_all(root.join("users/me")).unwrap();
        repository(&root.join("users"));
        let reason = refused(&root.join("users"), &around);
        assert!(reason.contains("holds your home directory"), "{reason}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uze_s_own_directory_and_what_is_below_or_above_it_are_not_checkouts() {
        let (root, mut around) = world("checkout-uze-home");
        repository(&root.join("home/.uze/store/plugin"));
        let below = refused(&root.join("home/.uze/store/plugin"), &around);
        assert!(below.contains("UZE's own directory"), "{below}");

        around.home = Some(root.join("elsewhere"));
        around.uze_home = root.join("work/uze-home");
        fs::create_dir_all(&around.uze_home).unwrap();
        repository(&root.join("work"));
        let above = refused(&root.join("work"), &around);
        assert!(above.contains("UZE's own directory or holds it"), "{above}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_repository_in_a_temporary_directory_is_not_a_checkout() {
        let (root, around) = world("checkout-temp");
        repository(&root.join("tmp/clone"));
        let reason = refused(&root.join("tmp/clone"), &around);
        assert!(reason.contains("temporary directory"), "{reason}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_volume_root_is_not_a_checkout() {
        let (root, around) = world("checkout-volume");
        repository(&root.join("mnt/c"));
        let reason = refused(&root.join("mnt/c"), &around);
        assert!(reason.contains("root of a volume"), "{reason}");
        fs::remove_dir_all(root).unwrap();
    }

    /// `/` itself, asked about directly: whether or not it is a
    /// repository on this machine, it is never a checkout.
    #[test]
    fn the_filesystem_root_is_not_a_checkout() {
        let (root, around) = world("checkout-slash");
        let slash = root.ancestors().last().unwrap().to_path_buf();
        assert!(extension_checkout(&slash, &around).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_bare_repository_is_not_a_checkout() {
        let (root, around) = world("checkout-bare");
        let bare = root.join("home/bare.git");
        fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        let reason = refused(&bare, &around);
        assert!(reason.contains("bare repository"), "{reason}");
        let inside = refused(&bare.join("refs"), &around);
        assert!(inside.contains("bare repository"), "{inside}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_plain_directory_is_not_a_checkout() {
        let (root, around) = world("checkout-plain");
        fs::create_dir_all(root.join("home/notes")).unwrap();
        let reason = refused(&root.join("home/notes"), &around);
        assert!(reason.contains("not inside a Git checkout"), "{reason}");
        fs::remove_dir_all(root).unwrap();
    }
}
