//! Whether a fact on disk proves a resource cannot work.
//!
//! Only facts: a file that is not there, a program `PATH` does not have, a
//! definition that does not parse. Nothing is run to find out, a server
//! reached by URL is never judged, and a command line too clever to read
//! with certainty (pipes, substitutions, a variable other than the home
//! directory) is given the benefit of the doubt.
//!
//! `PATH` is the one this process has, which need not be the one a
//! harness started from a desktop launcher has.

use std::path::{Path, PathBuf};

use super::model::Runs;

/// Programs that take the script they run as their first argument.
const INTERPRETERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "node",
    "bun",
    "deno",
    "python",
    "python3",
    "ruby",
    "perl",
    "pwsh",
    "powershell",
];

const SCRIPT_EXTENSIONS: &[&str] = &[
    "sh", "bash", "zsh", "js", "mjs", "cjs", "ts", "py", "rb", "pl", "ps1",
];

#[derive(Debug, Eq, PartialEq)]
enum Target {
    File(PathBuf),
    Program(String),
}

/// Why what `runs` names cannot run, if a fact says so.
pub(super) fn runs_problem(runs: &Runs) -> Option<String> {
    let target = match runs {
        Runs::Url(_) => return None,
        Runs::Program(program) => word_target(program)?,
        Runs::Shell(line) => shell_target(line)?,
    };
    match target {
        Target::File(path) if !path.exists() => {
            Some(format!("script missing · {}", path.display()))
        }
        Target::Program(name) if uze_platform::executable::on_path(&name).is_none() => {
            Some(format!("command not found · {name}"))
        }
        _ => None,
    }
}

/// What a command line runs: the script an interpreter is handed, or the
/// program itself.
fn shell_target(line: &str) -> Option<Target> {
    if line
        .chars()
        .any(|c| matches!(c, '|' | '&' | ';' | '`' | '<' | '>' | '(' | ')'))
    {
        return None;
    }
    let mut words = line
        .split_whitespace()
        .map(unquote)
        .skip_while(|word| is_assignment(word));
    let program = words.next()?;
    let target = word_target(program)?;
    let Target::Program(name) = &target else {
        return Some(target);
    };
    if !INTERPRETERS.contains(&program_stem(name)) {
        return Some(target);
    }
    match words.find(|word| !word.starts_with('-')) {
        Some(script) if looks_like_script(script) => word_target(script),
        _ => Some(target),
    }
}

fn word_target(word: &str) -> Option<Target> {
    let expanded = expand_home(word)?;
    if expanded.contains('/') || expanded.contains('\\') {
        Some(Target::File(PathBuf::from(expanded)))
    } else {
        Some(Target::Program(expanded))
    }
}

/// `~`, `$HOME` and `${HOME}` resolved; any other variable means the
/// word cannot be judged.
fn expand_home(word: &str) -> Option<String> {
    let home = crate::user_home();
    let home_text = || {
        home.as_deref()
            .map(|path| path.to_string_lossy().into_owned())
    };
    let expanded = if let Some(rest) = word.strip_prefix("~/") {
        format!("{}/{rest}", home_text()?)
    } else if let Some(rest) = word.strip_prefix("${HOME}") {
        format!("{}{rest}", home_text()?)
    } else if let Some(rest) = word.strip_prefix("$HOME") {
        format!("{}{rest}", home_text()?)
    } else {
        word.to_owned()
    };
    (!expanded.contains('$')).then_some(expanded)
}

fn unquote(word: &str) -> &str {
    word.trim_matches(|c| c == '"' || c == '\'')
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn program_stem(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.strip_suffix(".exe").unwrap_or(file)
}

fn looks_like_script(word: &str) -> bool {
    word.contains('/')
        || word.contains('\\')
        || word.starts_with('~')
        || Path::new(word)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| SCRIPT_EXTENSIONS.contains(&extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_whose_script_was_deleted_is_broken() {
        let reason = runs_problem(&Runs::Shell("bash /nowhere/lint-guard.sh --fix".into()));
        assert_eq!(
            reason.as_deref(),
            Some("script missing · /nowhere/lint-guard.sh")
        );
    }

    #[test]
    fn a_script_run_directly_is_judged_by_its_file() {
        assert!(runs_problem(&Runs::Shell("/nowhere/hook".into())).is_some());
        let here = std::env::current_exe().unwrap();
        assert!(runs_problem(&Runs::Shell(format!("{} --version", here.display()))).is_none());
    }

    #[test]
    fn a_program_missing_from_path_is_broken() {
        let reason = runs_problem(&Runs::Program("uze-no-such-program-anywhere".into()));
        assert_eq!(
            reason.as_deref(),
            Some("command not found · uze-no-such-program-anywhere")
        );
    }

    #[test]
    fn a_server_reached_by_url_is_never_judged() {
        assert!(runs_problem(&Runs::Url("https://example.invalid/mcp".into())).is_none());
    }

    #[test]
    fn a_line_too_clever_to_read_is_given_the_benefit_of_the_doubt() {
        for line in [
            "cat /nowhere | sh",
            "test -f x && /nowhere/run",
            "$TOOL_DIR/run.sh",
            "sh -c \"$(cat /nowhere)\"",
        ] {
            assert!(runs_problem(&Runs::Shell(line.into())).is_none(), "{line}");
        }
    }

    #[test]
    fn assignments_before_the_program_are_skipped() {
        assert_eq!(
            shell_target("FOO=1 BAR=x node /nowhere/hook.js"),
            Some(Target::File(PathBuf::from("/nowhere/hook.js")))
        );
    }

    #[test]
    fn an_interpreter_flag_is_not_the_script() {
        assert_eq!(
            shell_target("python3 -u /nowhere/hook.py"),
            Some(Target::File(PathBuf::from("/nowhere/hook.py")))
        );
        assert_eq!(
            shell_target("node --version"),
            Some(Target::Program("node".into()))
        );
    }
}
