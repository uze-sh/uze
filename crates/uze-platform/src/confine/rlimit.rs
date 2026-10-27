//! The resource limits of a confined program, set on the helper before it
//! becomes the program: `execve` keeps them, and every process it starts
//! inherits them.

use std::io;

use super::Limits;

/// Lowers `$resource` to `$value`, soft and hard alike so the program cannot
/// raise its own, or to the hard limit already in place when that is lower.
/// A macro because the resource's type differs between C libraries.
macro_rules! lower {
    ($resource:expr, $value:expr) => {{
        let mut current = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `current` is a valid, writable `rlimit` for the call.
        succeeded(unsafe { libc::getrlimit($resource, &mut current) })?;
        let value = ($value as libc::rlim_t).min(current.rlim_max);
        let limit = libc::rlimit {
            rlim_cur: value,
            rlim_max: value,
        };
        // SAFETY: `limit` is a valid `rlimit` for the call.
        succeeded(unsafe { libc::setrlimit($resource, &limit) })
    }};
}

pub(super) fn apply(limits: &Limits) -> io::Result<()> {
    lower!(libc::RLIMIT_CPU, limits.cpu_seconds)?;
    lower!(libc::RLIMIT_FSIZE, limits.file_size)?;
    lower!(libc::RLIMIT_NOFILE, limits.open_files)?;
    address_space(limits.address_space)
}

#[cfg(target_os = "linux")]
fn address_space(bytes: u64) -> io::Result<()> {
    lower!(libc::RLIMIT_AS, bytes)
}

/// macOS refuses an address-space limit below what a process already maps
/// (`EINVAL`), and a 64-bit process maps 4 GiB for its zero page alone: the
/// memory bound is not enforced there, which `Availability::memory` says.
#[cfg(not(target_os = "linux"))]
fn address_space(_bytes: u64) -> io::Result<()> {
    Ok(())
}

fn succeeded(status: libc::c_int) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
