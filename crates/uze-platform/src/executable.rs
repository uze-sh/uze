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

/// The file `PATH` resolves `name` to, as a person typing it would get:
/// the first directory holding one of its [`candidates`] that runs.
pub fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|directory| candidates(&directory, name))
        .find(|candidate| is_executable(candidate))
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

/// Whether replacing the program at `path` would fail because something is
/// running it. Never on Unix, where a running image keeps the file it
/// started from. On Windows a running image cannot be replaced, and a file
/// another process holds cannot be opened for this process alone.
pub fn in_use(path: &Path) -> bool {
    imp::in_use(path)
}

/// Whether `path` is a file this platform would run.
pub fn is_executable(path: &Path) -> bool {
    imp::is_executable(path)
}

/// Whether an image's `file_name` is `program`'s, in the forms a replaced
/// image takes here: `(deleted)` after it on Linux, `.old-<pid>` on Windows
/// ([`replace_running`]), any case there.
pub fn is_image_of(file_name: &str, program: &str) -> bool {
    imp::is_image_of(file_name, program)
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

/// Whether `path` carries the mark [`make_runnable`] sets: the executable
/// bits on Unix. Windows keeps no such mark, so a file there always has it.
pub fn is_marked_runnable(path: &Path) -> bool {
    imp::is_marked_runnable(path)
}

/// Writes `bytes` to `path` as a program, through a process that has exited
/// before this returns. On Unix a file cannot be run while any descriptor
/// to it is open for writing, and a process that spawns from several
/// threads copies every descriptor it opens into each child a sibling
/// forks, until that child's own `exec` closes it: the kernel's `ETXTBSY`.
/// Written by a child of its own, the descriptor is in a process that is
/// gone by the time anything runs the file. Windows runs a file without an
/// executable bit, and an inherited handle does not keep it from starting.
pub fn install(path: &Path, bytes: &[u8]) -> io::Result<()> {
    imp::install(path, bytes)
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

/// A file of its own at `at` that runs what `program` runs and knows itself
/// by `at`'s name, wherever what runs one file is seen on another: a hard
/// link on Unix (a copy where the two filesystems differ), which costs
/// nothing and locks nothing; a copy on Windows, where a running image
/// locks its file, which every hard link to it is, so one running would
/// read as every other being in use.
pub fn place_copy(program: &Path, at: &Path) -> io::Result<()> {
    imp::place_copy(program, at)
}

/// Places `program` again at every launcher in `directory` (see
/// [`place_launcher`]) once `program` has been replaced: a launcher that
/// is a copy (Windows) would otherwise go on running the program it was
/// copied from, while one that is a link already reaches the new one. What
/// an earlier replacement set aside there is left to [`sweep_replaced`].
pub fn refresh_launchers(program: &Path, directory: &Path) -> io::Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let launcher = entry?.path();
        let set_aside = launcher
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.contains(".old-"));
        if !set_aside {
            place_launcher(program, &launcher)?;
        }
    }
    Ok(())
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

    pub(super) fn place_copy(program: &Path, at: &Path) -> io::Result<()> {
        fs::hard_link(program, at).or_else(|_| fs::copy(program, at).map(|_| ()))
    }

    pub(super) fn install(path: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::process::{Command, Stdio};
        let mut writer = Command::new("/bin/sh")
            .args(["-c", r#"cat > "$1" && chmod 0755 "$1""#, "sh"])
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()?;
        let mut stdin = writer
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("stdin"))?;
        std::io::Write::write_all(&mut stdin, bytes)?;
        drop(stdin);
        let status = writer.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "writing {}: {status}",
                path.display()
            )))
        }
    }

    pub(super) fn is_marked_runnable(path: &Path) -> bool {
        fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }

    pub(super) fn is_image_of(file_name: &str, program: &str) -> bool {
        file_name.strip_suffix(" (deleted)").unwrap_or(file_name) == program
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

    pub(super) fn in_use(_path: &Path) -> bool {
        false
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

    pub(super) fn install(path: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(path, bytes)
    }

    pub(super) fn place_copy(program: &Path, at: &Path) -> io::Result<()> {
        fs::copy(program, at).map(|_| ())
    }

    pub(super) fn is_marked_runnable(path: &Path) -> bool {
        path.is_file()
    }

    pub(super) fn is_image_of(file_name: &str, program: &str) -> bool {
        let name = file_name.to_ascii_lowercase();
        let image = super::file_name(program).to_ascii_lowercase();
        name == image || name.starts_with(&format!("{image}.old-"))
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

    /// A program is in use while it runs, and free once it has ended.
    /// A batch launcher is found by the name a person types, through
    /// `PATHEXT`, as npm's `codex.cmd` is `codex`. Windows only: Unix runs
    /// no file by an extension it does not name.
    #[cfg(test)]
    #[test]
    fn a_batch_launcher_is_found_by_its_bare_name() {
        let root = uze_testkit::temp::scratch("pathext");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("tool.cmd"), "@echo off\r\necho launched\r\n").unwrap();
        let mut environment = uze_testkit::env::scope();
        environment.set("PATH", &root);
        environment.remove("PATHEXT");
        let found = crate::executable::on_path("tool");
        drop(environment);
        assert_eq!(found, Some(root.join("tool.cmd")));
        let output = std::process::Command::new(found.unwrap()).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "launched");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(test)]
    #[test]
    fn a_running_program_is_in_use_until_it_ends() {
        let root = uze_testkit::temp::scratch("in-use");
        fs::create_dir_all(&root).unwrap();
        let system = std::env::var_os("SystemRoot")
            .map(std::path::PathBuf::from)
            .unwrap();
        let program = root.join("waiter.exe");
        fs::copy(system.join("System32").join("ping.exe"), &program).unwrap();
        let mut running = std::process::Command::new(&program)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        assert!(in_use(&program));
        running.kill().unwrap();
        running.wait().unwrap();
        assert!(!in_use(&program));
        let _ = fs::remove_dir_all(root);
    }

    /// A running image is mapped, and a mapped file refuses to be opened
    /// for writing as being used by another process: what replacing it
    /// would meet.
    pub(super) fn in_use(path: &Path) -> bool {
        const ERROR_SHARING_VIOLATION: i32 = 32;
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .is_err_and(|error| error.raw_os_error() == Some(ERROR_SHARING_VIOLATION))
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

    /// A launcher refreshed after its program was replaced runs the new
    /// program, whether it is a link to it or a copy of it.
    #[test]
    fn a_refreshed_launcher_runs_the_replacement() {
        let root = uze_testkit::temp::scratch("refresh-launchers");
        let launchers = root.join("shims");
        std::fs::create_dir_all(&launchers).unwrap();
        let program = root.join(file_name("tool"));
        std::fs::write(&program, "first").unwrap();
        let launcher = launchers.join(file_name("harness"));
        place_launcher(&program, &launcher).unwrap();

        let staged = root.join("staged");
        std::fs::write(&staged, "second, longer").unwrap();
        replace_running(&staged, &program).unwrap();
        refresh_launchers(&program, &launchers).unwrap();

        assert_eq!(
            std::fs::read_to_string(&launcher).unwrap(),
            "second, longer"
        );
        refresh_launchers(&program, &root.join("absent")).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    /// A program nobody runs can be replaced.
    #[test]
    fn a_program_nobody_runs_is_not_in_use() {
        let root = uze_testkit::temp::scratch("not-in-use");
        std::fs::create_dir_all(&root).unwrap();
        let program = root.join(file_name("idle"));
        std::fs::write(&program, "idle").unwrap();
        assert!(!in_use(&program));
        let _ = std::fs::remove_dir_all(root);
    }
}
