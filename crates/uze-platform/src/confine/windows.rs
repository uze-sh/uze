//! Windows: an AppContainer made for the run, inside a Job Object of its
//! own. The run's token carries one capability per path and right it was
//! granted, and each granted directory carries an inheritable entry for
//! that capability. Windows cannot replace a process, so the helper starts
//! the program suspended, places it in the job, lets it run, waits for it
//! and ends with its exit code (`confinement-parity` D1 to D5).

use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::Path,
    ptr,
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_ALREADY_EXISTS, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, LocalFree,
        SetHandleInformation, WAIT_OBJECT_0,
    },
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
        Authorization::{
            ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE,
            SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID,
            TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
        },
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, DeriveCapabilitySidsFromName, EqualSid,
        FreeSid, GetAce, INHERIT_ONLY_ACE,
        Isolation::{CreateAppContainerProfile, DeleteAppContainerProfile},
        NO_INHERITANCE, OBJECT_INHERIT_ACE, PSECURITY_DESCRIPTOR, PSID, SECURITY_CAPABILITIES,
        SID_AND_ATTRIBUTES, SUB_CONTAINERS_AND_OBJECTS_INHERIT,
    },
    Storage::FileSystem::{
        DELETE, FILE_DELETE_CHILD, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_TRAVERSE,
    },
    System::{
        Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW,
            JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
            JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_DISPLAYSETTINGS,
            JOB_OBJECT_UILIMIT_EXITWINDOWS, JOB_OBJECT_UILIMIT_GLOBALATOMS,
            JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
            JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
            JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Registry::{HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW},
        Threading::{
            CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW,
            TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

use super::{Availability, Confinement};
use crate::win::{Owned, wide};

/// What every app container is: the SID Windows grants the system's own
/// directories to, so a path it already reaches needs no entry of ours.
const ALL_APPLICATION_PACKAGES: &str = "S-1-15-2-1";
/// The well-known `internetClient` capability: outbound connections to the
/// Internet, never to the loopback or the local network.
const INTERNET_CLIENT: &str = "S-1-15-3-1";
/// `SE_GROUP_ENABLED`: a capability counts only when enabled.
const ENABLED: u32 = 0x4;
/// Where Windows records each AppContainer profile, by SID, under `HKCU`.
const PROFILE_MAPPINGS: &str = "Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\AppContainer\\Mappings";

/// Every app container reads and executes the system and the programs
/// installed for every user, through `ALL APPLICATION PACKAGES`, and
/// Windows names no device by a path.
pub(super) const SYSTEM: super::System = super::System::NONE;

pub(super) fn availability() -> Availability {
    Availability::of_app_container()
}

pub(super) fn enter(confinement: &Confinement, argv: &[OsString]) -> io::Error {
    match run(confinement, argv) {
        // The exit code is the program's, bit for bit: an `NTSTATUS` it was
        // ended with reads back as the same unsigned value.
        Ok(code) => std::process::exit(code as i32),
        Err(error) => error,
    }
}

fn run(confinement: &Confinement, argv: &[OsString]) -> io::Result<u32> {
    if !confinement.devices.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows grants no device by its path",
        ));
    }
    if is_batch(Path::new(&argv[0])) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a batch file cannot run confined: its arguments are read again by cmd.exe",
        ));
    }
    let namespace = confinement.namespace.as_str();
    sweep_profiles(namespace);
    let mut capabilities = Vec::new();
    for (right, paths) in [
        (Right::Read, &confinement.read),
        (Right::ReadExecute, &confinement.read_execute),
        (Right::Write, &confinement.write),
    ] {
        for path in paths {
            capabilities.push(grant(namespace, path, right)?);
        }
    }
    if confinement.network {
        capabilities.push(Sid::parse(INTERNET_CLIENT)?);
    }
    ensure_local_app_data()?;
    let profile = Profile::create(namespace)?;
    let job = job(confinement)?;
    let child = start(argv, &profile, &capabilities)?;
    // SAFETY: both handles are valid; the child has not run yet.
    if unsafe { AssignProcessToJobObject(job.0, child.process.0) } == 0 {
        let error = io::Error::last_os_error();
        // SAFETY: a suspended process this helper created.
        unsafe { TerminateProcess(child.process.0, 1) };
        return Err(error);
    }
    // SAFETY: the primary thread of the suspended child.
    if unsafe { ResumeThread(child.thread.0) } == u32::MAX {
        let error = io::Error::last_os_error();
        // SAFETY: as above.
        unsafe { TerminateProcess(child.process.0, 1) };
        return Err(error);
    }
    // SAFETY: a valid process handle.
    if unsafe { WaitForSingleObject(child.process.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    let mut code = 0u32;
    // SAFETY: a valid process handle; `code` outlives the call.
    if unsafe { GetExitCodeProcess(child.process.0, &mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // Whatever the program left running ends with its job, before its
    // profile goes.
    drop(job);
    drop(profile);
    Ok(code)
}

// ------------------------------------------------------------- grants

#[derive(Clone, Copy)]
enum Right {
    Read,
    ReadExecute,
    Write,
}

impl Right {
    fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::ReadExecute => "read-execute",
            Self::Write => "write",
        }
    }

    /// The entries a grant of this right writes, each a mask and the
    /// inheritance it applies with, the one that covers `path` itself
    /// first. A directory a program may only read or write is still one
    /// it may enter, which takes `FILE_TRAVERSE`, the bit that on a file
    /// means execute: so the files beneath get an entry of their own,
    /// without it.
    fn entries(self, directory: bool) -> Vec<(u32, u32)> {
        match (self, directory) {
            (_, false) => vec![(self.mask(), NO_INHERITANCE)],
            (Self::ReadExecute, true) => {
                vec![(self.mask(), SUB_CONTAINERS_AND_OBJECTS_INHERIT)]
            }
            (Self::Read | Self::Write, true) => vec![
                (self.mask() | FILE_TRAVERSE, CONTAINER_INHERIT_ACE),
                (self.mask(), OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE),
            ],
        }
    }

    /// Never execute where a program may write, as under Landlock.
    fn mask(self) -> u32 {
        match self {
            Self::Read => FILE_GENERIC_READ,
            Self::ReadExecute => FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            Self::Write => FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE | FILE_DELETE_CHILD,
        }
    }
}

/// The capability that reaches `path` with `right`, its entry on `path`
/// written once and kept (D3): only a process created with the capability
/// holds it, and only this user's own processes can create one.
fn grant(namespace: &str, path: &Path, right: Right) -> io::Result<Sid> {
    let unreachable = |why: &dyn std::fmt::Display| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("cannot grant {}: {why}", path.display()),
        )
    };
    let metadata = std::fs::metadata(path).map_err(|error| unreachable(&error))?;
    let capability = Sid::capability(&format!("{namespace}.{}:{}", right.name(), path.display()))?;
    let dacl = Dacl::of(path).map_err(|error| unreachable(&error))?;
    let everyone_contained = Sid::parse(ALL_APPLICATION_PACKAGES)?;
    let entries = right.entries(metadata.is_dir());
    let (here, _) = entries[0];
    if dacl.allows(&capability, here) || dacl.allows(&everyone_contained, here) {
        return Ok(capability);
    }
    dacl.add(path, &capability, &entries)
        .map_err(|error| unreachable(&error))?;
    Ok(capability)
}

/// A SID this module allocated, freed the way it was allocated.
struct Sid {
    sid: PSID,
    free: fn(PSID),
}

impl Sid {
    fn parse(text: &str) -> io::Result<Self> {
        let text = wide(OsStr::new(text));
        let mut sid: PSID = ptr::null_mut();
        // SAFETY: `text` is NUL-terminated; the SID is freed with LocalFree.
        if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            sid,
            free: local_free,
        })
    }

    /// The capability SID Windows derives from `name`, the same for the
    /// same name on every machine.
    fn capability(name: &str) -> io::Result<Self> {
        let name = wide(OsStr::new(name));
        let mut groups: *mut PSID = ptr::null_mut();
        let mut group_count = 0u32;
        let mut sids: *mut PSID = ptr::null_mut();
        let mut sid_count = 0u32;
        // SAFETY: every out-pointer outlives the call; both arrays and the
        // SIDs in them are LocalAlloc'd, as the API documents.
        let derived = unsafe {
            DeriveCapabilitySidsFromName(
                name.as_ptr(),
                &mut groups,
                &mut group_count,
                &mut sids,
                &mut sid_count,
            )
        };
        if derived == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `groups` holds `group_count` SIDs and `sids` holds
        // `sid_count`; all but the one kept are freed here.
        unsafe {
            for index in 0..group_count as usize {
                LocalFree(*groups.add(index));
            }
            LocalFree(groups.cast());
            let kept = (sid_count > 0).then(|| *sids);
            for index in 1..sid_count as usize {
                LocalFree(*sids.add(index));
            }
            LocalFree(sids.cast());
            kept.map(|sid| Self {
                sid,
                free: local_free,
            })
            .ok_or_else(|| io::Error::other("no capability SID was derived"))
        }
    }
}

impl Drop for Sid {
    fn drop(&mut self) {
        (self.free)(self.sid);
    }
}

fn local_free(sid: PSID) {
    // SAFETY: allocated with LocalAlloc by the API that returned it.
    unsafe { LocalFree(sid) };
}

fn free_sid(sid: PSID) {
    // SAFETY: allocated by the API whose documentation says FreeSid.
    unsafe { FreeSid(sid) };
}

/// A path's discretionary ACL, with the descriptor that holds it.
struct Dacl {
    acl: *mut ACL,
    descriptor: PSECURITY_DESCRIPTOR,
}

impl Dacl {
    fn of(path: &Path) -> io::Result<Self> {
        let name = wide(path.as_os_str());
        let mut acl: *mut ACL = ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; the descriptor owns `acl` and
        // is freed with LocalFree in Drop.
        let status = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut acl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(Self { acl, descriptor })
    }

    /// Whether an allow entry that applies to the path itself gives `sid`
    /// every right of `mask`.
    fn allows(&self, sid: &Sid, mask: u32) -> bool {
        if self.acl.is_null() {
            // A null DACL lets everyone in; nothing this module writes
            // over it would narrow that.
            return true;
        }
        // SAFETY: a valid ACL from GetNamedSecurityInfoW.
        let count = unsafe { (*self.acl).AceCount };
        (0..u32::from(count)).any(|index| {
            let mut ace: *mut core::ffi::c_void = ptr::null_mut();
            // SAFETY: `index` is below the ACL's count; the entry lives as
            // long as the descriptor.
            if unsafe { GetAce(self.acl, index, &mut ace) } == 0 {
                return false;
            }
            // SAFETY: every entry starts with its header.
            let header = unsafe { &*(ace as *const ACE_HEADER) };
            if u32::from(header.AceType) != 0 || u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0
            {
                return false;
            }
            // SAFETY: type 0 is ACCESS_ALLOWED_ACE, its SID starting at
            // `SidStart`.
            let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
            let entry_sid = (&allowed.SidStart as *const u32).cast_mut().cast();
            // SAFETY: both SIDs are valid for the call.
            allowed.Mask & mask == mask && unsafe { EqualSid(entry_sid, sid.sid) } != 0
        })
    }

    /// Writes this DACL back with an allow entry for `sid`, which Windows
    /// propagates to everything beneath that inherits.
    fn add(&self, path: &Path, sid: &Sid, entries: &[(u32, u32)]) -> io::Result<()> {
        let accesses: Vec<EXPLICIT_ACCESS_W> = entries
            .iter()
            .map(|&(mask, inheritance)| EXPLICIT_ACCESS_W {
                grfAccessPermissions: mask,
                // Set, not grant: two entries for one SID that differ only
                // in inheritance must both stay rather than merge.
                grfAccessMode: SET_ACCESS,
                grfInheritance: inheritance,
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: ptr::null_mut(),
                    MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                    ptstrName: sid.sid.cast(),
                },
            })
            .collect();
        let mut merged: *mut ACL = ptr::null_mut();
        // SAFETY: `accesses` and the old ACL are valid; `merged` is
        // LocalAlloc'd and freed below.
        let status = unsafe {
            SetEntriesInAclW(
                accesses.len() as u32,
                accesses.as_ptr(),
                self.acl,
                &mut merged,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let name = wide(path.as_os_str());
        // SAFETY: `name` is NUL-terminated and `merged` a valid ACL.
        let status = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                merged,
                ptr::null(),
            )
        };
        // SAFETY: allocated by SetEntriesInAclW.
        unsafe { LocalFree(merged.cast()) };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(())
    }
}

impl Drop for Dacl {
    fn drop(&mut self) {
        // SAFETY: allocated by GetNamedSecurityInfoW with LocalAlloc.
        unsafe { LocalFree(self.descriptor) };
    }
}

// ------------------------------------------------------------ profile

/// The run's AppContainer profile, deleted when dropped (D1). Its folder
/// is where Windows points the program's `TEMP` and `LOCALAPPDATA`.
struct Profile {
    name: Vec<u16>,
    sid: Sid,
}

impl Profile {
    fn create(namespace: &str) -> io::Result<Self> {
        let pid = std::process::id();
        let Started::At(started) = process_started(pid) else {
            return Err(io::Error::other("this helper's own start time is unknown"));
        };
        let name = wide(OsStr::new(&profile_name(namespace, pid, started)));
        let mut sid: PSID = ptr::null_mut();
        // SAFETY: the strings are NUL-terminated; the SID is freed with
        // FreeSid, as the API documents.
        let mut created = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                ptr::null(),
                0,
                &mut sid,
            )
        };
        if created == hresult(ERROR_ALREADY_EXISTS) {
            // Left by an earlier helper with this pid, which is gone.
            // SAFETY: `name` is NUL-terminated.
            unsafe { DeleteAppContainerProfile(name.as_ptr()) };
            // SAFETY: as above.
            created = unsafe {
                CreateAppContainerProfile(
                    name.as_ptr(),
                    name.as_ptr(),
                    name.as_ptr(),
                    ptr::null(),
                    0,
                    &mut sid,
                )
            };
        }
        if created != 0 {
            return Err(io::Error::other(format!(
                "the run's AppContainer could not be made ({created:#x})"
            )));
        }
        Ok(Self {
            name,
            sid: Sid {
                sid,
                free: free_sid,
            },
        })
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        // SAFETY: `name` is NUL-terminated.
        unsafe { DeleteAppContainerProfile(self.name.as_ptr()) };
    }
}

/// Windows makes a profile's folder beneath `%LOCALAPPDATA%` and fails
/// with `ERROR_ENVVAR_NOT_FOUND` without it, while the runner hands the
/// helper only the environment a program may see. The folder is asked of
/// Windows instead; the program never sees it, since Windows points its
/// `LOCALAPPDATA` into the run's own profile.
fn ensure_local_app_data() -> io::Result<()> {
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
    };
    if std::env::var_os("LOCALAPPDATA").is_some() {
        return Ok(());
    }
    let mut path: *mut u16 = ptr::null_mut();
    // SAFETY: a known folder of this token; `path` is freed with
    // CoTaskMemFree, as the API documents, whatever it returned.
    let status =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, ptr::null_mut(), &mut path) };
    let folder = (status == 0).then(|| {
        // SAFETY: a NUL-terminated wide string from the call above.
        let length = (0..).take_while(|&i| unsafe { *path.add(i) } != 0).count();
        // SAFETY: `length` units precede the NUL.
        OsString::from_wide(unsafe { std::slice::from_raw_parts(path, length) })
    });
    // SAFETY: allocated by SHGetKnownFolderPath, or null.
    unsafe { CoTaskMemFree(path.cast()) };
    let folder = folder.ok_or_else(|| {
        io::Error::other(format!(
            "this user's local app data is unknown ({status:#x})"
        ))
    })?;
    // SAFETY: the helper has a single thread until it starts the program.
    unsafe { std::env::set_var("LOCALAPPDATA", folder) };
    Ok(())
}

/// Every run's profile is named this, followed by its helper's pid.
fn profile_prefix(namespace: &str) -> String {
    format!("{namespace}.run.")
}

/// A run's profile: its helper's pid and the time that process started,
/// which together name one process even after the pid is used again.
fn profile_name(namespace: &str, pid: u32, started: u64) -> String {
    format!("{}{pid}.{started}", profile_prefix(namespace))
}

/// The helper a profile named `moniker` was made by, when it is one of
/// this namespace's runs.
fn profile_owner(moniker: &str, prefix: &str) -> Option<(u32, u64)> {
    let (pid, started) = moniker.strip_prefix(prefix)?.split_once('.')?;
    Some((pid.parse().ok()?, started.parse().ok()?))
}

/// What Windows says of a pid when asked when it started.
#[derive(Debug, PartialEq, Eq)]
enum Started {
    /// It runs, and started at this time, in 100-nanosecond ticks.
    At(u64),
    /// Nothing runs as it: the one answer Windows gives plainly.
    Gone,
    /// It runs, or may, and this process may not ask (another user's, or
    /// one it is refused).
    Unknown,
}

fn process_started(pid: u32) -> Started {
    use windows_sys::Win32::{
        Foundation::{ERROR_INVALID_PARAMETER, FILETIME, GetLastError},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // SAFETY: opening a handle to ask its times; null on failure.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        // SAFETY: reads this thread's last error, set by the failed open.
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            Started::Gone
        } else {
            Started::Unknown
        };
    }
    let process = Owned(process);
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: a valid handle; every output outlives the call.
    let asked =
        unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) };
    if asked == 0 {
        return Started::Unknown;
    }
    Started::At((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// `HRESULT_FROM_WIN32`.
fn hresult(error: u32) -> i32 {
    ((error & 0xFFFF) | 0x8007_0000) as i32
}

/// Deletes the profile of every run whose helper is gone without deleting
/// its own, as a helper ended with its job does.
fn sweep_profiles(namespace: &str) {
    let prefix = profile_prefix(namespace);
    let Some(mappings) = Key::open(PROFILE_MAPPINGS) else {
        return;
    };
    let stale: Vec<String> = mappings
        .subkeys()
        .into_iter()
        .filter_map(|sid| {
            let moniker = crate::win::registry_string(
                HKEY_CURRENT_USER,
                &format!("{PROFILE_MAPPINGS}\\{sid}"),
                "Moniker",
                0,
            )
            .ok()
            .flatten()?
            .into_string()
            .ok()?;
            let (pid, started) = profile_owner(&moniker, &prefix)?;
            // A pid running again with another start time is another
            // process: the run that made this profile is gone.
            match process_started(pid) {
                Started::At(now) if now == started => None,
                Started::At(_) | Started::Gone => Some(moniker),
                // A run this process may not ask about is left alone.
                Started::Unknown => None,
            }
        })
        .collect();
    for moniker in stale {
        let name = wide(OsStr::new(&moniker));
        // SAFETY: `name` is NUL-terminated.
        unsafe { DeleteAppContainerProfile(name.as_ptr()) };
    }
}

struct Key(HKEY);

impl Key {
    fn open(path: &str) -> Option<Self> {
        let path = wide(OsStr::new(path));
        let mut key: HKEY = ptr::null_mut();
        // SAFETY: `path` is NUL-terminated; the key is closed in Drop.
        let opened =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) };
        (opened == 0).then_some(Self(key))
    }

    fn subkeys(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut buffer = [0u16; 256];
        for index in 0.. {
            let mut length = buffer.len() as u32;
            // SAFETY: `buffer` holds `length` units; the other outputs are
            // not asked for.
            let status = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    buffer.as_mut_ptr(),
                    &mut length,
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if status != 0 {
                break;
            }
            names.push(String::from_utf16_lossy(&buffer[..length as usize]));
        }
        names
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: opened by RegOpenKeyExW.
        unsafe { RegCloseKey(self.0) };
    }
}

// ---------------------------------------------------------------- job

/// A job that bounds what the program spends and ends everything it
/// started when the helper closes it (D4).
fn job(confinement: &Confinement) -> io::Result<Owned> {
    // SAFETY: an unnamed job with default security.
    let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = Owned(job);
    let limits = &confinement.limits;
    // SAFETY: zeroed is the documented empty limit structure.
    let mut extended: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    extended.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_PROCESS_TIME;
    extended.ProcessMemoryLimit = usize::try_from(limits.address_space).unwrap_or(usize::MAX);
    // In 100-nanosecond ticks.
    extended.BasicLimitInformation.PerProcessUserTimeLimit =
        i64::try_from(limits.cpu_seconds.saturating_mul(10_000_000)).unwrap_or(i64::MAX);
    set_information(&job, JobObjectExtendedLimitInformation, &extended)?;
    let interface = JOBOBJECT_BASIC_UI_RESTRICTIONS {
        UIRestrictionsClass: JOB_OBJECT_UILIMIT_DESKTOP
            | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
            | JOB_OBJECT_UILIMIT_EXITWINDOWS
            | JOB_OBJECT_UILIMIT_GLOBALATOMS
            | JOB_OBJECT_UILIMIT_HANDLES
            | JOB_OBJECT_UILIMIT_READCLIPBOARD
            | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
            | JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    };
    set_information(&job, JobObjectBasicUIRestrictions, &interface)?;
    Ok(job)
}

fn set_information<T>(
    job: &Owned,
    class: windows_sys::Win32::System::JobObjects::JOBOBJECTINFOCLASS,
    information: &T,
) -> io::Result<()> {
    // SAFETY: `information` is the structure `class` names, by size.
    let set = unsafe {
        SetInformationJobObject(
            job.0,
            class,
            (information as *const T).cast(),
            std::mem::size_of::<T>() as u32,
        )
    };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// -------------------------------------------------------------- child

struct Child {
    process: Owned,
    thread: Owned,
}

/// The program, created suspended in the run's container with exactly
/// `capabilities`, inheriting the standard handles and nothing else (D5).
fn start(argv: &[OsString], profile: &Profile, capabilities: &[Sid]) -> io::Result<Child> {
    let mut attributes: Vec<SID_AND_ATTRIBUTES> = capabilities
        .iter()
        .map(|capability| SID_AND_ATTRIBUTES {
            Sid: capability.sid,
            Attributes: ENABLED,
        })
        .collect();
    let security = SECURITY_CAPABILITIES {
        AppContainerSid: profile.sid.sid,
        Capabilities: if attributes.is_empty() {
            ptr::null_mut()
        } else {
            attributes.as_mut_ptr()
        },
        CapabilityCount: attributes.len() as u32,
        Reserved: 0,
    };
    let standard = standard_handles()?;
    let list = AttributeList::new(2)?;
    list.update(
        PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
        (&security as *const SECURITY_CAPABILITIES).cast(),
        std::mem::size_of::<SECURITY_CAPABILITIES>(),
    )?;
    let unique = unique_handles(&standard);
    if !unique.is_empty() {
        list.update(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            unique.as_ptr().cast(),
            std::mem::size_of_val(unique.as_slice()),
        )?;
    }
    // SAFETY: zeroed is a valid empty STARTUPINFOEXW once `cb` is set.
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = standard[0];
    startup.StartupInfo.hStdOutput = standard[1];
    startup.StartupInfo.hStdError = standard[2];
    startup.lpAttributeList = list.0.as_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
    let application = wide(&argv[0]);
    let mut command_line = command_line(argv);
    // SAFETY: zeroed is the documented empty output structure.
    let mut started: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: every pointer outlives the call; `command_line` is mutable
    // as the API requires; the environment and directory are the helper's.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            i32::from(!unique.is_empty()),
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED,
            ptr::null(),
            ptr::null(),
            &startup.StartupInfo,
            &mut started,
        )
    };
    drop(list);
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Child {
        process: Owned(started.hProcess),
        thread: Owned(started.hThread),
    })
}

/// The helper's standard handles, made inheritable; a missing one stays
/// null, which the program sees as no such stream.
fn standard_handles() -> io::Result<[HANDLE; 3]> {
    let mut handles = [ptr::null_mut(); 3];
    for (slot, which) in
        handles
            .iter_mut()
            .zip([STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE])
    {
        // SAFETY: asking for one of this process's standard handles.
        let handle = unsafe { GetStdHandle(which) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            continue;
        }
        // SAFETY: a valid handle of this process.
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            return Err(io::Error::last_os_error());
        }
        *slot = handle;
    }
    Ok(handles)
}

/// A handle list names each handle once, or the attribute is refused.
fn unique_handles(handles: &[HANDLE; 3]) -> Vec<HANDLE> {
    let mut unique: Vec<HANDLE> = Vec::new();
    for &handle in handles {
        if !handle.is_null() && !unique.contains(&handle) {
            unique.push(handle);
        }
    }
    unique
}

/// A process attribute list, freed when dropped.
struct AttributeList(Vec<usize>);

impl AttributeList {
    fn new(count: u32) -> io::Result<Self> {
        let mut size = 0usize;
        // SAFETY: a null list asks only for the size it needs.
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &mut size) };
        // Word-sized storage keeps the list aligned as the API needs.
        let mut storage = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        // SAFETY: `storage` holds at least `size` bytes.
        let initialised = unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), count, 0, &mut size)
        };
        if initialised == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(storage))
    }

    fn update(
        &self,
        attribute: usize,
        value: *const core::ffi::c_void,
        size: usize,
    ) -> io::Result<()> {
        // SAFETY: the list was initialised for this many attributes; the
        // value outlives the CreateProcessW call it is read by.
        let updated = unsafe {
            UpdateProcThreadAttribute(
                self.0.as_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST,
                0,
                attribute,
                value,
                size,
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if updated == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: initialised in `new`.
        unsafe { DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast()) };
    }
}

/// Whether Windows starts `program` through `cmd.exe`, which reads the
/// command line again by rules of its own that no quoting of the C
/// runtime's survives.
/// Windows opens a file by its name with any trailing dots and spaces
/// dropped, so the extension is read from that name.
fn is_batch(program: &Path) -> bool {
    let Some(name) = program.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let opened = Path::new(name.trim_end_matches(['.', ' ']));
    opened.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
    })
}

/// `argv` as one command line the C runtime splits back into the same
/// arguments: each quoted when it must be, backslashes doubled only before
/// a quote.
fn command_line(argv: &[OsString]) -> Vec<u16> {
    let mut line: Vec<u16> = Vec::new();
    for (index, argument) in argv.iter().enumerate() {
        if index > 0 {
            line.push(u16::from(b' '));
        }
        quote_into(&mut line, argument);
    }
    line.push(0);
    line
}

fn quote_into(line: &mut Vec<u16>, argument: &OsStr) {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;
    let units: Vec<u16> = argument.encode_wide().collect();
    let plain = !units.is_empty()
        && !units
            .iter()
            .any(|&unit| unit == u16::from(b' ') || unit == u16::from(b'\t') || unit == QUOTE);
    if plain {
        line.extend_from_slice(&units);
        return;
    }
    line.push(QUOTE);
    let mut backslashes = 0usize;
    for &unit in &units {
        if unit == BACKSLASH {
            backslashes += 1;
            continue;
        }
        let doubled = if unit == QUOTE {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        line.extend(std::iter::repeat_n(BACKSLASH, doubled));
        line.push(unit);
        backslashes = 0;
    }
    line.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
    line.push(QUOTE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(arguments: &[&str]) -> String {
        let argv: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        let mut wide = command_line(&argv);
        wide.pop();
        String::from_utf16(&wide).unwrap()
    }

    #[test]
    fn arguments_are_quoted_as_the_c_runtime_splits_them() {
        assert_eq!(line(&["C:\\a.exe", "plain"]), "C:\\a.exe plain");
        assert_eq!(line(&["C:\\a b\\c.exe", ""]), "\"C:\\a b\\c.exe\" \"\"");
        assert_eq!(line(&["x", "say \"hi\""]), "x \"say \\\"hi\\\"\"");
        assert_eq!(
            line(&["x", "C:\\dir with space\\"]),
            "x \"C:\\dir with space\\\\\""
        );
        assert_eq!(line(&["x", "a\\\\b c"]), "x \"a\\\\b c\"");
    }

    #[test]
    fn a_batch_file_is_told_from_a_program() {
        assert!(is_batch(Path::new("C:\\tools\\npx.cmd")));
        assert!(is_batch(Path::new("C:\\tools\\RUN.BAT")));
        assert!(!is_batch(Path::new("C:\\Windows\\System32\\cmd.exe")));
        assert!(!is_batch(Path::new("C:\\tools\\cmd")));
        for opened_the_same in ["tool.cmd.", "tool.bat ", "tool.cmd. .", "TOOL.Cmd.."] {
            let path = format!("C:\\tools\\{opened_the_same}");
            assert!(is_batch(Path::new(&path)), "{path}");
        }
    }

    #[test]
    fn a_profile_names_the_process_that_made_it() {
        let name = profile_name("uze", 4321, 133_000_000_000_000_000);
        assert_eq!(name, "uze.run.4321.133000000000000000");
        assert_eq!(
            profile_owner(&name, &profile_prefix("uze")),
            Some((4321, 133_000_000_000_000_000))
        );
        assert_eq!(profile_owner("uze.run.4321", &profile_prefix("uze")), None);
        assert_eq!(profile_owner(&name, &profile_prefix("other")), None);
    }

    #[test]
    fn this_process_knows_when_it_started() {
        assert!(matches!(process_started(std::process::id()), Started::At(started) if started > 0));
        // A pid this large is never handed out.
        assert_eq!(process_started(u32::MAX - 2), Started::Gone);
    }

    #[test]
    fn a_win32_error_reads_as_its_hresult() {
        assert_eq!(hresult(183) as u32, 0x8007_00B7);
    }
}
