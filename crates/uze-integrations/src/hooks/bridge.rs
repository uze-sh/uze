//! The OpenCode bridge: the owned plugin file that carries the hook contract into OpenCode's runtime.

use super::*;

/// The delivered plugin's path: `<config root>/plugins/hooks-<package>.ts`.
/// `<config root>/plugins/` is OpenCode's documented global plugin directory
/// (`~/.config/opencode/plugins/`), auto-discovered at startup — the file is
/// therefore the single, self-contained load source: no `plugin` entry in
/// `opencode.json` exists to duplicate it. (Verified against the real
/// harness: the legacy `.opencode/plugins/` path is project-scoped and NOT
/// auto-discovered under the global config directory.)
pub(crate) fn opencode_bridge_path(config_root: &Path, package_id: &str) -> PathBuf {
    config_root
        .join("plugins")
        .join(format!("hooks-{package_id}.ts"))
}

/// The package's groups as data for the generated plugin: translated
/// matchers (matched against the runtime native tool name), abi event name,
/// effect, and the authored handlers with `${PLUGIN_ROOT}` resolved.
pub(super) fn bridge_hooks(
    target: HookTarget,
    hooks: &[&PortableHook],
    package_root: &Path,
) -> serde_json::Value {
    let store_root = package_root;
    let package_root = &crate::shared::package_root::delivered(package_root);
    serde_json::Value::Array(
        hooks
            .iter()
            .map(|hook| {
                serde_json::json!({
                    "id": hook.id,
                    "event": hook.event.abi_name(),
                    "effect": hook.effect.abi_name(),
                    "matchers": hook.matchers.iter().flat_map(|m| tool_names(target, m)).collect::<Vec<_>>(),
                    "handlers": hook.handlers.iter().map(|handler| bridged_handler(handler, store_root, package_root)).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// The extensions OpenCode's embedded Bun runs as they are, with no
/// interpreter asked of the machine.
const OWN_RUNTIME: &[&str] = &["js", "mjs", "cjs", "ts"];

/// Whether the bridge runs `handler` in OpenCode's own Bun, asking nothing
/// of the machine: an exec-form JavaScript or TypeScript script with no
/// launcher of the author's.
pub(super) fn runs_in_own_runtime(handler: &uze_core::hook::CommandHook) -> bool {
    handler.interpreter.is_none()
        && handler.script().is_some_and(|script| {
            Path::new(script)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    OWN_RUNTIME.contains(&extension.to_ascii_lowercase().as_str())
                })
        })
}

/// One handler as the bridge spawns it: a shell line as the author wrote
/// it; an exec-form script from its words, with no shell between; or a
/// JavaScript one in OpenCode's own Bun (`process.execPath`, which is a
/// `bun build --compile` executable that acts as `bun` under `BUN_BE_BUN`).
fn bridged_handler(
    handler: &uze_core::hook::CommandHook,
    store_root: &Path,
    delivered_root: &Path,
) -> serde_json::Value {
    if runs_in_own_runtime(handler)
        && let Some(script) = handler.script()
    {
        let argv: Vec<String> = std::iter::once(delivered_root.join(script).display().to_string())
            .chain(handler.args.iter().flatten().cloned())
            .collect();
        return serde_json::json!({
            "command": handler.describe(),
            "argv": argv,
            "bun": true,
            "timeout": handler.timeout,
        });
    }
    match handler.invocation(
        store_root,
        delivered_root,
        uze_platform::shell::FAMILY,
        &uze_core::launcher::python_answers,
    ) {
        uze_core::hook::Invocation::Argv { argv, .. } => serde_json::json!({
            "command": handler.describe(),
            "argv": argv,
            "timeout": handler.timeout,
        }),
        uze_core::hook::Invocation::Line(line) => serde_json::json!({
            "command": uze_platform::shell::script_text(&uze_platform::shell::substitute(
                &line,
                crate::shared::package_root::TOKEN,
                &delivered_root.display().to_string(),
            )),
            "timeout": handler.timeout,
        }),
        uze_core::hook::Invocation::Unrunnable(_) => serde_json::json!({
            "command": handler.describe(),
            "timeout": handler.timeout,
        }),
    }
}

/// The alias table this harness's plugin reads, generated from the one
/// vocabulary: native tool name → portable alias plus the portable field
/// values, each read from that harness's own input field.
pub(super) fn bridge_alias_table(target: HookTarget) -> String {
    let mut rows = Vec::new();
    for (native, binding) in vocabulary(target).native_names() {
        let fields = binding
            .fields
            .iter()
            .map(|(portable, native_field)| {
                format!(
                    "{}: String(input.{native_field} ?? \"\")",
                    uze_core::hook::hook_field_variable(portable)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(format!(
            "  {native}: {{ tool: \"{}\", fields: (input) => ({{ {fields} }}) }},",
            binding.alias
        ));
    }
    rows.join("\n")
}

/// How every generated OpenCode plugin opens, whichever build wrote it.
pub(super) const BRIDGE_HEADER: &str =
    "// Generated from hooks.json — do not edit; regenerate instead.";

/// Whether `text` is a plugin some build generated for exactly these
/// groups — the runtime around them may be an earlier template's, and the
/// groups may be in an earlier build's order. The plugin is generated tier:
/// neither is a reason to block the removal that deletes it, and the next
/// attach reproduces the current bytes.
pub(crate) fn bridge_carries_groups(
    target: HookTarget,
    text: &str,
    hooks: &[&PortableHook],
    plugin_root: &Path,
) -> bool {
    let Some(rest) = text.strip_prefix(BRIDGE_HEADER) else {
        return false;
    };
    let Some(serde_json::Value::Array(mut carried)) = rest
        .lines()
        .find_map(|line| line.strip_prefix("const GROUPS = "))
        .and_then(|groups| serde_json::from_str(groups.trim_end_matches(';')).ok())
    else {
        return false;
    };
    let serde_json::Value::Array(mut expected) = bridge_hooks(target, hooks, plugin_root) else {
        return false;
    };
    carried.sort_by_cached_key(serde_json::Value::to_string);
    expected.sort_by_cached_key(serde_json::Value::to_string);
    carried == expected
}

/// The generated OpenCode plugin for one package. OpenCode has no
/// declarative hook file, so the plugin *is* the wrapper: the same contract
/// the `sh` wrapper implements on the other harnesses, with this package's
/// groups as data. JavaScript valid as TypeScript (no build step), no
/// dependencies, deterministic — and naming nothing but the hook contract.
///
/// Every event and effect rides OpenCode V2's own plugin API (anomalyco/
/// opencode `v2`, measured on 2.0.24): `execute.before` for observing,
/// rewriting and denying, and `permission.evaluate` for asking — the tool
/// hook sees the input but has no refusal of its own (a denial there
/// renames the call to a tool that does not exist), and the permission
/// hook can ask but carries no input and fires only for a tool that asks,
/// so the input is kept by call id between the two. A session's
/// start and the end of its turn are bus events (`session.created`,
/// `session.execution.succeeded`); a denied stop is answered the way the
/// harness's own plan plugin keeps a session going, with synthetic input.
pub(crate) fn opencode_bridge(
    target: HookTarget,
    hooks: &[&PortableHook],
    plugin_root: &Path,
    package_id: &str,
) -> String {
    let root = serde_json::to_string(&plugin_root.display().to_string())
        .expect("a path serializes as a JSON string");
    let groups = serde_json::to_string(&bridge_hooks(target, hooks, plugin_root))
        .expect("generated groups serialize");
    let aliases = bridge_alias_table(target);
    let prompting = match target.runner {
        HookRunner::Bridge { prompting } => prompting,
        HookRunner::Wrapper { .. } => &[],
    };
    let prompting = serde_json::to_string(prompting).expect("tool names serialize");
    // The shell a handler line is written for on this platform, decided
    // when the bridge is generated.
    let shell = serde_json::to_string(uze_platform::shell::ARGV).expect("shell words serialize");
    let deny_exit_code = uze_core::hook::DENY_EXIT_CODE;
    let reason_limit = HANDLER_REASON_LIMIT;
    let output_limit = wrapper::TRANSFORM_OUTPUT_LIMIT;
    let value_limit = wrapper::HOOK_VALUE_LIMIT;
    format!(
        r#"{BRIDGE_HEADER}
// OpenCode V2 (opencode.ai/v2/docs/build/plugins) has no hooks.json: the
// plugin is both the registration and the runner. Only GROUPS changes
// between packages; everything below it is the same runtime every time.
//
// Handler contract: the hook context arrives as HOOK_* environment and the
// decision leaves as an exit code — 0 allows, {deny_exit_code} denies with
// the reason on stderr, anything else is a failure that follows the group's
// effect (fail-closed for deny/ask, fail-open for observe/allow). Each
// handler is bounded by the deadline its author declared, and no HOOK_*
// value is longer than {value_limit} bytes: past that the group's effect
// decides, as on every other harness.
//
// The plugin is the definition object itself, with no import: OpenCode
// 2.0.18 does not resolve `@opencode-ai/plugin` for a file in its plugin
// directory and refuses to load one that imports it, while `Plugin.define`
// only hands its argument back.

const ROOT = {root};
const GROUPS = {groups};
const SHELL = {shell};
// The tools that ask for permission before they act: the only calls an
// `ask` group can put to the person.
const PROMPTING = {prompting};

// native tool name -> portable alias and its portable fields
const ALIASES = {{
{aliases}
}};

const closed = (effect) => effect === "deny" || effect === "ask";

// The hook context, or null when a value in it is longer than one
// environment string may be: the contract's bound, the same on every
// platform and every harness.
function environment(group, native, input, source) {{
  const alias = ALIASES[native];
  const context = {{
    HOOK_HARNESS: "opencode",
    HOOK_EVENT: group.event,
    HOOK_TOOL: alias?.tool ?? "",
    HOOK_TOOL_NATIVE: native ?? "",
    HOOK_CWD: process.cwd(),
    HOOK_INPUT: JSON.stringify(input ?? {{}}),
    HOOK_SOURCE: source ?? "",
    ...(alias ? alias.fields(input ?? {{}}) : {{}}),
  }};
  const bytes = new TextEncoder();
  if (Object.values(context).some((value) => bytes.encode(value).length > {value_limit})) return null;
  return {{ ...process.env, PLUGIN_ROOT: ROOT, ...context }};
}}
const OVERSIZED = "hooks: the tool input is larger than the {value_limit} bytes a HOOK_* variable carries";

// A handler and everything it started: it runs as the leader of a process
// group of its own, so the deadline reaches a child still holding a pipe.
// Windows has no process groups; the tree is ended by pid instead.
const WINDOWS = process.platform === "win32";
function stop(proc) {{
  try {{
    if (WINDOWS) Bun.spawnSync(["taskkill", "/T", "/F", "/PID", String(proc.pid)]);
    else process.kill(-proc.pid, "SIGKILL");
  }} catch {{}}
  proc.kill();
}}

// A handler's stderr, bounded like the sh wrapper's: the reason is a
// sentence for a person, not a transcript, and an unbounded one becomes the
// harness's document. Past the bound the stream is still drained, so a
// handler writing more is never blocked on a full pipe.
async function collect(stream) {{
  const kept = new Uint8Array({reason_limit});
  let length = 0;
  for (;;) {{
    const {{ done, value }} = await stream.read();
    if (done) break;
    const room = kept.length - length;
    if (room > 0) {{
      const part = value.subarray(0, room);
      kept.set(part, length);
      length += part.length;
    }}
  }}
  return new TextDecoder().decode(kept.subarray(0, length)).trim();
}}

// One handler: null when it allowed, otherwise the reason it answered with.
// A `transform` handler's stdout is kept: `rewrite` receives it when it
// allowed with something to say.
async function handler(entry, timeout, env, rewrite) {{
  const command = entry.command;
  const argv = !entry.argv
    ? [...SHELL, command]
    : entry.bun
      ? [process.execPath, ...entry.argv]
      : entry.argv;
  let proc;
  try {{
    proc = Bun.spawn(argv, {{
      cwd: ROOT,
      detached: !WINDOWS,
      env: entry.bun ? {{ ...env, BUN_BE_BUN: "1" }} : env,
      stdin: "ignore",
      stdout: rewrite ? "pipe" : "ignore",
      stderr: "pipe",
    }});
  }} catch (error) {{
    return {{ failed: true, reason: `handler failed to start: ${{command}} — ${{error.message}}` }};
  }}
  // The author's deadline, enforced here for the same reason the sh
  // wrapper enforces it: nothing else will. It races the whole answer, not
  // just the exit: a process the handler started can hold stderr open long
  // after the shell it was started from has been stopped.
  let timer;
  const deadline = new Promise((resolve) => {{
    timer = setTimeout(() => resolve("expired"), timeout * 1000);
  }});
  const stream = proc.stderr.getReader();
  const output = rewrite ? new Response(proc.stdout).text() : Promise.resolve("");
  const answer = Promise.all([collect(stream), proc.exited, output]);
  const outcome = await Promise.race([answer, deadline]);
  clearTimeout(timer);
  if (outcome === "expired") {{
    stop(proc);
    stream.cancel().catch(() => {{}});
    return {{ failed: true, reason: `handler timed out after ${{timeout}}s: ${{command}}` }};
  }}
  const [stderr, code, stdout] = outcome;
  if (code === 0) {{
    if (!rewrite || stdout.trim() === "") return null;
    if (new TextEncoder().encode(stdout).length > {output_limit}) {{
      return {{ failed: true, reason: `handler wrote more than {output_limit} bytes: ${{command}}` }};
    }}
    let rewritten;
    try {{
      rewritten = JSON.parse(stdout);
    }} catch {{
      rewritten = undefined;
    }}
    if (rewritten === null || typeof rewritten !== "object" || Array.isArray(rewritten)) {{
      return {{ failed: true, reason: `handler did not write a JSON object: ${{command}}` }};
    }}
    rewrite(rewritten);
    return null;
  }}
  if (code === {deny_exit_code}) return {{ failed: false, reason: stderr || `${{command}} denied the operation` }};
  return {{ failed: true, reason: `handler failed (exit ${{code}}): ${{command}}${{stderr ? " — " + stderr : ""}}` }};
}}

// Handlers in manifest order; the first denial stops the rest. A failure
// denies for a fail-closed group and is reported for the others.
async function run(group, native, input, source) {{
  const env = environment(group, native, input, source);
  if (!env) {{
    if (closed(group.effect)) return OVERSIZED;
    console.error(`[hooks:${{group.id}}]`, OVERSIZED);
    return null;
  }}
  for (const entry of group.handlers) {{
    const answer = await handler(entry, entry.timeout, env);
    if (answer === null) continue;
    if (answer.failed && !closed(group.effect)) {{
      console.error(`[hooks:${{group.id}}]`, answer.reason);
      continue;
    }}
    return answer.reason;
  }}
  return null;
}}

function matches(group, event, native) {{
  return (
    group.event === event &&
    (group.matchers.length === 0 || group.matchers.includes(native))
  );
}}

// Groups on `event` for `native`: the observing ones, the deciding ones
// (and of those the denying and the asking ones) and the rewriting ones.
const observing = (event, native) =>
  GROUPS.filter((group) => matches(group, event, native) && !closed(group.effect));
const deciding = (event, native) =>
  GROUPS.filter(
    (group) => matches(group, event, native) && closed(group.effect) && group.effect !== "transform",
  );
const denying = (event, native) =>
  GROUPS.filter((group) => matches(group, event, native) && group.effect === "deny");
const asking = (event, native) =>
  GROUPS.filter((group) => matches(group, event, native) && group.effect === "ask");
const rewriting = (event, native) =>
  GROUPS.filter((group) => matches(group, event, native) && group.effect === "transform");

// A transform group's handlers in order, each reading the input the one
// before it wrote; the first denial or failure stops it and closes the call.
async function transform(group, native, input) {{
  let current = input;
  for (const entry of group.handlers) {{
    const env = environment(group, native, current);
    if (!env) return {{ reason: OVERSIZED }};
    const answer = await handler(entry, entry.timeout, env, (rewritten) => {{
      current = rewritten;
    }});
    if (answer !== null) return {{ reason: answer.reason }};
  }}
  return {{ input: current }};
}}

// Refuses a call from `execute.before`, which has no refusal of its own:
// OpenCode looks the tool up by the name this event carries once the hook
// returns, so a name no tool has fails the call before it runs, with an
// error that hands the model the reason. Not every tool asks for
// permission (`read` never does), so this is the only refusal such a call
// meets. A call reached through code mode was looked up before the hook;
// without an input it fails to decode instead of running.
function refuse(event, reason) {{
  event.tool = `${{event.tool}} (refused by a hook: ${{reason}})`;
  event.input = undefined;
}}

// A tool call's name and input, kept from `execute.before` until the
// permission check of the same call decides on it.
const CALLS = new Map();

// Sessions a subagent runs in: their turns are the parent's work, not a
// person's session starting or stopping.
const CHILDREN = new Set();

async function decide(groups, native, input) {{
  for (const group of groups) {{
    const reason = await run(group, native, input);
    if (reason) return {{ group, reason }};
  }}
  return null;
}}

async function follow(ctx) {{
  for await (const event of ctx.event.subscribe()) {{
    const data = event.data ?? {{}};
    if (event.type === "session.created") {{
      if (data.parentID) {{
        CHILDREN.add(data.sessionID);
        continue;
      }}
      // A new session is the only start OpenCode announces; a resumed one
      // publishes nothing, so its groups never run here.
      for (const group of GROUPS) {{
        if (!matches(group, "session_start", "startup")) continue;
        const reason = await run(group, undefined, undefined, "startup");
        if (reason) console.error(`[hooks:${{group.id}}]`, reason);
      }}
    }} else if (event.type === "session.execution.succeeded") {{
      if (CHILDREN.has(data.sessionID)) continue;
      for (const group of observing("stop", undefined)) {{
        const reason = await run(group, undefined, undefined);
        if (reason) console.error(`[hooks:${{group.id}}]`, reason);
      }}
      const denied = await decide(deciding("stop", undefined), undefined, undefined);
      // A denied stop keeps the session going, told why.
      if (denied) {{
        await ctx.session.synthetic({{
          sessionID: data.sessionID,
          text: denied.reason,
          resume: true,
        }});
      }}
    }}
  }}
}}

export default {{
  id: "hooks-{package_id}",
  async setup(ctx) {{
    await ctx.tool.hook("execute.before", async (event) => {{
      const call = {{ tool: event.tool, input: event.input }};
      CALLS.set(event.id, call);
      // A rewrite is what the tool then runs, as OpenCode's own input
      // repair does it; a group that cannot rewrite closes the call.
      for (const group of rewriting("pre_tool_use", event.tool)) {{
        const result = await transform(group, event.tool, call.input);
        if (result.reason) {{
          call.refused = result.reason;
          break;
        }}
        call.input = result.input;
        event.input = result.input;
      }}
      for (const group of observing("pre_tool_use", event.tool)) {{
        const reason = await run(group, event.tool, event.input);
        if (reason) console.error(`[hooks:${{group.id}}]`, reason);
      }}
      if (!call.refused) {{
        const denied = await decide(denying("pre_tool_use", call.tool), call.tool, call.input);
        if (denied) call.refused = denied.reason;
      }}
      // An `ask` about a tool that never asks for permission would never
      // reach the person, and the call would run unasked: it is refused.
      if (!call.refused && !PROMPTING.includes(call.tool)) {{
        call.asked = true;
        const asked = await decide(asking("pre_tool_use", call.tool), call.tool, call.input);
        if (asked) {{
          call.refused = `${{asked.reason}} (OpenCode offers no permission prompt for \`${{call.tool}}\`, so the call is refused rather than run unasked)`;
        }}
      }}
      if (call.refused) refuse(event, call.refused);
    }});
    // Where a tool asks for permission, this is where an `ask` group asks,
    // and a call `execute.before` refused stays refused should it run all
    // the same.
    await ctx.permission.hook("evaluate", async (event) => {{
      if (event.source?.type !== "tool") return;
      const call = CALLS.get(event.source.id);
      if (!call || call.decided) return;
      call.decided = true;
      if (call.refused) {{
        event.effect = "deny";
        event.message = call.refused;
        return;
      }}
      if (call.asked) return;
      const asked = await decide(asking("pre_tool_use", call.tool), call.tool, call.input);
      if (!asked) return;
      event.effect = "ask";
      event.message = asked.reason;
    }});
    await ctx.tool.hook("execute.after", async (event) => {{
      CALLS.delete(event.id);
      for (const group of observing("post_tool_use", event.tool)) {{
        const reason = await run(group, event.tool, event.input);
        if (reason) console.error(`[hooks:${{group.id}}]`, reason);
      }}
      const denied = await decide(deciding("post_tool_use", event.tool), event.tool, event.input);
      // The tool already ran; what a denial can still do is tell the model.
      if (denied) {{
        await ctx.session.synthetic({{
          sessionID: event.sessionID,
          text: denied.reason,
          resume: false,
        }});
      }}
    }});
    follow(ctx).catch((error) => console.error("[hooks]", error));
  }},
}};
"#
    )
}

/// Removes the owned bridge file. The `plugins/` directory belongs to the
/// vendor's global plugin namespace — a foreign plugin file in it keeps it
/// alive; an empty directory left behind only by this file is removed.
pub(crate) fn remove_bridge_file(bridge_path: &Path) -> Result<()> {
    match fs::remove_file(bridge_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(UzeError::Write {
                path: bridge_path.to_path_buf(),
                source,
            });
        }
    }
    if let Some(plugins_dir) = bridge_path.parent()
        && fs::read_dir(plugins_dir).is_ok_and(|mut entries| entries.next().is_none())
    {
        let _ = fs::remove_dir(plugins_dir);
    }
    Ok(())
}
