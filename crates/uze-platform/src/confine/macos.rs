//! macOS: the resource limits, then `sandbox-exec` with the profile the
//! confinement becomes, which applies it and executes the program.

use std::{ffi::OsString, io, os::unix::process::CommandExt, path::Path, process::Command};

use super::{Availability, Confinement, Filesystem, rlimit, seatbelt};

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub(super) fn availability() -> Availability {
    if !Path::new(SANDBOX_EXEC).is_file() {
        return Availability::unavailable();
    }
    Availability {
        filesystem: Filesystem::Seatbelt,
        truncate: true,
        tcp: true,
        ioctl_dev: true,
        scoping: true,
        syscall_filter: false,
    }
}

pub(super) fn enter(confinement: &Confinement, argv: &[OsString]) -> io::Error {
    let profile = match seatbelt::profile(confinement) {
        Ok(profile) => profile,
        Err(error) => return error,
    };
    if let Err(error) = rlimit::apply(&confinement.limits) {
        return error;
    }
    Command::new(SANDBOX_EXEC)
        .arg("-p")
        .arg(profile)
        .arg("--")
        .args(argv)
        .exec()
}
