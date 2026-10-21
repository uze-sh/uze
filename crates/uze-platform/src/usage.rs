//! What a process uses and what the machine has: the CPU time a process
//! spent, the memory it holds that is its own, and the cores and memory
//! there are to share.
//!
//! A reading is cumulative, taken at one instant; a rate is the caller's
//! to compute from two of them.

use crate::process::CpuTime;

/// What this machine has to give.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Machine {
    /// The operating system as a person names it, with WSL said when it
    /// is: `linux (WSL)`, `macos`, `windows`.
    pub system: String,
    /// The processor architecture: `x86_64`, `aarch64`.
    pub architecture: &'static str,
    /// The cores this process may run on.
    pub cores: usize,
    /// Physical memory in bytes; `None` where the platform does not say.
    pub memory: Option<u64>,
}

/// This machine, as [`Machine`] describes it.
pub fn machine() -> Machine {
    Machine {
        system: imp::system(),
        architecture: std::env::consts::ARCH,
        cores: std::thread::available_parallelism().map_or(1, usize::from),
        memory: imp::total_memory(),
    }
}

/// Memory a process holds, counting what it shares with others only by
/// its share of it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Memory {
    pub bytes: u64,
    /// The platform would only give resident size, which counts every
    /// shared page in full.
    pub estimated: bool,
}

/// One process at one instant.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    /// [`crate::probe::started_at`]'s stamp: what tells this process from
    /// a later one that was given its pid, and, newer than every stamp of
    /// an earlier reading, a process started since.
    pub started: Option<u64>,
    /// The CPU it has spent, and where the platform accounts them to it,
    /// the children it has waited for.
    pub cpu: CpuTime,
    pub memory: Memory,
}

/// `pid` as [`Reading`] describes it, or `None` when it has ended, is not
/// this user's, or the platform does not say.
pub fn of(pid: u32) -> Option<Reading> {
    imp::of(pid)
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{CpuTime, Memory, Reading};
    use std::time::Duration;

    pub(super) fn system() -> String {
        if std::env::var_os("WSL_DISTRO_NAME").is_some() {
            "linux (WSL)".to_owned()
        } else {
            "linux".to_owned()
        }
    }

    pub(super) fn total_memory() -> Option<u64> {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        kibibytes_after(&meminfo, "MemTotal:")
    }

    pub(super) fn of(pid: u32) -> Option<Reading> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name is parenthesised and may hold spaces and
        // parentheses of its own; the fields after it are what is read.
        let (_, after_name) = stat.rsplit_once(')')?;
        let fields: Vec<&str> = after_name.split_whitespace().collect();
        let ticks = |index: usize| fields.get(index)?.parse::<u64>().ok();
        // SAFETY: `sysconf` reads a constant and touches no memory of ours.
        let per_second = u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) })
            .ok()
            .filter(|&ticks| ticks > 0)?;
        let time = |ticks: u64| Duration::from_nanos(ticks * 1_000_000_000 / per_second);
        let (user, system) = (ticks(11)?, ticks(12)?);
        let (children_user, children_system) = (ticks(13)?, ticks(14)?);
        Some(Reading {
            started: ticks(19),
            cpu: CpuTime {
                own: time(user + system),
                children: Some(time(children_user + children_system)),
            },
            memory: memory_of(pid)?,
        })
    }

    /// The proportional set size, which divides every shared page among
    /// the processes sharing it. `smaps_rollup` is 4.14's; before it, or
    /// under a `/proc` that withholds it, resident pages are all there is.
    fn memory_of(pid: u32) -> Option<Memory> {
        if let Some(bytes) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
            .ok()
            .and_then(|rollup| kibibytes_after(&rollup, "Pss:"))
        {
            return Some(Memory {
                bytes,
                estimated: false,
            });
        }
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        // SAFETY: `sysconf` reads a constant and touches no memory of ours.
        let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
        Some(Memory {
            bytes: pages * page,
            estimated: true,
        })
    }

    fn kibibytes_after(text: &str, key: &str) -> Option<u64> {
        let line = text.lines().find(|line| line.starts_with(key))?;
        let kib: u64 = line[key.len()..].split_whitespace().next()?.parse().ok()?;
        Some(kib * 1024)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{CpuTime, Memory, Reading};
    use std::time::Duration;

    pub(super) fn system() -> String {
        "macos".to_owned()
    }

    pub(super) fn total_memory() -> Option<u64> {
        let mut memory: u64 = 0;
        let mut size = size_of::<u64>();
        // SAFETY: the name is NUL-terminated, and `sysctlbyname` writes at
        // most `size` bytes into `memory`, which is that value's own size.
        let asked = unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                (&raw mut memory).cast(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        (asked == 0).then_some(memory)
    }

    pub(super) fn of(pid: u32) -> Option<Reading> {
        let pid_t = libc::pid_t::try_from(pid).ok().filter(|&pid| pid > 0)?;
        // SAFETY: `rusage_info_v2` is plain data, and `proc_pid_rusage`
        // writes one of the flavour asked for into it.
        let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
        let asked =
            unsafe { libc::proc_pid_rusage(pid_t, libc::RUSAGE_INFO_V2, (&raw mut info).cast()) };
        if asked != 0 {
            return None;
        }
        let time = mach_time();
        Some(Reading {
            started: crate::probe::started_at(pid),
            cpu: CpuTime {
                own: time(info.ri_user_time + info.ri_system_time),
                children: Some(time(info.ri_child_user_time + info.ri_child_system_time)),
            },
            memory: Memory {
                bytes: info.ri_phys_footprint,
                estimated: false,
            },
        })
    }

    /// `rusage_info` times are Mach absolute units, nanoseconds on Intel and
    /// ticks of the timebase on Apple silicon.
    ///
    /// `libc` deprecates its Mach bindings in favour of the `mach2` crate, a
    /// dependency for one call whose interface has not changed since it was
    /// published.
    #[allow(deprecated)]
    fn mach_time() -> impl Fn(u64) -> Duration {
        let mut timebase = libc::mach_timebase_info { numer: 1, denom: 1 };
        // SAFETY: writes the two fields of `timebase` and nothing else.
        let asked = unsafe { libc::mach_timebase_info(&raw mut timebase) };
        let (numer, denom) = if asked == 0 && timebase.denom != 0 {
            (u128::from(timebase.numer), u128::from(timebase.denom))
        } else {
            (1, 1)
        };
        move |units| {
            Duration::from_nanos(
                u64::try_from(u128::from(units) * numer / denom).unwrap_or(u64::MAX),
            )
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{CpuTime, Memory, Reading};
    use crate::win::Owned;
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2},
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    pub(super) fn system() -> String {
        "windows".to_owned()
    }

    pub(super) fn total_memory() -> Option<u64> {
        // SAFETY: zeroed is valid once dwLength is set.
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: `status` is sized as the API requires.
        (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys)
    }

    /// Windows accounts no child to its parent; a pane's job does, which
    /// is where a pane's short-lived processes are counted
    /// ([`crate::process::pane::Group::cpu_time`]).
    pub(super) fn of(pid: u32) -> Option<Reading> {
        // SAFETY: opening a handle to another process; null on failure.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let process = Owned(handle);
        // SAFETY: zeroed FILETIMEs are valid outputs.
        let mut times: [windows_sys::Win32::Foundation::FILETIME; 4] =
            unsafe { std::mem::zeroed() };
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: valid handle; every out-pointer outlives the call.
        if unsafe { GetProcessTimes(process.0, created, exited, kernel, user) } == 0 {
            return None;
        }
        let ticks = |time: &windows_sys::Win32::Foundation::FILETIME| {
            (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
        };
        // SAFETY: zeroed is valid once cb is set.
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = unsafe { std::mem::zeroed() };
        counters.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        // SAFETY: `counters` is sized as `cb` says; the API reads `cb` to
        // know which structure it was handed.
        let asked =
            unsafe { GetProcessMemoryInfo(process.0, (&raw mut counters).cast(), counters.cb) };
        // Windows before 10 1809 fills only the older structure, leaving the
        // private working set zero: resident size is then all there is.
        let memory = match (asked != 0, counters.PrivateWorkingSetSize) {
            (false, _) => return None,
            (true, 0) => Memory {
                bytes: counters.WorkingSetSize as u64,
                estimated: true,
            },
            (true, private) => Memory {
                bytes: private as u64,
                estimated: false,
            },
        };
        Some(Reading {
            started: Some(ticks(&times[0])),
            cpu: CpuTime {
                own: std::time::Duration::from_nanos((ticks(&times[2]) + ticks(&times[3])) * 100),
                children: None,
            },
            memory,
        })
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
mod imp {
    use super::Reading;

    pub(super) fn system() -> String {
        std::env::consts::OS.to_owned()
    }

    pub(super) fn total_memory() -> Option<u64> {
        None
    }

    pub(super) fn of(_pid: u32) -> Option<Reading> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn this_process_is_read_with_its_cpu_and_memory() {
        let before = of(std::process::id()).expect("this process reads");
        let started = std::time::Instant::now();
        let mut spin = 0u64;
        while started.elapsed() < Duration::from_millis(200) {
            spin = std::hint::black_box(spin.wrapping_add(1));
        }
        let after = of(std::process::id()).expect("this process reads");
        assert!(after.cpu.own > before.cpu.own, "{before:?} then {after:?}");
        assert!(after.memory.bytes > 0);
        assert_eq!(before.started, after.started);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_waited_for_is_accounted_to_its_parent() {
        let before = of(std::process::id()).expect("this process reads");
        std::process::Command::new("sh")
            .args(["-c", "i=0; while [ $i -lt 200000 ]; do i=$((i+1)); done"])
            .status()
            .expect("sh runs");
        let after = of(std::process::id()).expect("this process reads");
        assert!(
            after.cpu.children > before.cpu.children,
            "{before:?} then {after:?}"
        );
    }

    #[test]
    fn a_pid_nobody_holds_reads_nothing() {
        assert!(of(u32::MAX - 1).is_none());
    }

    #[test]
    fn the_machine_has_at_least_one_core() {
        let machine = machine();
        assert!(machine.cores >= 1);
        assert!(!machine.system.is_empty());
    }
}
