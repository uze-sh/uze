//! A hook group as a harness's own config entry: the handler line, the event name, and the named entries merged, inspected and removed by name.

use super::*;

/// POSIX single-quote quoting for a fragment embedded in a command line the
/// harness will run through its own shell. Spaces, quotes, and `$` all stay
/// literal inside single quotes; a single quote becomes the canonical
/// `'\''` splice.
pub(crate) fn shell_quote(fragment: &str) -> String {
    format!("'{}'", fragment.replace('\'', "'\\''"))
}

/// How a delivered hook is invoked by the harness: the generated wrapper,
/// in the form that harness's own entry takes (see [`hook_delivery`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HookInvocation {
    /// `command` plus `args`: the harness starts the wrapper directly, with
    /// no shell to quote for.
    Exec { command: String, args: Vec<String> },
    /// One shell line, for a harness whose hook entry carries only a
    /// command string.
    Line(String),
}

/// The native group entry for an event-array target (Claude settings.json
/// hooks, Codex hooks.json): `{ "matcher": ..., "hooks": [...] }` carrying
/// one invocation. The matcher key is omitted entirely for an unmatch-all
/// group.
pub(crate) fn group_entry(
    target: HookTarget,
    hook: &PortableHook,
    invocation: &HookInvocation,
) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    if let Some(matcher) = matcher(target, hook) {
        entry.insert("matcher".to_owned(), serde_json::Value::String(matcher));
    }
    entry.insert(
        "hooks".to_owned(),
        serde_json::Value::Array(vec![handler_entry(hook, invocation)]),
    );
    serde_json::Value::Object(entry)
}

/// One handler object — `{type, command[, args], timeout}` — the shape both
/// the grouped and the flat event forms are built from.
///
/// The native timeout is a backstop for the whole group, and it must never
/// be the bound that fires first: `uze_core::hook::group_timeout_bound` is
/// what the wrapper can spend, and `parse_manifest` refuses a group whose
/// bound exceeds the canonical 300s maximum — so the clamp below is the
/// cast's guard, never a truncation of a manifest UZE accepted.
pub(super) fn handler_entry(hook: &PortableHook, invocation: &HookInvocation) -> serde_json::Value {
    let timeout: u16 = uze_core::hook::group_timeout_bound(&hook.handlers)
        .min(u32::from(uze_core::hook::MAX_TIMEOUT_SECONDS)) as u16;
    let mut invoked = serde_json::Map::new();
    invoked.insert("type".to_owned(), serde_json::json!("command"));
    match invocation {
        HookInvocation::Exec { command, args } => {
            invoked.insert("command".to_owned(), serde_json::json!(command));
            invoked.insert("args".to_owned(), serde_json::json!(args));
        }
        HookInvocation::Line(line) => {
            invoked.insert("command".to_owned(), serde_json::json!(line));
        }
    }
    invoked.insert("timeout".to_owned(), serde_json::json!(timeout));
    serde_json::Value::Object(invoked)
}

/// Why a platform the `sh` template does not cover receives no hook at all.
/// There is one implementation of the contract and it is the wrapper: a
/// delivery that cannot write one has nothing honest to attach, so the hook
/// is reported Unsupported rather than carried by something else.
pub(crate) const NO_WRAPPER_TEMPLATE: &str =
    "no wrapper template for this platform, so the hook is not delivered";

pub(super) const fn hook_event_name(event: HookEvent) -> &'static str {
    match event {
        HookEvent::PreToolUse => "PreToolUse",
        HookEvent::PostToolUse => "PostToolUse",
        HookEvent::Stop => "Stop",
        HookEvent::SessionStart => "SessionStart",
    }
}

/// Whether this harness expects an event's entry in the grouped form
/// (`{matcher, hooks: [...]}`) or as a flat list of handler objects.
///
/// The vendor's own customization docs (`agy-customizations/docs/hooks.md`)
/// split them: the tool events carry a matcher and are grouped, while
/// `PreInvocation`/`PostInvocation`/`Stop` are "flat (list of handler
/// objects directly)". A `Stop` written in the grouped form is parsed as
/// invalid and silently dropped — only `--log-file` shows the reason
/// ("command hook must specify 'command'"), and `agy plugin validate` says
/// nothing (antigravity-cli#925, 1.1.24).
pub(super) const fn agy_event_is_grouped(event: HookEvent) -> bool {
    matches!(event, HookEvent::PreToolUse | HookEvent::PostToolUse)
}

/// One named hook as Antigravity CLI's shared `hooks.json` holds it: the
/// value under a root key, `{"<Event>": <entries>}` — grouped with the
/// translated matcher for a tool event, flat for `Stop` (the vendor parses
/// a grouped `Stop` as invalid and drops it silently, antigravity-cli#925).
///
/// The document root *is* the named-hook map: the vendor reads every root
/// key as one named hook, so a `hooks` wrapper key registers a single hook
/// called `hooks` whose "events" are our ids, and no handler ever runs
/// (1.1.24: `plugin validate` reports 1 hook processed instead of one per
/// group, and the loader fires nothing).
pub(crate) fn agy_named_entry(
    target: HookTarget,
    hook: &PortableHook,
    wrapper: &Path,
    package_root: &Path,
) -> serde_json::Value {
    let invocation = HookInvocation::Line(wrapper_command_line(wrapper, hook, package_root));
    let entries = if agy_event_is_grouped(hook.event) {
        vec![group_entry(target, hook, &invocation)]
    } else {
        vec![handler_entry(hook, &invocation)]
    };
    serde_json::json!({ hook_event_name(hook.event): entries })
}

/// Antigravity's shared `hooks.json` is a map of named hooks, not an event
/// array, so its merge is by *key*: this integration owns exactly the keys
/// it namespaces (`<package>:<group-id>`), and every other root key —
/// a hand-written hook, another tool's — keeps its value and its position
/// in the document. The file is re-emitted, not patched, so what a merge
/// does not preserve is formatting: indentation becomes two spaces and
/// whitespace between tokens is normalised.
pub(crate) fn merge_named_entry(
    config_path: &Path,
    entry_name: &str,
    entry: &serde_json::Value,
) -> Result<PathBuf> {
    let mut config = json_config::read_object(config_path)
        .map_err(|reason| UzeError::HarnessConfig(format!("cannot merge hook entry: {reason}")))?;
    config
        .as_object_mut()
        .expect("read_object returns an object")
        .insert(entry_name.to_owned(), entry.clone());
    json_config::write_object(config_path, &config)?;
    Ok(config_path.to_path_buf())
}

/// Content identity for one named hook: the key must exist and hold exactly
/// what the receipt recorded, and the wrapper it names must be the one UZE
/// writes. Anything else is drift, and drift blocks removal.
pub(crate) fn inspect_named_entry(
    config_path: &Path,
    entry_name: &str,
    expected: &str,
) -> AttachmentInspection {
    let Ok(config) = json_config::read_object(config_path) else {
        return blocked("hook config is missing or unreadable");
    };
    let Ok(expected) = serde_json::from_str::<serde_json::Value>(expected) else {
        return blocked("receipt carries an unreadable expected hook entry");
    };
    match config.get(entry_name) {
        Some(actual) if actual == &expected => AttachmentInspection {
            state: AttachmentState::Matched,
            reason: "managed hook entry matches the receipt".to_owned(),
        },
        Some(_) => AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "the managed hook entry differs from the receipt".to_owned(),
        },
        None => AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "the managed hook entry is absent".to_owned(),
        },
    }
}

/// Removes exactly the one named key this receipt owns, and the file itself
/// only when nothing else is left in it. A non-matched receipt blocks
/// removal; a foreign named hook is never touched.
pub(crate) fn remove_named_entry(
    config_path: &Path,
    entry_name: &str,
    expected: &str,
) -> Result<AttachmentInspection> {
    let inspection = inspect_named_entry(config_path, entry_name, expected);
    if inspection.state != AttachmentState::Matched {
        return Ok(inspection);
    }
    let mut config = json_config::read_object(config_path)
        .map_err(|reason| UzeError::HarnessConfig(format!("cannot detach hook entry: {reason}")))?;
    config
        .as_object_mut()
        .expect("read_object returns an object")
        .remove(entry_name);
    // A file that now holds nothing was created by UZE and is safe to
    // remove entirely; anything else stays exactly as the user left it.
    if config.as_object().is_some_and(|root| root.is_empty()) {
        match fs::remove_file(config_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(UzeError::Write {
                    path: config_path.to_path_buf(),
                    source,
                });
            }
        }
    } else {
        json_config::write_object(config_path, &config)?;
    }
    Ok(AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "managed hook entry detached".to_owned(),
    })
}

// ============================================================================
// Generated wrapper: hooks/exec
// ============================================================================
