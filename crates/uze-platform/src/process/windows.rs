//! Processes on Windows: Job Objects, process trees walked from a snapshot,
//! named stop events, and creation flags that keep a child off the
//! launching console.

use std::{
    collections::HashMap, ffi::OsStr, io, os::windows::process::CommandExt, process::Command, ptr,
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
        STILL_ACTIVE,
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
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CreateEventW,
            EVENT_MODIFY_STATE, GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, INFINITE,
            OpenEventW, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SET_QUOTA, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, SetEvent, TerminateProcess,
            WaitForSingleObject,
        },
    },
};

use crate::win::{Owned, wide};

fn open(pid: u32, access: u32) -> Option<Owned> {
    // SAFETY: opening a handle to another process; null on failure.
    let handle = unsafe { OpenProcess(access, 0, pid) };
    (!handle.is_null()).then_some(Owned(handle))
}

/// A pid nothing runs is the one answer Windows gives plainly, as an
/// invalid parameter; another user's or a protected process is unknown.
pub(super) fn alive(pid: u32) -> Option<bool> {
    let Some(process) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        // SAFETY: reads this thread's last error, set by the failed open.
        return (unsafe { GetLastError() } == ERROR_INVALID_PARAMETER).then_some(false);
    };
    let mut code = 0u32;
    // SAFETY: valid handle; `code` outlives the call.
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return None;
    }
    Some(code == STILL_ACTIVE as u32)
}

pub(super) fn terminate(pid: u32) {
    if let Some(process) = open(pid, PROCESS_TERMINATE) {
        // SAFETY: valid handle with terminate access.
        unsafe { TerminateProcess(process.0, 1) };
    }
}

/// The stop event of `channel`, in this session's namespace.
fn stop_event(channel: &str) -> Vec<u16> {
    wide(OsStr::new(&format!("Local\\{channel}-stop")))
}

pub(super) fn request_stop(_pid: u32, channel: &str) {
    let name = stop_event(channel);
    // SAFETY: opens an existing event by name; null when nobody listens.
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
    if !event.is_null() {
        let event = Owned(event);
        // SAFETY: valid event handle with modify access.
        unsafe { SetEvent(event.0) };
    }
}

pub(super) fn listen_for_stop(channel: &str, on_stop: impl FnOnce() + Send + 'static) {
    let name = stop_event(channel);
    // SAFETY: a named manual-reset event with default security.
    let event = unsafe { CreateEventW(ptr::null(), 1, 0, name.as_ptr()) };
    if event.is_null() {
        return;
    }
    let event = Owned(event);
    std::thread::spawn(move || {
        // The whole handle, not its field: the thread owns it from here on.
        let event = event;
        // SAFETY: valid event handle.
        unsafe { WaitForSingleObject(event.0, INFINITE) };
        on_stop();
    });
}

pub(super) fn end_group(pid: u32) {
    for member in tree_of(pid) {
        terminate(member);
    }
}

/// Nothing: once its leader is reaped a Windows pid may already name
/// somebody else's process, and no group outlives its leader the way a
/// Unix one does. [`end_group`] reached the tree while it could.
pub(super) fn end_reaped_group(_pid: u32) {}

pub(super) fn wait_without_reaping(pid: u32) {
    if let Some(process) = open(pid, PROCESS_SYNCHRONIZE) {
        // SAFETY: valid handle with synchronize access.
        unsafe { WaitForSingleObject(process.0, INFINITE) };
    }
}

/// A process group of its own, so a Ctrl+C on this console does not reach
/// it; the tree is what [`end_group`] ends.
pub(super) fn in_own_group(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

/// A hidden console of its own: nothing it asks can reach the person's
/// terminal, and no window flashes up for it.
pub(super) fn without_terminal(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

/// A hidden console of its own (not the launching one, so neither closing
/// it nor a Ctrl+C there reaches the child), and out of the launching
/// terminal's job, which some hosts close with everything in it. A host
/// that forbids leaving its job gets a child that ends with it.
/// Windows has no `exec`: the program runs as a child sharing this
/// console, and its exit status becomes this process's.
pub(super) fn run_in_place(command: &mut Command) -> io::Error {
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => error,
    }
}

pub(super) fn cpu_time() -> super::CpuTime {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let mut times = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    let [creation, exit, kernel, user] = &mut times;
    // SAFETY: the pseudo-handle needs no closing, and every out-pointer is
    // a live `FILETIME`.
    unsafe { GetProcessTimes(GetCurrentProcess(), creation, exit, kernel, user) };
    let hundreds =
        |time: &FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    super::CpuTime {
        own: std::time::Duration::from_nanos((hundreds(&times[2]) + hundreds(&times[3])) * 100),
        children: None,
    }
}

pub(super) fn spawn_detached(command: &mut Command) -> io::Result<super::Detached> {
    if let Ok(child) = command
        .creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        return Ok(super::Detached {
            child,
            outlives_host: true,
        });
    }
    Ok(super::Detached {
        child: command.creation_flags(CREATE_NO_WINDOW).spawn()?,
        outlives_host: false,
    })
}

pub(super) fn current_user() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: the pseudo-handle of this process; `token` is closed by `Owned`.
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
    // SAFETY: the buffer now holds a TOKEN_USER; read unaligned, since a
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

/// When `pid` started; what tells a process from a later one that reused
/// its pid.
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
    // SAFETY: a snapshot of every process, closed by `Owned`.
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

/// `pid` and everything it started, descendants first. A parent pid names
/// the parent at the child's creation and Windows reuses pids, so a
/// "child" older than the process now at that pid is a stranger's.
fn tree_of(pid: u32) -> Vec<u32> {
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
            let younger = matches!(
                (current_started, child_started),
                (Some(parent), Some(child)) if child >= parent
            );
            if younger && !ordered.contains(&child) {
                pending.push((child, child_started));
            }
        }
        ordered.push(current);
    }
    ordered.reverse();
    ordered
}

/// A Job Object: everything a member starts joins it, and closing the last
/// handle to it ends every member.
pub(super) struct Group(Owned);

impl Group {
    pub(super) fn adopt(pid: u32) -> Option<Self> {
        // SAFETY: an unnamed job with default security.
        let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if job.is_null() {
            return None;
        }
        let job = Owned(job);
        // SAFETY: zeroed is the documented empty limit structure.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `limits` is the structure the class names, by size.
        let limited = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        let process = open(pid, PROCESS_SET_QUOTA | PROCESS_TERMINATE)?;
        // SAFETY: both handles are valid.
        let assigned = unsafe { AssignProcessToJobObject(job.0, process.0) };
        (limited != 0 && assigned != 0).then_some(Self(job))
    }

    pub(super) fn end(&self) {
        // SAFETY: valid job handle.
        unsafe { TerminateJobObject(self.0.0, 1) };
    }

    pub(super) fn members(&self) -> Vec<u32> {
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
