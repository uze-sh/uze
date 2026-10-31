//! Probe: which memory bound a macOS process can put on itself, unprivileged.
//! Each attempt runs in a fresh child (`probe <mode> <arg>`), so one that
//! sticks cannot spoil the next.

use std::process::Command;

unsafe extern "C" {
    fn memorystatus_control(command: u32, pid: i32, flags: u32, buffer: *mut libc::c_void, size: usize) -> i32;
}

fn limit(resource: libc::c_int) -> (u64, u64) {
    let mut l = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    unsafe { libc::getrlimit(resource, &mut l) };
    (l.rlim_cur as u64, l.rlim_max as u64)
}

fn set(resource: libc::c_int, bytes: u64) -> Result<(), String> {
    let l = libc::rlimit { rlim_cur: bytes as libc::rlim_t, rlim_max: bytes as libc::rlim_t };
    if unsafe { libc::setrlimit(resource, &l) } == 0 { Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) }
}

/// Virtual size and footprint of this task, from `ps` (no mach bindings needed).
fn sizes() -> String {
    let out = Command::new("/bin/ps").args(["-o", "vsz=,rss=", "-p", &std::process::id().to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Tries to commit `mib` MiB: a `malloc` touched page by page, and an `mmap`.
fn allocate(mib: usize) -> String {
    let bytes = mib << 20;
    let p = unsafe { libc::malloc(bytes) } as *mut u8;
    let malloc = if p.is_null() {
        format!("malloc {mib} MiB refused ({})", std::io::Error::last_os_error())
    } else {
        let mut i = 0;
        while i < bytes { unsafe { *p.add(i) = 1 }; i += 16384; }
        format!("malloc {mib} MiB committed")
    };
    let m = unsafe { libc::mmap(std::ptr::null_mut(), bytes, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANON, -1, 0) };
    let mmap = if m == libc::MAP_FAILED { format!("mmap refused ({})", std::io::Error::last_os_error()) } else { "mmap mapped".to_owned() };
    format!("{malloc}; {mmap}")
}

fn child(mode: &str, arg: u64) {
    println!("  before: vsz,rss {} | AS {:?} DATA {:?}", sizes(), limit(libc::RLIMIT_AS), limit(libc::RLIMIT_DATA));
    let applied = match mode {
        "as" => set(libc::RLIMIT_AS, arg),
        "as-over-current" => {
            let vsz_kib: u64 = sizes().split_whitespace().next().unwrap().parse().unwrap();
            set(libc::RLIMIT_AS, vsz_kib * 1024 + arg)
        }
        "data" => set(libc::RLIMIT_DATA, arg),
        "jetsam" => {
            // MEMORYSTATUS_CMD_SET_JETSAM_TASK_LIMIT = 6, flags = limit in MiB
            let r = unsafe { memorystatus_control(6, std::process::id() as i32, (arg >> 20) as u32, std::ptr::null_mut(), 0) };
            if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) }
        }
        "memlimit-properties" => {
            // MEMORYSTATUS_CMD_SET_MEMLIMIT_PROPERTIES = 7
            #[repr(C)]
            struct Props { active: i32, active_attr: u32, inactive: i32, inactive_attr: u32 }
            let mut props = Props { active: (arg >> 20) as i32, active_attr: 1, inactive: (arg >> 20) as i32, inactive_attr: 1 };
            let r = unsafe { memorystatus_control(7, std::process::id() as i32, 0, (&mut props as *mut Props).cast(), std::mem::size_of::<Props>()) };
            if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) }
        }
        _ => Err("unknown mode".into()),
    };
    println!("  set {mode} {arg}: {applied:?} | AS now {:?}", limit(libc::RLIMIT_AS));
    if applied.is_ok() {
        for mib in [64, 512, 1024, 3072] {
            println!("  alloc: {}", allocate(mib));
        }
        let shell = Command::new("/bin/sh").args(["-c", "echo child-shell-ok"]).output();
        println!("  a child shell under it: {:?}", shell.map(|o| (o.status.code(), String::from_utf8_lossy(&o.stdout).trim().to_owned(), String::from_utf8_lossy(&o.stderr).trim().to_owned())));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 {
        return child(&args[1], args[2].parse().unwrap());
    }
    let me = std::env::current_exe().unwrap();
    println!("arch {} | ps vsz,rss {}", std::env::consts::ARCH, sizes());
    let gib = 1u64 << 30;
    let attempts: &[(&str, u64)] = &[
        ("as", 2 * gib), ("as", 8 * gib), ("as", 64 * gib), ("as", 512 * gib), ("as", 1 << 40),
        ("as-over-current", 2 * gib), ("as-over-current", 4 * gib),
        ("data", 2 * gib),
        ("jetsam", 768 << 20),
        ("memlimit-properties", 768 << 20),
    ];
    for &(mode, arg) in attempts {
        println!("== {mode} {arg}");
        let out = Command::new(&me).args([mode, &arg.to_string()]).output().unwrap();
        print!("{}", String::from_utf8_lossy(&out.stdout));
        let err = String::from_utf8_lossy(&out.stderr);
        if !err.trim().is_empty() { println!("  stderr: {}", err.trim()); }
        println!("  exit {:?}", out.status);
    }
}
