//! The POSIX `sh` wrapper: what Linux and macOS run at hook time.

use super::*;

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
pub(crate) struct PosixWrapper;

impl WrapperTemplate for PosixWrapper {
    const RELATIVE_PATH: &'static str = "hooks/exec";
    const HEADER: &'static str = HEADER;

    fn unfired(target: HookTarget) -> &'static [Unfired] {
        target
            .dialect()
            .map_or(&[], |dialect| dialect.posix.unfired)
    }

    fn source(target: HookTarget) -> Option<String> {
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
            payload:
                PayloadPaths {
                    tool: tool_filter,
                    input: input_filter,
                    cwd: cwd_filter,
                    implied_source,
                },
            posix:
                Decisions {
                    deny: deny_document,
                    allow: allow_document,
                    transform: transform_document,
                    ..
                },
            deny_exit,
            unwritten_exit,
            ..
        } = dialect;
        let value_limit = HOOK_VALUE_LIMIT;
        // Where the harness takes no rewritten input a transform group is
        // never delivered; the wrapper still answers it, closed.
        let transform_document = transform_document
            .unwrap_or("deny_native \"hooks/exec: this harness takes no rewritten input\"");
        let output_limit = TRANSFORM_OUTPUT_LIMIT;
        let source_filter = implied_source.map_or_else(
            || ".source // empty".to_owned(),
            |source| format!(".source // \"{source}\""),
        );
        Some(format!(
            r#"{HEADER}, one per harness. The harness runs
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
deny_native() {{                 # $1 reason, plain text; $2 decision (deny)
  decision=${{2:-deny}}
  printf '%s\n' "$1" >&2
  # A session start decides nothing: a denial there is a report, and the
  # session opens as if the handler had allowed.
  [ "$HOOK_EVENT" = session_start ] && {{ allow_native; exit 0; }}
  # The decision must not depend on starting a program: a reason that
  # cannot be encoded is replaced, never left out of the document.
  reason_json=$(json_string "$1") && [ -n "$reason_json" ] \
    || reason_json='"hooks/exec: denied; the reason could not be encoded"'
  {deny_document} || exit {unwritten_exit}
  exit {deny_exit}                                # this harness's block signal
}}

allow_native() {{
  {allow_document}
}}

transform_native() {{                             # the rewritten input
  updated_json=$HOOK_INPUT
  {transform_document}
  exit 0
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
# Inherited from the harness, any of these would already be exported, and
# so handed to every program below whatever its size.
unset HOOK_TOOL HOOK_TOOL_NATIVE HOOK_CWD HOOK_INPUT HOOK_SOURCE {field_exports}
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
  && HOOK_SOURCE=$(printf '%s' "$payload" | "$JQ" -r '{source_filter}')
# Whether every named variable fits the contract's bound, the same on
# every platform: one Windows can hold, and far inside Linux's
# MAX_ARG_STRLEN, past which no program starts at all, this wrapper's own
# jq and sh included. A value that does not fit is never exported: the
# group's effect decides, as for any context that cannot be built. Counted
# in UTF-8 bytes.
bounded() {{
  for name in "$@"; do
    eval "value=\${{$name}}"
    [ "$(LC_ALL=C; printf '%s' "${{#value}}")" -le {value_limit} ] || return 1
  done
}}
# The portable fields are read from the input, so a rewrite reads them again.
portable_fields() {{
  HOOK_TOOL= {field_defaults}
  case "$HOOK_TOOL_NATIVE" in                     # the portable vocabulary
{aliases}  esac
  bounded HOOK_TOOL_NATIVE HOOK_CWD HOOK_INPUT HOOK_SOURCE {field_exports} \
    || fail "hooks/exec: the tool input is larger than the {value_limit} bytes a HOOK_* variable carries"
  export HOOK_TOOL HOOK_TOOL_NATIVE HOOK_CWD HOOK_INPUT HOOK_SOURCE {field_exports}
}}
portable_fields

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
# hold this shell open long past the deadline it just enforced. The file
# sits in a directory made for this call and only this user can enter: a
# predictable name in a shared directory is one somebody else can create
# first. With nowhere private to write, a handler's answer cannot be read,
# which is a failure like any other.
workspace=$(mktemp -d "${{TMPDIR:-/tmp}}/hooks-exec.XXXXXX" 2>/dev/null) && [ -d "$workspace" ] \
  || fail "hooks/exec: no private directory to collect a handler's answer in"
discard_workspace() {{ rm -rf "$workspace"; }}
trap discard_workspace EXIT
reasons=$workspace/reasons
# A transform handler answers with the rewritten input on stdout.
rewrites=/dev/null
[ "$effect" = transform ] && rewrites=$workspace/rewrites
# A signal ends the wrapper. A trap that only cleaned up would return into
# the loop and run the next handler for a harness that has stopped waiting.
trap 'exit 130' INT
trap 'exit 143' TERM

# $1 seconds, $2 command, $3 where its stdout goes. Leaves what the handler
# wrote on stderr in $reasons and answers with its exit status — or 124, the conventional
# timeout status, when the deadline stopped it. A handler that exits 124 of
# its own accord therefore reads as a timeout; `timeout(1)` carries the
# same ambiguity.
guarded() {{
  (
    sh -c "$2" </dev/null >"$3" 2>"$reasons" &
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
  [ "$rewrites" = /dev/null ] || : > "$rewrites"
  guarded "$seconds" "$handler" "$rewrites"; status=$?
  if [ "$status" = 0 ] && [ -s "$rewrites" ]; then
    # A rewrite: the complete input, as one JSON object. The next handler
    # reads it as its HOOK_INPUT, and the last one is what the tool runs.
    [ "$(wc -c < "$rewrites")" -le {output_limit} ] \
      || fail "handler wrote more than {output_limit} bytes: $handler"
    updated=$("$JQ" -ce 'if type == "object" then . else error end' < "$rewrites" 2>/dev/null) \
      || fail "handler did not write a JSON object: $handler"
    HOOK_INPUT=$updated
    rewritten=1
    portable_fields
  fi
  [ "$status" = 0 ] && continue                   # allowed; on to the next
  reason=$(head -c {reason_limit} "$reasons" 2>/dev/null)
  case $status in
    # A handler's own denial in an `ask` group asks the person; only a
    # failure falls back to denying.
    {deny_exit_code}) deny_native "${{reason:-$handler denied the operation}}" "$( [ "$effect" = ask ] && echo ask || echo deny )" ;;
    124) fail "handler timed out after ${{seconds}}s: $handler" ;;
    *) fail "handler failed (exit $status): $handler${{reason:+ — $reason}}" ;;
  esac
done
[ -n "${{rewritten:-}}" ] && transform_native
allow_native
exit 0
"#
        ))
    }
}

/// How every generated POSIX wrapper opens, whichever build wrote it: what
/// tells a wrapper an earlier template produced from one somebody else
/// wrote.
const HEADER: &str = "#!/bin/sh\n# hooks/exec — generated from hooks.json";

/// The `case` arm list translating this harness's native tool names into
/// `HOOK_TOOL` and the matched alias's portable field variables, generated
/// from the one vocabulary the matchers are generated from.
fn wrapper_alias_table(target: HookTarget) -> String {
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
