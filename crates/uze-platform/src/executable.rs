//! Where a program named on `PATH` can be, and whether a file can run.

use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};

/// The files a program called `name` could be in `dir`, in the order the
/// platform tries them: the name itself on Unix; every `PATHEXT` extension
/// on Windows, where `claude` is `claude.exe` and an npm launcher is
/// `codex.cmd`.
pub fn candidates(dir: &Path, name: &str) -> Vec<PathBuf> {
    imp::candidates(dir, name)
}

/// `command` and `arguments` as a program that starts another directly,
/// with no shell in between, has to name it: a harness launching an MCP
/// server, say. Unchanged on Unix. On Windows a batch launcher (`npx` is
/// `npx.cmd`) is no executable image to start, only something `cmd`
/// runs, so one is named through `cmd /c`. `command` is looked for in its
/// own directory when it names one, else on `PATH`.
pub fn direct_launch(command: &str, arguments: Vec<String>) -> (String, Vec<String>) {
    imp::direct_launch(command, arguments)
}

/// Whether `path` is a file this platform would run.
pub fn is_executable(path: &Path) -> bool {
    imp::is_executable(path)
}

/// `name` as an executable's file name here: `uze` or `uze.exe`.
pub fn file_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The name a program was invoked by, from its `argv[0]`, as [`file_name`]
/// would spell it without the suffix: `claude` for `C:\...\Claude.EXE`.
pub fn invoked_name(argv0: &OsStr) -> Option<String> {
    imp::invoked_name(Path::new(argv0))
}

/// Marks `path` as a program this platform may run.
pub fn make_runnable(path: &Path) -> io::Result<()> {
    imp::make_runnable(path)
}

/// Puts `new` in `target`'s place, `target` possibly being the image this
/// very process runs. Where the platform refuses to replace a running
/// image, the old one is renamed aside and removed later by
/// [`sweep_replaced`]; a failure halfway puts it back.
pub fn replace_running(new: &Path, target: &Path) -> io::Result<()> {
    imp::replace_running(new, target)
}

/// Puts a launcher for `program` at `at`, in a directory this process owns:
/// running `at` runs `program`, which sees `at`'s name as its own.
///
/// A symbolic link on Unix, repointed when it points elsewhere; anything at
/// `at` that is no link is somebody else's, an error of kind
/// `AlreadyExists`. On Windows, where a link to an executable needs a
/// privilege most accounts lack, a copy: kept while it is still `program`
/// (same length, same modification time — a copy keeps the latter),
/// otherwise replaced through [`replace_running`], since a pane may be
/// running it.
pub fn place_launcher(program: &Path, at: &Path) -> io::Result<()> {
    imp::place_launcher(program, at)
}

/// Removes what [`replace_running`] set aside beside `running`, once nothing
/// runs it. A removal that fails is tried again at a later start.
pub fn sweep_replaced(running: &Path) {
    imp::sweep_replaced(running)
}

#[cfg(unix)]
mod imp {
    use std::{
        fs, io,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    pub(super) fn invoked_name(argv0: &Path) -> Option<String> {
        Some(argv0.file_name()?.to_str()?.to_owned())
    }

    pub(super) fn make_runnable(path: &Path) -> io::Result<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }

    /// A rename over a running image is allowed: the running process keeps
    /// the inode it started from.
    pub(super) fn replace_running(new: &Path, target: &Path) -> io::Result<()> {
        fs::rename(new, target)
    }

    pub(super) fn sweep_replaced(_running: &Path) {}

    pub(super) fn place_launcher(program: &Path, at: &Path) -> io::Result<()> {
        match fs::symlink_metadata(at) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if fs::read_link(at)? == program {
                    return Ok(());
                }
                fs::remove_file(at)?;
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} is not a launcher this process placed", at.display()),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::os::unix::fs::symlink(program, at)
    }

    pub(super) fn candidates(dir: &Path, name: &str) -> Vec<PathBuf> {
        vec![dir.join(name)]
    }

    pub(super) fn direct_launch(command: &str, arguments: Vec<String>) -> (String, Vec<String>) {
        (command.to_owned(), arguments)
    }

    pub(super) fn is_executable(path: &Path) -> bool {
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
}

#[cfg(windows)]
mod imp {
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    /// A program is typed in whatever case, and runs with its suffix.
    pub(super) fn invoked_name(argv0: &Path) -> Option<String> {
        Some(argv0.file_stem()?.to_str()?.to_ascii_lowercase())
    }

    /// Whether a file runs is its name's business here.
    pub(super) fn make_runnable(_path: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Windows refuses to replace an image that is running but lets it be
    /// renamed, so the running one steps aside first.
    pub(super) fn replace_running(new: &Path, target: &Path) -> io::Result<()> {
        let aside = set_aside_name(target);
        let had_target = target.exists();
        if had_target {
            crate::fs::rename(target, &aside)?;
        }
        crate::fs::rename(new, target).inspect_err(|_| {
            if had_target {
                let _ = crate::fs::rename(&aside, target);
            }
        })
    }

    pub(super) fn sweep_replaced(running: &Path) {
        let (Some(directory), Some(name)) = (running.parent(), running.file_name()) else {
            return;
        };
        let prefix = set_aside_prefix(&name.to_string_lossy());
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(&prefix)
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    pub(super) fn place_launcher(program: &Path, at: &Path) -> io::Result<()> {
        let source = fs::metadata(program)?;
        if let Ok(placed) = fs::metadata(at)
            && placed.len() == source.len()
            && placed.modified().ok() == source.modified().ok()
        {
            return Ok(());
        }
        let mut staged = at.as_os_str().to_owned();
        staged.push(format!(".new-{}", std::process::id()));
        let staged = PathBuf::from(staged);
        fs::copy(program, &staged)?;
        replace_running(&staged, at).inspect_err(|_| {
            let _ = fs::remove_file(&staged);
        })?;
        sweep_replaced(at);
        Ok(())
    }

    fn set_aside_prefix(name: &str) -> String {
        format!("{name}.old-").to_ascii_lowercase()
    }

    fn set_aside_name(target: &Path) -> PathBuf {
        let name = target.file_name().unwrap_or_default().to_string_lossy();
        target.with_file_name(format!("{name}.old-{}", std::process::id()))
    }

    const DEFAULT_EXTENSIONS: &str = ".COM;.EXE;.BAT;.CMD";

    fn extensions() -> Vec<String> {
        std::env::var("PATHEXT")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_EXTENSIONS.to_owned())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    }

    pub(super) fn candidates(dir: &Path, name: &str) -> Vec<PathBuf> {
        if Path::new(name).extension().is_some() {
            return vec![dir.join(name)];
        }
        extensions()
            .into_iter()
            .map(|extension| dir.join(format!("{name}{extension}")))
            .collect()
    }

    pub(super) fn direct_launch(command: &str, arguments: Vec<String>) -> (String, Vec<String>) {
        let path = Path::new(command);
        let directories: Vec<PathBuf> = match path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            Some(parent) => vec![parent.to_path_buf()],
            None => std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
        };
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(command);
        let batch = directories
            .iter()
            .flat_map(|directory| candidates(directory, name))
            .find(|candidate| candidate.is_file())
            .and_then(|found| {
                found
                    .extension()
                    .map(|extension| extension.to_ascii_lowercase())
            })
            .is_some_and(|extension| extension == "cmd" || extension == "bat");
        if !batch {
            return (command.to_owned(), arguments);
        }
        let mut through_cmd = vec!["/c".to_owned(), command.to_owned()];
        through_cmd.extend(arguments);
        ("cmd".to_owned(), through_cmd)
    }

    pub(super) fn is_executable(path: &Path) -> bool {
        let runnable = path
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy().to_ascii_lowercase()))
            .is_some_and(|extension| extensions().contains(&extension));
        runnable && path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A launcher written as a batch file on Windows and a script on Unix
    /// runs when started the way a harness starts an MCP server: directly,
    /// by the name `direct_launch` gives it.
    #[test]
    fn a_launcher_started_directly_runs() {
        let directory = uze_testkit::temp::scratch("direct-launch");
        std::fs::create_dir_all(&directory).unwrap();
        uze_testkit::process::install_executable(
            &directory.join("tool"),
            b"#!/bin/sh\nexit \"$1\"\n",
        );
        std::fs::write(directory.join("tool.cmd"), "@exit /b %1\r\n").unwrap();
        let command = directory.join("tool").to_string_lossy().into_owned();
        let (program, arguments) = direct_launch(&command, vec!["5".to_owned()]);
        let status = std::process::Command::new(program)
            .args(arguments)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(5));
        let _ = std::fs::remove_dir_all(directory);
    }
}
