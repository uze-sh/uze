//! What the filesystem does differently per platform: who may enter a
//! directory, links, swapping two directories at once, durability of a
//! rename, and how much disk a file really takes.

use std::{
    fs::{File, Metadata, OpenOptions},
    io,
    path::Path,
};

/// The device a program is told to write to when what it writes is unwanted.
#[cfg(unix)]
pub const NULL_DEVICE: &str = "/dev/null";
#[cfg(windows)]
pub const NULL_DEVICE: &str = "NUL";

/// Creates `directory` (and its parents) so that only this user can enter
/// it: mode `0700` on Unix. On Windows a directory inherits its parent's
/// ACL, which under the user's profile is already the user's alone.
pub fn create_private_dir_all(directory: &Path) -> io::Result<()> {
    imp::create_private_dir(directory, true)
}

/// [`create_private_dir_all`] for one directory whose parent exists; an
/// existing one is an error, so a path somebody else created first is never
/// adopted.
pub fn create_private_dir(directory: &Path) -> io::Result<()> {
    imp::create_private_dir(directory, false)
}

/// Makes a file `options` creates readable by this user alone: mode `0600`
/// on Unix; on Windows it inherits its directory's ACL.
pub fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
    imp::private_file(options)
}

/// Narrows an existing file to this user alone where the platform keeps a
/// mode for it (`0600`): a file renamed into place does not keep the mode
/// the one it replaced had.
pub fn restrict_to_owner(path: &Path) {
    imp::restrict_to_owner(path)
}

/// Opens `path` for reading without ever blocking on it: a FIFO planted
/// where a file is expected would otherwise hang the reader on Unix.
pub fn open_without_blocking(path: &Path) -> io::Result<File> {
    imp::open_without_blocking(path)
}

/// Creates `link` pointing at `target`. Windows grants symbolic links only
/// with Developer Mode or elevation; without either this is an error of
/// kind [`io::ErrorKind::Unsupported`], never a silent copy.
pub fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    imp::symlink(target, link)
}

/// Removes the link at `link`, never what it points at, whichever kind of
/// target it was made for: Windows keeps a link to a directory as a
/// directory entry, which only `remove_dir` takes.
pub fn remove_link(link: &Path) -> io::Result<()> {
    imp::remove_link(link)
}

/// Moves the link at `from` over `to`, which may itself be a link. One
/// atomic rename on Unix. On Windows, where a rename cannot replace a link
/// to a directory, the old one steps aside under a name of this attempt's
/// own and is put back if the move fails: not atomic, so a writer racing
/// another can lose with an error while the other's link stands at `to`.
pub fn rename_link_over(from: &Path, to: &Path) -> io::Result<()> {
    imp::rename_link_over(from, to)
}

/// Sets when `path`, a file or a directory, was last modified. Windows
/// opens a directory only when asked for one, and changes its time only
/// through a handle opened to write.
pub fn set_modified(path: &Path, time: std::time::SystemTime) -> io::Result<()> {
    imp::open_for_times(path)?.set_modified(time)
}

/// Swaps two directories in one step where the kernel offers it (Linux);
/// elsewhere an error of kind [`io::ErrorKind::Unsupported`], and the
/// caller swaps them in two renames.
pub fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    imp::exchange(a, b)
}

/// Makes the renames in `directory` durable where the platform needs to be
/// told; a no-op where a directory cannot be synced.
pub fn sync_directory(directory: &Path) {
    imp::sync_directory(directory)
}

/// A directory for Unix-domain sockets that only this user can reach, short
/// enough for a socket path with `room` bytes left in it; `None` where none
/// can be had, or where the platform's tools take no socket (Windows'
/// OpenSSH has no connection sharing).
pub fn user_socket_directory(label: &str, room: usize) -> Option<std::path::PathBuf> {
    imp::user_socket_directory(label, room)
}

/// How much disk a file takes, which on Unix is its allocated blocks (a
/// sparse file takes less than its length) and on Windows its length.
pub fn allocated_size(metadata: &Metadata) -> u64 {
    imp::allocated_size(metadata)
}

#[cfg(unix)]
mod imp {
    use std::{
        fs::{DirBuilder, File, Metadata, OpenOptions},
        io,
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        path::Path,
    };

    pub(super) fn create_private_dir(directory: &Path, recursive: bool) -> io::Result<()> {
        if recursive && directory.is_dir() {
            return Ok(());
        }
        DirBuilder::new()
            .recursive(recursive)
            .mode(0o700)
            .create(directory)
    }

    pub(super) fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
        options.mode(0o600)
    }

    pub(super) fn restrict_to_owner(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    pub(super) fn open_without_blocking(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
    }

    pub(super) fn symlink(target: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    pub(super) fn remove_link(link: &Path) -> io::Result<()> {
        std::fs::remove_file(link)
    }

    pub(super) fn open_for_times(path: &Path) -> io::Result<File> {
        File::open(path)
    }

    pub(super) fn rename_link_over(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn exchange(a: &Path, b: &Path) -> io::Result<()> {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};

        const RENAME_EXCHANGE: libc::c_uint = 1 << 1;
        let a = CString::new(a.as_os_str().as_bytes())?;
        let b = CString::new(b.as_os_str().as_bytes())?;
        // A raw syscall rather than `libc::renameat2`, which the musl
        // release targets do not all export.
        // SAFETY: both paths are NUL-terminated and outlive the call.
        let status = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                a.as_ptr(),
                libc::AT_FDCWD,
                b.as_ptr(),
                RENAME_EXCHANGE,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn exchange(_a: &Path, _b: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(super) fn sync_directory(directory: &Path) {
        if let Ok(directory) = File::open(directory) {
            let _ = directory.sync_all();
        }
    }

    pub(super) fn allocated_size(metadata: &Metadata) -> u64 {
        metadata.blocks() * 512
    }

    /// Where `sun_path` (about 100 bytes) still holds the socket's own
    /// name. Never followed: a link planted here would point the socket
    /// elsewhere.
    pub(super) fn user_socket_directory(label: &str, room: usize) -> Option<std::path::PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: getuid cannot fail and touches no memory.
        let uid = unsafe { libc::getuid() };
        let directory = match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(runtime) => std::path::PathBuf::from(runtime).join(label),
            None => std::env::temp_dir().join(format!("{label}-{uid}")),
        };
        let _ = DirBuilder::new().mode(0o700).create(&directory);
        let metadata = std::fs::symlink_metadata(&directory).ok()?;
        let private = metadata.is_dir()
            && metadata.uid() == uid
            && metadata.permissions().mode() & 0o077 == 0;
        let fits = directory.as_os_str().len() + room < 100;
        (private && fits && !directory.to_string_lossy().contains('"')).then_some(directory)
    }
}

#[cfg(windows)]
mod imp {
    use std::{
        fs::{self, File, Metadata, OpenOptions},
        io,
        path::Path,
    };

    pub(super) fn create_private_dir(directory: &Path, recursive: bool) -> io::Result<()> {
        if recursive {
            fs::create_dir_all(directory)
        } else {
            fs::create_dir(directory)
        }
    }

    pub(super) fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
        options
    }

    pub(super) fn restrict_to_owner(_path: &Path) {}

    pub(super) fn open_without_blocking(path: &Path) -> io::Result<File> {
        File::open(path)
    }

    pub(super) fn symlink(target: &Path, link: &Path) -> io::Result<()> {
        let linked = if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        };
        linked.map_err(|error| {
            // ERROR_PRIVILEGE_NOT_HELD: neither Developer Mode nor elevation.
            if error.raw_os_error() == Some(1314) {
                io::Error::new(io::ErrorKind::Unsupported, error)
            } else {
                error
            }
        })
    }

    pub(super) fn open_for_times(path: &Path) -> io::Result<File> {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        File::options()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }

    pub(super) fn remove_link(link: &Path) -> io::Result<()> {
        use std::os::windows::fs::FileTypeExt;
        if std::fs::symlink_metadata(link)?
            .file_type()
            .is_symlink_dir()
        {
            std::fs::remove_dir(link)
        } else {
            std::fs::remove_file(link)
        }
    }

    pub(super) fn rename_link_over(from: &Path, to: &Path) -> io::Result<()> {
        if std::fs::symlink_metadata(to).is_err() {
            return std::fs::rename(from, to);
        }
        static ATTEMPT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut aside = to.as_os_str().to_owned();
        aside.push(format!(
            ".old-{}-{}",
            std::process::id(),
            ATTEMPT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let aside = std::path::PathBuf::from(aside);
        // Gone already means another writer stepped it aside first: there
        // is nothing in the way.
        let stepped_aside = match std::fs::rename(to, &aside) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        match std::fs::rename(from, to) {
            Ok(()) => {
                if stepped_aside {
                    let _ = remove_link(&aside);
                }
                Ok(())
            }
            Err(error) => {
                // Put back unless another writer's link took the place.
                if stepped_aside && std::fs::rename(&aside, to).is_err() {
                    let _ = remove_link(&aside);
                }
                Err(error)
            }
        }
    }

    pub(super) fn exchange(_a: &Path, _b: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(super) fn sync_directory(_directory: &Path) {}

    pub(super) fn allocated_size(metadata: &Metadata) -> u64 {
        metadata.len()
    }

    pub(super) fn user_socket_directory(_label: &str, _room: usize) -> Option<std::path::PathBuf> {
        None
    }
}
