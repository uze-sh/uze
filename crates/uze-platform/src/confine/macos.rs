//! macOS: the resource limits, then `sandbox-exec` with the profile the
//! confinement becomes, which applies it and executes the program.

use std::{ffi::OsString, io, os::unix::process::CommandExt, path::Path, process::Command};

use super::{Availability, Confinement, rlimit, seatbelt};

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub(super) fn availability() -> Availability {
    if !Path::new(SANDBOX_EXEC).is_file() {
        return Availability::unavailable();
    }
    Availability::of_seatbelt()
}

pub(super) fn enter(confinement: &Confinement, argv: &[OsString]) -> io::Error {
    // Seatbelt matches the paths the kernel resolves: `/var`, `/tmp` and
    // `/etc` are links into `/private`, so a rule written through one of
    // them would never match. Every path must exist (`Confinement`).
    let confinement = match real_paths(confinement) {
        Ok(real) => real,
        Err(error) => return error,
    };
    let confinement = &confinement;
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

/// `confinement` with every path it grants resolved to the one the kernel
/// matches, or the error of the first that does not exist.
fn real_paths(confinement: &Confinement) -> io::Result<Confinement> {
    let resolve = |paths: &[std::path::PathBuf]| -> io::Result<Vec<std::path::PathBuf>> {
        paths.iter().map(std::fs::canonicalize).collect()
    };
    Ok(Confinement {
        read: resolve(&confinement.read)?,
        read_execute: resolve(&confinement.read_execute)?,
        write: resolve(&confinement.write)?,
        devices: resolve(&confinement.devices)?,
        network: confinement.network,
        limits: confinement.limits,
        namespace: confinement.namespace.clone(),
    })
}
