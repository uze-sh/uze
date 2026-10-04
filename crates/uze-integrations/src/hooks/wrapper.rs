//! The generated hook wrapper: each harness's dialect, the template that
//! compiles it for this platform's shell, materializing it and taking it
//! back.

use super::*;

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
}

/// Where the harness's payload keeps what the hook context is made of, as
/// `jq` path filters (`.a.b[0] // .c // empty`), which every template reads.
#[derive(Clone, Copy)]
pub(crate) struct PayloadPaths {
    /// The native tool name.
    pub(crate) tool: &'static str,
    /// The tool input object.
    pub(crate) input: &'static str,
    /// The workspace directory.
    pub(crate) cwd: &'static str,
}

/// What the wrapper writes on stdout to deny, and when nothing is denied, in
/// one template's language.
#[derive(Clone, Copy)]
pub(crate) struct Decisions {
    pub(crate) deny: &'static str,
    pub(crate) allow: &'static str,
}

/// The `case` arm list translating this harness's native tool names into
/// `HOOK_TOOL` and the matched alias's portable field variables, generated
/// from the one vocabulary the matchers are generated from.
pub(super) fn wrapper_alias_table(target: HookTarget) -> String {
    let mut arms = String::new();
    for (native, binding) in vocabulary(target).native_names() {
        let mut assignments = format!("HOOK_TOOL={};", binding.alias);
        for (portable, native_field) in binding.fields {
            let variable = uze_core::hook::hook_field_variable(portable);
            assignments.push_str(&format!(
                " {variable}=$(printf '%s' \"$HOOK_INPUT\" | \"$JQ\" -r '.{native_field} // empty');"
            ));
        }
        arms.push_str(&format!("    {native}) {assignments} ;;\n"));
    }
    arms
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
/// platform runs is [`HostWrapper`].
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
// not run is still generated and tested where the suite runs.
#[cfg_attr(not(unix), allow(dead_code))]
mod posix;
#[cfg_attr(not(windows), allow(dead_code))]
mod powershell;
#[cfg_attr(not(unix), allow(unused_imports))]
pub(crate) use posix::PosixWrapper;
#[cfg_attr(not(windows), allow(unused_imports))]
pub(crate) use powershell::PowerShellWrapper;

/// The wrapper template this platform's harnesses run.
#[cfg(unix)]
pub(crate) type HostWrapper = PosixWrapper;
#[cfg(windows)]
pub(crate) type HostWrapper = PowerShellWrapper;

/// The wrapper this platform's harness runs (see [`WrapperTemplate`]).
pub(crate) fn wrapper_source(target: HookTarget) -> Option<String> {
    HostWrapper::source(target)
}

pub(super) const WRAPPER_HEADER: &str = HostWrapper::HEADER;
pub(crate) const WRAPPER_RELATIVE_PATH: &str = HostWrapper::RELATIVE_PATH;

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
    // the harness. The executable bit is not part of that content, and
    // `write_atomic` publishes under the umask before the chmod lands — a
    // crash in between leaves the right bytes with the wrong mode, which
    // only a second chmod repairs.
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
    uze_core::persistence::write_atomic(path, source.as_bytes())?;
    make_executable(path)
}

/// Whether the wrapper on disk can be run at all. A harness that cannot
/// execute it reports exit 126, which a `deny` group turns into a
/// permanent block — so this is drift, not a cosmetic difference. On a
/// platform without Unix modes there is no bit to lose.
pub(super) fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

pub(super) fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|source| {
            UzeError::Write {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Removes a shared wrapper once no hook entry of this integration is left
/// to run it. A wrapper another group's entry still points at is kept: it
/// is one file serving every package.
///
/// "Left" is read from the harness's own config files, not from the receipt
/// ledger. The prune runs inside a detach, and the lifecycle only rewrites
/// the ledger once every detach of the removal has returned — so the
/// receipt being detached, and during `uze remove` each of its siblings, is
/// still listed there while its entry is already gone from the config.
pub(super) fn prune_shared_wrapper(uze_home: &UzeHome, integration_id: &str, target: HookTarget) {
    // A ledger that cannot be read has not said the wrapper is unused; it
    // has said nothing. Deleting on that answer is a destructive mutation
    // authorized by an unreadable ledger, which is exactly what receipts
    // exist to refuse.
    let still_used = match uze_core::state::receipts(uze_home, None) {
        Ok(ledger) => ledger.iter().any(|receipt| {
            receipt.integration == integration_id && entry_is_attached(receipt, target)
        }),
        Err(_) => true,
    };
    if still_used {
        return;
    }
    let path = target.wrapper_path(uze_home);
    let _ = fs::remove_file(&path);
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
}

/// Whether this receipt's hook entry is still in the harness's config file.
/// The wrapper is deliberately not part of the question: it is what the
/// prune is deciding about, so inspecting it would answer "nothing is
/// attached" for every entry the moment it went missing.
///
/// The target decides the file's shape — Antigravity keys its entries by
/// name, the other command-hook harnesses by event — and every receipt
/// records its event either way, so the shape cannot be read off the
/// receipt.
///
/// The question is "does anything still run this wrapper", not "is this
/// entry exactly as UZE wrote it": only an entry that is *absent* has
/// stopped running it. An unreadable config, drift, a hand-edited entry —
/// each of those still fires the wrapper, and an event-array config reports
/// an edited entry as absent, so the wrapper's own path in the file is the
/// last word.
pub(super) fn entry_is_attached(
    receipt: &uze_core::integration::AttachmentReceipt,
    target: HookTarget,
) -> bool {
    let Some(entry) = HookEntry::recorded(&receipt.artifact) else {
        return false;
    };
    if target.entry_state(&entry).state != AttachmentState::Missing {
        return true;
    }
    fs::read_to_string(entry.config_file)
        .is_ok_and(|config| config.contains(&entry.wrapper.display().to_string()))
}

/// The native command an entry runs: the wrapper, the package root, the
/// group's event and effect, then the author's handlers — each as
/// `<seconds>:<command>`, with its declared deadline and `${PLUGIN_ROOT}`
/// already resolved. Everything harness-specific is decided here, at
/// generation time, so the native entry reads as what will run.
pub(crate) fn wrapper_arguments(
    hook: &PortableHook,
    package_root: &Path,
    handlers: &[CommandHook],
) -> Vec<String> {
    let package_root = &crate::shared::package_root::delivered(package_root);
    let mut arguments = vec![
        package_root.display().to_string(),
        hook.event.abi_name().to_owned(),
        hook.effect.abi_name().to_owned(),
    ];
    for handler in handlers {
        arguments.push(format!(
            "{}:{}",
            handler.timeout,
            handler
                .command
                .here()
                .unwrap_or_default()
                .replace("${PLUGIN_ROOT}", &package_root.display().to_string())
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
    package_root: &Path,
) -> HookInvocation {
    let (command, args) = wrapper_words(wrapper, hook, package_root);
    HookInvocation::Exec { command, args }
}

/// The same invocation as one line in this platform's shell, for the
/// harnesses whose hook entry carries a command string rather than a
/// command plus arguments.
pub(crate) fn wrapper_command_line(
    wrapper: &Path,
    hook: &PortableHook,
    package_root: &Path,
) -> String {
    let (program, arguments) = wrapper_words(wrapper, hook, package_root);
    uze_platform::shell::command_line(&program, &arguments)
}

fn wrapper_words(
    wrapper: &Path,
    hook: &PortableHook,
    package_root: &Path,
) -> (String, Vec<String>) {
    let (program, mut arguments) = uze_platform::shell::script(&wrapper.display().to_string());
    arguments.extend(wrapper_arguments(hook, package_root, &hook.handlers));
    (program, arguments)
}

// ============================================================================
// Event-array config merge (Claude settings.json, Codex hooks.json)
// ============================================================================
