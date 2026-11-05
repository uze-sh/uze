//! A program run under a [`Confinement`], against the real kernel: what each
//! grant's doc comment says it allows is allowed, and what it says it withholds
//! is refused. What the kernel lacks is skipped, unless the run declared it
//! ([`uze_testkit::env::KERNEL_CONFINES`]).
//!
//! The helper a confinement is entered from is this test binary run again,
//! alone on its main thread, as [`enter`] asks.
#![cfg(target_os = "linux")]

use std::{
    ffi::OsString,
    io::Read as _,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use uze_platform::confine::{
    Confinement, Filesystem, Limits, NOT_ENFORCED_EXIT, availability, enter,
};
use uze_testkit::{env::kernel_has, temp::TempDir};

const CONFINEMENT: &str = "UZE_TEST_CONFINEMENT";
const PROGRAM: &str = "UZE_TEST_CONFINED_PROGRAM";
const TCP_ABI: u32 = 4;

/// Not a test of its own: what [`run`] starts this binary as.
#[test]
#[ignore = "started by the other tests in this file as their helper process"]
fn helper_enters_the_confinement_it_was_handed() {
    let confinement: Confinement =
        serde_json::from_str(&std::env::var(CONFINEMENT).unwrap()).unwrap();
    let program: Vec<String> = serde_json::from_str(&std::env::var(PROGRAM).unwrap()).unwrap();
    let program: Vec<OsString> = program.into_iter().map(OsString::from).collect();
    let refused = enter(&confinement, &program);
    eprintln!("not confined: {refused}");
    std::process::exit(NOT_ENFORCED_EXIT);
}

/// `program` run under `confinement`. Its output follows the few lines the test
/// runner says before the helper becomes the program.
fn run(confinement: &Confinement, program: &[&str]) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_enters_the_confinement_it_was_handed"])
        .args(["--ignored", "--quiet", "--test-threads=1"])
        .env(CONFINEMENT, serde_json::to_string(confinement).unwrap())
        .env(PROGRAM, serde_json::to_string(program).unwrap())
        .output()
        .unwrap()
}

fn shell(confinement: &Confinement, line: &str) -> Output {
    run(confinement, &["/bin/sh", "-c", line])
}

/// Whether the program ran and was refused, which a helper that failed before
/// it ever became the program would also look like by its status alone.
fn refused_by_the_kernel(output: &Output) -> bool {
    !output.status.success()
        && String::from_utf8_lossy(&output.stderr).contains("Permission denied")
}

/// What any program needs to start, and nothing else.
fn nothing_granted() -> Confinement {
    Confinement {
        read: Vec::new(),
        read_execute: ["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/etc"]
            .iter()
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .collect(),
        write: Vec::new(),
        devices: Vec::new(),
        network: false,
        limits: Limits::with_cpu_seconds(20),
        namespace: "uze-test".to_owned(),
    }
}

fn confines() -> bool {
    kernel_has(availability().is_available(), "confinement")
}

/// The Landlock ABI this machine confines with, `0` with none.
fn abi() -> u32 {
    match availability().filesystem {
        Filesystem::Landlock { abi } => abi,
        _ => 0,
    }
}

/// Each thing `Confinement::write` allows beneath a grant, on its own, so that
/// losing any one right says which.
#[test]
fn a_write_grant_lets_a_program_do_all_it_says() {
    if !confines() {
        return;
    }
    let root = TempDir::new("confined-write");
    let granted = root.path().join("granted");
    std::fs::create_dir_all(granted.join("from")).unwrap();
    std::fs::create_dir_all(granted.join("to")).unwrap();
    std::fs::write(granted.join("removed"), "x").unwrap();
    std::fs::create_dir(granted.join("gone")).unwrap();
    std::fs::write(granted.join("from/moved"), "x").unwrap();
    std::fs::write(granted.join("emptied"), "x").unwrap();
    std::fs::write(granted.join("cut"), "x").unwrap();
    let mut confinement = nothing_granted();
    confinement.write.push(granted.clone());

    let mut allowed = vec![
        "echo written > written",
        "rm removed",
        "rmdir gone",
        "mkdir made",
        "mkfifo pipe",
        "ln -s written link",
        ": > emptied",
        "truncate -s 0 cut",
    ];
    if abi() >= 2 {
        // A rename into another directory, the right ABI 2 brought.
        allowed.push("mv from/moved to/moved");
    }
    for line in allowed {
        let output = shell(&confinement, &format!("cd {} && {line}", granted.display()));
        assert!(output.status.success(), "{line}: {output:?}");
    }

    assert!(granted.join("written").is_file());
    assert!(!granted.join("removed").exists());
    assert!(!granted.join("gone").exists());
    assert!(granted.join("made").is_dir());
    assert!(granted.join("pipe").exists());
    assert!(granted.join("link").is_symlink());
    assert_eq!(std::fs::read(granted.join("emptied")).unwrap(), b"");
    assert_eq!(std::fs::read(granted.join("cut")).unwrap(), b"");
    if abi() >= 2 {
        assert!(granted.join("to/moved").is_file());
    }
}

/// "Never execute": a script written beneath a write grant, executable by its
/// mode, does not start.
#[test]
fn a_script_written_in_a_write_grant_does_not_execute() {
    if !confines() {
        return;
    }
    let root = TempDir::new("confined-execute");
    let mut confinement = nothing_granted();
    confinement.write.push(root.path().to_path_buf());

    let output = shell(
        &confinement,
        &format!(
            "cd {} && printf '#!/bin/sh\\necho ran\\n' > script && chmod +x script && ./script",
            root.path().display()
        ),
    );

    assert!(root.path().join("script").is_file(), "{output:?}");
    assert!(refused_by_the_kernel(&output), "{output:?}");
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(!printed.lines().any(|line| line == "ran"), "{output:?}");
}

/// "With it, still no listening TCP port": with network a program connects to
/// a local listener and cannot bind a port of its own.
#[test]
fn with_network_a_program_connects_and_binds_no_port() {
    if !confines() || !kernel_has(abi() >= TCP_ABI, "Landlock TCP rules") {
        return;
    }
    let python = "/usr/bin/python3";
    if !Path::new(python).is_file() {
        eprintln!("skipped: no {python} to open sockets with");
        return;
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut said = String::new();
        stream.read_to_string(&mut said).unwrap();
        said
    });
    let probe = format!(
        "import socket\n\
         connection = socket.create_connection(('127.0.0.1', {port}), 5)\n\
         connection.sendall(b'connected')\n\
         connection.close()\n\
         try:\n  socket.socket().bind(('127.0.0.1', 0)); print('bound')\n\
         except PermissionError: print('no port')\n"
    );
    let mut confinement = nothing_granted();
    confinement.network = true;

    let output = run(&confinement, &[python, "-I", "-c", &probe]);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(accepted.join().unwrap(), "connected");
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.lines().any(|line| line == "no port"), "{output:?}");
}

/// `Confinement::devices`: a device granted is written to.
#[test]
fn a_device_grant_lets_a_program_write_to_the_device() {
    if !confines() {
        return;
    }
    let mut confinement = nothing_granted();
    confinement.devices.push(PathBuf::from("/dev/null"));

    let output = shell(&confinement, "echo discarded > /dev/null");

    assert!(output.status.success(), "{output:?}");
}

/// `Confinement::devices`: "never create or remove", a directory granted as
/// one included.
#[test]
fn a_device_grant_lets_a_program_create_nothing_beneath_it() {
    if !confines() {
        return;
    }
    let root = TempDir::new("confined-device");
    let mut confinement = nothing_granted();
    confinement.devices.push(root.path().to_path_buf());

    let output = shell(&confinement, &format!("> {}/made", root.path().display()));

    assert!(refused_by_the_kernel(&output), "{output:?}");
    assert!(!root.path().join("made").exists());
}

/// `Limits::ADDRESS_SPACE` reaches the program, in the KiB a shell counts it in.
/// Only read, so nothing is allocated against it.
#[test]
fn a_confined_program_runs_under_its_address_space_limit() {
    if !confines() {
        return;
    }
    let output = shell(&nothing_granted(), "ulimit -v");

    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.lines().any(|line| line == "2097152"), "{output:?}");
}
