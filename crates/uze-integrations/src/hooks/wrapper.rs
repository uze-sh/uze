//! The generated hook wrapper: each harness's dialect, the template that
//! compiles it for this platform's shell, materializing it and taking it
//! back.

use super::*;
use uze_platform::shell::{self, Family};

/// How one harness's payload is read and how its decision is written — the
/// only slots that differ between the generated wrappers.
#[derive(Clone, Copy)]
pub(crate) struct WrapperDialect {
    pub(crate) payload: PayloadPaths,
    /// The decisions in POSIX `sh`, with `$reason_json` holding the reason
    /// as a JSON string literal and `$HOOK_EVENT` the ABI event name.
    pub(crate) posix: Decisions,
    /// The decisions in Windows PowerShell, with `$reasonJson` and
    /// `$hookEvent` holding the same. `None` where no measured Windows entry
    /// form reaches this harness yet: its hooks are not delivered there,
    /// and the report says so.
    pub(crate) powershell: Option<Decisions>,
    /// The status the wrapper exits with after writing a denial. Claude and
    /// Codex document exit 2 as the block signal and read the decision only
    /// alongside it; Antigravity reads the decision from stdout and treats
    /// *any* non-zero exit as a failed hook — "pre-tool hook failed", the
    /// permission prompt, and the command runs anyway (measured on 1.1.24,
    /// `command_hook_executor.go`). So the code is a per-harness fact.
    pub(crate) deny_exit: &'static str,
    /// The status the wrapper exits with when the decision document itself
    /// could not be written. Where [`Self::deny_exit`] is 0 the document is
    /// the whole denial, so failing to write it must not read as an
    /// allowance wherever the harness has a status that blocks.
    pub(crate) unwritten_exit: &'static str,
    /// The key and value the harness's own entry takes to block a call
    /// whose hook cannot start or outlives its timeout, written on every
    /// fail-closed group. Without it the wrapper is the only thing failing
    /// closed, and a wrapper that never runs fails open. `None` where the
    /// harness offers no such control.
    pub(crate) failure_marker: Option<(&'static str, &'static str)>,
}

/// Where the harness's payload keeps what the hook context is made of, as
/// `jq` path filters (`.a.b[0] // .c // empty`), which every template reads.
#[derive(Clone, Copy)]
pub(crate) struct PayloadPaths {
    /// The native tool name.
    pub(crate) tool: &'static str,
    /// The tool input object.
    pub(crate) input: &'static str,
    /// Where the workspace (`HOOK_CWD`) is read from.
    pub(crate) workspace: Workspace,
    /// The directory a shell command runs in, where the harness reports
    /// one; `None` where its command tool runs every command in the
    /// workspace. A relative answer is resolved against the workspace.
    pub(crate) workdir: Option<&'static str>,
    /// How a session began when the harness's session-start payload does
    /// not say: a harness that announces only a new session implies it.
    pub(crate) implied_source: Option<&'static str>,
}

/// Where a harness says which workspace a hook belongs to: a source the
/// call itself does not choose. Where a command runs, which the call does
/// choose, is [`PayloadPaths::workdir`].
#[derive(Clone, Copy)]
pub(crate) enum Workspace {
    /// A `jq` path filter into the payload.
    Payload(&'static str),
    /// An environment variable the harness sets for every hook it starts.
    Environment(&'static str),
    /// The directory the harness starts the hook in, read before the
    /// wrapper moves.
    LaunchDirectory,
}

/// The portable alias whose calls carry `HOOK_WORKDIR`.
pub(crate) const WORKDIR_ALIAS: &str = "shell";

/// The most one `HOOK_*` value may carry into a handler's environment, in
/// UTF-8 bytes, on every platform alike. Windows holds an environment
/// variable to 32,767 UTF-16 units, and a string never has more UTF-16
/// units than UTF-8 bytes, so a value inside this bound fits there; Linux's
/// `MAX_ARG_STRLEN` (128 KiB), which no program starts past, the wrapper's
/// own `jq` and `sh` included, is further still. A value past it is never
/// exported: the group's effect decides, as for any context that cannot be
/// built. One bound for every platform, so a call a guard sees on one is
/// the call it sees on all.
pub(crate) const HOOK_VALUE_LIMIT: usize = 32_000;

/// The most a `transform` handler may write as the rewritten input, in
/// bytes; past it the handler has failed, and the group closes. The rewrite
/// becomes the next handler's `HOOK_INPUT`, so it is that bound.
pub(crate) const TRANSFORM_OUTPUT_LIMIT: usize = HOOK_VALUE_LIMIT;

/// What the wrapper writes on stdout to deny, and when nothing is denied, in
/// one template's language.
#[derive(Clone, Copy)]
pub(crate) struct Decisions {
    pub(crate) deny: &'static str,
    pub(crate) allow: &'static str,
    /// The harness's document for a call a `transform` group rewrote, read
    /// from the rewritten input as JSON (`$updated_json` in `sh`,
    /// `$updatedJson` in PowerShell); `None` where the harness takes no
    /// rewritten input, which keeps `transform` from being claimed there.
    pub(crate) transform: Option<&'static str>,
}

/// Every portable field variable any alias of this harness can set. They are
/// declared empty up front so an unmatched tool leaves a defined (and empty)
/// variable rather than tripping `set -u` in the handler.
pub(super) fn wrapper_field_variables(target: HookTarget) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for binding in vocabulary(target).bindings {
        for (portable, _) in binding.fields {
            let variable = uze_core::hook::hook_field_variable(portable);
            if !names.contains(&variable) {
                names.push(variable);
            }
        }
    }
    names
}

/// A generated wrapper's template: the one contract (ADR-040) compiled for
/// one shell. Each is a pure function of the harness, and the one this
/// platform runs is the one its shell family names ([`wrapper_source`]).
pub(crate) trait WrapperTemplate {
    /// Where the wrapper sits inside its delivered artifact: one path an
    /// author or reviewer can look for on every harness.
    const RELATIVE_PATH: &'static str;
    /// How every wrapper this template writes opens, whichever build wrote
    /// it: what tells a wrapper an earlier template produced from one
    /// somebody else wrote.
    const HEADER: &'static str;
    /// The wrapper for `target`, or `None` where this template has no
    /// dialect for it: then its hooks are not delivered, and say so.
    fn source(target: HookTarget) -> Option<String>;
}

// Every template is compiled on every platform, so the one a platform does
// not run is still generated and tested where the suite runs; which one a
// harness here gets is the shell family the platform names.
mod posix;
mod powershell;
pub(crate) use posix::PosixWrapper;
pub(crate) use powershell::PowerShellWrapper;

/// The wrapper this platform's harness runs (see [`WrapperTemplate`]).
pub(crate) fn wrapper_source(target: HookTarget) -> Option<String> {
    match shell::FAMILY {
        Family::Posix => PosixWrapper::source(target),
        Family::PowerShell => PowerShellWrapper::source(target),
    }
}

/// Whether this platform's wrapper can hand the harness a rewritten input.
pub(crate) fn transforms_here(target: HookTarget) -> bool {
    target.dialect().is_some_and(|dialect| match shell::FAMILY {
        Family::Posix => dialect.posix.transform.is_some(),
        Family::PowerShell => dialect
            .powershell
            .is_some_and(|decisions| decisions.transform.is_some()),
    })
}

/// The system programs this platform's wrapper runs beside the shell: the
/// `sh` wrapper reads the harness's payload with `jq`; the PowerShell one
/// uses the .NET reader every Windows carries.
pub(crate) fn dependencies_here() -> &'static [&'static str] {
    match shell::FAMILY {
        Family::Posix => &[uze_core::hook::WRAPPER_DEPENDENCY],
        Family::PowerShell => &[],
    }
}

pub(super) const WRAPPER_HEADER: &str = match shell::FAMILY {
    Family::Posix => PosixWrapper::HEADER,
    Family::PowerShell => PowerShellWrapper::HEADER,
};
pub(crate) const WRAPPER_RELATIVE_PATH: &str = match shell::FAMILY {
    Family::Posix => PosixWrapper::RELATIVE_PATH,
    Family::PowerShell => PowerShellWrapper::RELATIVE_PATH,
};

/// How much of a handler's stderr becomes the reason a harness is handed.
/// "Bounded output" is part of the hook ABI (ADR-033), and the generated
/// wrapper and the OpenCode bridge are the two places that can still hold
/// it: without a bound a handler writing megabytes turns into a decision
/// document that big, which the harness then has to parse.
pub(crate) const HANDLER_REASON_LIMIT: usize = 4096;

/// Writes (or refreshes) a generated wrapper, executable. Idempotent: the
/// content is a pure function of the harness.
pub(crate) fn materialize_wrapper(path: &Path, source: &str) -> Result<()> {
    // Rewriting an identical wrapper would replace a file a harness may be
    // executing right now, for no gain: the content is a pure function of
    // the harness. The executable bit is not part of that content: a fresh
    // file is published under the umask before the chmod lands, and a crash
    // in between leaves the right bytes with the wrong mode, which only a
    // second chmod repairs.
    if fs::read_to_string(path).is_ok_and(|current| current == source) {
        return if is_executable(path) {
            Ok(())
        } else {
            make_executable(path)
        };
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(UzeError::write(parent))?;
    }
    // A wrapper being replaced is one another package's entry may fire at
    // any moment, so the new file is published already carrying the old
    // one's executable mode; only a first write, which no entry names yet,
    // waits on the chmod.
    uze_core::persistence::write_atomic_preserving(path, source.as_bytes())?;
    make_executable(path)
}

/// Writes `target`'s wrapper into a package's build being rendered at
/// `dir`, at the reserved path, when the package has hooks and a wrapper
/// can run here. Each build carries its own, so an entry names nothing
/// outside its package's build, and the wrapper leaves with it.
pub(crate) fn render_wrapper(
    target: HookTarget,
    package: &uze_core::store::StoredPackage,
    dir: &Path,
) -> Result<()> {
    if !matches!(target.runner, HookRunner::Wrapper { .. }) {
        return Ok(());
    }
    let has_hooks =
        super::package_hook_groups(&package.root).is_ok_and(|groups| !groups.is_empty());
    match wrapper_source(target) {
        Some(source) if has_hooks => materialize_wrapper(&dir.join(WRAPPER_RELATIVE_PATH), &source),
        _ => Ok(()),
    }
}

/// Whether the wrapper on disk can be run at all. A harness that cannot
/// execute it reports exit 126, which a `deny` group turns into a
/// permanent block — so this is drift, not a cosmetic difference. On a
/// platform without Unix modes there is no bit to lose.
pub(super) fn is_executable(path: &Path) -> bool {
    uze_platform::executable::is_marked_runnable(path)
}

pub(super) fn make_executable(path: &Path) -> Result<()> {
    uze_platform::executable::make_runnable(path).map_err(|source| UzeError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// The native command an entry runs: the wrapper, the package root, the
/// group's event and effect, then the author's handlers — each as
/// `<seconds>:<command>`, with its declared deadline and `${PLUGIN_ROOT}`
/// already resolved. Everything harness-specific is decided here, at
/// generation time, so the native entry reads as what will run.
///
/// `roots` are where the package's files are read from and the root the
/// harness is handed (its build).
pub(crate) fn wrapper_arguments(
    hook: &PortableHook,
    roots: Roots<'_>,
    handlers: &[CommandHook],
) -> Vec<String> {
    let (store_root, package_root) = (roots.sources, roots.plugin_root());
    let mut arguments = vec![
        package_root.path().display().to_string(),
        hook.event.abi_name().to_owned(),
        hook.effect.abi_name().to_owned(),
    ];
    for handler in handlers {
        arguments.push(format!(
            "{}:{}",
            handler.timeout,
            super::handler_line(handler, store_root, &package_root)
        ));
    }
    arguments
}

/// The wrapper's invocation as a program and its arguments, for the
/// harnesses whose entry carries both: the script itself, or the shell told
/// to run it, as this platform runs a script.
pub(crate) fn wrapper_exec(
    wrapper: &Path,
    hook: &PortableHook,
    roots: Roots<'_>,
) -> HookInvocation {
    let (command, args) = wrapper_words(wrapper, hook, roots);
    HookInvocation::Exec { command, args }
}

/// The same invocation as one line in this platform's shell, for the
/// harnesses whose hook entry carries a command string rather than a
/// command plus arguments.
pub(crate) fn wrapper_command_line(
    wrapper: &Path,
    hook: &PortableHook,
    roots: Roots<'_>,
) -> String {
    let (program, arguments) = wrapper_words(wrapper, hook, roots);
    uze_platform::shell::command_line(&program, &arguments)
}

/// [`wrapper_command_line`] for a harness that hands the line to a shell
/// of its own choosing, re-quoting it on the way: Antigravity runs a hook as
/// `cmd /c "<line>"` on Windows, escaping its quotes as `cmd` does not read,
/// so a path with a space or a handler with a quote broke the line there
/// (measured on 1.2.16). The words reach the wrapper intact (see
/// [`uze_platform::shell::sealed_script_line`]).
pub(crate) fn sealed_wrapper_command_line(
    wrapper: &Path,
    hook: &PortableHook,
    roots: Roots<'_>,
) -> String {
    uze_platform::shell::sealed_script_line(
        &wrapper.display().to_string(),
        &wrapper_arguments(hook, roots, &hook.handlers),
    )
}

/// Where a package's files are read from, and the root its harness is
/// handed: its sources and its build, or one directory for both where the
/// package is read in place.
#[derive(Clone, Copy)]
pub(crate) struct Roots<'a> {
    pub sources: &'a Path,
    pub delivered: &'a Path,
    /// What the build keeps under its carried directory
    /// ([`crate::shared::package_root::PluginRoot`]).
    pub carried: &'static [&'static str],
}

impl<'a> Roots<'a> {
    /// One directory that is both.
    #[cfg(test)]
    pub(crate) fn same(root: &'a Path) -> Self {
        Self {
            sources: root,
            delivered: root,
            carried: &[],
        }
    }

    /// The root `${PLUGIN_ROOT}` resolves to in the harness.
    pub(crate) fn plugin_root(&self) -> crate::shared::package_root::PluginRoot {
        crate::shared::package_root::PluginRoot::carrying(self.delivered, self.carried)
    }
}

fn wrapper_words(wrapper: &Path, hook: &PortableHook, roots: Roots<'_>) -> (String, Vec<String>) {
    let (program, mut arguments) = uze_platform::shell::script(&wrapper.display().to_string());
    arguments.extend(wrapper_arguments(hook, roots, &hook.handlers));
    (program, arguments)
}

// ============================================================================
// Event-array config merge (Claude settings.json, Codex hooks.json)
// ============================================================================
