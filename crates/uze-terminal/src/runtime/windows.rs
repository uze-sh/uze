//! What the runtime asks of Windows itself: the user's SID, a process's
//! image, ending a process, Job Objects for panes, the workspace lock, and
//! the stop event an old server is retired through.

use std::{
    ffi::OsStr,
    fs, io,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    path::PathBuf,
    ptr,
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_LOCK_VIOLATION, GetLastError, HANDLE, LocalFree, STILL_ACTIVE,
    },
    Security::{
        Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    },
    Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, UnlockFileEx,
    },
    System::{
        IO::OVERLAPPED,
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CreateEventW, EVENT_MODIFY_STATE, GetCurrentProcess, GetExitCodeProcess, INFINITE,
            OpenEventW, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, SetEvent, TerminateProcess, WaitForSingleObject,
        },
    },
};

pub(crate) fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

struct Owned(HANDLE);

// SAFETY: a kernel handle is usable from any thread.
unsafe impl Send for Owned {}
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: owned, closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// The current token's user SID, as its string form (`S-1-5-21-…`).
pub(crate) fn current_user_sid() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: the pseudo-handle of this process; `token` receives a handle
    // closed by `Owned`.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Owned(token);
    let mut needed = 0u32;
    // SAFETY: a size query with no buffer.
    unsafe { GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut needed) };
    let mut buffer = vec![0u8; needed as usize];
    // SAFETY: `buffer` is `needed` bytes, as the size query asked for.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the buffer now holds a TOKEN_USER; read unaligned since a
    // Vec<u8> promises no alignment.
    let user: TOKEN_USER = unsafe { ptr::read_unaligned(buffer.as_ptr().cast()) };
    let mut text: *mut u16 = ptr::null_mut();
    // SAFETY: the SID points into `buffer`, alive for the call; `text` is
    // freed with LocalFree below.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a NUL-terminated wide string from the conversion above.
    let sid = unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
    };
    // SAFETY: allocated by ConvertSidToStringSidW with LocalAlloc.
    unsafe { LocalFree(text.cast()) };
    Ok(sid)
}

fn open(pid: u32, access: u32) -> Option<Owned> {
    // SAFETY: opening a handle to another process; null on failure.
    let handle = unsafe { OpenProcess(access, 0, pid) };
    (!handle.is_null()).then_some(Owned(handle))
}

/// The full path of the image `pid` runs, or `None` when it cannot be read
/// (gone, protected, or another user's).
pub(crate) fn image_of(pid: u32) -> Option<PathBuf> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut buffer = vec![0u16; 32 * 1024];
    let mut len = buffer.len() as u32;
    // SAFETY: `buffer` holds `len` u16s and outlives the call.
    if unsafe {
        QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    } == 0
    {
        return None;
    }
    buffer.truncate(len as usize);
    Some(PathBuf::from(String::from_utf16_lossy(&buffer)))
}

/// Whether `pid` is still running: `None` when Windows will not say.
#[allow(dead_code)]
pub(crate) fn is_alive(pid: u32) -> Option<bool> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut code = 0u32;
    // SAFETY: valid handle; `code` outlives the call.
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return None;
    }
    Some(code == STILL_ACTIVE as u32)
}

/// Ends `pid` outright. Only ever called on a process the caller has just
/// confirmed runs `uze`.
pub(crate) fn terminate(pid: u32) {
    if let Some(process) = open(pid, PROCESS_TERMINATE) {
        // SAFETY: valid handle with terminate access.
        unsafe { TerminateProcess(process.0, 1) };
    }
}

/// A pane's processes, as one unit: everything the pane's program starts
/// is in it, and closing the pane ends all of them.
pub(crate) struct PaneJob(Owned);

impl PaneJob {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: an unnamed job with default security.
        let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Owned(job);
        // SAFETY: zeroed is the documented empty limit structure.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `limits` is the structure the class names, by size.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(job))
    }

    pub(crate) fn assign(&self, pid: u32) -> io::Result<()> {
        let process = open(pid, PROCESS_SET_QUOTA | PROCESS_TERMINATE)
            .ok_or_else(io::Error::last_os_error)?;
        // SAFETY: both handles are valid.
        if unsafe { AssignProcessToJobObject(self.0.0, process.0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(crate) fn terminate(&self) {
        // SAFETY: valid job handle.
        unsafe { TerminateJobObject(self.0.0, 1) };
    }

    /// The pids in the job, oldest first, as Windows lists them.
    pub(crate) fn pids(&self) -> Vec<u32> {
        const CAPACITY: usize = 256;
        let size = std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
            + CAPACITY * std::mem::size_of::<usize>();
        let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        // SAFETY: `buffer` is at least `size` bytes, usize-aligned.
        let ok = unsafe {
            QueryInformationJobObject(
                self.0.0,
                JobObjectBasicProcessIdList,
                buffer.as_mut_ptr().cast(),
                size as u32,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Vec::new();
        }
        // SAFETY: the buffer now starts with a JOBOBJECT_BASIC_PROCESS_ID_LIST.
        let list = unsafe { &*(buffer.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST) };
        let count = (list.NumberOfProcessIdsInList as usize).min(CAPACITY);
        // SAFETY: `count` ids follow the header within `buffer`.
        let ids = unsafe { std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), count) };
        ids.iter().map(|&id| id as u32).collect()
    }
}

/// Why the workspace lock was refused.
pub(crate) enum LockAnswer {
    Taken,
    Contended,
    Failed(io::Error),
}

/// The lock covers one byte far past anything the file holds, so the pid
/// written at its start stays readable by others: Windows locks are
/// mandatory, and a lock over the whole file would hide the holder from
/// the very readers that need to name it.
const LOCKED_OFFSET: u64 = u64::MAX - 1;

fn lock_overlapped() -> OVERLAPPED {
    // SAFETY: zeroed is the documented initial state.
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.Anonymous.Anonymous.Offset = LOCKED_OFFSET as u32;
    overlapped.Anonymous.Anonymous.OffsetHigh = (LOCKED_OFFSET >> 32) as u32;
    overlapped
}

pub(crate) fn lock(file: &fs::File, exclusive: bool) -> LockAnswer {
    let mut overlapped = lock_overlapped();
    let mut flags = LOCKFILE_FAIL_IMMEDIATELY;
    if exclusive {
        flags |= LOCKFILE_EXCLUSIVE_LOCK;
    }
    // SAFETY: `file` owns its handle for the call; a synchronous handle
    // completes the lock before returning.
    if unsafe { LockFileEx(file.as_raw_handle(), flags, 0, 1, 0, &mut overlapped) } != 0 {
        return LockAnswer::Taken;
    }
    // SAFETY: reads this thread's last error.
    match unsafe { GetLastError() } {
        ERROR_LOCK_VIOLATION => LockAnswer::Contended,
        // A file opened for synchronous I/O answers a contended immediate
        // lock with ERROR_IO_PENDING on some builds; it is still "held".
        997 => LockAnswer::Contended,
        error => LockAnswer::Failed(io::Error::from_raw_os_error(error as i32)),
    }
}

pub(crate) fn unlock(file: &fs::File) {
    let mut overlapped = lock_overlapped();
    // SAFETY: as in `lock`.
    unsafe { UnlockFileEx(file.as_raw_handle(), 0, 1, 0, &mut overlapped) };
}

/// The event a server waits on to stop: named after its endpoint, and a
/// shape no build ever changes, so any build can retire any other.
pub(crate) fn stop_event_name(endpoint: &std::path::Path) -> String {
    let leaf = endpoint
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("Local\\{leaf}-stop")
}

/// Blocks until the stop event for `name` is signalled.
pub(crate) fn wait_for_stop(name: &str) -> io::Result<()> {
    let wide_name = wide(OsStr::new(name));
    // SAFETY: a named manual-reset event, default security (the caller's
    // own session namespace).
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, wide_name.as_ptr()) };
    if event.is_null() {
        return Err(io::Error::last_os_error());
    }
    let event = Owned(event);
    // SAFETY: valid event handle.
    unsafe { WaitForSingleObject(event.0, INFINITE) };
    Ok(())
}

/// Asks the server waiting on `name` to stop. `false` when no server of
/// that name is waiting.
pub(crate) fn signal_stop(name: &str) -> bool {
    let wide_name = wide(OsStr::new(name));
    // SAFETY: opens an existing event by name; null when there is none.
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide_name.as_ptr()) };
    if event.is_null() {
        return false;
    }
    let event = Owned(event);
    // SAFETY: valid event handle with modify access.
    unsafe { SetEvent(event.0) != 0 }
}
