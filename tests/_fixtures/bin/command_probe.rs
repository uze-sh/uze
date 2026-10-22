//! The program the portable-shell suite runs instead of a real tool: it
//! records what reached it and exits as told.
//!
//! Each start appends one line to `$PROBE_LOG`: its arguments, its working
//! directory and every `PROBE_VAR_*` variable it was given, so a test can
//! compare what a line delivered on one platform with what it delivered on
//! another. The first argument may direct it:
//!
//! - `exit=N` exits `N` after recording.
//! - `flood` writes more than the bounded output keeps, to both streams.
//! - `hold` connects to `$PROBE_HOLD`, starts a second probe that does the
//!   same (`held`), and both wait forever: the connections close only when
//!   the processes are gone, which is how a test sees a tree was ended
//!   without polling for it.
//!
//! Hand-rolled on purpose, like the other fixture binaries: a record is
//! one line of tab-separated fields, every field with `\`, tab and newline
//! escaped, read back by `tests/cli/portable_shell.rs`.

use std::{
    io::Write as _,
    net::TcpStream,
    process::{Command, ExitCode, Stdio},
};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    record(&arguments);
    match arguments.first().map(String::as_str) {
        Some(directive) if directive.starts_with("exit=") => {
            ExitCode::from(directive["exit=".len()..].parse::<u8>().unwrap_or(255))
        }
        Some("flood") => {
            let line = "x".repeat(1023) + "\n";
            for _ in 0..256 {
                print!("{line}");
                eprint!("{line}");
            }
            ExitCode::SUCCESS
        }
        Some("hold") => {
            let _connection = hold();
            // The second probe waits forever, so waiting for it is this
            // one waiting forever too.
            let _ = Command::new(std::env::current_exe().expect("the probe knows itself"))
                .arg("held")
                .stdin(Stdio::null())
                .spawn()
                .expect("the probe starts a second one")
                .wait();
            ExitCode::FAILURE
        }
        Some("held") => {
            let _connection = hold();
            wait_forever()
        }
        _ => ExitCode::SUCCESS,
    }
}

fn hold() -> TcpStream {
    let address = std::env::var("PROBE_HOLD").expect("PROBE_HOLD names the test's listener");
    let mut connection = TcpStream::connect(address).expect("the test listens");
    writeln!(connection, "{}", std::process::id()).expect("the test reads");
    connection
}

fn wait_forever() -> ExitCode {
    loop {
        std::thread::park();
    }
}

fn record(arguments: &[String]) {
    let Some(log) = std::env::var_os("PROBE_LOG") else {
        return;
    };
    let cwd = std::env::current_dir()
        .map(|cwd| cwd.display().to_string())
        .unwrap_or_default();
    let mut variables: Vec<String> = std::env::vars()
        .filter(|(name, _)| name.starts_with("PROBE_VAR_"))
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    variables.sort();
    let fields = [
        arguments
            .iter()
            .map(|word| escape(word))
            .collect::<Vec<_>>()
            .join("\u{1f}"),
        escape(&cwd),
        variables
            .iter()
            .map(|pair| escape(pair))
            .collect::<Vec<_>>()
            .join("\u{1f}"),
    ];
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .expect("PROBE_LOG is writable");
    // One write per record, so records from a pipeline's concurrent
    // commands never interleave within a line.
    file.write_all(format!("{}\n", fields.join("\t")).as_bytes())
        .expect("the record is written");
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\u{1f}', "\\u")
}
