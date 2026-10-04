//! A command line an author wrote, and the shell it was written for.
//!
//! A project's setup steps and gates, and a hook's handler, are text in
//! one shell's language: `npm ci && cp .env.example .env` is POSIX and
//! means nothing to PowerShell. So a command is either one line — POSIX, as
//! every manifest written so far is — or a `posix`/`windows` pair, and each
//! platform runs only the spelling written for it. A command with no
//! spelling for this platform is never run in another shell; the caller
//! reports it by name.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShellCommand {
    /// One line, for the POSIX shell.
    Line(String),
    PerPlatform(Spellings),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spellings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<String>,
}

impl ShellCommand {
    /// The spelling this platform runs, if the author wrote one.
    pub fn here(&self) -> Option<&str> {
        match self {
            Self::Line(line) => (!cfg!(windows)).then_some(line.as_str()),
            Self::PerPlatform(spellings) => if cfg!(windows) {
                spellings.windows.as_deref()
            } else {
                spellings.posix.as_deref()
            }
            .filter(|line| !line.trim().is_empty()),
        }
    }

    /// What names this command to a person: the spelling that runs here,
    /// else whichever one was written.
    pub fn label(&self) -> &str {
        match self {
            Self::Line(line) => line,
            Self::PerPlatform(spellings) => self
                .here()
                .or(spellings.posix.as_deref())
                .or(spellings.windows.as_deref())
                .unwrap_or_default(),
        }
    }

    /// The platform this build runs on, as a manifest key names it.
    pub fn platform() -> &'static str {
        if cfg!(windows) { "windows" } else { "posix" }
    }
}

impl From<&str> for ShellCommand {
    fn from(line: &str) -> Self {
        Self::Line(line.to_owned())
    }
}

impl From<String> for ShellCommand {
    fn from(line: String) -> Self {
        Self::Line(line)
    }
}

impl fmt::Display for ShellCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_line_is_posix_and_a_pair_answers_per_platform() {
        let line: ShellCommand = serde_json::from_str(r#""pnpm install""#).unwrap();
        let pair: ShellCommand =
            serde_json::from_str(r#"{"posix": "./guard", "windows": "./guard.ps1"}"#).unwrap();
        let windows_only: ShellCommand =
            serde_json::from_str(r#"{"windows": "Copy-Item a b"}"#).unwrap();
        if cfg!(windows) {
            assert_eq!(line.here(), None);
            assert_eq!(pair.here(), Some("./guard.ps1"));
            assert_eq!(windows_only.here(), Some("Copy-Item a b"));
        } else {
            assert_eq!(line.here(), Some("pnpm install"));
            assert_eq!(pair.here(), Some("./guard"));
            assert_eq!(windows_only.here(), None);
        }
        assert_eq!(line.label(), "pnpm install");
    }

    #[test]
    fn an_unknown_platform_key_is_refused() {
        assert!(serde_json::from_str::<ShellCommand>(r#"{"macos": "x"}"#).is_err());
    }
}
