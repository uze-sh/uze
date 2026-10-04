//! Where this user's processes are working.
//!
//! A checkout somebody is working in must not be reset or removed, and
//! "somebody" is not only a pane UZE drew: a subagent's shell, an editor or
//! a build started from a terminal UZE never saw are all a process whose
//! working directory is inside it. The answer is read from the process
//! table, once per decision, and fails closed: when the table cannot be
//! read at all, every directory is taken to be in use.

use crate::path::Canonical as _;
use std::path::{Path, PathBuf};

/// The directories this user's processes were working in when observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Presence {
    Known(Vec<PathBuf>),
    /// The process table could not be read.
    Unknown,
}

impl Presence {
    /// Reads the process table. This process and the ones it started are
    /// left out: the `uze` deciding, and the Git it runs inside the very
    /// checkouts it is deciding about, are not somebody at work there.
    pub fn observe() -> Self {
        match working_directories() {
            Some(directories) => Self::Known(directories),
            None => Self::Unknown,
        }
    }

    /// The process table, plus directories the caller already knows
    /// somebody is in — a pane the terminal drew, before its shell is in
    /// the table.
    pub fn observe_with(known: &[PathBuf]) -> Self {
        match Self::observe() {
            Self::Known(mut directories) => {
                directories.extend_from_slice(known);
                Self::Known(directories)
            }
            Self::Unknown => Self::Unknown,
        }
    }

    /// Whether any observed process works in `directory` or below it.
    pub fn inside(&self, directory: &Path) -> bool {
        let Self::Known(directories) = self else {
            return true;
        };
        let canonical = directory
            .canonical()
            .unwrap_or_else(|_| directory.to_path_buf());
        directories
            .iter()
            .any(|working| working.starts_with(&canonical) || working.starts_with(directory))
    }
}

/// Every working directory of this user's other processes, or `None` when
/// the table could not be enumerated. A process that exits while being
/// read, or whose directory the platform withholds, is skipped: those are
/// ordinary on a desktop, and failing closed on them would hold every
/// checkout forever.
#[cfg(target_os = "linux")]
fn working_directories() -> Option<Vec<PathBuf>> {
    use std::os::unix::fs::MetadataExt;

    let own = std::process::id();
    // SAFETY: `getuid` takes no arguments, cannot fail, and touches no
    // memory of ours.
    let uid = unsafe { libc::getuid() };
    let entries = std::fs::read_dir("/proc").ok()?;
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let process = entry.path();
        if pid == own || parent_of(&process) == Some(own) {
            continue;
        }
        if std::fs::metadata(&process)
            .map(|metadata| metadata.uid())
            .ok()
            != Some(uid)
        {
            continue;
        }
        if let Ok(directory) = std::fs::read_link(process.join("cwd")) {
            directories.push(directory);
        }
    }
    Some(directories)
}

/// The parent pid in `/proc/<pid>/stat`, read after the command name,
/// which is parenthesised and may itself hold spaces and parentheses.
#[cfg(target_os = "linux")]
fn parent_of(process: &Path) -> Option<u32> {
    let stat = std::fs::read_to_string(process.join("stat")).ok()?;
    let (_, after_name) = stat.rsplit_once(')')?;
    after_name.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(target_os = "macos")]
fn working_directories() -> Option<Vec<PathBuf>> {
    use std::{
        ffi::{CStr, c_int, c_void},
        mem::size_of,
        os::unix::ffi::OsStrExt,
    };

    let own = std::process::id();
    // SAFETY: `getuid` takes no arguments, cannot fail, and touches no
    // memory of ours.
    let uid = unsafe { libc::getuid() };
    // SAFETY: a null buffer asks only for the number of pids.
    let estimate = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if estimate <= 0 {
        return None;
    }
    // Room for processes started between the two calls.
    let mut pids = vec![0 as libc::pid_t; estimate as usize + 64];
    let capacity = c_int::try_from(pids.len() * size_of::<libc::pid_t>()).ok()?;
    // SAFETY: the buffer is `capacity` bytes of pids, as the call expects.
    let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast::<c_void>(), capacity) };
    if listed <= 0 {
        return None;
    }
    pids.truncate(listed as usize);

    let mut directories = Vec::new();
    for pid in pids.into_iter().filter(|pid| *pid > 0) {
        // SAFETY: both structs are plain C data for which all-zero is valid.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let info_size = size_of::<libc::proc_bsdinfo>() as c_int;
        // SAFETY: `info` is `info_size` bytes, the size this flavor fills.
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&raw mut info).cast::<c_void>(),
                info_size,
            )
        };
        if read != info_size || info.pbi_uid != uid || info.pbi_pid == own || info.pbi_ppid == own {
            continue;
        }
        let mut paths: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
        let paths_size = size_of::<libc::proc_vnodepathinfo>() as c_int;
        // SAFETY: `paths` is `paths_size` bytes, the size this flavor fills.
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                (&raw mut paths).cast::<c_void>(),
                paths_size,
            )
        };
        if read != paths_size {
            continue;
        }
        // SAFETY: `vip_path` is a NUL-terminated C string of at most
        // MAXPATHLEN bytes, laid out as a flat array.
        let path = unsafe { CStr::from_ptr(paths.pvi_cdir.vip_path.as_ptr().cast()) };
        if !path.to_bytes().is_empty() {
            directories.push(PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes())));
        }
    }
    Some(directories)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn working_directories() -> Option<Vec<PathBuf>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_process_working_in_a_directory_is_seen_there() {
        let directory = uze_testkit::temp::scratch("process-cwd");
        std::fs::create_dir_all(&directory).unwrap();
        // Not a child of this process once `sh` has forked it off and
        // exited, which is what somebody else's process looks like.
        let mut starter = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 30 >/dev/null 2>&1 & echo $!")
            .current_dir(&directory)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut spelled = String::new();
        std::io::Read::read_to_string(starter.stdout.as_mut().unwrap(), &mut spelled).unwrap();
        starter.wait().unwrap();
        let sleeper: libc::pid_t = spelled.trim().parse().unwrap();

        let seen = Presence::observe().inside(&directory);
        // SAFETY: `sleeper` is the positive pid of the process started above.
        unsafe { libc::kill(sleeper, libc::SIGKILL) };
        assert!(seen, "a process working in the directory was not seen");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_directory_nobody_works_in_is_free() {
        let directory = uze_testkit::temp::scratch("process-cwd-empty");
        std::fs::create_dir_all(&directory).unwrap();
        if Presence::observe() != Presence::Unknown {
            assert!(!Presence::observe().inside(&directory));
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_unreadable_process_table_holds_everything() {
        assert!(Presence::Unknown.inside(Path::new("/anywhere")));
    }
}
