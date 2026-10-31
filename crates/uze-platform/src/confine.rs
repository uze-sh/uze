//! Confinement: a program run with only the reach it was granted.
//!
//! A [`Confinement`] names what a program may read, read and execute, and
//! write, whether it may open an Internet socket, and the resources it may
//! spend. [`enter`] applies it to the calling process and then replaces that
//! process with the program, so it is meant for a helper process started for
//! that purpose alone, single-threaded, before it does anything else: on
//! Linux a Landlock ruleset and a seccomp filter, on macOS a Seatbelt
//! profile, on Windows an AppContainer made for the run inside a Job
//! Object, and elsewhere nothing, said by [`availability`]. Windows cannot
//! replace a process, so there the helper starts the program, waits for
//! it and ends with its exit code.
//!
//! What this machine enforces is reported right by right, because a kernel
//! can enforce some rights and not others; what it cannot enforce is never
//! applied as a silent best effort: [`enter`] fails instead.

use std::{ffi::OsString, io, path::PathBuf};

use serde::{Deserialize, Serialize};

/// The exit status a confinement helper ends with when the confinement
/// could not be applied, so the program never ran: distinct from any status
/// of the program's own, which only begins after `execve`.
pub const NOT_ENFORCED_EXIT: i32 = 125;

/// What a confined program may reach. Every path must exist when the
/// confinement is entered: one that does not is a failure, never a rule
/// skipped, so a caller lists only what it found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confinement {
    /// Read beneath, never execute.
    pub read: Vec<PathBuf>,
    /// Read and execute beneath: the program itself, its interpreter and
    /// the libraries they load.
    pub read_execute: Vec<PathBuf>,
    /// Read, write, create, remove and truncate beneath, never execute.
    /// `/dev/null` is the caller's to list.
    pub write: Vec<PathBuf>,
    /// Devices a program speaks to: read, write and `ioctl` on each, never
    /// create or remove. A GPU's control node (`/dev/nvidiactl`,
    /// `/dev/dxg`) is what one declares.
    #[serde(default)]
    pub devices: Vec<PathBuf>,
    /// Whether the program may open an Internet socket (`AF_INET`,
    /// `AF_INET6`) and connect it. Without it no socket opens at all; with
    /// it, still no Unix socket and no listening TCP port.
    pub network: bool,
    pub limits: Limits,
    /// The name the objects a confinement makes for itself go under, the
    /// caller's to choose (on Windows, the run's container and the
    /// capability each grant writes): never empty, on every platform.
    pub namespace: String,
}

/// What a confined program may spend, as resource limits on its process.
/// The number of processes is not one of them: that limit counts every
/// process of the user, the agents' included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub cpu_seconds: u64,
    pub file_size: u64,
    pub open_files: u64,
    pub address_space: u64,
}

impl Limits {
    pub const FILE_SIZE: u64 = 64 * 1024 * 1024;
    pub const OPEN_FILES: u64 = 256;
    pub const ADDRESS_SPACE: u64 = 2 * 1024 * 1024 * 1024;

    /// The limits for a run bounded by `cpu_seconds` of processor time
    /// (twice the run's time limit, so the wall clock is what ends a run
    /// that waits and the processor limit only one that spins).
    pub fn with_cpu_seconds(cpu_seconds: u64) -> Self {
        Self {
            cpu_seconds: cpu_seconds.max(1),
            file_size: Self::FILE_SIZE,
            open_files: Self::OPEN_FILES,
            address_space: Self::ADDRESS_SPACE,
        }
    }
}

/// What every confined program needs of the system it runs on, beyond what
/// its own run grants. Only the paths that exist are to be granted, since a
/// grant of a missing path fails the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct System {
    /// Where the system keeps its programs and the libraries they load:
    /// read and executed beneath.
    pub read_execute: &'static [&'static str],
    /// What a program reads that is no file of anybody's: randomness and
    /// the empty devices every runtime opens.
    pub read: &'static [&'static str],
    /// What name resolution reads, for a program given the network.
    pub resolver: &'static [&'static str],
    /// What a program writes what it discards to, where that is a path.
    pub discard: &'static [&'static str],
    /// Beneath each, a path a program declares is a device: spoken to, not
    /// read.
    pub device_roots: &'static [&'static str],
}

impl System {
    /// A system whose own files every confined program already reaches.
    pub const NONE: Self = Self {
        read_execute: &[],
        read: &[],
        resolver: &[],
        discard: &[],
        device_roots: &[],
    };
}

/// A Unix system's: what Landlock and Seatbelt are told about.
#[cfg(unix)]
const UNIX: System = System {
    read_execute: &[
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib32",
        "/lib64",
        "/libx32",
        "/etc",
        "/opt",
        "/nix/store",
        "/snap",
    ],
    read: &[
        "/dev/zero",
        "/dev/random",
        "/dev/urandom",
        "/proc/self",
        "/proc/sys/kernel/random",
        "/proc/sys/kernel/osrelease",
    ],
    resolver: &[
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/gai.conf",
    ],
    discard: &["/dev/null"],
    device_roots: &["/dev/"],
};

/// This machine's [`System`].
pub fn system() -> System {
    imp::SYSTEM
}

/// What confines a program on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Filesystem {
    /// The Linux Landlock LSM, at the ABI this kernel and UZE both know,
    /// with a seccomp filter.
    Landlock { abi: u32 },
    /// macOS's Seatbelt, through `sandbox-exec`.
    Seatbelt,
    /// An AppContainer of the run's own, in a Job Object (Windows).
    AppContainer,
    /// Nothing: a program runs with the reach of the user running it.
    Unavailable,
}

/// What this machine enforces, each field an outcome: what a confined
/// program cannot do, or which of its resources is bounded. Every platform
/// answers every field, whatever mechanism it answers with, so a person
/// compares machines by what a program could do on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Availability {
    pub filesystem: Filesystem,
    /// It cannot empty a file it may only read.
    pub truncate: bool,
    /// Without network it reaches no host; with it, Internet hosts and
    /// never a local socket.
    pub network: bool,
    /// Nothing reaches a port it listens on.
    pub listening: bool,
    /// It reaches no device it was not granted.
    pub devices: bool,
    /// It cannot signal, debug or read the memory of a process outside its
    /// run.
    pub processes: bool,
    /// Nothing it starts outlives its run.
    pub tree: bool,
    pub memory: bool,
    /// Its processor time is bounded: user and system time on Unix, user
    /// time alone in a Windows job, the run's wall clock ending the rest.
    pub cpu: bool,
    pub file_size: bool,
    pub open_files: bool,
}

impl Availability {
    /// A machine that confines nothing.
    pub const fn unavailable() -> Self {
        Self {
            filesystem: Filesystem::Unavailable,
            truncate: false,
            network: false,
            listening: false,
            devices: false,
            processes: false,
            tree: false,
            memory: false,
            cpu: false,
            file_size: false,
            open_files: false,
        }
    }

    /// What a Linux kernel answering Landlock `abi` (0 or less: none) and
    /// with seccomp filters or not enforces. Landlock alone does not keep a
    /// program off the network, so without seccomp nothing is available.
    /// The filter refuses `listen` and every socket but the Internet's at
    /// any ABI; signals stop at the run's edge only from ABI 6, below
    /// which the filter checks their target as far as an argument can say.
    pub fn of_linux(abi: i32, seccomp: bool) -> Self {
        let Ok(abi) = u32::try_from(abi) else {
            return Self::unavailable();
        };
        if abi == 0 || !seccomp {
            return Self::unavailable();
        }
        Self {
            filesystem: Filesystem::Landlock { abi },
            truncate: abi >= 3,
            network: true,
            listening: true,
            devices: abi >= 5,
            processes: abi >= 6,
            tree: true,
            memory: true,
            cpu: true,
            file_size: true,
            open_files: true,
        }
    }

    /// What Seatbelt enforces. Seatbelt filters no system call, so a
    /// program can leave the process group its run is ended by. macOS
    /// bounds no process's memory for it either: `RLIMIT_AS` is accepted
    /// only above the hundreds of GiB a process already maps and then
    /// refuses `mmap` alone, never `malloc`.
    pub const fn of_seatbelt() -> Self {
        Self {
            filesystem: Filesystem::Seatbelt,
            truncate: true,
            network: true,
            listening: true,
            devices: true,
            processes: true,
            tree: false,
            memory: false,
            cpu: true,
            file_size: true,
            open_files: true,
        }
    }

    /// What an AppContainer in a Job Object enforces. A job bounds memory
    /// and processor time and nothing else a process spends.
    pub const fn of_app_container() -> Self {
        Self {
            filesystem: Filesystem::AppContainer,
            truncate: true,
            network: true,
            listening: true,
            devices: true,
            processes: true,
            tree: true,
            memory: true,
            cpu: true,
            file_size: false,
            open_files: false,
        }
    }

    /// Whether a program can be confined here at all.
    pub fn is_available(&self) -> bool {
        self.filesystem != Filesystem::Unavailable
    }

    /// One line a person reads: what confines, what a program can still
    /// reach, and which limits it runs without.
    pub fn summary(&self) -> String {
        let mechanism = match self.filesystem {
            Filesystem::Unavailable => return "programs cannot be confined here".to_owned(),
            Filesystem::Landlock { abi } => format!("Landlock ABI {abi} and seccomp"),
            Filesystem::Seatbelt => "Seatbelt".to_owned(),
            Filesystem::AppContainer => "AppContainer".to_owned(),
        };
        let reaches = unenforced(&[
            (self.network, "reach the network"),
            (self.listening, "accept connections"),
            (self.truncate, "empty files it may only read"),
            (self.devices, "reach ungranted devices"),
            (self.processes, "signal other processes"),
            (self.tree, "outlive its run"),
        ]);
        let limits = unenforced(&[
            (self.memory, "memory"),
            (self.cpu, "processor time"),
            (self.file_size, "file size"),
            (self.open_files, "open files"),
        ]);
        let mut line = format!("confined by {mechanism}");
        if !limits.is_empty() {
            line.push_str(&format!(", without a {} limit", listed(&limits, "or")));
        }
        if !reaches.is_empty() {
            let joint = if limits.is_empty() { ", but" } else { ", and" };
            line.push_str(&format!(
                "{joint} a program can still {}",
                listed(&reaches, "and")
            ));
        }
        line
    }
}

fn unenforced<'a>(rights: &[(bool, &'a str)]) -> Vec<&'a str> {
    rights
        .iter()
        .filter_map(|&(enforced, right)| (!enforced).then_some(right))
        .collect()
}

/// `a`, `a or b`, `a, b or c`.
fn listed(items: &[&str], conjunction: &str) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [init @ .., last] => format!("{} {conjunction} {last}", init.join(", ")),
    }
}

/// What this machine can enforce, asked of the kernel each time.
pub fn availability() -> Availability {
    imp::availability()
}

/// Applies `confinement` to this process and becomes `argv` (`argv[0]` an
/// absolute path), under the current environment: on Unix it replaces this
/// process; on Windows it starts the program, waits for it and ends this
/// process with its exit code. Returns only when the confinement or the
/// start fails, and then the program never ran: the caller ends with
/// [`NOT_ENFORCED_EXIT`]. Meant for a process started for this alone, while
/// it has a single thread.
pub fn enter(confinement: &Confinement, argv: &[OsString]) -> io::Error {
    let Some(program) = argv.first() else {
        return io::Error::new(io::ErrorKind::InvalidInput, "no program to run");
    };
    if !std::path::Path::new(program).is_absolute() {
        return io::Error::new(
            io::ErrorKind::InvalidInput,
            "the program to run must be an absolute path",
        );
    }
    if confinement.namespace.is_empty() {
        return io::Error::new(
            io::ErrorKind::InvalidInput,
            "a confinement names the namespace it makes things under",
        );
    }
    imp::enter(confinement, argv)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod rlimit;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod seatbelt;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod unavailable;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "linux")]
pub(crate) use linux::landlock_abi;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
use unavailable as imp;
#[cfg(windows)]
use windows as imp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_landlock_abi_enforces_the_outcomes_it_introduced() {
        let v2 = Availability::of_linux(2, true);
        assert!(!v2.truncate && !v2.devices && !v2.processes);
        assert!(v2.network && v2.listening, "the filter's at every ABI");
        let v3 = Availability::of_linux(3, true);
        assert_eq!(v3.filesystem, Filesystem::Landlock { abi: 3 });
        assert!(v3.truncate && !v3.devices && !v3.processes);
        let v5 = Availability::of_linux(5, true);
        assert!(v5.devices && !v5.processes);
        let v6 = Availability::of_linux(6, true);
        assert!(v6.truncate && v6.devices && v6.processes);
        assert!(v6.memory && v6.cpu && v6.file_size && v6.open_files);
    }

    #[test]
    fn no_landlock_or_no_seccomp_is_unavailable() {
        for (abi, seccomp) in [(0, true), (-1, true), (7, false)] {
            let availability = Availability::of_linux(abi, seccomp);
            assert_eq!(availability, Availability::unavailable());
            assert!(!availability.is_available());
        }
    }

    #[test]
    fn the_summary_names_what_a_program_can_still_do_and_the_limits_it_lacks() {
        assert_eq!(
            Availability::of_linux(7, true).summary(),
            "confined by Landlock ABI 7 and seccomp"
        );
        assert_eq!(
            Availability::of_linux(3, true).summary(),
            "confined by Landlock ABI 3 and seccomp, but a program can still \
             reach ungranted devices and signal other processes"
        );
        assert_eq!(
            Availability::of_linux(2, true).summary(),
            "confined by Landlock ABI 2 and seccomp, but a program can still \
             empty files it may only read, reach ungranted devices and signal other processes"
        );
        assert_eq!(
            Availability::of_seatbelt().summary(),
            "confined by Seatbelt, without a memory limit, and a program can still \
             outlive its run"
        );
        assert_eq!(
            Availability::of_app_container().summary(),
            "confined by AppContainer, without a file size or open files limit"
        );
        let mut reach_only = Availability::of_app_container();
        reach_only.file_size = true;
        reach_only.open_files = true;
        reach_only.devices = false;
        assert_eq!(
            reach_only.summary(),
            "confined by AppContainer, but a program can still reach ungranted devices"
        );
        assert_eq!(
            Availability::unavailable().summary(),
            "programs cannot be confined here"
        );
    }

    #[test]
    fn a_confinement_crosses_a_process_as_json_and_refuses_an_unknown_field() {
        let confinement = Confinement {
            read: vec!["/r".into()],
            read_execute: vec!["/usr".into()],
            write: vec!["/dev/null".into()],
            devices: Vec::new(),
            network: false,
            limits: Limits::with_cpu_seconds(20),
            namespace: "probe".to_owned(),
        };
        let json = serde_json::to_string(&confinement).unwrap();
        assert_eq!(
            serde_json::from_str::<Confinement>(&json).unwrap(),
            confinement
        );
        let widened = json.replacen('{', "{\"unrestricted\":true,", 1);
        assert!(serde_json::from_str::<Confinement>(&widened).is_err());
    }

    #[test]
    fn limits_follow_the_design_and_never_reach_zero_cpu() {
        let limits = Limits::with_cpu_seconds(0);
        assert_eq!(limits.cpu_seconds, 1);
        assert_eq!(limits.file_size, 64 << 20);
        assert_eq!(limits.open_files, 256);
        assert_eq!(limits.address_space, 2 << 30);
    }

    /// A Unix path means nothing on Windows: `/usr` there is a folder at
    /// the root of whatever drive the run starts on.
    #[test]
    fn only_a_unix_system_names_unix_paths() {
        if cfg!(windows) {
            assert_eq!(system(), System::NONE);
        } else if cfg!(unix) {
            assert_eq!(system().discard, ["/dev/null"]);
            assert!(system().read_execute.contains(&"/usr"));
        }
    }

    #[test]
    fn a_relative_program_is_refused_before_anything_is_applied() {
        let confinement = Confinement {
            read: vec![],
            read_execute: vec![],
            write: vec![],
            devices: Vec::new(),
            network: false,
            limits: Limits::with_cpu_seconds(1),
            namespace: "probe".to_owned(),
        };
        let error = enter(&confinement, &["sh".into()]);
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(enter(&confinement, &[]).kind(), io::ErrorKind::InvalidInput);
        let unnamed = Confinement {
            namespace: String::new(),
            ..confinement
        };
        let absolute = std::env::current_exe().unwrap().into_os_string();
        assert_eq!(
            enter(&unnamed, &[absolute]).kind(),
            io::ErrorKind::InvalidInput,
            "a confinement without its namespace is refused before anything applies"
        );
    }
}
