//! Whether this machine has what a package needs, and the command that
//! would give it what it lacks. Nothing here installs anything.
//!
//! An executable counts only when it answers. A name on `PATH` is not
//! enough: a clean Windows carries `python.exe` and `python3.exe` App
//! Execution Aliases that open the Store and exit 9009, and macOS's
//! `/usr/bin/python3` without the Command Line Tools asks to install them.
//! Both pass a lookup; neither runs a hook.
//!
//! The check runs in the environment of the command asking, which is not
//! necessarily the environment a harness runs its hooks in (a harness
//! started from a desktop launcher can have a shorter `PATH`). Callers say
//! so rather than claiming more than was checked.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    home::UzeHome,
    requirement::{Requirement, Version},
    subprocess::Captured,
};

/// Long enough for an interpreter's cold start on Windows, short enough
/// that a program waiting on input does not stall a command.
const PROBE_DEADLINE: Duration = Duration::from_secs(5);

/// What the machine has of one requirement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RequirementStatus {
    /// Present, at `version` when its version line could be read.
    Met {
        version: Option<String>,
    },
    TooOld {
        found: String,
        minimum: String,
    },
    Missing,
}

impl RequirementStatus {
    pub fn is_met(&self) -> bool {
        matches!(self, Self::Met { .. })
    }
}

/// What a resolved executable said when asked its version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Answer {
    Answered {
        version: Option<String>,
    },
    /// Started and did not answer as the program: a Store alias, an
    /// install prompt, a hang, or nothing that could be started.
    Stub,
}

/// Reads what `program` at `resolved` said. Pure, so every platform's
/// stubs are tested on every host.
pub fn classify(resolved: &Path, captured: Option<&Captured>) -> Answer {
    let Some(captured) = captured else {
        return Answer::Stub;
    };
    if captured.timed_out {
        return Answer::Stub;
    }
    let said = format!("{}\n{}", captured.stdout, captured.stderr);
    let version = || {
        Version::find(&captured.stdout)
            .or_else(|| Version::find(&captured.stderr))
            .map(|version| version.to_string())
    };
    match captured.code {
        Some(0) => Answer::Answered { version: version() },
        Some(9009) => Answer::Stub,
        Some(_) if is_store_alias(resolved) || names_an_install_prompt(&said) => Answer::Stub,
        Some(_) => Answer::Answered { version: version() },
        None => Answer::Stub,
    }
}

fn is_store_alias(resolved: &Path) -> bool {
    resolved
        .components()
        .any(|component| component.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

fn names_an_install_prompt(said: &str) -> bool {
    let said = said.to_ascii_lowercase();
    [
        "xcode-select",
        "microsoft store",
        "command line developer tools",
    ]
    .iter()
    .any(|prompt| said.contains(prompt))
}

/// The status of `requirement` given what its executable answered.
pub fn status_of(requirement: &Requirement, answer: Option<&Answer>) -> RequirementStatus {
    let Some(Answer::Answered { version }) = answer else {
        return RequirementStatus::Missing;
    };
    if let (Some(minimum), Some(found)) = (
        requirement.minimum(),
        version.as_deref().and_then(Version::find),
    ) && found < minimum
    {
        return RequirementStatus::TooOld {
            found: found.to_string(),
            minimum: minimum.to_string(),
        };
    }
    RequirementStatus::Met {
        version: version.clone(),
    }
}

/// How each executable prints its version, where `--version` is not it.
fn version_arguments(executable: &str) -> &'static [&'static str] {
    match executable {
        "java" => &["-version"],
        "go" => &["version"],
        _ => &["--version"],
    }
}

#[derive(Default, Deserialize, Serialize)]
struct ProbeFile {
    #[serde(default)]
    entries: BTreeMap<String, Probed>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Probed {
    length: u64,
    modified_nanos: u128,
    answer: Answer,
}

/// Checks requirements against this machine, remembering each answer by
/// the file that gave it so a command asking again starts no process.
pub struct RequirementChecker {
    cache_path: PathBuf,
    search: Vec<PathBuf>,
    probes: Mutex<ProbeFile>,
    changed: Mutex<bool>,
}

impl RequirementChecker {
    pub fn new(home: &UzeHome) -> Self {
        Self::searching(
            home.requirement_probe_cache_path(),
            crate::harness_runtime::harness_search_path(),
        )
    }

    /// A checker looking only in `search`, for a caller (and a test) that
    /// decides the `PATH` itself.
    pub fn searching(cache_path: PathBuf, search: Vec<PathBuf>) -> Self {
        let probes = fs::read(&cache_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            cache_path,
            search,
            probes: Mutex::new(probes),
            changed: Mutex::new(false),
        }
    }

    pub fn status(&self, requirement: &Requirement) -> RequirementStatus {
        let answer = self
            .resolve(&requirement.executable)
            .and_then(|resolved| self.answer(&requirement.executable, &resolved));
        status_of(requirement, answer.as_ref())
    }

    fn resolve(&self, executable: &str) -> Option<PathBuf> {
        self.search.iter().find_map(|directory| {
            crate::harness_runtime::executable_candidates(directory, executable)
                .into_iter()
                .find(|candidate| crate::harness_runtime::is_executable_file(candidate))
        })
    }

    fn answer(&self, executable: &str, resolved: &Path) -> Option<Answer> {
        let metadata = fs::metadata(resolved).ok()?;
        let length = metadata.len();
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos());
        let key = resolved.display().to_string();
        if let Some(probed) = self.probes.lock().ok()?.entries.get(&key)
            && probed.length == length
            && probed.modified_nanos == modified_nanos
        {
            return Some(probed.answer.clone());
        }
        let captured = crate::subprocess::run_program_bounded(
            resolved,
            version_arguments(executable),
            PROBE_DEADLINE,
        );
        let answer = classify(resolved, captured.as_ref());
        self.probes.lock().ok()?.entries.insert(
            key,
            Probed {
                length,
                modified_nanos,
                answer: answer.clone(),
            },
        );
        if let Ok(mut changed) = self.changed.lock() {
            *changed = true;
        }
        Some(answer)
    }
}

impl Drop for RequirementChecker {
    /// Best-effort, like every cache: a read-only `UZE_HOME` costs the next
    /// command a probe, never this one its answer.
    fn drop(&mut self) {
        let changed = self
            .changed
            .get_mut()
            .map(|changed| *changed)
            .unwrap_or(false);
        if !changed {
            return;
        }
        if let Ok(probes) = self.probes.get_mut()
            && let Ok(payload) = serde_json::to_vec_pretty(probes)
        {
            let _ = crate::persistence::write_atomic(&self.cache_path, &payload);
        }
    }
}

/// A package manager UZE can suggest a command for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageManager {
    Mise,
    Homebrew,
    Apt,
    Dnf,
    Pacman,
    Apk,
    Winget,
}

impl PackageManager {
    /// In the order they are preferred: a user-level manager the person
    /// already uses, then Homebrew, then the system's own.
    const PREFERENCE: [Self; 7] = [
        Self::Mise,
        Self::Homebrew,
        Self::Apt,
        Self::Dnf,
        Self::Pacman,
        Self::Apk,
        Self::Winget,
    ];

    fn program(self) -> &'static str {
        match self {
            Self::Mise => "mise",
            Self::Homebrew => "brew",
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Pacman => "pacman",
            Self::Apk => "apk",
            Self::Winget => "winget",
        }
    }

    fn serves(self, family: crate::shell::Family) -> bool {
        match family {
            crate::shell::Family::Posix => self != Self::Winget,
            crate::shell::Family::PowerShell => matches!(self, Self::Mise | Self::Winget),
        }
    }

    /// The package that provides `executable` under this manager, when one
    /// is known.
    fn package(self, executable: &str) -> Option<&'static str> {
        use PackageManager::*;
        let row: &[(PackageManager, &str)] = match executable {
            "jq" => &[
                (Mise, "jq"),
                (Homebrew, "jq"),
                (Apt, "jq"),
                (Dnf, "jq"),
                (Pacman, "jq"),
                (Apk, "jq"),
                (Winget, "jqlang.jq"),
            ],
            "python3" => &[
                (Mise, "python"),
                (Homebrew, "python"),
                (Apt, "python3"),
                (Dnf, "python3"),
                (Pacman, "python"),
                (Apk, "python3"),
                (Winget, "Python.Python.3.13"),
            ],
            "python" | "py" => &[
                (Mise, "python"),
                (Homebrew, "python"),
                (Apt, "python-is-python3"),
                (Pacman, "python"),
                (Winget, "Python.Python.3.13"),
            ],
            "node" => &[
                (Mise, "node"),
                (Homebrew, "node"),
                (Apt, "nodejs"),
                (Dnf, "nodejs"),
                (Pacman, "nodejs"),
                (Apk, "nodejs"),
                (Winget, "OpenJS.NodeJS.LTS"),
            ],
            "pwsh" => &[
                (Homebrew, "--cask powershell"),
                (Winget, "Microsoft.PowerShell"),
            ],
            "git" => &[
                (Homebrew, "git"),
                (Apt, "git"),
                (Dnf, "git"),
                (Pacman, "git"),
                (Apk, "git"),
                (Winget, "Git.Git"),
            ],
            "uv" => &[
                (Mise, "uv"),
                (Homebrew, "uv"),
                (Pacman, "uv"),
                (Winget, "astral-sh.uv"),
            ],
            "bun" => &[
                (Mise, "bun"),
                (Homebrew, "oven-sh/bun/bun"),
                (Winget, "Oven-sh.Bun"),
            ],
            _ => &[],
        };
        row.iter()
            .find(|(manager, _)| *manager == self)
            .map(|(_, package)| *package)
    }

    fn command(self, package: &str) -> String {
        match self {
            Self::Mise => format!("mise use -g {package}@latest"),
            Self::Homebrew => format!("brew install {package}"),
            Self::Apt => format!("sudo apt-get install -y {package}"),
            Self::Dnf => format!("sudo dnf install -y {package}"),
            Self::Pacman => format!("sudo pacman -S --needed {package}"),
            Self::Apk => format!("sudo apk add {package}"),
            Self::Winget => format!("winget install --id {package} -e"),
        }
    }
}

/// The package managers this machine offers, most preferred first.
pub fn available_managers() -> Vec<PackageManager> {
    PackageManager::PREFERENCE
        .into_iter()
        .filter(|manager| manager.serves(uze_platform::shell::FAMILY))
        .filter(|manager| crate::subprocess::program_on_path(manager.program()))
        .collect()
}

/// The command that installs `executable` with the first of `managers`
/// that knows a package for it. Text for the person to run; UZE never runs
/// it.
pub fn install_command(executable: &str, managers: &[PackageManager]) -> Option<String> {
    managers.iter().find_map(|manager| {
        manager
            .package(executable)
            .map(|package| manager.command(package))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(code: Option<i32>, stdout: &str, stderr: &str) -> Captured {
        Captured {
            code,
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
            timed_out: false,
        }
    }

    #[test]
    fn an_executable_that_answers_is_present_with_its_version() {
        assert_eq!(
            classify(
                Path::new("/usr/bin/jq"),
                Some(&said(Some(0), "jq-1.7.1\n", ""))
            ),
            Answer::Answered {
                version: Some("1.7.1".to_owned())
            }
        );
        assert_eq!(
            classify(
                Path::new("/usr/bin/java"),
                Some(&said(Some(0), "", "openjdk version \"21.0.2\""))
            ),
            Answer::Answered {
                version: Some("21.0.2".to_owned())
            }
        );
    }

    #[test]
    fn a_windows_store_alias_is_not_python() {
        let alias = Path::new(r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\python3.exe");
        assert_eq!(
            classify(alias, Some(&said(Some(9009), "", ""))),
            Answer::Stub
        );
        assert_eq!(
            classify(
                alias,
                Some(&said(
                    Some(1),
                    "",
                    "Python was not found; run without arguments to install from the Microsoft Store"
                ))
            ),
            Answer::Stub
        );
    }

    #[test]
    fn the_macos_python_stub_is_not_python() {
        assert_eq!(
            classify(
                Path::new("/usr/bin/python3"),
                Some(&said(
                    Some(1),
                    "",
                    "xcode-select: note: No developer tools were found, requesting install."
                ))
            ),
            Answer::Stub
        );
    }

    #[test]
    fn a_program_that_hangs_or_cannot_start_is_absent_and_one_without_a_version_flag_is_not() {
        let mut hung = said(None, "", "");
        hung.timed_out = true;
        assert_eq!(classify(Path::new("/bin/x"), Some(&hung)), Answer::Stub);
        assert_eq!(classify(Path::new("/bin/x"), None), Answer::Stub);
        assert_eq!(
            classify(
                Path::new("/bin/x"),
                Some(&said(Some(2), "", "unknown option"))
            ),
            Answer::Answered { version: None }
        );
    }

    #[test]
    fn a_version_below_the_minimum_is_too_old_and_an_unknown_one_is_met() {
        let jq = Requirement {
            executable: "jq".to_owned(),
            version: Some(">=1.7".to_owned()),
            purpose: None,
        };
        let answered = |version: Option<&str>| Answer::Answered {
            version: version.map(str::to_owned),
        };
        assert_eq!(
            status_of(&jq, Some(&answered(Some("1.6")))),
            RequirementStatus::TooOld {
                found: "1.6".to_owned(),
                minimum: "1.7".to_owned()
            }
        );
        assert!(status_of(&jq, Some(&answered(Some("1.7.1")))).is_met());
        assert!(status_of(&jq, Some(&answered(None))).is_met());
        assert_eq!(
            status_of(&jq, Some(&Answer::Stub)),
            RequirementStatus::Missing
        );
        assert_eq!(status_of(&jq, None), RequirementStatus::Missing);
    }

    #[test]
    fn the_suggestion_follows_preference_and_falls_back_to_nothing() {
        use PackageManager::*;
        assert_eq!(
            install_command("jq", &[Apt]).as_deref(),
            Some("sudo apt-get install -y jq")
        );
        assert_eq!(
            install_command("node", &[Mise, Apt]).as_deref(),
            Some("mise use -g node@latest")
        );
        assert_eq!(
            install_command("pwsh", &[Mise, Apt, Homebrew]).as_deref(),
            Some("brew install --cask powershell"),
            "a manager with no package for it is skipped"
        );
        assert_eq!(
            install_command("python", &[Winget]).as_deref(),
            Some("winget install --id Python.Python.3.13 -e")
        );
        assert_eq!(install_command("frobnicate", &[Apt, Homebrew]), None);
        assert_eq!(install_command("jq", &[]), None);
    }

    #[test]
    fn winget_is_never_offered_outside_windows_and_apt_never_on_it() {
        use crate::shell::Family;
        assert!(!PackageManager::Winget.serves(Family::Posix));
        assert!(PackageManager::Winget.serves(Family::PowerShell));
        assert!(!PackageManager::Apt.serves(Family::PowerShell));
        assert!(PackageManager::Mise.serves(Family::PowerShell));
    }

    // The probed program is a `#!/bin/sh` script made executable by its
    // mode bits, which only Unix runs as a program.
    #[cfg(unix)]
    #[test]
    fn a_checked_executable_is_asked_once_and_again_when_it_changes() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("uze-requirement-check-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let tool = bin.join("probe-tool");
        let counter = root.join("asked");
        let write_tool = |version: &str| {
            fs::write(
                &tool,
                format!(
                    "#!/bin/sh\necho x >> '{}'\necho 'probe-tool {version}'\n",
                    counter.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        };
        write_tool("1.0.0");
        let cache = root.join("requirements.json");
        let requirement = Requirement {
            executable: "probe-tool".to_owned(),
            version: Some(">=2".to_owned()),
            purpose: None,
        };
        let asked = || {
            fs::read_to_string(&counter)
                .map(|text| text.lines().count())
                .unwrap_or(0)
        };

        let first = RequirementChecker::searching(cache.clone(), vec![bin.clone()]);
        assert_eq!(
            first.status(&requirement),
            RequirementStatus::TooOld {
                found: "1.0.0".to_owned(),
                minimum: "2".to_owned()
            }
        );
        drop(first);
        let again = RequirementChecker::searching(cache.clone(), vec![bin.clone()]);
        assert!(!again.status(&requirement).is_met());
        drop(again);
        assert_eq!(
            asked(),
            1,
            "an unchanged executable is answered from the cache"
        );

        write_tool("2.10.0-longer");
        let after = RequirementChecker::searching(cache.clone(), vec![bin.clone()]);
        assert!(after.status(&requirement).is_met());
        assert_eq!(asked(), 2, "a changed executable is asked again");

        let elsewhere = RequirementChecker::searching(cache, vec![root.join("empty")]);
        assert_eq!(elsewhere.status(&requirement), RequirementStatus::Missing);
        let _ = fs::remove_dir_all(&root);
    }
}
