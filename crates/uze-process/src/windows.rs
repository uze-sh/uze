//! What UZE asks of Windows about processes: the user's SID, a process's
//! image and liveness, ending one process or a whole tree, and Job Objects.

use std::{collections::HashMap, ffi::OsStr, io, os::windows::ffi::OsStrExt, path::PathBuf, ptr};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
        LocalFree, STILL_ACTIVE,
    },
    Security::{
        Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, OpenProcess, OpenProcessToken,
            PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
            PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
        },
    },
};

pub fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

pub struct Owned(pub HANDLE);

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
pub fn current_user_sid() -> io::Result<String> {
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
pub fn image_of(pid: u32) -> Option<PathBuf> {
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

/// Whether `pid` is still running: `None` when Windows will not say (a
/// process of another user, a protected one). A pid nothing runs is the
/// one answer Windows gives plainly, as an invalid parameter.
pub fn is_alive(pid: u32) -> Option<bool> {
    let Some(process) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        // SAFETY: reads this thread's last error, set by the failed open.
        return (unsafe { GetLastError() } == ERROR_INVALID_PARAMETER).then_some(false);
    };
    let mut code = 0u32;
    // SAFETY: valid handle; `code` outlives the call.
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return None;
    }
    Some(code != STILL_ACTIVE as u32).map(|exited| !exited)
}

/// Ends `pid` outright. Only ever called on a process the caller has just
/// confirmed runs `uze`.
pub fn terminate(pid: u32) {
    if let Some(process) = open(pid, PROCESS_TERMINATE) {
        // SAFETY: valid handle with terminate access.
        unsafe { TerminateProcess(process.0, 1) };
    }
}

/// A set of processes ended as one: everything a member starts joins it,
/// and closing the last handle to it ends every member.
pub struct Job(Owned);

impl Job {
    pub fn new() -> io::Result<Self> {
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

    pub fn assign(&self, pid: u32) -> io::Result<()> {
        let process = open(pid, PROCESS_SET_QUOTA | PROCESS_TERMINATE)
            .ok_or_else(io::Error::last_os_error)?;
        // SAFETY: both handles are valid.
        if unsafe { AssignProcessToJobObject(self.0.0, process.0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn terminate(&self) {
        // SAFETY: valid job handle.
        unsafe { TerminateJobObject(self.0.0, 1) };
    }

    /// The pids in the job, oldest first, as Windows lists them.
    pub fn pids(&self) -> Vec<u32> {
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

/// When `pid` started, as Windows records it; `None` once it cannot be
/// opened. What tells a process from a later one that reused its pid.
fn started(pid: u32) -> Option<u64> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    // SAFETY: zeroed FILETIMEs are valid outputs.
    let (mut created, mut exited, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
        unsafe { std::mem::zeroed() };
    // SAFETY: valid handle; every out-pointer outlives the call.
    if unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return None;
    }
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Every running process's parent, read in one snapshot.
fn parents() -> Vec<(u32, u32)> {
    // SAFETY: a snapshot of every process; closed by `Owned`.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let snapshot = Owned(snapshot);
    // SAFETY: zeroed is valid once dwSize is set.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = Vec::new();
    // SAFETY: `entry` is sized as the API requires and outlives each call.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        found.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    found
}

/// `pid` and everything it started, descendants first.
///
/// A parent pid names the parent at the moment the child was created, and
/// Windows reuses pids: a "child" that started before the process now at
/// that pid is a stranger's, and is not followed.
pub fn tree_of(pid: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (child, parent) in parents() {
        if child != parent {
            children.entry(parent).or_default().push(child);
        }
    }
    let mut ordered = Vec::new();
    let mut pending = vec![(pid, started(pid))];
    while let Some((current, current_started)) = pending.pop() {
        for &child in children
            .get(&current)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let child_started = started(child);
            let younger = match (current_started, child_started) {
                (Some(parent), Some(child)) => child >= parent,
                _ => false,
            };
            if younger && !ordered.contains(&child) {
                pending.push((child, child_started));
            }
        }
        ordered.push(current);
    }
    ordered.reverse();
    ordered
}

/// Ends `pid` and every process it started. Only for a process the caller
/// still holds a handle to (an unreaped `Child`), so the pid cannot have
/// been handed to anybody else.
pub fn kill_tree(pid: u32) {
    for member in tree_of(pid) {
        terminate(member);
    }
}
