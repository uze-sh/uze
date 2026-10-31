//! `uze __confine '<confinement>' -- <program> <arguments…>`: the helper a
//! program runs through to be confined. Answered before anything else in
//! `main` — no home, no configuration, no telemetry, no thread — because
//! the confinement applies to this process alone and only while it has a
//! single thread, and nothing it reaches first should be reachable at all.

use std::ffi::{OsStr, OsString};

use uze_platform::confine::{self, Confinement, NOT_ENFORCED_EXIT};

pub(crate) const CONFINE_COMMAND: &str = "__confine";

/// Becomes the confined program when this invocation is the helper's, and
/// otherwise returns at once.
pub(crate) fn confine_if_asked() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(OsStr::new(CONFINE_COMMAND)) {
        return;
    }
    let error = match parse(arguments.collect()) {
        Ok((confinement, program)) => confine::enter(&confinement, &program).to_string(),
        Err(reason) => reason,
    };
    eprintln!("uze {CONFINE_COMMAND}: not confined: {error}");
    std::process::exit(NOT_ENFORCED_EXIT);
}

fn parse(arguments: Vec<OsString>) -> Result<(Confinement, Vec<OsString>), String> {
    let mut arguments = arguments.into_iter();
    let confinement = arguments
        .next()
        .ok_or("no confinement given")?
        .into_string()
        .map_err(|_| "the confinement is not UTF-8".to_owned())?;
    if arguments.next().as_deref() != Some(OsStr::new("--")) {
        return Err("`--` must come before the program".to_owned());
    }
    let program: Vec<OsString> = arguments.collect();
    if program.is_empty() {
        return Err("no program to run".to_owned());
    }
    let confinement = serde_json::from_str(&confinement)
        .map_err(|error| format!("the confinement does not read: {error}"))?;
    Ok((confinement, program))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFINEMENT: &str = r#"{"read":[],"read_execute":["/usr"],"write":[],
        "network":false,"limits":{"cpu_seconds":2,"file_size":1,"open_files":8,
        "address_space":1},"namespace":"uze"}"#;

    fn words(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_program_follows_the_separator() {
        let (confinement, program) =
            parse(words(&[CONFINEMENT, "--", "/bin/sh", "-c", "true"])).unwrap();
        assert_eq!(
            confinement.read_execute,
            vec![std::path::PathBuf::from("/usr")]
        );
        assert_eq!(program, words(&["/bin/sh", "-c", "true"]));
    }

    #[test]
    fn a_missing_separator_program_or_unreadable_confinement_is_refused() {
        assert!(parse(words(&[CONFINEMENT, "/bin/sh"])).is_err());
        assert!(parse(words(&[CONFINEMENT, "--"])).is_err());
        assert!(parse(words(&["{}", "--", "/bin/sh"])).is_err());
        assert!(parse(words(&[])).is_err());
    }
}
