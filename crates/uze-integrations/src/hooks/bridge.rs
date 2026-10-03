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
                    "handlers": hook.handlers.iter().map(|handler| serde_json::json!({
                        "command": handler.command.replace(
                            "${PLUGIN_ROOT}",
                            &package_root.display().to_string(),
                        ),
                        "timeout": handler.timeout,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
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
/// The V2 tool hooks see the tool input but cannot block, and the only
/// decision point (`permission.evaluate`) carries the action and its
/// resources rather than the tool input; deny/ask are therefore diagnosed
/// before attach and never fabricated here.
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
    let deny_exit_code = uze_core::hook::DENY_EXIT_CODE;
    let reason_limit = HANDLER_REASON_LIMIT;
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
// handler is bounded by the deadline its author declared.
//
// The plugin is the definition object itself, with no import: OpenCode
// 2.0.18 does not resolve `@opencode-ai/plugin` for a file in its plugin
// directory and refuses to load one that imports it, while `Plugin.define`
// only hands its argument back.

const ROOT = {root};
const GROUPS = {groups};

// native tool name -> portable alias and its portable fields
const ALIASES = {{
{aliases}
}};

const closed = (effect) => effect === "deny" || effect === "ask";

function environment(group, native, input) {{
  const alias = ALIASES[native];
  return {{
    ...process.env,
    PLUGIN_ROOT: ROOT,
    HOOK_HARNESS: "opencode",
    HOOK_EVENT: group.event,
    HOOK_TOOL: alias?.tool ?? "",
    HOOK_TOOL_NATIVE: native,
    HOOK_CWD: process.cwd(),
    HOOK_INPUT: JSON.stringify(input ?? {{}}),
    ...(alias ? alias.fields(input ?? {{}}) : {{}}),
  }};
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
async function handler(command, timeout, env) {{
  let proc;
  try {{
    proc = Bun.spawn(["/bin/sh", "-c", command], {{
      cwd: ROOT,
      env,
      stdin: "ignore",
      stdout: "ignore",
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
  const answer = Promise.all([collect(stream), proc.exited]);
  const outcome = await Promise.race([answer, deadline]);
  clearTimeout(timer);
  if (outcome === "expired") {{
    proc.kill();
    stream.cancel().catch(() => {{}});
    return {{ failed: true, reason: `handler timed out after ${{timeout}}s: ${{command}}` }};
  }}
  const [stderr, code] = outcome;
  if (code === 0) return null;
  if (code === {deny_exit_code}) return {{ failed: false, reason: stderr || `${{command}} denied the operation` }};
  return {{ failed: true, reason: `handler failed (exit ${{code}}): ${{command}}${{stderr ? " — " + stderr : ""}}` }};
}}

// Handlers in manifest order; the first denial stops the rest. A failure
// denies for a fail-closed group and is reported for the others.
async function run(group, native, input) {{
  const env = environment(group, native, input);
  for (const entry of group.handlers) {{
    const answer = await handler(entry.command, entry.timeout, env);
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

export default {{
  id: "hooks-{package_id}",
  async setup(ctx) {{
    await ctx.tool.hook("execute.before", async (event) => {{
      for (const group of GROUPS) {{
        if (!matches(group, "pre_tool_use", event.tool)) continue;
        const reason = await run(group, event.tool, event.input);
        if (reason) console.error(`[hooks:${{group.id}}]`, reason);
      }}
    }});
    await ctx.tool.hook("execute.after", async (event) => {{
      for (const group of GROUPS) {{
        if (!matches(group, "post_tool_use", event.tool)) continue;
        const reason = await run(group, event.tool, event.input);
        if (reason) console.error(`[hooks:${{group.id}}]`, reason);
      }}
    }});
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
