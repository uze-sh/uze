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

const POSIX: &str = "posix";
const WINDOWS: &str = "windows";

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
    /// A command spelled for both shells.
    pub fn spelled(posix: impl Into<String>, windows: impl Into<String>) -> Self {
        Self::PerPlatform(Spellings {
            posix: Some(posix.into()),
            windows: Some(windows.into()),
        })
    }

    /// The spelling this platform runs, if the author wrote one.
    pub fn here(&self) -> Option<&str> {
        self.spelling(Self::platform())
    }

    /// The spelling written for the platform `key` names (`posix`,
    /// `windows`): a plain line is the POSIX one.
    pub fn spelling(&self, key: &str) -> Option<&str> {
        let written = match (self, key) {
            (Self::Line(line), POSIX) => Some(line.as_str()),
            (Self::Line(_), _) => None,
            (Self::PerPlatform(spellings), POSIX) => spellings.posix.as_deref(),
            (Self::PerPlatform(spellings), WINDOWS) => spellings.windows.as_deref(),
            (Self::PerPlatform(_), _) => None,
        };
        written.filter(|line| !line.trim().is_empty())
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

    /// Whether no spelling was written at all.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Line(line) => line.trim().is_empty(),
            Self::PerPlatform(spellings) => {
                [&spellings.posix, &spellings.windows]
                    .into_iter()
                    .all(|spelling| {
                        spelling
                            .as_deref()
                            .is_none_or(|line| line.trim().is_empty())
                    })
            }
        }
    }

    /// Every spelling, as one line a person approving it reads: a plain
    /// line as written, a pair with both halves named. What trust shows
    /// and compares, so changing either spelling asks again.
    pub fn describe(&self) -> String {
        match self {
            Self::Line(line) => line.clone(),
            Self::PerPlatform(spellings) => {
                [("posix", &spellings.posix), ("windows", &spellings.windows)]
                    .into_iter()
                    .filter_map(|(platform, spelling)| {
                        spelling
                            .as_deref()
                            .map(|line| format!("{platform}: {line}"))
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
        }
    }

    /// The platform this build runs on, as a manifest key names it.
    pub fn platform() -> &'static str {
        uze_platform::shell::KEY
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
        assert_eq!(line.spelling(POSIX), Some("pnpm install"));
        assert_eq!(line.spelling(WINDOWS), None);
        assert_eq!(pair.spelling(POSIX), Some("./guard"));
        assert_eq!(pair.spelling(WINDOWS), Some("./guard.ps1"));
        assert_eq!(windows_only.spelling(POSIX), None);
        assert_eq!(windows_only.spelling(WINDOWS), Some("Copy-Item a b"));
        assert_eq!(line.label(), "pnpm install");
        assert_eq!(pair.here(), pair.spelling(ShellCommand::platform()));
    }

    #[test]
    fn an_unknown_platform_key_is_refused() {
        assert!(serde_json::from_str::<ShellCommand>(r#"{"macos": "x"}"#).is_err());
    }
}
