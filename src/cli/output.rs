//! How a command answers: JSON or text, a spinner while it works.

use crate::*;

pub(crate) fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("application report serializable")
    );
}

/// Prints `report` the way `format` asks: as JSON for a program, or drawn
/// by `render` for a person.
pub(crate) fn emit<T: serde::Serialize>(
    format: OutputFormat,
    report: &T,
    render: impl FnOnce(&T) -> String,
) {
    match format {
        OutputFormat::Text => print!("{}", render(report)),
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
