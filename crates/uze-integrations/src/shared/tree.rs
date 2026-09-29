//! Mirroring a Store package into a generated envelope.
//!
//! A harness that installs a plugin from a directory marketplace copies it
//! into its own cache, and that copy does not follow links: an envelope made
//! of links into the Store arrived there hollow. An envelope is therefore
//! made of real files, and the Store is never linked from one — so a harness
//! writing into its copy can never write into the Store either.

use std::{fs, path::Path};

use uze_core::{Result, UzeError};

/// Mirrors `source` into `destination` as real files and directories,
/// leaving out every path in `excluded` (relative to `source`, naming a file
/// or a whole directory). `package_root` is the canonical root the mirrored
/// tree belongs to: a symlink inside it is resolved to the bytes it names
/// when it stays inside that root; a symlinked directory is left out,
/// exactly as package discovery never descends into one (the containment
/// invariant); and a link that escapes the package or dangles is refused by
/// name rather than silently dropped — a silent drop is the failure this
/// mirror exists to prevent. Written in one deterministic order.
pub(crate) fn mirror_tree(
    source: &Path,
    destination: &Path,
    package_root: &Path,
    excluded: &[&str],
) -> Result<()> {
    mirror_dir(source, source, destination, package_root, excluded)
}

fn mirror_dir(
    base: &Path,
    source: &Path,
    destination: &Path,
    package_root: &Path,
    excluded: &[&str],
) -> Result<()> {
    fs::create_dir_all(destination).map_err(|error| UzeError::Write {
        path: destination.to_path_buf(),
        source: error,
    })?;
    for entry in sorted_entries(source)? {
        let from = entry.path();
        let relative = from.strip_prefix(base).unwrap_or(&from);
        if excluded.iter().any(|path| relative == Path::new(path)) {
            continue;
        }
        let to = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&from).map_err(|error| UzeError::Read {
            path: from.clone(),
            source: error,
        })?;
        if metadata.is_dir() {
            mirror_dir(base, &from, &to, package_root, excluded)?;
        } else if metadata.is_file() {
            mirror_file(&from, &to)?;
        } else if metadata.file_type().is_symlink() {
            mirror_link(&from, &to, package_root)?;
        } else {
            return Err(UzeError::ExposureUnavailable(format!(
                "a generated envelope cannot carry special filesystem entry `{}`",
                from.display()
            )));
        }
    }
    Ok(())
}

fn mirror_link(link: &Path, destination: &Path, package_root: &Path) -> Result<()> {
    let resolved = fs::canonicalize(link).map_err(|error| UzeError::Read {
        path: link.to_path_buf(),
        source: error,
    })?;
    if !resolved.starts_with(package_root) {
        return Err(UzeError::ExposureUnavailable(format!(
            "a generated envelope refuses symlink `{}`: it resolves outside the package",
            link.display()
        )));
    }
    if resolved.is_dir() {
        return Ok(());
    }
    mirror_file(&resolved, destination)
}

/// `fs::copy` carries the permission bits, so a helper script stays
/// executable in the envelope.
fn mirror_file(source: &Path, destination: &Path) -> Result<()> {
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| UzeError::Write {
            path: destination.to_path_buf(),
            source: error,
        })
}

fn sorted_entries(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)
        .map_err(|error| UzeError::Read {
            path: dir.to_path_buf(),
            source: error,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| UzeError::Read {
            path: dir.to_path_buf(),
            source: error,
        })?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_files_leaves_out_excluded_paths_and_refuses_an_escaping_link() {
        let root = uze_testkit::temp::scratch("tree-mirror");
        let source = root.join("package");
        fs::create_dir_all(source.join("skills/probe/scripts")).unwrap();
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::create_dir_all(source.join("hooks")).unwrap();
        fs::write(source.join("skills/probe/SKILL.md"), "body").unwrap();
        fs::write(source.join("skills/probe/scripts/run.sh"), "#!/bin/sh\n").unwrap();
        fs::write(source.join("bin/tool"), "tool").unwrap();
        fs::write(source.join("hooks/hooks.json"), "{}").unwrap();
        fs::write(source.join("hooks/ensure.sh"), "script").unwrap();
        uze_core::persistence::create_symlink(
            &source.join("hooks/ensure.sh"),
            &source.join("linked.sh"),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                source.join("skills/probe/scripts/run.sh"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        let package_root = fs::canonicalize(&source).unwrap();

        let target = root.join("envelope");
        mirror_tree(
            &source,
            &target,
            &package_root,
            &["bin", "hooks/hooks.json"],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(target.join("skills/probe/SKILL.md")).unwrap(),
            "body"
        );
        assert!(!target.join("bin").exists());
        assert!(!target.join("hooks/hooks.json").exists());
        assert!(target.join("hooks/ensure.sh").is_file());
        let linked = target.join("linked.sh");
        assert!(!linked.is_symlink());
        assert_eq!(fs::read_to_string(linked).unwrap(), "script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(target.join("skills/probe/scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }

        fs::write(root.join("secret"), "outside").unwrap();
        uze_core::persistence::create_symlink(&root.join("secret"), &source.join("leak")).unwrap();
        let refused = mirror_tree(&source, &root.join("again"), &package_root, &[]);
        assert!(
            matches!(refused, Err(UzeError::ExposureUnavailable(_))),
            "a link out of the package is refused, never copied: {refused:?}"
        );
    }
}
