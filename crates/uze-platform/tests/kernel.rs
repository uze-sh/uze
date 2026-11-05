//! What this machine reports it can confine and seal, against what the kernel
//! itself answers. `docs/architecture/invariants.md` holds both to the kernel,
//! and every test that needs one skips where it reports none: a report that is
//! wrong would pass quietly otherwise. Linux only: Landlock and seccomp are the
//! kernel's answers asked here.
#![cfg(target_os = "linux")]

use uze_platform::{
    confine::{Filesystem, availability},
    seal,
};

const SCOPING_ABI: i64 = 6;

/// The Landlock ABI, asked of the kernel here rather than through the code
/// this file is testing.
fn kernel_abi() -> i64 {
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1;
    // SAFETY: asking for the version passes no attribute and no size.
    unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0 as libc::size_t,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    }
}

/// Whether the kernel filters system calls, as it tells every process.
fn kernel_filters_system_calls() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .any(|line| line.starts_with("Seccomp:"))
}

fn architecture_has_a_filter() -> bool {
    // The architectures Linux's system-call filter is written for here.
    cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
}

/// What the kernel's own answers say this machine enforces is what it reports.
#[test]
fn availability_reports_the_abi_the_kernel_answers() {
    let abi = kernel_abi();
    let reported = availability();
    if abi > 0 && kernel_filters_system_calls() && architecture_has_a_filter() {
        assert_eq!(
            reported.filesystem,
            Filesystem::Landlock { abi: abi as u32 }
        );
        assert!(reported.is_available());
    } else {
        assert_eq!(reported.filesystem, Filesystem::Unavailable);
        assert!(!reported.is_available());
    }
}

/// A seal is the Landlock ABI that scopes abstract sockets: this kernel can
/// have one exactly from that ABI on.
#[test]
fn a_seal_is_available_exactly_from_the_abi_that_scopes() {
    assert_eq!(seal::available(), kernel_abi() >= SCOPING_ABI);
}

/// A run that declares its kernel seals and confines has what it declared.
#[test]
fn a_declared_kernel_seals_and_confines() {
    if std::env::var_os(uze_testkit::env::KERNEL_CONFINES).is_none_or(|declared| declared != "1") {
        return;
    }
    assert!(
        availability().is_available(),
        "{}",
        availability().summary()
    );
    assert!(seal::available());
    assert!(kernel_abi() >= SCOPING_ABI, "ABI {}", kernel_abi());
}
