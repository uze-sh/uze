//! What the binary does only because an earlier release spelled something
//! differently (the `earlier-releases` rule).
//!
//! `uze workspace --resumed` and the hidden `uze terminal` are not declared
//! here: an older client that self-updated still speaks them to the binary
//! now on disk, so every later release keeps reading them.

use uze_application::earlier::{Earlier, Lasts};

/// What `uze marketplace plugin new --instructions` answers: the flag is
/// gone, and a script still passing it is told what replaced it.
pub const INSTRUCTIONS_REFUSAL: &str = "`--instructions` is gone: a plugin's AGENTS.md is no \
     longer delivered. A plugin speaks to every agent of a project that declares it through a \
     rule with `trigger: always_on`; scaffold one with `--rules`";

/// Removes the gate and setup logs earlier releases kept loose in `logs`
/// (`gate-<agent>.log`, `setup-<harness>.log`), now one directory each.
pub fn remove_loose_logs(logs: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(logs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        (name.starts_with("gate-") || name.starts_with("setup-"))
                            && name.ends_with(".log")
                    })
        })
        .filter(|path| std::fs::remove_file(path).is_ok())
        .collect()
}

/// The binary's entries.
pub fn earlier() -> Vec<Earlier> {
    vec![
        Earlier {
            name: "gate and setup logs earlier releases kept loose in cache/logs",
            // Removed when the logs' collector next runs: unbounded.
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
        Earlier {
            name: "the retired --instructions flag of plugin scaffolding",
            // Refused whenever it is passed; nothing on a machine holds it.
            lasts: Lasts::Unbounded,
            spelled: &[],
            site: file!(),
        },
    ]
}
