//! Processes on Unix: process groups, sessions and signals, through
//! `kill(2)` itself.
//!
//! Never a shell-out to `/bin/kill`: procps-ng's `kill` parses a negative
//! pid by its first digit only, so `kill -KILL -1234` becomes
//! `kill(-1, SIGKILL)` — every process the user owns. That once took the
//! whole WSL session down whenever a timed-out child's pid started with `1`.

use std::{io, os::unix::process::CommandExt, process::Command};

/// `pid` as something `kill(2)` may be given — only where it names one
/// process. `0` is the caller's own group, a negative pid a group, `-1`
/// every process the user owns.
fn single(pid: u32) -> Option<libc::pid_t> {
    libc::pid_t::try_from(pid).ok().filter(|pid| *pid > 1)
}

pub(super) fn alive(pid: u32) -> Option<bool> {
    let pid = single(pid)?;
    // SAFETY: signal 0 checks existence only.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return Some(true);
    }
    // EPERM: it exists, and is somebody else's.
    Some(io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

pub(super) fn terminate(pid: u32) {
    if let Some(pid) = single(pid) {
        // SAFETY: a positive pid naming one process.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
}

pub(super) fn request_stop(pid: u32, _channel: &str) {
    if let Some(pid) = single(pid) {
        // SAFETY: as above.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

pub(super) fn listen_for_stop(_channel: &str, _on_stop: impl FnOnce() + Send + 'static) {}

pub(super) struct Tree(u32);

impl Tree {
    pub(super) fn end(&self, leader_unreaped: bool) {
        signal_group(self.0, leader_unreaped);
    }
}

pub(super) fn spawn_tree(
    command: &mut Command,
    seat: super::Seat,
) -> io::Result<(std::process::Child, Tree)> {
    match seat {
        super::Seat::OwnGroup => {
            command.process_group(0);
        }
        // A session of its own is also a group of its own, led by the
        // child: `setsid` refuses a process that already leads one.
        // SAFETY: `setsid` is async-signal-safe and touches no memory,
        // which is all a `pre_exec` hook may do between fork and exec.
        super::Seat::NoTerminal => unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        },
    }
    let child = command.spawn()?;
    let tree = Tree(child.id());
    Ok((child, tree))
}

fn signal_group(pid: u32, leader_unreaped: bool) {
    let Some(pgid) = single(pid) else {
        return;
    };
    // SAFETY: plain `kill(2)` calls on a group this process started.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
        // Also the child itself, in case it changed its own group before
        // the group signal landed.
        if leader_unreaped {
            libc::kill(pgid, libc::SIGKILL);
        }
    }
    // Belt and braces for a descendant that left the group (`setsid`)
    // between the two signals — a WSL2 quirk. The signals above are what
    // end the group; a platform without `/proc` loses the backstop only.
    for member in group_members(pid) {
        if let Some(member) = single(member) {
            // SAFETY: as above.
            unsafe { libc::kill(member, libc::SIGKILL) };
        }
    }
}

/// Every pid `/proc` reports in group `pgid`; empty without `/proc`.
fn group_members(pgid: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
            let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
            // comm is parenthesized and may hold spaces or parens itself.
            let (_, after_comm) = stat.rsplit_once(')')?;
            // After comm: state(0) ppid(1) pgrp(2) ...
            let group = after_comm.split_whitespace().nth(2)?.parse::<u32>().ok()?;
            (group == pgid).then_some(pid)
        })
        .collect()
}

pub(super) fn wait_without_reaping(pid: u32) {
    let id = libc::id_t::from(pid);
    // SAFETY: plain data the kernel fills in; zeroed is valid for it.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: `info` outlives the call; WNOWAIT leaves the child for
        // `Child::wait` to reap.
        let outcome =
            unsafe { libc::waitid(libc::P_PID, id, &mut info, libc::WEXITED | libc::WNOWAIT) };
        if outcome == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return;
        }
    }
}

pub(super) fn spawn_detached(command: &mut Command) -> io::Result<super::Detached> {
    // A group of its own, or a `SIGHUP` when the launching terminal closes
    // and a `Ctrl+C` to its foreground group reach the child.
    Ok(super::Detached {
        child: command.process_group(0).spawn()?,
        outlives_host: true,
    })
}

pub(super) const SYSTEM_ENVIRONMENT: &[&str] = &[];

pub(super) fn launched_by(pid: u32, launcher: u32) -> bool {
    pid == launcher
}

/// `exec`: the program replaces this one, keeping its pid.
pub(super) fn run_in_place(command: &mut Command) -> io::Error {
    command.exec()
}

pub(super) fn cpu_time() -> super::CpuTime {
    let of = |who| {
        // SAFETY: `rusage` is plain data, and `getrusage` only writes it.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(who, &mut usage) };
        let time = |t: libc::timeval| {
            std::time::Duration::from_secs(t.tv_sec as u64)
                + std::time::Duration::from_micros(t.tv_usec as u64)
        };
        time(usage.ru_utime) + time(usage.ru_stime)
    };
    super::CpuTime {
        own: of(libc::RUSAGE_SELF),
        children: Some(of(libc::RUSAGE_CHILDREN)),
    }
}

pub(super) fn current_user() -> io::Result<String> {
    // SAFETY: `getuid` takes no arguments and cannot fail.
    Ok(unsafe { libc::getuid() }.to_string())
}

pub(super) struct Group(libc::pid_t);

impl Group {
    /// A pane's program starts a session of its own, so its group is its
    /// pid. Anything else — above all this process's own group, which a
    /// group signal must never reach — is not adopted.
    pub(super) fn adopt(pid: u32) -> Option<Self> {
        let pid = single(pid)?;
        // SAFETY: `getpgid` reads the group of a positive pid; `getpgrp`
        // takes no arguments.
        let (group, ours) = unsafe { (libc::getpgid(pid), libc::getpgrp()) };
        (group == pid && group != ours).then_some(Self(group))
    }

    pub(super) fn end(&self) {
        // SAFETY: a positive group id that is the pane's own, not ours.
        unsafe { libc::kill(-self.0, libc::SIGKILL) };
    }

    pub(super) fn members(&self) -> Vec<u32> {
        group_members(self.0 as u32)
    }
}

pub(super) fn foreground(
    leader: u32,
    _group: Option<&Group>,
    _passes_on: impl Fn(&super::Member, &super::Member) -> bool,
) -> Option<u32> {
    crate::probe::terminal_foreground_of(leader)
}

pub(super) fn finding_command(program: &str, arguments: &str) -> String {
    format!("`pgrep -fa '{program} {arguments}'`")
}

#[cfg(test)]
mod tests {
    /// `kill(2)` reads `0` as the caller's own process group and a negative
    /// pid as a group, `-1` as every process the user owns. A pid that does
    /// not fit is never turned into one of those.
    #[test]
    fn a_pid_that_does_not_name_one_process_is_never_signalled() {
        assert_eq!(super::single(0), None);
        assert_eq!(super::single(1), None, "init is nobody's to signal");
        assert_eq!(super::single(u32::MAX), None, "which would read as -1");
        assert_eq!(super::single(i32::MAX as u32 + 1), None);
        assert_eq!(super::single(4192325), Some(4192325));
    }
}
