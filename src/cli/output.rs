//! How a command answers: JSON or text, a spinner while it works.

use crate::*;

pub(crate) fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        json_for_terminal(
            &serde_json::to_string_pretty(value).expect("application report serializable")
        )
    );
}

/// `json` with every terminal control written as a `\uXXXX` escape.
/// `serde_json` escapes C0 only, and a report piped to a terminal or a
/// log viewer would carry DEL, the C1 controls and the bidi overrides a
/// package wrote as they are. Each can only stand inside a string, where
/// the escape is the same character, so the document reads back unchanged.
pub(crate) fn json_for_terminal(json: &str) -> std::borrow::Cow<'_, str> {
    if !json.chars().any(uze_application::is_terminal_control) {
        return std::borrow::Cow::Borrowed(json);
    }
    let mut out = String::with_capacity(json.len() + 16);
    for character in json.chars() {
        if uze_application::is_terminal_control(character) {
            out.push_str(&format!("\\u{:04x}", u32::from(character)));
        } else {
            out.push(character);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Prints `report` the way `format` asks: as JSON for a program, or drawn
/// by `render` for a person.
pub(crate) fn emit<T: serde::Serialize>(
    format: OutputFormat,
    report: &T,
    render: impl FnOnce(&T) -> String,
) {
    match format {
        OutputFormat::Text => print!("{}", progress::for_terminal(&render(report))),
        OutputFormat::Json => print_json(report),
    }
}

/// Runs `operation` under a spinner saying `message`, which is gone before
/// anything else is printed. A failure is returned, never printed here:
/// `main` says it once.
pub(crate) fn with_spinner<T>(message: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let spinner = progress::spinner(message);
    let outcome = operation();
    progress::settle_steps(&spinner, outcome.is_ok());
    spinner.finish_and_clear();
    outcome
}

/// The directory the command was run from, which a project command speaks
/// about.
pub(crate) fn cwd() -> Result<PathBuf> {
    std::env::current_dir().map_err(|source| uze_application::UzeError::Read {
        path: PathBuf::from("."),
        source,
    })
}

pub(crate) fn terminal_error(error: impl std::fmt::Display) -> uze_application::UzeError {
    uze_application::UzeError::TerminalRuntime(error.to_string())
}

#[cfg(test)]
mod tests {
    /// A C1 introducer, DEL and a bidi override in a value come out as
    /// escapes, and the document still reads back to the same value.
    #[test]
    fn json_output_carries_no_raw_terminal_control() {
        let value = serde_json::json!({ "name": "a\u{9b}2Jb\u{7f}\u{202e}c\u{1b}" });
        let printed =
            super::json_for_terminal(&serde_json::to_string_pretty(&value).unwrap()).into_owned();
        assert!(
            !printed.chars().any(uze_application::is_terminal_control),
            "{printed:?}"
        );
        assert!(printed.contains("\\u009b") && printed.contains("\\u202e"));
        let read: serde_json::Value = serde_json::from_str(&printed).unwrap();
        assert_eq!(read, value);
    }
}
