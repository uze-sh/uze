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

/// The binary's entries.
pub fn earlier() -> Vec<Earlier> {
    vec![Earlier {
        name: "the retired --instructions flag of plugin scaffolding",
        // Refused whenever it is passed; nothing on a machine holds it.
        lasts: Lasts::Unbounded,
        spelled: &[],
        site: file!(),
    }]
}
