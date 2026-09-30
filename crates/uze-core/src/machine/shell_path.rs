//! Taking back the `PATH` block earlier builds wrote into a shell's startup
//! file.
//!
//! UZE no longer edits shell startup files: the runtime shim belongs to the
//! workspace, which puts the shims first on `PATH` in the panes it starts.
//! A build before that wrote a whole-line marked block into the operator's
//! `.bashrc`/`.zshrc`/`config.fish`; the block is UZE's, so `uze setup`
//! removes it, and nothing else. Only a block whose two markers verify is
//! removed, every byte outside it kept; a file with one marker, or with
//! them out of order, is left alone and reported.
//!
//! This exists only to take back what UZE wrote. It is deleted, with its
//! caller and the test that holds the date, once no supported release is a
//! build that wrote the block (`the_block_is_taken_back_only_until_1_0_0`).

use std::{env, fs, path::Path, path::PathBuf};

use crate::{
    error::{Result, UzeError},
    persistence::write_atomic_preserving,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellRcTarget {
    pub kind: ShellKind,
    pub rc_file: PathBuf,
}

/// Which file bash actually reads, which is not the same question on both
/// platforms.
///
/// Bash reads `~/.bashrc` for an interactive non-login shell and
/// `~/.bash_profile` for a login one. On Linux a terminal window opens the
/// former; on macOS every terminal window is a login shell, so a `PATH` line
/// written to `.bashrc` there is a line the user never runs — the shims
/// would simply not be found, with a file on disk saying they should be.
///
/// zsh needs no such split: it reads `.zshrc` for any interactive shell,
/// login or not, which is why the zsh arm below names one file for both.
#[cfg(target_os = "macos")]
const BASH_RC_FILE: &str = ".bash_profile";
#[cfg(not(target_os = "macos"))]
const BASH_RC_FILE: &str = ".bashrc";

/// Detects the user's shell from `$SHELL` and the conventional rc file for
/// it under `home_dir`. `None` for anything not recognized (POSIX `sh`,
/// `dash`, `csh`, `$SHELL` unset, …) — deliberately conservative, never
/// guesses at an unfamiliar shell's syntax or startup file.
pub fn detect_shell_rc(home_dir: &Path) -> Option<ShellRcTarget> {
    let shell = env::var("SHELL").ok()?;
    let name = Path::new(&shell).file_name()?.to_str()?;
    match name {
        "bash" => Some(ShellRcTarget {
            kind: ShellKind::Bash,
            rc_file: home_dir.join(BASH_RC_FILE),
        }),
        "zsh" => Some(ShellRcTarget {
            kind: ShellKind::Zsh,
            rc_file: home_dir.join(".zshrc"),
        }),
        "fish" => Some(ShellRcTarget {
            kind: ShellKind::Fish,
            rc_file: home_dir.join(".config/fish/config.fish"),
        }),
        _ => None,
    }
}

const BEGIN: &str = "# >>> uze shims path >>>";
const END: &str = "# <<< uze shims path <<<";

/// The line ending the file already uses, so rewriting it does not convert
/// someone's CRLF file.
fn line_ending(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Removes the marked block an earlier build wrote into `target.rc_file`,
/// keeping every other byte. `Ok(true)` when it removed one, `Ok(false)`
/// when the file holds none (or does not exist). Refuses (`Err`) a file
/// whose markers do not verify — one of them, or both out of order —
/// because something other than UZE edited it last.
pub fn take_back_path_block(target: &ShellRcTarget) -> Result<bool> {
    let existing = match fs::read_to_string(&target.rc_file) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(UzeError::Read {
                path: target.rc_file.clone(),
                source: error,
            });
        }
    };
    let newline = line_ending(&existing);
    let lines: Vec<&str> = existing.lines().collect();
    let begins = lines.iter().filter(|line| **line == BEGIN).count();
    let ends = lines.iter().filter(|line| **line == END).count();
    let begin = lines.iter().position(|line| *line == BEGIN);
    let end = lines.iter().position(|line| *line == END);
    match (begin, end) {
        (None, None) => Ok(false),
        (Some(b), Some(e)) if e > b && begins == 1 && ends == 1 => {
            let kept: Vec<&str> = lines[..b].iter().chain(&lines[e + 1..]).copied().collect();
            let mut rebuilt = kept.join(newline);
            if !kept.is_empty() && existing.ends_with('\n') {
                rebuilt.push_str(newline);
            }
            write_atomic_preserving(&target.rc_file, rebuilt.as_bytes())?;
            Ok(true)
        }
        _ => Err(UzeError::ManagedRegionDrift(target.rc_file.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_with(label: &str, content: &str) -> ShellRcTarget {
        let root = uze_testkit::temp::scratch(label);
        let rc_file = root.join(".zshrc");
        fs::write(&rc_file, content).unwrap();
        ShellRcTarget {
            kind: ShellKind::Zsh,
            rc_file,
        }
    }

    const BLOCK: &str = "# >>> uze shims path >>>\nexport PATH=\"/home/x/.uze/shims:$PATH\"\n# <<< uze shims path <<<\n";

    #[test]
    fn the_block_is_removed_and_every_other_byte_kept() {
        let before = "export EDITOR=vim\n";
        let after = "alias ll='ls -l'\n";
        let target = target_with("shell-take-back", &format!("{before}{BLOCK}{after}"));
        assert!(take_back_path_block(&target).unwrap());
        assert_eq!(
            fs::read_to_string(&target.rc_file).unwrap(),
            format!("{before}{after}")
        );
        assert!(
            !take_back_path_block(&target).unwrap(),
            "a second pass has nothing to take back"
        );
    }

    #[test]
    fn a_file_holding_only_the_block_is_left_empty() {
        let target = target_with("shell-take-back-only", BLOCK);
        assert!(take_back_path_block(&target).unwrap());
        assert_eq!(fs::read_to_string(&target.rc_file).unwrap(), "");
    }

    #[test]
    fn a_crlf_file_keeps_its_line_endings() {
        let content = format!(
            "export A=1\r\n{}export B=2\r\n",
            BLOCK.replace('\n', "\r\n")
        );
        let target = target_with("shell-take-back-crlf", &content);
        assert!(take_back_path_block(&target).unwrap());
        assert_eq!(
            fs::read_to_string(&target.rc_file).unwrap(),
            "export A=1\r\nexport B=2\r\n"
        );
    }

    #[test]
    fn a_file_without_the_block_is_never_written() {
        let target = target_with("shell-take-back-none", "export A=1");
        assert!(!take_back_path_block(&target).unwrap());
        assert_eq!(fs::read_to_string(&target.rc_file).unwrap(), "export A=1");
    }

    #[test]
    fn markers_that_do_not_verify_are_left_untouched() {
        let content = format!("{BEGIN}\nsomething odd\n");
        let target = target_with("shell-take-back-malformed", &content);
        assert!(take_back_path_block(&target).is_err());
        assert_eq!(fs::read_to_string(&target.rc_file).unwrap(), content);
    }

    /// The removal is the one place UZE still edits a shell startup file,
    /// and only to take back its own block. Every build before the one that
    /// stopped writing it is a beta; once 1.0.0 is released nobody can
    /// upgrade to a supported release straight from one of them without
    /// having run `uze setup` since, so the removal, its caller in `setup`,
    /// and this test go.
    #[test]
    fn the_block_is_taken_back_only_until_1_0_0() {
        let version = env!("CARGO_PKG_VERSION");
        assert!(
            version.starts_with("0.") || version.starts_with("1.0.0-"),
            "uze {version} is past 1.0.0: delete `take_back_path_block`, its call in \
             `ensure_runtime_shim`, and this test"
        );
    }

    /// `$SHELL` is process-global and `detect_shell_rc` reads it, so the
    /// three tests below cannot each set it to a different value and run at
    /// the same time. The testkit's scope both serializes them against every
    /// other env mutation in this binary and restores the previous value on
    /// drop — including on a panic.
    fn shell(value: &str) -> uze_testkit::env::ProcessEnvGuard<'static> {
        let mut scope = uze_testkit::env::scope();
        scope.set("SHELL", value);
        scope
    }

    #[test]
    fn unrecognized_shell_detects_to_none() {
        let _shell = shell("/bin/dash");
        let result = detect_shell_rc(Path::new("/home/x"));
        assert_eq!(result, None);
    }

    /// The file bash is told to read has to be the file bash reads. On macOS
    /// a terminal window is a login shell and never opens `.bashrc`, so
    /// writing there would leave the shims unreachable behind a file that
    /// says otherwise. Asserted per platform rather than skipped off Linux:
    /// the whole point is that the two answers differ.
    #[test]
    fn bash_is_pointed_at_the_file_this_platform_actually_reads() {
        let _shell = shell("/bin/bash");
        let target = detect_shell_rc(Path::new("/home/x")).expect("bash is recognized");
        let expected = if cfg!(target_os = "macos") {
            "/home/x/.bash_profile"
        } else {
            "/home/x/.bashrc"
        };
        assert_eq!(target.rc_file, Path::new(expected));
    }

    /// zsh, by contrast, reads `.zshrc` for any interactive shell — login or
    /// not — so it has one answer on every platform, and a change that gave
    /// it two would be a mistake this catches.
    #[test]
    fn zsh_reads_the_same_file_everywhere() {
        let _shell = shell("/bin/zsh");
        let target = detect_shell_rc(Path::new("/home/x")).expect("zsh is recognized");
        assert_eq!(target.rc_file, Path::new("/home/x/.zshrc"));
    }
}
