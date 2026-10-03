//! The generated POSIX `hooks/exec` wrapper: its per-harness dialect, its source, materializing it and taking it back.

use super::*;

/// How one harness's payload is read and how its decision is written — the
/// only slots that differ between the generated `hooks/exec` wrappers.
pub(super) struct WrapperDialect {
    /// `jq` filter selecting the native tool name from the payload.
    pub(super) tool_filter: &'static str,
    /// `jq` filter selecting the tool input object.
    pub(super) input_filter: &'static str,
    /// `jq` filter selecting the workspace directory.
    pub(super) cwd_filter: &'static str,
    /// The `sh` body that writes this harness's own denial on stdout, with
    /// `$1` already holding the reason as a JSON string literal.
    pub(super) deny_document: &'static str,
    /// The `sh` body that writes what this harness expects when nothing is
    /// denied, with `$1` holding the ABI event name.
    pub(super) allow_document: &'static str,
    /// The status the wrapper exits with after writing a denial. Claude and
    /// Codex document exit 2 as the block signal and read the decision only
    /// alongside it; Antigravity reads the decision from stdout and treats
    /// *any* non-zero exit as a failed hook — "pre-tool hook failed", the
    /// permission prompt, and the command runs anyway (measured on 1.1.24,
    /// `command_hook_executor.go`). So the code is a per-harness fact.
    pub(super) deny_exit: &'static str,
}

impl HookTarget {
    /// How this harness's payload is read and its decision written; `None`
    /// for OpenCode, whose generated plugin is its own runner.
    pub(super) fn dialect(self) -> Option<WrapperDialect> {
        match self {
            HookTarget::Claude => Some(WrapperDialect {
                tool_filter: ".tool_name // empty",
                input_filter: ".tool_input // {}",
                cwd_filter: ".cwd // .context.cwd // empty",
                // The event name is echoed back in `hookEventName`, which the
                // harness matches against the event it fired. Every event that
                // can deny is named: `session_start` never gets this far.
                deny_document: concat!(
                    "case $HOOK_EVENT in\n",
                    "    pre_tool_use) name=PreToolUse ;;\n",
                    "    post_tool_use) name=PostToolUse ;;\n",
                    "    stop) name=Stop ;;\n",
                    "  esac\n",
                    "  printf '{\"hookSpecificOutput\":{\"hookEventName\":\"%s\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":%s}}' \"$name\" \"$reason_json\"",
                ),
                allow_document: ":",
                deny_exit: "2",
            }),
            HookTarget::Codex => Some(WrapperDialect {
                tool_filter: ".tool_name // empty",
                input_filter: ".tool_input // {}",
                cwd_filter: ".cwd // empty",
                deny_document: "printf '{\"hookSpecificOutput\":{\"permissionDecision\":\"deny\",\"permissionDecisionReason\":%s}}' \"$reason_json\"",
                // Stop is the one event whose stdout must parse as JSON even
                // when nothing was decided.
                allow_document: "[ \"$HOOK_EVENT\" = stop ] && printf '{}'",
                deny_exit: "2",
            }),
            HookTarget::Antigravity => Some(WrapperDialect {
                tool_filter: ".toolCall.name // empty",
                input_filter: ".toolCall.args // {}",
                cwd_filter: ".workspacePaths[0] // empty",
                deny_document: "printf '{\"decision\":\"deny\",\"reason\":%s}' \"$reason_json\"",
                // Only the pre-tool event carries a decision; the others answer
                // with the empty object the vendor's contract requires.
                allow_document: "[ \"$HOOK_EVENT\" = pre_tool_use ] || printf '{}'",
                // The decision is the stdout document; a non-zero exit is a
                // failed hook here, not a block.
                deny_exit: "0",
            }),
            HookTarget::OpenCode => None,
        }
    }
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

/// The wrapper a harness actually executes at hook time: POSIX `sh`, one per
/// harness, byte-identical for every package. It reads the harness's payload
/// from stdin, exposes the hook context as `HOOK_*` environment, runs the
/// handlers sequentially, and answers in the harness's own dialect.
///
/// Ordering, first-deny-wins and fail-closed are compiled in here because no
/// harness provides them: a group's hooks may run in parallel, and a hook
/// that exits non-zero is non-blocking, so a `deny` guard that crashes would
/// otherwise let the tool through. `jq` is the wrapper's own dependency and
/// is guarded by the same rule.
///
/// Nothing in this file names the packager: the contract is the file, and
/// any tool that can write it can deliver a portable hook.
///
/// The ABI's "bounded output" lives here too: the wrapper is the only route
/// left, so the bound the removed in-binary runtime carried has to be the
/// one [`HANDLER_REASON_LIMIT`] states.
pub(crate) fn wrapper_source(target: HookTarget) -> Option<String> {
    let dialect = target.dialect()?;
    let harness = target.key();
    let fields = wrapper_field_variables(target);
    let field_defaults = fields
        .iter()
        .map(|name| format!("{name}="))
        .collect::<Vec<_>>()
        .join(" ");
    let field_exports = fields.join(" ");
    let aliases = wrapper_alias_table(target);
    let deny_exit_code = uze_core::hook::DENY_EXIT_CODE;
    let reason_limit = HANDLER_REASON_LIMIT;
    let WrapperDialect {
        tool_filter,
        input_filter,
        cwd_filter,
        deny_document,
        allow_document,
        deny_exit,
    } = dialect;
    Some(format!(
        r#"{WRAPPER_HEADER}, one per harness. The harness runs
# this; it runs the author's handlers. The handlers never see a harness
# payload and never write harness JSON: the context arrives as HOOK_*
# environment and the decision leaves as an exit code: 0 allows, while
# {deny_exit_code} denies and the reason is read from stderr. Anything else
# is a failure that follows the group's effect. Only the first {reason_limit}
# bytes of a handler's stderr become the reason a harness is handed.
#
#   usage: exec <plugin-root> <event> <effect> <seconds>:<handler>...
#     event    pre_tool_use | post_tool_use | stop | session_start
#     effect   observe | allow | ask | deny
#     seconds  this handler's own deadline; past it the handler and
#              everything it started are stopped, and the group's effect
#              decides, exactly as for any other handler failure
set -u
PLUGIN_ROOT=$1
HOOK_EVENT=$2
effect=$3
shift 3
HOOK_HARNESS={harness}
export PLUGIN_ROOT HOOK_EVENT HOOK_HARNESS

# --- this harness's decision dialect ------------------------------------
deny_native() {{                                  # $1 reason, plain text
  printf '%s\n' "$1" >&2
  # A session start decides nothing: a denial there is a report, and the
  # session opens as if the handler had allowed.
  [ "$HOOK_EVENT" = session_start ] && {{ allow_native; exit 0; }}
  reason_json=$(json_string "$1")
  {deny_document}
  exit {deny_exit}                                # this harness's block signal
}}

allow_native() {{
  {allow_document}
}}

# fail-closed effects: a guard that cannot be evaluated denies. `transform`
# is one of them — a rewrite that did not happen must not let the original
# through as if it had.
closed() {{ case $effect in deny|ask|transform) return 0 ;; *) return 1 ;; esac; }}
fail() {{ closed && deny_native "$1"; printf '%s\n' "$1" >&2; allow_native; exit 0; }}

# jq escapes the reason once it is available; before that (its own absence
# is the only reason reported then) a literal with neither quote nor
# newline needs no escaping.
json_string() {{
  if [ -n "${{JQ_READY:-}}" ]; then
    printf '%s' "$1" | "$JQ" -Rsa .
  else
    printf '"%s"' "$1"
  fi
}}

# --- the harness's payload becomes the hook context ----------------------
JQ=${{HOOK_JQ:-jq}}
command -v "$JQ" >/dev/null 2>&1 || fail "hooks/exec: jq is not installed"
JQ_READY=1
payload=$(cat)
# A payload jq cannot read leaves every extraction below empty, and a guard
# written the documented way (`case "$HOOK_COMMAND" in ...`) then sees
# nothing and allows. The context is the whole basis of the decision, so a
# payload that does not parse is a failure like any other.
printf '%s' "$payload" | "$JQ" -e . >/dev/null 2>&1 \
  || fail "hooks/exec: the harness payload is not JSON"
HOOK_TOOL_NATIVE=$(printf '%s' "$payload" | "$JQ" -r '{tool_filter}')
HOOK_CWD=$(printf '%s' "$payload" | "$JQ" -r '{cwd_filter}')
HOOK_INPUT=$(printf '%s' "$payload" | "$JQ" -c '{input_filter}')
HOOK_SOURCE=
[ "$HOOK_EVENT" = session_start ] \
  && HOOK_SOURCE=$(printf '%s' "$payload" | "$JQ" -r '.source // empty')
HOOK_TOOL= {field_defaults}
case "$HOOK_TOOL_NATIVE" in                       # the portable vocabulary
{aliases}esac
export HOOK_TOOL HOOK_TOOL_NATIVE HOOK_CWD HOOK_INPUT HOOK_SOURCE {field_exports}

# --- one handler, under its own deadline ---------------------------------
# There is no portable `timeout(1)` (macOS ships none) and no job control in
# a script, so there is no process group to signal: the deadline is a
# sleeper this shell can cancel, and what it stops is the handler plus every
# process the handler started. That second part is not thoroughness — a
# child still holding the pipe keeps this shell waiting long past the
# deadline it just enforced.
family() {{                                       # $1 pid -> $1 and its issue
  snapshot=$(ps -A -o pid=,ppid= 2>/dev/null)
  all=$1 layer=$1
  while [ -n "$layer" ]; do
    layer=$(printf '%s\n' "$snapshot" | while read -r pid parent; do
      for one in $layer; do
        [ "$parent" = "$one" ] && printf '%s ' "$pid"
      done
    done)
    all="$all $layer"
  done
  printf '%s' "$all"
}}

# Where a handler's reason is collected: a file, never a pipe. Anything the
# handler starts inherits a pipe, and one that outlives its deadline would
# hold this shell open long past the deadline it just enforced. `set -C`
# refuses a path that already exists, so a planted file or symlink is never
# written through; with nowhere to write at all, the reason is dropped
# rather than the hook.
reasons=${{TMPDIR:-/tmp}}/hooks-exec.$$
(set -C; : > "$reasons") 2>/dev/null || reasons=/dev/null
discard_reasons() {{ [ "$reasons" = /dev/null ] || rm -f "$reasons"; }}
trap discard_reasons EXIT
# A signal ends the wrapper. A trap that only cleaned up would return into
# the loop and run the next handler for a harness that has stopped waiting.
trap 'exit 130' INT
trap 'exit 143' TERM

# $1 seconds, $2 command. Leaves what the handler wrote on stderr in
# $reasons and answers with its exit status — or 124, the conventional
# timeout status, when the deadline stopped it. A handler that exits 124 of
# its own accord therefore reads as a timeout; `timeout(1)` carries the
# same ambiguity.
guarded() {{
  (
    sh -c "$2" </dev/null >/dev/null 2>"$reasons" &
    child=$!
    (
      napper= fired=
      # The parent cancels this watchdog by TERMing it the moment the
      # handler answers — but the handler answering *because* the sweep
      # below reached it is the one case where that TERM must be ignored,
      # or `exit 0` cuts the escalation short and a child that ignored
      # TERM outlives the hook.
      trap '[ -n "$fired" ] || {{ [ -n "$napper" ] && kill "$napper" 2>/dev/null; exit 0; }}' TERM
      sleep "$1" & napper=$!
      wait "$napper" 2>/dev/null
      fired=1
      doomed=$(family "$child")
      for one in $doomed; do kill -TERM "$one" 2>/dev/null; done
      sleep 1                                     # then the ones that stayed
      for one in $doomed; do kill -KILL "$one" 2>/dev/null; done
    ) >/dev/null 2>&1 &
    watchdog=$!
    wait "$child"; code=$?
    kill -TERM "$watchdog" 2>/dev/null            # cancels the sleeper too
    case $code in
      137|143) exit 124 ;;
      *) exit "$code" ;;
    esac
  ) 2>/dev/null                                   # the shell's own job notices
}}

# --- the handlers, in order; the first denial stops the rest --------------
# A handler is a shell command line, run from the package root: the same
# contract the canonical manifest documents, so `sh scripts/check --strict`
# means here exactly what it means when a person types it.
# A root that is gone is not a directory to fall back from: the handlers
# are relative to the package, so the harness's own working directory would
# run the *project's* same-named script instead of the author's.
cd "$PLUGIN_ROOT" 2>/dev/null || fail "hooks/exec: the package root is gone: $PLUGIN_ROOT"
for entry in "$@"; do
  seconds=${{entry%%:*}}
  handler=${{entry#*:}}
  case $seconds in
    ''|*[!0-9]*) fail "malformed handler argument: $entry" ;;
  esac
  guarded "$seconds" "$handler"; status=$?
  [ "$status" = 0 ] && continue                   # allowed; on to the next
  reason=$(head -c {reason_limit} "$reasons" 2>/dev/null)
  case $status in
    {deny_exit_code}) deny_native "${{reason:-$handler denied the operation}}" ;;
    124) fail "handler timed out after ${{seconds}}s: $handler" ;;
    *) fail "handler failed (exit $status): $handler${{reason:+ — $reason}}" ;;
  esac
done
allow_native
exit 0
"#
    ))
}

/// How every generated wrapper opens, whichever build wrote it: what tells
/// a wrapper an earlier template produced from one somebody else wrote.
pub(super) const WRAPPER_HEADER: &str = "#!/bin/sh\n# hooks/exec — generated from hooks.json";

/// The name of the wrapper inside its delivered artifact. `hooks/exec` on
/// every harness: one path an author or reviewer can look for.
pub(crate) const WRAPPER_RELATIVE_PATH: &str = "hooks/exec";

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
    let uze_core::integration::ManagedArtifact::HookConfigEntry {
        config_file,
        entry_name,
        event,
        expected,
        wrapper,
    } = &receipt.artifact
    else {
        return false;
    };
    let inspection = target.entry_state(&HookEntry {
        config_file,
        entry_name,
        event: *event,
        expected,
        wrapper,
    });
    if inspection.state != AttachmentState::Missing {
        return true;
    }
    fs::read_to_string(config_file)
        .is_ok_and(|config| config.contains(&wrapper.display().to_string()))
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
                .replace("${PLUGIN_ROOT}", &package_root.display().to_string())
        ));
    }
    arguments
}

/// The same invocation as one shell line, for the harnesses whose hook
/// entry carries a command string rather than a command plus arguments.
pub(crate) fn wrapper_command_line(
    wrapper: &Path,
    hook: &PortableHook,
    package_root: &Path,
) -> String {
    let mut parts = vec![shell_quote(&wrapper.display().to_string())];
    for argument in wrapper_arguments(hook, package_root, &hook.handlers) {
        parts.push(shell_quote(&argument));
    }
    parts.join(" ")
}

// ============================================================================
// Event-array config merge (Claude settings.json, Codex hooks.json)
// ============================================================================
