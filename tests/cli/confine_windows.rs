//! `uze __confine` on Windows: a program run through the helper in an
//! AppContainer of its run's own reaches what its confinement grants and
//! nothing else (`confinement-parity`). The probes are the system's own
//! `cmd.exe` and Windows PowerShell, which every Windows machine has.

use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use uze_platform::confine::{Confinement, Limits, NOT_ENFORCED_EXIT, availability};
use uze_testkit::temp::TempDir;

fn uze() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_uze"))
}

fn system(program: &str) -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned());
    format!("{root}\\{program}")
}

fn cmd() -> String {
    system("System32\\cmd.exe")
}

fn powershell() -> String {
    system("System32\\WindowsPowerShell\\v1.0\\powershell.exe")
}

fn confinement(read: &[&Path], write: &[&Path], network: bool) -> Confinement {
    Confinement {
        read: read.iter().map(|path| path.to_path_buf()).collect(),
        read_execute: Vec::new(),
        write: write.iter().map(|path| path.to_path_buf()).collect(),
        devices: Vec::new(),
        network,
        limits: Limits::with_cpu_seconds(60),
        namespace: "uze-test".to_owned(),
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
    run(confinement, &[&cmd(), "/d", "/c", line])
}

/// A PowerShell line, its answer the one line it prints.
fn script(confinement: &Confinement, line: &str) -> String {
    let output = run(
        confinement,
        &[
            &powershell(),
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            line,
        ],
    );
    assert!(output.status.success(), "{output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A directory the test owns, outside the profile Windows hides from a
/// container: the system's temporary directory is beneath it.
fn root() -> TempDir {
    TempDir::new("confine")
}

#[test]
fn this_machine_confines_in_an_app_container() {
    let availability = availability();
    assert!(availability.is_available(), "{}", availability.summary());
    assert_eq!(
        availability.summary(),
        "confined by AppContainer, without a file size or open files limit"
    );
}

#[test]
fn a_program_writes_only_where_it_was_granted() {
    let root = root();
    let granted = root.path().join("granted");
    let denied = root.path().join("denied");
    std::fs::create_dir_all(&granted).unwrap();
    std::fs::create_dir_all(&denied).unwrap();
    let confinement = confinement(&[&denied], &[&granted], false);

    let inside = shell(
        &confinement,
        &format!("echo ok> {}\\file", granted.display()),
    );
    assert!(inside.status.success(), "{inside:?}");
    assert_eq!(
        std::fs::read_to_string(granted.join("file")).unwrap(),
        "ok\r\n"
    );

    let outside = shell(
        &confinement,
        &format!("echo no> {}\\file", denied.display()),
    );
    assert!(!outside.status.success(), "{outside:?}");
    assert!(!denied.join("file").exists());
}

#[test]
fn a_program_reads_its_grants_and_nothing_else() {
    let root = root();
    let checkout = root.path().join("checkout");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(checkout.join("readme"), "public").unwrap();
    std::fs::write(outside.join("id_ed25519"), "secret").unwrap();
    let confinement = confinement(&[&checkout], &[], false);

    let granted = shell(
        &confinement,
        &format!("type {}", checkout.join("readme").display()),
    );
    assert!(granted.status.success(), "{granted:?}");
    assert_eq!(stdout(&granted), "public");

    let secret = shell(
        &confinement,
        &format!("type {}", outside.join("id_ed25519").display()),
    );
    assert!(!secret.status.success(), "{secret:?}");
    assert!(secret.stdout.is_empty(), "{secret:?}");

    let profile = shell(&confinement, "dir /b \"%USERPROFILE%\\Desktop\"");
    assert!(
        !profile.status.success(),
        "the profile is hidden: {profile:?}"
    );
}

#[test]
fn a_read_grant_executes_nothing() {
    let root = root();
    let checkout = root.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let copy = checkout.join("whoami.exe");
    std::fs::copy(system("System32\\whoami.exe"), &copy).unwrap();
    let executed = shell(
        &confinement(&[&checkout], &[], false),
        &copy.display().to_string(),
    );
    assert!(!executed.status.success(), "{executed:?}");
    assert!(executed.stdout.is_empty(), "{executed:?}");
}

#[test]
fn without_network_nothing_answers_and_with_it_the_loopback_still_does_not() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let connect = |host: &str, port: u16| {
        format!(
            "try {{ $c = New-Object Net.Sockets.TcpClient; \
             if ($c.ConnectAsync('{host}', {port}).Wait(5000)) {{ 'connected' }} else {{ 'timed out' }} }} \
             catch {{ 'refused' }}"
        )
    };
    let offline = confinement(&[], &[], false);
    let online = confinement(&[], &[], true);

    assert_ne!(script(&offline, &connect("127.0.0.1", port)), "connected");
    assert_ne!(script(&offline, &connect("1.1.1.1", 443)), "connected");
    assert_ne!(
        script(&online, &connect("127.0.0.1", port)),
        "connected",
        "network reaches the Internet, never a local service"
    );
    // Only a machine that reaches the Internet itself can say whether the
    // grant lets a program reach it.
    let reachable = std::net::TcpStream::connect_timeout(
        &"1.1.1.1:443".parse().unwrap(),
        std::time::Duration::from_secs(5),
    )
    .is_ok();
    if reachable {
        assert_eq!(script(&online, &connect("1.1.1.1", 443)), "connected");
    } else {
        eprintln!("skipped the Internet half: this machine reaches no Internet host");
    }
}

#[test]
fn another_process_is_out_of_reach() {
    let mut bystander = Command::new(cmd())
        .args(["/d", "/c", "ping -n 30 127.0.0.1 >NUL"])
        .spawn()
        .unwrap();
    let pid = bystander.id();
    let answer = script(
        &confinement(&[], &[], false),
        &format!(
            "try {{ (Get-Process -Id {pid} -ErrorAction Stop).Kill(); 'killed' }} \
             catch {{ 'refused' }}"
        ),
    );
    assert_eq!(answer, "refused");
    assert!(bystander.try_wait().unwrap().is_none(), "it still runs");
    let _ = bystander.kill();
    let _ = bystander.wait();
}

#[test]
fn memory_past_the_bound_is_refused() {
    let mut confinement = confinement(&[], &[], false);
    confinement.limits.address_space = 512 << 20;
    let output = run(
        &confinement,
        &[
            &powershell(),
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$a = New-Object byte[] (1GB); $a[0] = 1; 'allocated'",
        ],
    );
    assert!(!stdout(&output).contains("allocated"), "{output:?}");
}

#[test]
fn the_program_gets_its_environment_and_its_own_exit_status() {
    let confinement = confinement(&[], &[], false);
    let output = confined(&confinement, &[&cmd(), "/d", "/c", "echo %FIRST%-%SECOND%"])
        .env("FIRST", "1")
        .env("SECOND", "two words")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(stdout(&output), "1-two words");

    let status = shell(&confinement, "exit 7");
    assert_eq!(status.status.code(), Some(7));
}

#[test]
fn the_run_leaves_no_container_behind() {
    let output = shell(&confinement(&[], &[], false), "echo %LOCALAPPDATA%");
    assert!(output.status.success(), "{output:?}");
    let container = PathBuf::from(stdout(&output));
    assert!(
        container.to_string_lossy().contains("uze-test.run."),
        "the run's own folder: {container:?}"
    );
    assert!(!container.exists(), "{container:?} outlived its run");
}

#[test]
fn a_confinement_that_cannot_be_applied_runs_nothing_and_says_so() {
    let root = root();
    let marker = root.path().join("ran");
    let missing = root.path().join("missing");
    let output = shell(
        &confinement(&[&missing], &[root.path()], false),
        &format!("echo x> {}", marker.display()),
    );
    assert_eq!(output.status.code(), Some(NOT_ENFORCED_EXIT), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("not confined"), "{stderr}");
    assert!(!marker.exists());

    let mut device = confinement(&[], &[], false);
    device.devices = vec![PathBuf::from("\\\\.\\PhysicalDrive0")];
    let output = shell(&device, "echo x");
    assert_eq!(output.status.code(), Some(NOT_ENFORCED_EXIT), "{output:?}");
}

/// Git, installed where Windows lets every app container read, reads a
/// checkout it was granted: the program an extension runs most.
#[test]
fn git_reads_the_checkout_it_was_granted() {
    let program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_owned());
    let git = PathBuf::from(program_files).join("Git\\cmd\\git.exe");
    if !git.is_file() {
        eprintln!("skipped: no Git at {}", git.display());
        return;
    }
    let root = root();
    let checkout = root.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(checkout.join("readme"), "public").unwrap();
    uze_testkit::git::commit_everything_in(&checkout);
    let confinement = confinement(&[&checkout], &[], false);
    let git = git.display().to_string();
    let checkout_arg = checkout.display().to_string();
    let listed = run(&confinement, &[&git, "-C", &checkout_arg, "ls-files"]);
    assert!(listed.status.success(), "{listed:?}");
    assert_eq!(stdout(&listed), "readme");
    let inside = confined(&confinement, &[&git, "log", "-1", "--format=%H"])
        .current_dir(&checkout)
        .output()
        .unwrap();
    assert!(inside.status.success(), "{inside:?}");
    assert_eq!(stdout(&inside).len(), 40, "{inside:?}");
}
