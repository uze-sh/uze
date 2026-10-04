//! The shell a person's command line is written for, on this platform.
//!
//! A project's setup steps, its gates and a hook's handlers are lines in
//! one shell's language. This is that shell, here: the POSIX shell on Unix,
//! Windows PowerShell 5.1 on Windows, the one every supported Windows
//! carries, so what runs a line does not depend on what else is installed.

use std::process::Command;

/// The key a manifest spells this platform's command under.
pub const KEY: &str = imp::KEY;

/// What starts this shell on a line, before the line itself: what a
/// generated runtime spawns (`[...ARGV, line]`).
pub const ARGV: &[&str] = imp::ARGV;

/// The program and arguments that run `line` in this shell, for a caller
/// that describes a process rather than spawning it (see [`command`]).
pub fn invocation(line: &str) -> (String, Vec<String>) {
    let mut argv = ARGV.iter().map(|word| (*word).to_owned());
    let program = argv.next().unwrap_or_default();
    (
        program,
        argv.chain(std::iter::once(script_text(line))).collect(),
    )
}

/// A command that runs `line` in this shell.
///
/// PowerShell does not stop a line at a native command that fails, and its
/// own exit status is not that command's: the line runs with cmdlet errors
/// terminating, and ends by exiting with the last native command's code
/// when that code is not zero. `pnpm install; Copy-Item a b` therefore
/// fails when `pnpm install` does, unless a later native command succeeds
/// after it.
pub fn command(line: &str) -> Command {
    let mut command = Command::new(ARGV[0]);
    command.args(&ARGV[1..]).arg(script_text(line));
    command
}

/// `line` as the text this shell is handed after [`ARGV`] (see [`command`]
/// for what it adds), for a runtime that spawns the shell itself.
pub fn script_text(line: &str) -> String {
    imp::line(line)
}

/// `fragment` as one literal word on a line this shell reads.
pub fn quote(fragment: &str) -> String {
    imp::quote(fragment)
}

/// The program and arguments that run a script file this shell reads:
/// the script itself on Unix, PowerShell told to run it on Windows, where a
/// script is not an executable.
pub fn script(path: &str) -> (String, Vec<String>) {
    imp::script(path)
}

/// `program` and `arguments` as one line this shell runs as that command,
/// every word literal.
pub fn command_line(program: &str, arguments: &[String]) -> String {
    imp::command_line(program, arguments)
}

/// The words a line [`command_line`] wrote, read back: the program, then
/// its arguments. `None` for a line this shell would refuse (an
/// unterminated quote).
pub fn words(line: &str) -> Option<Vec<String>> {
    imp::words(line)
}

/// Of a line written once for each shell, the one this platform's shell
/// runs.
pub fn spelling<'a>(posix: &'a str, windows: &'a str) -> &'a str {
    imp::spelling(posix, windows)
}

/// The extension of a script file this shell reads, without the dot.
pub const SCRIPT_EXTENSION: Option<&str> = imp::SCRIPT_EXTENSION;

#[cfg(unix)]
mod imp {
    pub(super) const KEY: &str = "posix";

    pub(super) fn spelling<'a>(posix: &'a str, _windows: &'a str) -> &'a str {
        posix
    }
    pub(super) const ARGV: &[&str] = &["sh", "-c"];
    pub(super) const SCRIPT_EXTENSION: Option<&str> = None;

    pub(super) fn line(line: &str) -> String {
        line.to_owned()
    }

    /// Single quotes keep spaces, quotes and `$` literal; a single quote
    /// becomes the `'\''` splice.
    pub(super) fn quote(fragment: &str) -> String {
        format!("'{}'", fragment.replace('\'', "'\\''"))
    }

    pub(super) fn script(path: &str) -> (String, Vec<String>) {
        (path.to_owned(), Vec::new())
    }

    /// Bare words, single-quoted runs and backslash-escaped characters.
    pub(super) fn words(line: &str) -> Option<Vec<String>> {
        let mut words = Vec::new();
        let mut word: Option<String> = None;
        let mut characters = line.chars();
        while let Some(character) = characters.next() {
            match character {
                '\'' => {
                    let quoted = word.get_or_insert_with(String::new);
                    loop {
                        match characters.next()? {
                            '\'' => break,
                            inside => quoted.push(inside),
                        }
                    }
                }
                '\\' => word
                    .get_or_insert_with(String::new)
                    .push(characters.next()?),
                separator if separator.is_whitespace() => words.extend(word.take()),
                other => word.get_or_insert_with(String::new).push(other),
            }
        }
        words.extend(word);
        Some(words)
    }

    pub(super) fn command_line(program: &str, arguments: &[String]) -> String {
        std::iter::once(program)
            .chain(arguments.iter().map(String::as_str))
            .map(quote)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(windows)]
mod imp {
    pub(super) const KEY: &str = "windows";

    pub(super) fn spelling<'a>(_posix: &'a str, windows: &'a str) -> &'a str {
        windows
    }
    pub(super) const ARGV: &[&str] = &[
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
    ];
    pub(super) const SCRIPT_EXTENSION: Option<&str> = Some("ps1");

    pub(super) fn line(line: &str) -> String {
        format!(
            "$ErrorActionPreference = 'Stop'; \
             [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false; \
             {line}\nif ($LASTEXITCODE) {{ exit $LASTEXITCODE }}"
        )
    }

    /// Single quotes expand nothing; a quote inside is written twice.
    pub(super) fn quote(fragment: &str) -> String {
        format!("'{}'", fragment.replace('\'', "''"))
    }

    /// The call operator, then bare words and single-quoted strings, in
    /// which a doubled quote is one quote.
    pub(super) fn words(line: &str) -> Option<Vec<String>> {
        let line = line.trim_start();
        let line = line.strip_prefix('&').unwrap_or(line);
        let mut words = Vec::new();
        let mut word: Option<String> = None;
        let mut characters = line.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                '\'' => {
                    let quoted = word.get_or_insert_with(String::new);
                    loop {
                        match characters.next()? {
                            '\'' if characters.peek() == Some(&'\'') => {
                                characters.next();
                                quoted.push('\'');
                            }
                            '\'' => break,
                            inside => quoted.push(inside),
                        }
                    }
                }
                separator if separator.is_whitespace() => words.extend(word.take()),
                other => word.get_or_insert_with(String::new).push(other),
            }
        }
        words.extend(word);
        Some(words)
    }

    /// A quoted first word is a string to PowerShell, not a command: the
    /// call operator runs it.
    pub(super) fn command_line(program: &str, arguments: &[String]) -> String {
        std::iter::once(format!("& {}", quote(program)))
            .chain(arguments.iter().map(|argument| quote(argument)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(super) fn script(path: &str) -> (String, Vec<String>) {
        (
            "powershell.exe".to_owned(),
            [
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                path,
            ]
            .map(str::to_owned)
            .to_vec(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_reads_back_as_the_words_it_was_written_from() {
        let arguments = ["/tmp/plugin root", "it's", "a'b'c", ""]
            .map(str::to_owned)
            .to_vec();
        let line = command_line("/state/hooks/exec", &arguments);
        let mut expected = vec!["/state/hooks/exec".to_owned()];
        expected.extend(arguments);
        assert_eq!(words(&line).unwrap(), expected);
        assert_eq!(words("'unterminated"), None);
    }
}
