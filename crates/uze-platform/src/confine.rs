//! Confinement: a program run with only the reach it was granted.
//!
//! A [`Confinement`] names what a program may read, read and execute, and
//! write, whether it may open an Internet socket, and the resources it may
//! spend. [`enter`] applies it to the calling process and then replaces that
//! process with the program, so it is meant for a helper process started for
//! that purpose alone, single-threaded, before it does anything else: on
//! Linux a Landlock ruleset and a seccomp filter, on macOS a Seatbelt
//! profile, and elsewhere nothing, said by [`availability`].
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

/// What confines a program's filesystem reach on this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Filesystem {
    /// The Linux Landlock LSM, at the ABI this kernel and UZE both know.
    Landlock { abi: u32 },
    /// macOS's Seatbelt, through `sandbox-exec`.
    Seatbelt,
    /// Nothing: a program runs with the reach of the user running it.
    Unavailable,
}

/// What this machine enforces, right by right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Availability {
    pub filesystem: Filesystem,
    /// Truncating a file outside the writable paths is refused (Landlock ABI 3).
    pub truncate: bool,
    /// TCP binding, and without network connecting, is refused by the
    /// filesystem mechanism itself (Landlock ABI 4).
    pub tcp: bool,
    /// `ioctl` on a device outside the granted paths is refused (Landlock ABI 5).
    pub ioctl_dev: bool,
    /// Abstract Unix sockets and signals stop at the confinement's edge
    /// (Landlock ABI 6); below it signals are limited by the syscall filter.
    pub scoping: bool,
    /// System calls are filtered (seccomp), which is what refuses sockets
    /// and `ptrace` on Linux.
    pub syscall_filter: bool,
    /// The program's address space is bounded (`RLIMIT_AS`); macOS refuses
    /// any bound below what a process already maps.
    pub memory: bool,
}

impl Availability {
    /// A machine that confines nothing.
    pub const fn unavailable() -> Self {
        Self {
            filesystem: Filesystem::Unavailable,
            truncate: false,
            tcp: false,
            ioctl_dev: false,
            scoping: false,
            syscall_filter: false,
            memory: false,
        }
    }

    /// What a Linux kernel answering Landlock `abi` (0 or less: none) and
    /// with seccomp filters or not enforces. Landlock alone does not keep a
    /// program off the network, so without seccomp nothing is available.
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
            tcp: abi >= 4,
            ioctl_dev: abi >= 5,
            scoping: abi >= 6,
            syscall_filter: true,
            memory: true,
        }
    }

    /// Whether a program can be confined here at all.
    pub fn is_available(&self) -> bool {
        self.filesystem != Filesystem::Unavailable
    }

    /// One line a person reads: what confines, and each right it does not.
    pub fn summary(&self) -> String {
        let mechanism = match self.filesystem {
            Filesystem::Unavailable => return "programs cannot be confined here".to_owned(),
            Filesystem::Landlock { abi } => format!("Landlock ABI {abi} and seccomp"),
            Filesystem::Seatbelt => "Seatbelt".to_owned(),
        };
        let missing: Vec<&str> = [
            (self.truncate, "truncate"),
            (self.tcp, "TCP"),
            (self.ioctl_dev, "device ioctl"),
            (self.scoping, "signal and abstract socket scoping"),
            (self.memory, "a memory bound"),
        ]
        .into_iter()
        .filter_map(|(enforced, right)| (!enforced).then_some(right))
        .collect();
        if missing.is_empty() {
            format!("confined by {mechanism}")
        } else {
            format!("confined by {mechanism}, without {}", missing.join(", "))
        }
    }
}

/// What this machine can enforce, asked of the kernel each time.
pub fn availability() -> Availability {
    imp::availability()
}

/// Applies `confinement` to this process and replaces it with `argv`
/// (`argv[0]` an absolute path), under the current environment. Returns only
/// when either fails, and then the program never ran: the caller ends with
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
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod unavailable;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use unavailable as imp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_landlock_abi_enforces_the_rights_it_introduced() {
        let v3 = Availability::of_linux(3, true);
        assert_eq!(v3.filesystem, Filesystem::Landlock { abi: 3 });
        assert!(v3.truncate && !v3.tcp && !v3.ioctl_dev && !v3.scoping);
        let v6 = Availability::of_linux(6, true);
        assert!(v6.truncate && v6.tcp && v6.ioctl_dev && v6.scoping);
        assert!(v6.syscall_filter);
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
    fn the_summary_names_each_right_not_enforced() {
        assert_eq!(
            Availability::of_linux(7, true).summary(),
            "confined by Landlock ABI 7 and seccomp"
        );
        assert_eq!(
            Availability::of_linux(3, true).summary(),
            "confined by Landlock ABI 3 and seccomp, without TCP, device ioctl, \
             signal and abstract socket scoping"
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

    #[test]
    fn a_relative_program_is_refused_before_anything_is_applied() {
        let confinement = Confinement {
            read: vec![],
            read_execute: vec![],
            write: vec![],
            devices: Vec::new(),
            network: false,
            limits: Limits::with_cpu_seconds(1),
        };
        let error = enter(&confinement, &["sh".into()]);
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(enter(&confinement, &[]).kind(), io::ErrorKind::InvalidInput);
    }
}
