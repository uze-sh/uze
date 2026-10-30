//! `uze __confine` against the real kernel: a program run through the
//! helper reaches what its confinement grants and nothing else. Skipped,
//! with the reason printed, on a kernel without Landlock or seccomp.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use uze_platform::confine::{Confinement, Limits, NOT_ENFORCED_EXIT, availability};
use uze_testkit::temp::TempDir;

fn uze() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_uze"))
}

fn confinable() -> bool {
    let availability = availability();
    if !availability.is_available() {
        eprintln!("skipped: {}", availability.summary());
    }
    availability.is_available()
}

/// What any program needs to start: the system prefixes, to read and
/// execute, and `/dev/null` to write.
fn confinement(read: &[&Path], write: &[&Path], network: bool) -> Confinement {
    let system = ["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/etc"];
    Confinement {
        read: read.iter().map(|path| path.to_path_buf()).collect(),
        read_execute: system
            .iter()
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .collect(),
        write: write
            .iter()
            .map(|path| path.to_path_buf())
            .chain([PathBuf::from("/dev/null")])
            .collect(),
        devices: Vec::new(),
        network,
        limits: Limits::with_cpu_seconds(20),
    }
}

fn confined(confinement: &Confinement, program: &[&str]) -> Command {
    let mut command = Command::new(uze());
    command
        .arg("__confine")
        .arg(serde_json::to_string(confinement).unwrap())
        .arg("--")
        .args(program);
    command
}

fn run(confinement: &Confinement, program: &[&str]) -> Output {
    confined(confinement, program).output().unwrap()
}

fn shell(confinement: &Confinement, line: &str) -> Output {
    run(confinement, &["/bin/sh", "-c", line])
}

fn python() -> Option<&'static str> {
    let path = "/usr/bin/python3";
    if Path::new(path).is_file() {
        Some(path)
    } else {
        eprintln!("skipped: no {path}");
        None
    }
}

#[test]
fn a_program_writes_only_where_it_was_granted() {
    if !confinable() {
        return;
    }
    let root = TempDir::new("confine");
    let granted = root.path().join("granted");
    let denied = root.path().join("denied");
    std::fs::create_dir_all(&granted).unwrap();
    std::fs::create_dir_all(&denied).unwrap();
    let confinement = confinement(&[], &[&granted], false);

    let inside = shell(
        &confinement,
        &format!("echo ok > {}/file", granted.display()),
    );
    assert!(inside.status.success(), "{inside:?}");
    assert_eq!(
        std::fs::read_to_string(granted.join("file")).unwrap(),
        "ok\n"
    );

    let outside = shell(
        &confinement,
        &format!("echo no > {}/file", denied.display()),
    );
    assert!(!outside.status.success(), "{outside:?}");
    assert!(
        String::from_utf8_lossy(&outside.stderr).contains("Permission denied"),
        "{outside:?}"
    );
    assert!(!denied.join("file").exists());
}

#[test]
fn a_program_reads_its_grants_and_nothing_of_home() {
    if !confinable() {
        return;
    }
    let root = TempDir::new("confine");
    let home = root.path().join("home");
    let checkout = root.path().join("checkout");
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(home.join(".ssh/id"), "secret").unwrap();
    std::fs::write(checkout.join("readme"), "public").unwrap();
    let script = checkout.join("script");
    uze_testkit::process::install_executable(&script, b"#!/bin/sh\necho ran\n");
    let confinement = confinement(&[&checkout], &[], false);

    let granted = run(
        &confinement,
        &["/bin/cat", checkout.join("readme").to_str().unwrap()],
    );
    assert!(granted.status.success(), "{granted:?}");
    assert_eq!(granted.stdout, b"public");

    let secret = confined(&confinement, &["/bin/sh", "-c", "cat \"$HOME/.ssh/id\""])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(!secret.status.success(), "{secret:?}");
    assert!(secret.stdout.is_empty());

    let executed = shell(&confinement, &script.display().to_string());
    assert!(!executed.status.success(), "a read grant executes nothing");
    assert!(executed.stdout.is_empty(), "{executed:?}");
}

#[test]
fn without_network_no_socket_opens_and_with_it_one_does() {
    if !confinable() {
        return;
    }
    let Some(python) = python() else { return };
    let open_a_socket = [python, "-I", "-c", "import socket; socket.socket()"];

    let refused = run(&confinement(&[], &[], false), &open_a_socket);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("PermissionError"));

    let allowed = run(&confinement(&[], &[], true), &open_a_socket);
    assert!(allowed.status.success(), "{allowed:?}");

    let unix = [
        python,
        "-I",
        "-c",
        "import socket; socket.socket(socket.AF_UNIX)",
    ];
    let unix = run(&confinement(&[], &[], true), &unix);
    assert!(!unix.status.success(), "no Unix socket even with network");
}

#[test]
fn a_program_cannot_leave_the_group_it_is_ended_by() {
    if !confinable() {
        return;
    }
    let Some(python) = python() else { return };
    let leave = run(
        &confinement(&[], &[], false),
        &[python, "-I", "-c", "import os; os.setsid()"],
    );
    assert!(!leave.status.success(), "{leave:?}");
    assert!(String::from_utf8_lossy(&leave.stderr).contains("PermissionError"));
}

#[test]
fn a_confinement_that_cannot_be_applied_runs_nothing_and_says_so() {
    if !confinable() {
        return;
    }
    let root = TempDir::new("confine");
    let marker = root.path().join("ran");
    let missing = root.path().join("missing");
    let confinement = confinement(&[&missing], &[root.path()], false);
    let output = shell(&confinement, &format!("touch {}", marker.display()));
    assert_eq!(output.status.code(), Some(NOT_ENFORCED_EXIT), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("not confined"), "{stderr}");
    assert!(!marker.exists());

    let unreadable = Command::new(uze())
        .args(["__confine", "{\"read\":[]}", "--", "/bin/true"])
        .output()
        .unwrap();
    assert_eq!(unreadable.status.code(), Some(NOT_ENFORCED_EXIT));
}

#[test]
fn the_program_gets_exactly_its_environment_and_its_own_exit_status() {
    if !confinable() {
        return;
    }
    let confinement = confinement(&[], &[], false);
    let output = confined(&confinement, &["/usr/bin/env"])
        .env_clear()
        .env("FIRST", "1")
        .env("SECOND", "two words")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    // A coverage build's profiling runtime sets its own variable in the
    // helper's environment at startup; it is the instrumentation's, never
    // the caller's, and no ordinary build has it.
    let environment: String = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.starts_with("__LLVM_PROFILE_"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(environment, "FIRST=1\nSECOND=two words\n");

    let status = shell(&confinement, "exit 7");
    assert_eq!(status.status.code(), Some(7));
}
