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
/// it: mode `0700` on Unix; on Windows a protected ACL granting this user
/// alone, which what is created inside inherits. A `directory` that is
/// already there is narrowed to its owner: what it lets other users do is
/// taken away, and what it lets the owner do is left as it is. One a
/// permissive umask or an older build made is otherwise private in name
/// only.
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
/// on Unix; on Windows it inherits its directory's ACL, which
/// [`create_private_dir_all`] made this user's alone.
pub fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
    imp::private_file(options)
}

/// Narrows an existing file to this user alone (`0600`; a protected ACL on
/// Windows): a file renamed into place does not keep what the one it
/// replaced had.
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    imp::restrict_to_owner(path)
}

/// Whether `path` belongs to this user or to the machine's administrator
/// (`root`; the Administrators group on Windows), who can already change
/// anything this user can. A file anybody else wrote is not one to act on
/// as though this user had.
pub fn owned_by_user_or_administrator(path: &Path) -> io::Result<bool> {
    imp::owned_by_user_or_administrator(path)
}

/// Opens `path` for reading without ever blocking on it: a FIFO planted
/// where a file is expected would otherwise hang the reader on Unix.
pub fn open_without_blocking(path: &Path) -> io::Result<File> {
    imp::open_without_blocking(path)
}

/// Creates `link` pointing at `target`. A symbolic link on Unix. On Windows
/// a link to a directory is a junction, which every user may make and the
/// standard library reads as a link like any other (`is_symlink`,
/// `read_link`, removed by [`remove_link`]); a link to a file is a symbolic
/// link, which Windows grants only with Developer Mode or elevation, and
/// without either this is an error of kind [`io::ErrorKind::Unsupported`],
/// never a silent copy.
pub fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    imp::symlink(target, link)
}

/// Renames `from` to `to`, replacing a file there. On Windows another
/// program holding either file a moment (an antivirus scanning what was
/// just written, the search indexer, an editor saving) makes a rename fail
/// with a sharing violation or access denied until it lets go; that brief
/// hold is waited out, for a bounded time, the way Git for Windows does.
/// Any other failure, and one that outlasts the wait, is returned.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    imp::rename(from, to)
}

/// Makes `link` reach `target`, a file or a directory, with what every
/// user may make here: a symbolic link on Unix; on Windows a junction for a
/// directory and, for a file, a hard link (the same file under a second
/// name, so on the same volume), since a symbolic link to a file needs a
/// privilege an ordinary session lacks. Reads through it see the target as
/// it is now; a target replaced by rename is no longer reached.
pub fn link_entry(target: &Path, link: &Path) -> io::Result<()> {
    imp::link_entry(target, link)
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
            use std::os::unix::fs::PermissionsExt;
            // Only what reaches other users is taken away: the owner's own
            // bits are the owner's, a read-only directory among them.
            let mode = std::fs::metadata(directory)?.mode() & 0o7777;
            if mode & 0o077 != 0 {
                std::fs::set_permissions(
                    directory,
                    std::fs::Permissions::from_mode(mode & !0o077),
                )?;
            }
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

    pub(super) fn restrict_to_owner(path: &Path) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }

    pub(super) fn owned_by_user_or_administrator(path: &Path) -> io::Result<bool> {
        let owner = std::fs::metadata(path)?.uid();
        // SAFETY: getuid cannot fail and touches no memory.
        let user = unsafe { libc::getuid() };
        Ok(owner == user || owner == 0)
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

    pub(super) fn link_entry(target: &Path, link: &Path) -> io::Result<()> {
        symlink(target, link)
    }

    pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
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
            fs::create_dir_all(directory)?;
        } else {
            fs::create_dir(directory)?;
        }
        owner_only(directory, "OICI")
    }

    pub(super) fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
        options
    }

    pub(super) fn restrict_to_owner(path: &Path) -> io::Result<()> {
        owner_only(path, "")
    }

    /// Replaces `path`'s ACL with one granting the current token's user,
    /// and no one else, full access; `inherited` is the SDDL inheritance
    /// its children take (`OICI` for a directory). The user and not the
    /// owner: an elevated token's owner is the Administrators group.
    fn owner_only(path: &Path, inherited: &str) -> io::Result<()> {
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
                DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SetFileSecurityW,
            },
        };
        let sid = crate::process::current_user()?;
        let sddl = crate::win::wide(std::ffi::OsStr::new(&format!(
            "D:P(A;{inherited};GA;;;{sid})"
        )));
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; the descriptor is freed below
        // with LocalFree, as the API requires.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let target = crate::win::wide(path.as_os_str());
        // SAFETY: both pointers are valid for the call.
        let applied =
            unsafe { SetFileSecurityW(target.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) };
        // SAFETY: allocated by the conversion above with LocalAlloc.
        unsafe { LocalFree(descriptor) };
        if applied == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// The well-known SID of the built-in Administrators group, which owns
    /// what an elevated token creates.
    const ADMINISTRATORS: &str = "S-1-5-32-544";

    pub(super) fn owned_by_user_or_administrator(path: &Path) -> io::Result<bool> {
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::{ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT},
                OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
            },
        };
        let target = crate::win::wide(path.as_os_str());
        let mut owner: PSID = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `target` is NUL-terminated; `owner` points into
        // `descriptor`, which is freed with LocalFree below.
        let status = unsafe {
            GetNamedSecurityInfoW(
                target.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: `owner` is valid while `descriptor` is; `text` is freed
        // with LocalFree below.
        let converted = unsafe { ConvertSidToStringSidW(owner, &mut text) };
        let sid = (converted != 0).then(|| {
            // SAFETY: a NUL-terminated wide string from the conversion.
            unsafe {
                let len = (0..).take_while(|&i| *text.add(i) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
            }
        });
        // SAFETY: both were allocated by the calls above with LocalAlloc.
        unsafe {
            if !text.is_null() {
                LocalFree(text.cast());
            }
            LocalFree(descriptor);
        }
        let sid = sid.ok_or_else(io::Error::last_os_error)?;
        Ok(sid == ADMINISTRATORS || sid == crate::process::current_user()?)
    }

    pub(super) fn open_without_blocking(path: &Path) -> io::Result<File> {
        File::open(path)
    }

    pub(super) fn symlink(target: &Path, link: &Path) -> io::Result<()> {
        let anchored = link
            .parent()
            .map_or_else(|| target.to_path_buf(), |parent| parent.join(target));
        if anchored.is_dir() {
            return junction(&anchored, link);
        }
        std::os::windows::fs::symlink_file(target, link).map_err(|error| {
            // ERROR_PRIVILEGE_NOT_HELD: neither Developer Mode nor elevation.
            if error.raw_os_error() == Some(1314) {
                io::Error::new(io::ErrorKind::Unsupported, error)
            } else {
                error
            }
        })
    }

    pub(super) fn rename(from: &Path, to: &Path) -> io::Result<()> {
        const ERROR_ACCESS_DENIED: i32 = 5;
        const ERROR_SHARING_VIOLATION: i32 = 32;
        const ERROR_LOCK_VIOLATION: i32 = 33;
        // About two seconds in all: a scan of a file just written ends in
        // milliseconds, and a hold that lasts longer is not a brief one.
        const WAITS_MS: [u64; 8] = [10, 20, 40, 80, 160, 320, 500, 800];
        let mut waits = WAITS_MS.iter();
        loop {
            match std::fs::rename(from, to) {
                Err(error)
                    if matches!(
                        error.raw_os_error(),
                        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
                    ) =>
                {
                    let Some(wait) = waits.next() else {
                        return Err(error);
                    };
                    std::thread::sleep(std::time::Duration::from_millis(*wait));
                }
                outcome => return outcome,
            }
        }
    }

    /// A file held a moment with no sharing, the way a scanner opens
    /// what was just written, is replaced once it is let go.
    #[cfg(test)]
    #[test]
    fn a_rename_waits_out_a_brief_hold() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = uze_testkit::temp::scratch("rename-held");
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("record.json");
        let staged = root.join("record.json.tmp");
        std::fs::write(&target, "old").unwrap();
        std::fs::write(&staged, "new").unwrap();
        let held = File::options()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            drop(held);
        });
        rename(&staged, &target).unwrap();
        release.join().unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        let _ = std::fs::remove_dir_all(root);
    }

    pub(super) fn link_entry(target: &Path, link: &Path) -> io::Result<()> {
        if target.is_dir() {
            symlink(target, link)
        } else {
            std::fs::hard_link(target, link)
        }
    }

    /// An empty directory at `link` made a mount point for `target`, an
    /// absolute path: the reparse data `mklink /J` writes.
    fn junction(target: &Path, link: &Path) -> io::Result<()> {
        let target = crate::path::strip_verbatim(&std::path::absolute(target)?);
        std::fs::create_dir(link)?;
        let mounted = mount(link, &target);
        if mounted.is_err() {
            let _ = std::fs::remove_dir(link);
        }
        mounted
    }

    fn mount(link: &Path, target: &Path) -> io::Result<()> {
        use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::System::IO::DeviceIoControl;

        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

        let directory = File::options()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(link)?;
        let printed: Vec<u16> = target.as_os_str().encode_wide().collect();
        let substitute: Vec<u16> = std::ffi::OsStr::new(r"\??\")
            .encode_wide()
            .chain(printed.iter().copied())
            .collect();
        let bytes = |units: &[u16]| u16::try_from(units.len() * 2).ok();
        let (Some(substitute_length), Some(printed_length)) = (bytes(&substitute), bytes(&printed))
        else {
            return Err(io::ErrorKind::InvalidFilename.into());
        };
        // The mount point buffer: four offsets and lengths, then both names,
        // each NUL-terminated.
        let mut names = substitute;
        names.push(0);
        names.extend(&printed);
        names.push(0);
        let data_length = u16::try_from(8 + names.len() * 2)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidFilename))?;
        let mut buffer = Vec::with_capacity(8 + usize::from(data_length));
        buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend(data_length.to_le_bytes());
        buffer.extend(0u16.to_le_bytes());
        buffer.extend(0u16.to_le_bytes());
        buffer.extend(substitute_length.to_le_bytes());
        buffer.extend((substitute_length + 2).to_le_bytes());
        buffer.extend(printed_length.to_le_bytes());
        buffer.extend(names.iter().flat_map(|unit| unit.to_le_bytes()));
        let mut returned = 0u32;
        // SAFETY: the handle is open for writing for the call, and the
        // buffer holds exactly the bytes its length says.
        let set = unsafe {
            DeviceIoControl(
                directory.as_raw_handle(),
                FSCTL_SET_REPARSE_POINT,
                buffer.as_ptr().cast(),
                buffer.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if set == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
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
        // A link already reaching what `from` reaches is left standing: its
        // writer and this one agree, and stepping it aside would leave the
        // path empty for a moment to everyone reading it.
        let agrees = || {
            std::fs::read_link(from)
                .is_ok_and(|target| std::fs::read_link(to).is_ok_and(|standing| standing == target))
        };
        if agrees() {
            return remove_link(from);
        }
        if std::fs::symlink_metadata(to).is_err() {
            return match std::fs::rename(from, to) {
                // Placed by another writer in between: a directory link
                // cannot be renamed over another one.
                Err(_) if agrees() => remove_link(from),
                placed => placed,
            };
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
                if agrees() {
                    return remove_link(from);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What this user just wrote is theirs; the filesystem root, owned by
    /// the administrator, counts as trusted too.
    #[test]
    fn a_file_this_user_wrote_is_owned_by_them() {
        let root = uze_testkit::temp::scratch("platform-owned");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("mine");
        std::fs::write(&file, b"x").unwrap();
        assert!(owned_by_user_or_administrator(&file).unwrap());
        assert!(owned_by_user_or_administrator(&root).unwrap());
        assert!(owned_by_user_or_administrator(&root.join("absent")).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A private directory is still this user's to use: what is created in
    /// it, and a file narrowed to its owner, read back.
    #[test]
    fn a_private_directory_stays_usable_by_its_owner() {
        let root = uze_testkit::temp::scratch("private-dir");
        let directory = root.join("one").join("two");
        create_private_dir_all(&directory).unwrap();
        let file = directory.join("record.json");
        private_file(OpenOptions::new().write(true).create_new(true))
            .open(&file)
            .unwrap();
        std::fs::write(&file, "{}").unwrap();
        restrict_to_owner(&file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        assert!(create_private_dir(&directory).is_err(), "never adopted");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A link to a directory is read through, reads back as a link to it,
    /// and goes without what it pointed at: what every caller relies on,
    /// on a platform where it is a junction as much as where it is a
    /// symbolic link.
    #[test]
    fn a_directory_link_reads_through_and_goes_alone() {
        let root = uze_testkit::temp::scratch("directory-link");
        let target = root.join("target dir");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("f.txt"), "x").unwrap();
        let link = root.join("link");

        symlink(&target, &link).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        assert_eq!(std::fs::read_to_string(link.join("f.txt")).unwrap(), "x");

        remove_link(&link).unwrap();
        assert!(!link.exists());
        assert!(target.join("f.txt").is_file());
        let _ = std::fs::remove_dir_all(root);
    }
}
