//! The login Keychain, through the system's `security` by path. The value
//! is handed over on standard input, in `security`'s interactive mode, so
//! it is in no process's arguments.

use std::{
    io::{self, Write as _},
    process::{Output, Stdio},
};

use super::SecretKey;

pub(super) const KEPT_IN: &str = "the Keychain";

/// What `security` exits with when the item is not there.
const NOT_FOUND: i32 = 44;

pub(super) fn store(key: &SecretKey<'_>, value: &str) -> io::Result<()> {
    let mut child = crate::tools::system("security")
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let line = format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        quoted(key.service),
        quoted(key.account),
        quoted(value)
    );
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(line.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    succeeded(&output, "add-generic-password")
}

pub(super) fn read(key: &SecretKey<'_>) -> io::Result<Option<String>> {
    let output = crate::tools::system("security")
        .args([
            "find-generic-password",
            "-s",
            key.service,
            "-a",
            key.account,
            "-w",
        ])
        .stdin(Stdio::null())
        .output()?;
    if output.status.code() == Some(NOT_FOUND) {
        return Ok(None);
    }
    succeeded(&output, "find-generic-password")?;
    let value = String::from_utf8_lossy(&output.stdout);
    Ok(Some(value.trim_end_matches('\n').to_owned()))
}

pub(super) fn delete(key: &SecretKey<'_>) -> io::Result<()> {
    let output = crate::tools::system("security")
        .args([
            "delete-generic-password",
            "-s",
            key.service,
            "-a",
            key.account,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()?;
    if output.status.code() == Some(NOT_FOUND) {
        return Ok(());
    }
    succeeded(&output, "delete-generic-password")
}

/// A word of `security -i`'s line, in double quotes with `\` and `"`
/// escaped.
fn quoted(word: &str) -> String {
    format!("\"{}\"", word.replace('\\', "\\\\").replace('"', "\\\""))
}

fn succeeded(output: &Output, what: &str) -> io::Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "security {what} failed ({})",
            output.status
        )))
    }
}
