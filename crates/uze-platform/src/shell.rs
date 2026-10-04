//! The shell a person's command line is written for, on this platform.
//!
//! A project's setup steps, its gates and a hook's handlers are lines in
//! one shell's language. This is that shell, here: the POSIX shell on Unix,
//! Windows PowerShell 5.1 on Windows, the one every supported Windows
//! carries, so what runs a line does not depend on what else is installed.

use std::process::Command;

/// The key a manifest spells this platform's command under.
pub const KEY: &str = imp::KEY;

/// The language this platform's shell reads, for a caller that writes a
/// script in it: one template per family, chosen by this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `sh`, as POSIX defines it.
    Posix,
    /// Windows PowerShell 5.1.
    PowerShell,
}

/// The family of this platform's shell.
pub const FAMILY: Family = imp::FAMILY;

/// The shell a person's own terminal opens: `$SHELL` on Unix; on Windows
/// PowerShell 7 when it is installed, else the Windows PowerShell every
/// supported Windows carries. A person's interactive shell is a preference,
/// so the newer one is preferred; the commands UZE runs always use the one
/// that is always there ([`command`]).
pub fn interactive() -> String {
    imp::interactive()
}

/// What a terminal emulator puts in the environment of the programs it
/// runs: `TERM` on Unix, when the emulator's own environment has none; on
/// Windows `COLORTERM`, since its programs read no `TERM`, and a few change
/// behaviour when it names a Unix terminal.
pub fn terminal_environment() -> Vec<(&'static str, &'static str)> {
    imp::terminal_environment()
}

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

/// Why this machine's shell would refuse what UZE hands it, if it would:
/// asked of the shell itself, so it costs a process (callers cache it).
/// Never on Unix. On Windows, a Group Policy execution policy that does not
/// let a local script run (`-ExecutionPolicy Bypass` yields to it, and the
/// hook wrapper is a script file), or a language mode short of
/// `FullLanguage` (the wrapper loads a .NET serializer).
pub fn refusal() -> Option<String> {
    imp::refusal()
}

/// The extension of a script file this shell reads, without the dot.
pub const SCRIPT_EXTENSION: Option<&str> = imp::SCRIPT_EXTENSION;

#[cfg(unix)]
mod imp {
    pub(super) const KEY: &str = "posix";
    pub(super) const FAMILY: super::Family = super::Family::Posix;

    pub(super) fn interactive() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }

    pub(super) fn terminal_environment() -> Vec<(&'static str, &'static str)> {
        if std::env::var_os("TERM").is_some() {
            Vec::new()
        } else {
            vec![("TERM", "xterm-256color")]
        }
    }

    pub(super) fn spelling<'a>(posix: &'a str, _windows: &'a str) -> &'a str {
        posix
    }

    pub(super) fn refusal() -> Option<String> {
        None
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
    pub(super) const FAMILY: super::Family = super::Family::PowerShell;

    /// Asked once per process: a pane is opened far more often than
    /// PowerShell 7 is installed, and a `PATH` walk over `PATHEXT` per pane
    /// is the cost of asking again. One installed while a server runs is
    /// what that server's panes open once it is restarted.
    pub(super) fn interactive() -> String {
        static INSTALLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let installed = *INSTALLED.get_or_init(|| crate::executable::on_path("pwsh").is_some());
        if installed {
            "pwsh.exe"
        } else {
            "powershell.exe"
        }
        .to_owned()
    }

    pub(super) fn terminal_environment() -> Vec<(&'static str, &'static str)> {
        vec![("COLORTERM", "truecolor")]
    }

    pub(super) fn spelling<'a>(_posix: &'a str, windows: &'a str) -> &'a str {
        windows
    }

    pub(super) fn refusal() -> Option<String> {
        let probe = "(Get-ExecutionPolicy -Scope MachinePolicy).ToString(); \
                     (Get-ExecutionPolicy -Scope UserPolicy).ToString(); \
                     $ExecutionContext.SessionState.LanguageMode.ToString()";
        let output = super::command(probe)
            .stdin(std::process::Stdio::null())
            .output()
            .ok()?;
        let answer = String::from_utf8_lossy(&output.stdout).into_owned();
        refusal_in(&answer)
    }

    /// The probe's three lines: the machine's and the user's Group Policy
    /// execution policy, then the language mode.
    pub(super) fn refusal_in(answer: &str) -> Option<String> {
        let mut lines = answer.lines().map(str::trim);
        let (machine, user, language) = (lines.next()?, lines.next()?, lines.next()?);
        let blocking = |policy: &str| matches!(policy, "Restricted" | "AllSigned");
        if let Some((scope, policy)) = [("the machine", machine), ("this user", user)]
            .into_iter()
            .find(|(_, policy)| blocking(policy))
        {
            return Some(format!(
                "Group Policy sets PowerShell's execution policy for {scope} to {policy}, so \
                 the hook wrappers UZE generates cannot run"
            ));
        }
        (language != "FullLanguage").then(|| {
            format!(
                "PowerShell runs in {language} mode, in which the hook wrappers UZE \
                 generates cannot run"
            )
        })
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

    /// What PowerShell reads as a single quote: the ASCII one, and the
    /// typographic ones a person types without meaning to (`don’t`).
    /// Doubling only the first left `’` ending the string early.
    const SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

    /// Single quotes expand nothing; a quote inside is written twice.
    pub(super) fn quote(fragment: &str) -> String {
        let mut quoted = String::with_capacity(fragment.len() + 2);
        quoted.push('\'');
        for character in fragment.chars() {
            quoted.push(character);
            if SINGLE_QUOTES.contains(&character) {
                quoted.push(character);
            }
        }
        quoted.push('\'');
        quoted
    }

    /// The call operator, then bare words and single-quoted strings, in
    /// which a doubled quote is one quote.
    pub(super) fn words(line: &str) -> Option<Vec<String>> {
        let line = line.trim_start();
        let line = line.strip_prefix('&').unwrap_or(line);
        let mut words = Vec::new();
        let mut word: Option<String> = None;
        let mut characters = line.chars().peekable();
        let is_quote = |character: &char| SINGLE_QUOTES.contains(character);
        while let Some(character) = characters.next() {
            match character {
                opening if is_quote(&opening) => {
                    let quoted = word.get_or_insert_with(String::new);
                    loop {
                        match characters.next()? {
                            inside if is_quote(&inside) => match characters.next_if(is_quote) {
                                Some(doubled) => quoted.push(doubled),
                                None => break,
                            },
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

    #[cfg(test)]
    #[test]
    fn a_policy_or_a_language_mode_that_stops_scripts_is_named() {
        assert_eq!(
            refusal_in("Undefined\r\nUndefined\r\nFullLanguage\r\n"),
            None
        );
        assert_eq!(refusal_in("RemoteSigned\nBypass\nFullLanguage"), None);
        let machine = refusal_in("AllSigned\nUndefined\nFullLanguage").unwrap();
        assert!(machine.contains("the machine to AllSigned"), "{machine}");
        let user = refusal_in("Undefined\nRestricted\nFullLanguage").unwrap();
        assert!(user.contains("this user to Restricted"), "{user}");
        let constrained = refusal_in("Undefined\nUndefined\nConstrainedLanguage").unwrap();
        assert!(constrained.contains("ConstrainedLanguage"), "{constrained}");
    }

    /// A machine no policy reaches (the Sandbox, a CI runner) runs what UZE
    /// hands its shell: the probe itself answers.
    #[cfg(test)]
    #[test]
    fn an_unmanaged_machine_s_shell_refuses_nothing() {
        assert_eq!(refusal(), None);
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
        let arguments = ["/tmp/plugin root", "it's", "a'b'c", "", "don’t"]
            .map(str::to_owned)
            .to_vec();
        let line = command_line("/state/hooks/exec", &arguments);
        let mut expected = vec!["/state/hooks/exec".to_owned()];
        expected.extend(arguments);
        assert_eq!(words(&line).unwrap(), expected);
        assert_eq!(words("'unterminated"), None);
    }

    /// A native command's exit code is the line's, and in PowerShell a
    /// native command that fails before a cmdlet that succeeds still fails
    /// the line, as it would under `sh -e`.
    /// What PowerShell itself reads back from a quoted fragment, typographic
    /// quotes included: one of them unquoted ended the string early and ran
    /// the rest as code. Windows only: the line runs in PowerShell.
    #[cfg(windows)]
    #[test]
    fn powershell_reads_a_quoted_fragment_back_as_written() {
        for fragment in ["don’t", "‘a’ ‚b‛", "it's", "C:\\Users\\D’Angelo"] {
            let output = command(&format!("Write-Output {}", quote(fragment)))
                .output()
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim_end(),
                fragment,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn a_native_failure_is_the_line_s_exit_code() {
        let code = |line: &str| command(line).status().unwrap().code();
        assert_eq!(code(spelling("exit 3", "cmd /c exit 3")), Some(3));
        assert_eq!(
            code(spelling(
                "sh -c 'exit 4' || exit $?; echo after",
                "cmd /c exit 4; Write-Output after"
            )),
            Some(4)
        );
        assert_eq!(code(spelling("true", "cmd /c exit 0")), Some(0));
    }
}
