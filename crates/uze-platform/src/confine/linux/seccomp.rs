//! The system-call filter a confined program runs under: a fixed classic
//! BPF program written here over `libc` rather than taken from a crate.
//!
//! Landlock confines paths, not sockets: this filter is what keeps a program
//! without network off it (no `socket` of any family), what keeps one with
//! network to the Internet families (never a Unix socket such as an agent's
//! or Docker's), and what refuses `ptrace`, `process_vm_*`, `io_uring` and
//! leaving the process group the runner ends. A refused call fails with
//! `EPERM`; a call from another architecture's ABI ends the process.

use std::io;

/// What the filter is built from.
#[derive(Clone, Copy, Debug)]
pub(super) struct Policy {
    pub network: bool,
    /// Landlock scopes signals on ABI 6 and later; below it the filter
    /// limits them, as far as an argument check can.
    pub limit_signals: bool,
    /// The process group the program is ended by, which it may signal.
    pub own_group: libc::pid_t,
}

// The classic BPF and seccomp ABI (linux/filter.h, linux/seccomp.h,
// linux/audit.h): stable kernel ABI, named here because `libc` does not
// export every one of them on every C library.
// BPF_LD | BPF_W | BPF_ABS, the first two being zero.
const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JMP_JEQ_K: u16 = 0x05 | 0x10;
const BPF_JMP_JGE_K: u16 = 0x05 | 0x30;
const BPF_JMP_JSET_K: u16 = 0x05 | 0x40;
const BPF_RET_K: u16 = 0x06;

const RET_KILL_PROCESS: u32 = 0x8000_0000;
const RET_ERRNO: u32 = 0x0005_0000;
const RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_SET_MODE_FILTER: libc::c_ulong = 1;
const SECCOMP_GET_ACTION_AVAIL: libc::c_ulong = 2;

// `struct seccomp_data`: the call number, the audit architecture, the
// instruction pointer, then six 64-bit arguments. Only the low half of the
// first argument is read, which is where every argument checked here (an
// `int`) lives on a little-endian machine.
const OFFSET_NR: u32 = 0;
const OFFSET_ARCH: u32 = 4;
const OFFSET_ARG0_LOW: u32 = 16;

/// System calls numbered with this bit belong to the x32 ABI, which shares
/// x86-64's audit architecture: refused, so no call escapes the table below
/// by being spelled another way.
const X32_SYSCALL_BIT: u32 = 0x4000_0000;
const SIGN_BIT: u32 = 0x8000_0000;

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: Option<u32> = Some(0xc000_003e);
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: Option<u32> = Some(0xc000_00b7);
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const AUDIT_ARCH: Option<u32> = None;

/// Refused in every mode.
const ALWAYS_REFUSED: &[libc::c_long] = &[
    libc::SYS_io_uring_setup,
    libc::SYS_io_uring_enter,
    libc::SYS_io_uring_register,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_setsid,
    libc::SYS_setpgid,
];

/// Refused without network, `socket` itself included.
const REFUSED_WITHOUT_NETWORK: &[libc::c_long] = &[
    libc::SYS_socket,
    libc::SYS_connect,
    libc::SYS_bind,
    libc::SYS_listen,
    libc::SYS_accept,
    libc::SYS_accept4,
    libc::SYS_sendto,
    libc::SYS_sendmmsg,
    libc::SYS_recvmmsg,
    libc::SYS_getsockopt,
    libc::SYS_setsockopt,
];

/// Limited to non-negative targets or the program's own group when
/// signals are not scoped by Landlock.
const SIGNALLING: &[libc::c_long] = &[
    libc::SYS_kill,
    libc::SYS_tkill,
    libc::SYS_tgkill,
    libc::SYS_rt_sigqueueinfo,
    libc::SYS_rt_tgsigqueueinfo,
];

/// Whether this kernel filters system calls with the actions the program
/// uses.
pub(super) fn available() -> bool {
    AUDIT_ARCH.is_some()
        && [RET_ERRNO, RET_KILL_PROCESS, RET_ALLOW]
            .iter()
            .all(|action| {
                // SAFETY: the action is a valid, readable `u32` for the call.
                unsafe {
                    libc::syscall(
                        libc::SYS_seccomp,
                        SECCOMP_GET_ACTION_AVAIL,
                        0 as libc::c_ulong,
                        action as *const u32,
                    ) == 0
                }
            })
}

/// Installs the filter on the calling thread, which `execve` keeps. Needs
/// `no_new_privs` already set.
pub(super) fn install(policy: &Policy) -> io::Result<()> {
    let Some(arch) = AUDIT_ARCH else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no system-call filter for this architecture",
        ));
    };
    let mut filter = program(arch, policy);
    let program = libc::sock_fprog {
        len: filter.len() as libc::c_ushort,
        filter: filter.as_mut_ptr(),
    };
    // SAFETY: `program` points at `filter`, alive for the call; the kernel
    // copies it.
    let status = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            SECCOMP_SET_MODE_FILTER,
            0 as libc::c_ulong,
            &program as *const libc::sock_fprog,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The filter for `arch`, `policy` applied.
fn program(arch: u32, policy: &Policy) -> Vec<libc::sock_filter> {
    let refuse = RET_ERRNO | libc::EPERM as u32;
    let mut program = vec![
        load(OFFSET_ARCH),
        jump(BPF_JMP_JEQ_K, arch, 1, 0),
        ret(RET_KILL_PROCESS),
        load(OFFSET_NR),
        jump(BPF_JMP_JGE_K, X32_SYSCALL_BIT, 0, 1),
        ret(refuse),
    ];
    let mut refused = ALWAYS_REFUSED.to_vec();
    if policy.network {
        program.extend(first_argument_one_of(
            libc::SYS_socket,
            &[libc::AF_INET as u32, libc::AF_INET6 as u32],
        ));
    } else {
        refused.extend_from_slice(REFUSED_WITHOUT_NETWORK);
    }
    if policy.limit_signals {
        refused.push(libc::SYS_pidfd_send_signal);
        for &call in SIGNALLING {
            program.extend(signal_target_limited(call, policy.own_group));
        }
    }
    program.extend(first_argument_one_of(
        libc::SYS_socketpair,
        &[libc::AF_UNIX as u32],
    ));
    for call in refused {
        program.push(jump(BPF_JMP_JEQ_K, call as u32, 0, 1));
        program.push(ret(refuse));
    }
    program.push(ret(RET_ALLOW));
    program
}

/// `call` is allowed only when its first argument is one of `allowed`.
/// Entered and left with the call number loaded; a match ends in a verdict.
fn first_argument_one_of(call: libc::c_long, allowed: &[u32]) -> Vec<libc::sock_filter> {
    let compares = allowed.len() as u8;
    let mut block = vec![
        jump(BPF_JMP_JEQ_K, call as u32, 0, compares + 3),
        load(OFFSET_ARG0_LOW),
    ];
    for (index, &value) in allowed.iter().enumerate() {
        let to_allow = compares - index as u8;
        block.push(jump(BPF_JMP_JEQ_K, value, to_allow, 0));
    }
    block.push(ret(RET_ERRNO | libc::EPERM as u32));
    block.push(ret(RET_ALLOW));
    block
}

/// `call` may target a process by a non-negative id (0, itself, a child it
/// started) or the program's own group, never another group or every
/// process (`-1`).
fn signal_target_limited(call: libc::c_long, own_group: libc::pid_t) -> Vec<libc::sock_filter> {
    vec![
        jump(BPF_JMP_JEQ_K, call as u32, 0, 5),
        load(OFFSET_ARG0_LOW),
        jump(BPF_JMP_JSET_K, SIGN_BIT, 0, 2),
        jump(BPF_JMP_JEQ_K, own_group.wrapping_neg() as u32, 1, 0),
        ret(RET_ERRNO | libc::EPERM as u32),
        ret(RET_ALLOW),
    ]
}

fn load(offset: u32) -> libc::sock_filter {
    libc::sock_filter {
        code: BPF_LD_W_ABS,
        jt: 0,
        jf: 0,
        k: offset,
    }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

fn ret(verdict: u32) -> libc::sock_filter {
    libc::sock_filter {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k: verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCH: u32 = 0xc000_003e;

    fn policy(network: bool, limit_signals: bool) -> Policy {
        Policy {
            network,
            limit_signals,
            own_group: 4242,
        }
    }

    /// Runs `program` the way the kernel does, over a call number and a
    /// first argument, for the instructions this module emits.
    fn verdict(program: &[libc::sock_filter], arch: u32, nr: u32, arg0: u32) -> u32 {
        let mut accumulator = 0u32;
        let mut pc = 0usize;
        loop {
            let instruction = program[pc];
            match instruction.code {
                BPF_LD_W_ABS => {
                    accumulator = match instruction.k {
                        OFFSET_NR => nr,
                        OFFSET_ARCH => arch,
                        OFFSET_ARG0_LOW => arg0,
                        other => panic!("unexpected offset {other}"),
                    };
                    pc += 1;
                }
                BPF_RET_K => return instruction.k,
                code => {
                    let taken = match code {
                        BPF_JMP_JEQ_K => accumulator == instruction.k,
                        BPF_JMP_JGE_K => accumulator >= instruction.k,
                        BPF_JMP_JSET_K => accumulator & instruction.k != 0,
                        other => panic!("unexpected instruction {other:#x}"),
                    };
                    let offset = if taken {
                        instruction.jt
                    } else {
                        instruction.jf
                    };
                    pc += 1 + offset as usize;
                }
            }
        }
    }

    const EPERM: u32 = RET_ERRNO | libc::EPERM as u32;

    #[test]
    fn the_architecture_is_checked_first_and_a_mismatch_kills() {
        let program = program(ARCH, &policy(false, false));
        assert_eq!(program[0].k, OFFSET_ARCH);
        assert_eq!((program[1].code, program[1].k), (BPF_JMP_JEQ_K, ARCH));
        assert_eq!(program[2].k, RET_KILL_PROCESS);
        assert_eq!(
            verdict(&program, 0x4000_0003, libc::SYS_read as u32, 0),
            RET_KILL_PROCESS
        );
        assert_eq!(
            verdict(&program, ARCH, libc::SYS_read as u32 | X32_SYSCALL_BIT, 0),
            EPERM
        );
    }

    #[test]
    fn without_network_no_socket_opens_and_unix_pairs_still_do() {
        let program = program(ARCH, &policy(false, false));
        for family in [libc::AF_INET, libc::AF_INET6, libc::AF_UNIX, libc::AF_VSOCK] {
            let nr = libc::SYS_socket as u32;
            assert_eq!(verdict(&program, ARCH, nr, family as u32), EPERM);
        }
        let pair = libc::SYS_socketpair as u32;
        assert_eq!(
            verdict(&program, ARCH, pair, libc::AF_UNIX as u32),
            RET_ALLOW
        );
        assert_eq!(verdict(&program, ARCH, pair, libc::AF_VSOCK as u32), EPERM);
        for &call in REFUSED_WITHOUT_NETWORK.iter().chain(ALWAYS_REFUSED) {
            assert_eq!(verdict(&program, ARCH, call as u32, 0), EPERM, "{call}");
        }
        assert_eq!(verdict(&program, ARCH, libc::SYS_read as u32, 0), RET_ALLOW);
    }

    #[test]
    fn with_network_only_internet_sockets_open() {
        let program = program(ARCH, &policy(true, false));
        let nr = libc::SYS_socket as u32;
        assert_eq!(verdict(&program, ARCH, nr, libc::AF_INET as u32), RET_ALLOW);
        assert_eq!(
            verdict(&program, ARCH, nr, libc::AF_INET6 as u32),
            RET_ALLOW
        );
        assert_eq!(verdict(&program, ARCH, nr, libc::AF_UNIX as u32), EPERM);
        assert_eq!(verdict(&program, ARCH, nr, libc::AF_NETLINK as u32), EPERM);
        assert_eq!(
            verdict(&program, ARCH, libc::SYS_connect as u32, 3),
            RET_ALLOW
        );
        assert_eq!(verdict(&program, ARCH, libc::SYS_ptrace as u32, 0), EPERM);
    }

    #[test]
    fn unscoped_signals_reach_no_other_group_and_not_everyone() {
        let program = program(ARCH, &policy(false, true));
        let kill = libc::SYS_kill as u32;
        for target in [0i32, 1, 77, 4242, -4242] {
            assert_eq!(
                verdict(&program, ARCH, kill, target as u32),
                RET_ALLOW,
                "{target}"
            );
        }
        for target in [-1i32, -77] {
            assert_eq!(
                verdict(&program, ARCH, kill, target as u32),
                EPERM,
                "{target}"
            );
        }
        let pidfd = libc::SYS_pidfd_send_signal as u32;
        assert_eq!(verdict(&program, ARCH, pidfd, 3), EPERM);
        let scoped = super::program(ARCH, &policy(false, false));
        assert_eq!(verdict(&scoped, ARCH, kill, -1i32 as u32), RET_ALLOW);
    }

    #[test]
    fn the_program_stays_small() {
        let smallest = program(ARCH, &policy(true, false)).len();
        let largest = program(ARCH, &policy(false, true)).len();
        assert_eq!(smallest, 6 + 6 + 5 + 2 * ALWAYS_REFUSED.len() + 1);
        assert_eq!(
            largest,
            6 + 6 * SIGNALLING.len()
                + 5
                + 2 * (ALWAYS_REFUSED.len() + REFUSED_WITHOUT_NETWORK.len() + 1)
                + 1
        );
        assert!(largest < 256);
    }
}
