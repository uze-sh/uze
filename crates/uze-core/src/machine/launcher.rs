//! The program that runs an author's script, on the platform it is
//! delivered to.
//!
//! A hook handler in exec form names a script and its words; something has
//! to start it. On POSIX a file with an execute bit starts itself, so its
//! shebang (a venv's python, `uv run --script`) is the author's choice and
//! wins. Otherwise, and always on Windows, which reads no shebang, the
//! file's extension picks from a small fixed table. File associations and
//! `PATHEXT` are never consulted: they depend on the machine, and a `.cmd`
//! re-parses its arguments in `cmd.exe`.
//!
//! The table is a function of the platform family rather than of the build,
//! so both columns are tested on every host.

use std::path::Path;

use crate::shell::Family;

/// How a script starts, or why nothing here can start it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    /// The words to run: the program, then the script, then the author's
    /// arguments. `needs` is the executable this asks of the machine, when
    /// it is not one every machine of the platform carries.
    Argv {
        argv: Vec<String>,
        needs: Option<String>,
    },
    Unplaceable {
        reason: String,
    },
}

/// What [`launch`] is asked about one script.
pub struct Script<'a> {
    /// The script's absolute path where the harness will run it.
    pub path: &'a Path,
    /// Whether the file carries an execute bit (always false on Windows,
    /// where the mark does not exist and is never consulted).
    pub executable: bool,
    /// The author's own launcher, replacing the table.
    pub interpreter: Option<&'a [String]>,
    pub args: &'a [String],
}

/// The Python a Windows machine answers with, in the order its installers
/// leave one: the `py` launcher, then `python`, then the Store's `python3`.
/// `py` is deprecated from Python 3.14, which is why it is a candidate and
/// not the answer.
pub const WINDOWS_PYTHONS: &[&[&str]] = &[&["py", "-3"], &["python"], &["python3"]];

/// The word a delivered line names when no Windows Python answers, so the
/// line runs once the person installs one.
const WINDOWS_PYTHON_FALLBACK: &str = "python";

/// How `script` starts on `family`'s platform. `python_answers` runs a
/// candidate's words with `--version` and says whether a Python 3 answered;
/// it is asked only on Windows, only for a `.py` script.
pub fn launch(
    script: &Script<'_>,
    family: Family,
    python_answers: &dyn Fn(&[&str]) -> bool,
) -> Launch {
    let path = script.path.display().to_string();
    let run = |program: &[String], needs: Option<String>| Launch::Argv {
        argv: program
            .iter()
            .cloned()
            .chain(std::iter::once(path.clone()))
            .chain(script.args.iter().cloned())
            .collect(),
        needs,
    };
    if let Some(interpreter) = script.interpreter {
        let needs = interpreter.first().cloned();
        return run(interpreter, needs);
    }
    if family == Family::Posix && script.executable {
        return run(&[], None);
    }
    let extension = script
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let words = |program: &[&str]| {
        program
            .iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    match (family, extension.as_str()) {
        (Family::Posix, "py") => run(&words(&["python3"]), Some("python3".to_owned())),
        (Family::PowerShell, "py") => {
            let found = WINDOWS_PYTHONS
                .iter()
                .find(|candidate| python_answers(candidate));
            match found {
                Some(candidate) => run(&words(candidate), Some(candidate[0].to_owned())),
                None => run(
                    &words(&[WINDOWS_PYTHON_FALLBACK]),
                    Some(WINDOWS_PYTHON_FALLBACK.to_owned()),
                ),
            }
        }
        (_, "js" | "mjs" | "cjs") => run(&words(&["node"]), Some("node".to_owned())),
        (Family::Posix, "sh") => run(&words(&["sh"]), None),
        (Family::Posix, "ps1") => run(
            &words(&["pwsh", "-NoProfile", "-File"]),
            Some("pwsh".to_owned()),
        ),
        (Family::PowerShell, "ps1") => run(
            &words(&[
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]),
            None,
        ),
        (Family::PowerShell, "exe") => run(&[], None),
        (Family::PowerShell, "sh") => Launch::Unplaceable {
            reason: "a `.sh` script has no Windows launcher".to_owned(),
        },
        (Family::Posix, _) => Launch::Unplaceable {
            reason: format!(
                "`{}` is not executable and its extension names no interpreter",
                script.path.display()
            ),
        },
        (Family::PowerShell, _) => Launch::Unplaceable {
            reason: format!(
                "`{}` has an extension with no Windows launcher",
                script.path.display()
            ),
        },
    }
}

/// Whether `words` answer `--version` as a Python 3, within a short
/// deadline. A Windows App Execution Alias passes a `PATH` lookup and then
/// exits 9009 or opens the Store; only an answer counts.
pub fn python_answers(words: &[&str]) -> bool {
    use std::process::Stdio;

    use crate::subprocess::{Seat, read_bounded, spawn_tree, wait_with_timeout};

    let Some((program, arguments)) = words.split_first() else {
        return false;
    };
    let mut command = std::process::Command::new(program);
    command
        .args(arguments)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Ok((mut child, tree)) = spawn_tree(&mut command, Seat::OwnGroup) else {
        return false;
    };
    // `--version` writes one short line, far below what a pipe holds, so
    // reading after the exit cannot block the child.
    match wait_with_timeout(&mut child, &tree, std::time::Duration::from_secs(5)) {
        Ok((status, false)) if status.success() => {
            let mut answer = Vec::new();
            if let Some(stdout) = child.stdout.take() {
                answer.extend(read_bounded(stdout, 256).0);
            }
            if let Some(stderr) = child.stderr.take() {
                answer.extend(read_bounded(stderr, 256).0);
            }
            String::from_utf8_lossy(&answer)
                .trim_start()
                .starts_with("Python 3")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn launched(
        path: &str,
        executable: bool,
        family: Family,
        python: &dyn Fn(&[&str]) -> bool,
    ) -> Launch {
        let path = PathBuf::from(path);
        let args = vec!["--strict".to_owned()];
        launch(
            &Script {
                path: &path,
                executable,
                interpreter: None,
                args: &args,
            },
            family,
            python,
        )
    }

    fn argv(words: &[&str], needs: Option<&str>) -> Launch {
        Launch::Argv {
            argv: words.iter().map(|word| (*word).to_owned()).collect(),
            needs: needs.map(str::to_owned),
        }
    }

    const NO_PYTHON: &dyn Fn(&[&str]) -> bool = &|_| false;

    #[test]
    fn a_python_script_on_posix_runs_in_python3_and_needs_it() {
        assert_eq!(
            launched("/root/hooks/guard.py", false, Family::Posix, NO_PYTHON),
            argv(
                &["python3", "/root/hooks/guard.py", "--strict"],
                Some("python3")
            )
        );
    }

    #[test]
    fn an_executable_file_on_posix_starts_itself_whatever_its_extension() {
        assert_eq!(
            launched("/root/hooks/guard.py", true, Family::Posix, NO_PYTHON),
            argv(&["/root/hooks/guard.py", "--strict"], None)
        );
    }

    #[test]
    fn windows_takes_the_first_python_that_answers() {
        let only_python = |words: &[&str]| words == ["python"];
        assert_eq!(
            launched(r"C:\root\guard.py", false, Family::PowerShell, &only_python),
            argv(&["python", r"C:\root\guard.py", "--strict"], Some("python"))
        );
        let launcher_too = |_: &[&str]| true;
        assert_eq!(
            launched(
                r"C:\root\guard.py",
                false,
                Family::PowerShell,
                &launcher_too
            ),
            argv(&["py", "-3", r"C:\root\guard.py", "--strict"], Some("py"))
        );
    }

    #[test]
    fn with_no_windows_python_the_line_names_python_and_needs_it() {
        assert_eq!(
            launched(r"C:\root\guard.py", false, Family::PowerShell, NO_PYTHON),
            argv(&["python", r"C:\root\guard.py", "--strict"], Some("python"))
        );
    }

    #[test]
    fn the_execute_bit_is_never_consulted_on_windows() {
        let asked = std::cell::Cell::new(false);
        let python = |_: &[&str]| {
            asked.set(true);
            true
        };
        let launch = launched(r"C:\root\guard.py", true, Family::PowerShell, &python);
        assert!(asked.get());
        assert!(matches!(launch, Launch::Argv { ref argv, .. } if argv[0] == "py"));
    }

    #[test]
    fn powershell_scripts_run_in_the_shell_each_platform_fixes() {
        assert_eq!(
            launched(r"C:\root\guard.ps1", false, Family::PowerShell, NO_PYTHON),
            argv(
                &[
                    "powershell.exe",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    r"C:\root\guard.ps1",
                    "--strict"
                ],
                None
            )
        );
        assert_eq!(
            launched("/root/guard.ps1", false, Family::Posix, NO_PYTHON),
            argv(
                &["pwsh", "-NoProfile", "-File", "/root/guard.ps1", "--strict"],
                Some("pwsh")
            )
        );
    }

    #[test]
    fn javascript_needs_node_on_both_platforms() {
        for (path, family) in [
            ("/root/guard.mjs", Family::Posix),
            (r"C:\root\guard.js", Family::PowerShell),
        ] {
            assert!(matches!(
                launched(path, false, family, NO_PYTHON),
                Launch::Argv { needs: Some(ref needs), .. } if needs == "node"
            ));
        }
    }

    #[test]
    fn a_shell_script_runs_in_sh_on_posix_and_nowhere_on_windows() {
        assert_eq!(
            launched("/root/guard.sh", false, Family::Posix, NO_PYTHON),
            argv(&["sh", "/root/guard.sh", "--strict"], None)
        );
        assert!(matches!(
            launched(r"C:\root\guard.sh", false, Family::PowerShell, NO_PYTHON),
            Launch::Unplaceable { .. }
        ));
    }

    #[test]
    fn a_script_nothing_can_place_is_reported() {
        assert!(matches!(
            launched("/root/guard", false, Family::Posix, NO_PYTHON),
            Launch::Unplaceable { .. }
        ));
        assert!(matches!(
            launched(r"C:\root\guard.rb", false, Family::PowerShell, NO_PYTHON),
            Launch::Unplaceable { .. }
        ));
    }

    #[test]
    fn the_authors_interpreter_replaces_the_table() {
        let path = PathBuf::from("/root/guard.ts");
        let interpreter = vec!["deno".to_owned(), "run".to_owned()];
        assert_eq!(
            launch(
                &Script {
                    path: &path,
                    executable: false,
                    interpreter: Some(&interpreter),
                    args: &[],
                },
                Family::Posix,
                NO_PYTHON,
            ),
            argv(&["deno", "run", "/root/guard.ts"], Some("deno"))
        );
    }
}
