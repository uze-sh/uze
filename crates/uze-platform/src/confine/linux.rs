//! Linux: Landlock for paths, TCP and scoping, then a seccomp filter for
//! what Landlock does not cover, applied in that order by the helper to
//! itself, then `execve`.

mod seccomp;

use std::{
    ffi::OsString,
    io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

use landlock::{
    ABI, Access, AccessFs, AccessNet, BitFlags, CompatLevel, Compatible, PathBeneath, PathFd,
    Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus, Scope,
};

use super::{Availability, Confinement, rlimit};

const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1;

pub(super) fn availability() -> Availability {
    Availability::of_linux(landlock_abi(), seccomp::available())
}

/// The Landlock ABI this kernel answers, or 0 or less when it has none.
fn landlock_abi() -> i32 {
    // SAFETY: asking for the version passes no attribute and no size.
    let version = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0 as libc::size_t,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    i32::try_from(version).unwrap_or(0)
}

pub(super) fn enter(confinement: &Confinement, argv: &[OsString]) -> io::Error {
    if let Err(error) = confine(confinement) {
        return error;
    }
    Command::new(&argv[0]).args(&argv[1..]).exec()
}

fn confine(confinement: &Confinement) -> io::Result<()> {
    set_no_new_privs()?;
    rlimit::apply(&confinement.limits)?;
    let abi = ABI::from(landlock_abi());
    if abi == ABI::Unsupported {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this kernel has no Landlock",
        ));
    }
    restrict_paths(confinement, abi)?;
    seccomp::install(&seccomp::Policy {
        network: confinement.network,
        limit_signals: Scope::from_all(abi).is_empty(),
        own_pid: std::process::id() as libc::pid_t,
        // SAFETY: `getpgrp` has no preconditions and cannot fail.
        own_group: unsafe { libc::getpgrp() },
    })
}

fn set_no_new_privs() -> io::Result<()> {
    // SAFETY: `PR_SET_NO_NEW_PRIVS` takes plain integers.
    let status = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Every right this ABI knows is handled and must be enforced
/// ([`CompatLevel::HardRequirement`]), so nothing consent promised becomes
/// a best effort; each path then gets back only what its list grants.
fn restrict_paths(confinement: &Confinement, abi: ABI) -> io::Result<()> {
    let handled = AccessFs::from_all(abi);
    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(handled)
        .map_err(landlock_error)?;
    let tcp = AccessNet::from_all(abi);
    if !tcp.is_empty() {
        // With network, connecting is left to the program and only binding a
        // port is handled, so none is allowed; without it, both are.
        let denied = if confinement.network {
            tcp & AccessNet::BindTcp
        } else {
            tcp
        };
        ruleset = ruleset.handle_access(denied).map_err(landlock_error)?;
    }
    let scopes = Scope::from_all(abi);
    if !scopes.is_empty() {
        ruleset = ruleset.scope(scopes).map_err(landlock_error)?;
    }
    let read = AccessFs::ReadFile | AccessFs::ReadDir;
    let read_execute = read | AccessFs::Execute;
    let write = read | (writing() & handled);
    let device = read | ((AccessFs::WriteFile | AccessFs::IoctlDev) & handled);
    let grants = [
        (&confinement.read, read),
        (&confinement.read_execute, read_execute),
        (&confinement.write, write),
        (&confinement.devices, device),
    ];
    let mut created = ruleset.create().map_err(landlock_error)?;
    for (paths, rights) in grants {
        for path in paths {
            created = created
                .add_rule(beneath(path, rights, abi)?)
                .map_err(landlock_error)?;
        }
    }
    let status = created.restrict_self().map_err(landlock_error)?;
    if status.ruleset == RulesetStatus::FullyEnforced {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Landlock enforced the confinement only {:?}",
            status.ruleset
        )))
    }
}

/// What a writable path allows beyond reading: never a device, a socket or
/// an `ioctl`.
fn writing() -> BitFlags<AccessFs> {
    AccessFs::WriteFile
        | AccessFs::RemoveDir
        | AccessFs::RemoveFile
        | AccessFs::MakeDir
        | AccessFs::MakeReg
        | AccessFs::MakeFifo
        | AccessFs::MakeSym
        | AccessFs::Refer
        | AccessFs::Truncate
}

/// The rule for `path`; a file rather than a directory keeps only the
/// rights a file can have, which the kernel otherwise refuses.
fn beneath(path: &Path, rights: BitFlags<AccessFs>, abi: ABI) -> io::Result<PathBeneath<PathFd>> {
    let unreachable = |error: &dyn std::fmt::Display| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("cannot grant {}: {error}", path.display()),
        )
    };
    let metadata = std::fs::metadata(path).map_err(|error| unreachable(&error))?;
    let rights = if metadata.is_dir() {
        rights
    } else {
        rights & AccessFs::from_file(abi)
    };
    let fd = PathFd::new(PathBuf::from(path)).map_err(|error| unreachable(&error))?;
    Ok(PathBeneath::new(fd, rights))
}

fn landlock_error(error: landlock::RulesetError) -> io::Error {
    io::Error::other(format!("Landlock: {error}"))
}
