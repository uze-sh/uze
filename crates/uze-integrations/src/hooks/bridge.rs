//! The OpenCode bridge: the owned plugin file that carries the hook contract into OpenCode's runtime.

use super::*;

/// Where an earlier release delivered a package's hooks, as a plugin of its
/// own: `<config root>/plugins/hooks-<package>.ts`. Read only to inspect,
/// detach and replace what such a release left.
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

/// The version of the per-package hook document the runtime plugin reads;
/// a document of another version is unreadable to it, and refuses calls.
pub(crate) const OPENCODE_DOCUMENT_VERSION: u32 = 1;

/// Where OpenCode's runtime plugin reads every package's hook groups: one
/// document per package, generated from the Store like the other
/// harnesses' wrapper beside it.
pub(crate) fn opencode_documents_dir(uze_home: &UzeHome, target: HookTarget) -> PathBuf {
    crate::shared::path::attachment_root(uze_home, target.key).join("hook-groups")
}

/// One package's hook document.
pub(crate) fn opencode_document_path(
    uze_home: &UzeHome,
    target: HookTarget,
    package_id: &str,
) -> PathBuf {
    opencode_documents_dir(uze_home, target).join(format!("{package_id}.json"))
}

/// The document for one package: the root its handlers run in and its
/// groups, in manifest order. It names nothing but the hook contract.
pub(crate) fn opencode_hook_document(
    target: HookTarget,
    hooks: &[&PortableHook],
    plugin_root: &Path,
) -> String {
    let document = serde_json::json!({
        "version": OPENCODE_DOCUMENT_VERSION,
        "root": plugin_root.display().to_string(),
        "groups": bridge_hooks(target, hooks, plugin_root),
    });
    let mut text = serde_json::to_string_pretty(&document).expect("a document serializes");
    text.push('\n');
    text
}

/// Whether `text` is a document carrying exactly these groups, in any
/// order an earlier build wrote them in.
pub(crate) fn document_carries_groups(
    target: HookTarget,
    text: &str,
    hooks: &[&PortableHook],
    plugin_root: &Path,
) -> bool {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    let serde_json::Value::Array(mut carried) = document["groups"].clone() else {
        return false;
    };
    let serde_json::Value::Array(mut expected) = bridge_hooks(target, hooks, plugin_root) else {
        return false;
    };
    carried.sort_by_cached_key(serde_json::Value::to_string);
    expected.sort_by_cached_key(serde_json::Value::to_string);
    document["version"] == OPENCODE_DOCUMENT_VERSION
        && document["root"] == plugin_root.display().to_string()
        && carried == expected
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
        let fields = if fields.is_empty() {
            "() => ({})".to_owned()
        } else {
            format!("(input) => ({{ {fields} }})")
        };
        rows.push(format!(
            "    {native}: {{ tool: \"{}\", fields: {fields} }},",
            binding.alias
        ));
    }
    rows.join("\n")
}

/// How every generated OpenCode plugin opens, whichever build wrote it.
pub(crate) const BRIDGE_HEADER: &str =
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

/// The runtime plugin's hooks section, reading every package's groups from
/// the documents in `documents`. OpenCode has no declarative hook file, so
/// this *is* the wrapper: the same contract the `sh` wrapper implements on
/// the other harnesses, run once over every package's groups.
///
/// Every event and effect rides OpenCode V2's own plugin API (anomalyco/
/// opencode `v2`, measured on 2.0.24): `execute.before` for observing,
/// rewriting and denying, and `permission.evaluate` for asking — the tool
/// hook sees the input but has no refusal of its own (a denial there
/// renames the call to a tool that does not exist), and the permission
/// hook can ask but fires only for a tool that asks and names the call by
/// id, with no more of its input than the shell's command line (2.1.4,
/// `experiments/opencode/permission-evaluate`), so the input is kept by call
/// id between the two. A session's
/// start and the end of its turn are bus events (`session.created`,
/// `session.execution.succeeded`); a denied stop is answered the way the
/// harness's own plan plugin keeps a session going, with synthetic input.
pub(crate) fn opencode_hooks_section(target: HookTarget, documents: &Path) -> String {
    let documents = serde_json::to_string(&documents.display().to_string())
        .expect("a path serializes as a JSON string");
    let version = OPENCODE_DOCUMENT_VERSION;
    let aliases = bridge_alias_table(target);
    let prompting = match target.runner {
        HookRunner::Bridge { prompting } => prompting,
        HookRunner::Wrapper { .. } => &[],
    };
    let prompting = crate::opencode::runtime_plugin::literal(&serde_json::json!(prompting), 1);
    // The shell a handler line is written for on this platform, decided
    // when the section is generated.
    let shell =
        crate::opencode::runtime_plugin::literal(&serde_json::json!(uze_platform::shell::ARGV), 1);
    let deny = uze_core::hook::DENY_EXIT_CODE;
    let reason_limit = HANDLER_REASON_LIMIT;
    let output_limit = wrapper::TRANSFORM_OUTPUT_LIMIT;
    let value_limit = wrapper::HOOK_VALUE_LIMIT;
    format!(
        r#"// Portable hooks. A handler receives the hook context as HOOK_* environment
// and answers with an exit code: 0 allows, {deny} denies with the reason on
// stderr, and anything else is a failure the group's effect decides, closed
// for deny, ask and transform and open for observe and allow. Each handler is
// bounded by the deadline its author declared, and no HOOK_* value is longer
// than {value_limit} bytes.
//
// Every package's groups are one document each in DOCUMENTS, read in package
// order and again whenever the directory changes, so every group sees a call
// in one pass, and the first denial, from whichever package, stops the rest.
async function hooks(ctx) {{
  const DOCUMENTS = {documents};
  const VERSION = {version};
  // The words that run a handler's shell line on this platform.
  const SHELL = {shell};
  // The tools that ask for permission before they act: the only calls an
  // `ask` group can put to the person.
  const PROMPTING = {prompting};
  // A native tool's portable alias, and the portable fields read from its
  // input.
  const ALIASES = {{
{aliases}
  }};
  const OVERSIZED = "the tool input is larger than the {value_limit} bytes a HOOK_* variable carries";
  // A handler leads a process group of its own, so its deadline reaches a
  // child still holding a pipe; Windows has no process groups, and the tree
  // is ended by pid instead.
  const WINDOWS = process.platform === "win32";

  let groups = [];
  // Documents that cannot be read. Which calls they guarded is unknown, so
  // while one is there every call is refused.
  let unreadable = [];
  const load = () => {{
    const loaded = [];
    const broken = [];
    let names = [];
    try {{
      names = readdirSync(DOCUMENTS).filter((name) => name.endsWith(".json")).sort();
    }} catch {{}}
    for (const name of names) {{
      const path = join(DOCUMENTS, name);
      try {{
        const document = JSON.parse(readFileSync(path, "utf8"));
        if (document?.version !== VERSION || typeof document.root !== "string" || !Array.isArray(document.groups)) {{
          throw new Error("not a hook document");
        }}
        for (const group of document.groups) loaded.push({{ ...group, root: document.root }});
      }} catch {{
        broken.push(path);
      }}
    }}
    groups = loaded;
    unreadable = broken;
  }};

  const closed = (effect) => effect === "deny" || effect === "ask" || effect === "transform";
  const select = (event, native, accept) =>
    groups.filter(
      (group) =>
        group.event === event &&
        (group.matchers.length === 0 || group.matchers.includes(native)) &&
        accept(group.effect),
    );
  const observing = (event, native) => select(event, native, (effect) => !closed(effect));
  const deciding = (event, native) => select(event, native, (effect) => effect === "deny" || effect === "ask");
  const denying = (event, native) => select(event, native, (effect) => effect === "deny");
  const asking = (event, native) => select(event, native, (effect) => effect === "ask");
  const rewriting = (event, native) => select(event, native, (effect) => effect === "transform");

  // The handler's environment, or null when a value in it is longer than one
  // environment string may be, which is the contract's bound on every
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
    return {{ ...process.env, PLUGIN_ROOT: group.root, ...context }};
  }}

  function stop(proc) {{
    try {{
      if (WINDOWS) Bun.spawnSync(["taskkill", "/T", "/F", "/PID", String(proc.pid)]);
      else process.kill(-proc.pid, "SIGKILL");
    }} catch {{}}
    proc.kill();
  }}

  // A handler's stderr, bounded like the sh wrapper's: the reason is a
  // sentence for a person, not a transcript. Past the bound the stream is
  // still drained, so a handler writing more never blocks on a full pipe.
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

  // One handler: null when it allowed, otherwise what it answered with. A
  // `transform` handler's stdout goes to `rewrite` when it allowed with
  // something to say.
  async function handler(entry, env, rewrite) {{
    const command = entry.command;
    const argv = !entry.argv ? [...SHELL, command] : entry.bun ? [process.execPath, ...entry.argv] : entry.argv;
    let proc;
    try {{
      proc = Bun.spawn(argv, {{
        cwd: env.PLUGIN_ROOT,
        detached: !WINDOWS,
        env: entry.bun ? {{ ...env, BUN_BE_BUN: "1" }} : env,
        stdin: "ignore",
        stdout: rewrite ? "pipe" : "ignore",
        stderr: "pipe",
      }});
    }} catch (error) {{
      return {{ failed: true, reason: `handler failed to start: ${{command}} — ${{error.message}}` }};
    }}
    // The author's deadline races the whole answer, not just the exit: a
    // process the handler started can hold stderr open long after the shell
    // it was started from has been stopped.
    let timer;
    const deadline = new Promise((resolve) => {{
      timer = setTimeout(() => resolve("expired"), entry.timeout * 1000);
    }});
    const stream = proc.stderr.getReader();
    const output = rewrite ? new Response(proc.stdout).text() : Promise.resolve("");
    const outcome = await Promise.race([Promise.all([collect(stream), proc.exited, output]), deadline]);
    clearTimeout(timer);
    if (outcome === "expired") {{
      stop(proc);
      stream.cancel().catch(() => {{}});
      return {{ failed: true, reason: `handler timed out after ${{entry.timeout}}s: ${{command}}` }};
    }}
    const [stderr, code, stdout] = outcome;
    if (code === {deny}) return {{ failed: false, reason: stderr || `${{command}} denied the operation` }};
    if (code !== 0) {{
      return {{ failed: true, reason: `handler failed (exit ${{code}}): ${{command}}${{stderr ? ` — ${{stderr}}` : ""}}` }};
    }}
    if (!rewrite || stdout.trim() === "") return null;
    if (new TextEncoder().encode(stdout).length > {output_limit}) {{
      return {{ failed: true, reason: `handler wrote more than {output_limit} bytes: ${{command}}` }};
    }}
    let rewritten;
    try {{
      rewritten = JSON.parse(stdout);
    }} catch {{}}
    if (rewritten === null || typeof rewritten !== "object" || Array.isArray(rewritten)) {{
      return {{ failed: true, reason: `handler did not write a JSON object: ${{command}}` }};
    }}
    rewrite(rewritten);
    return null;
  }}

  // A group's handlers in manifest order; the first denial stops the rest. A
  // failure denies for a closed group and is reported for the others.
  async function run(group, native, input, source) {{
    const env = environment(group, native, input, source);
    if (!env) {{
      if (closed(group.effect)) return OVERSIZED;
      report(`hooks:${{group.id}}`, OVERSIZED);
      return null;
    }}
    for (const entry of group.handlers) {{
      const answer = await handler(entry, env);
      if (answer === null) continue;
      if (answer.failed && !closed(group.effect)) {{
        report(`hooks:${{group.id}}`, answer.reason);
        continue;
      }}
      return answer.reason;
    }}
    return null;
  }}

  // Observing groups never decide: what they answer is only reported.
  async function observe(selected, native, input, source) {{
    for (const group of selected) {{
      const reason = await run(group, native, input, source);
      if (reason) report(`hooks:${{group.id}}`, reason);
    }}
  }}

  // The first of `selected` to deny, and why.
  async function decide(selected, native, input) {{
    for (const group of selected) {{
      const reason = await run(group, native, input);
      if (reason) return {{ group, reason }};
    }}
    return null;
  }}

  // A transform group's handlers in order, each reading the input the one
  // before it wrote; the first denial or failure stops it and closes the call.
  async function transform(group, native, input) {{
    let current = input;
    for (const entry of group.handlers) {{
      const env = environment(group, native, current);
      if (!env) return {{ reason: OVERSIZED }};
      const answer = await handler(entry, env, (rewritten) => {{
        current = rewritten;
      }});
      if (answer !== null) return {{ reason: answer.reason }};
    }}
    return {{ input: current }};
  }}

  // `execute.before` has no refusal of its own. OpenCode looks the tool up by
  // the name the event carries once the hook returns, so a name no tool has
  // fails the call before it runs, with an error that hands the model the
  // reason. A tool that never asks for permission (`read`) meets no other
  // refusal. A call reached through code mode was looked up before the hook,
  // and without an input it fails to decode instead of running.
  function refuse(event, reason) {{
    event.tool = `${{event.tool}} (refused by a hook: ${{reason}})`;
    event.input = undefined;
  }}

  // A tool call's name and input, kept from `execute.before` until the
  // permission check of the same call decides on it.
  const calls = new Map();
  // Sessions a subagent runs in: their turns are the parent's work, not a
  // person's session starting or stopping.
  const children = new Set();

  // What `execute.before` decides about one call: the input it runs with,
  // or the reason it is refused.
  async function before(call) {{
    if (unreadable.length) {{
      call.refused = `${{unreadable.join(", ")}} cannot be read, so no call runs until \`uze install\` writes it again`;
      return;
    }}
    // A rewrite is what the tool then runs, as OpenCode's own input repair
    // does it; a group that cannot rewrite closes the call, which the
    // observers still see.
    for (const group of rewriting("pre_tool_use", call.tool)) {{
      const result = await transform(group, call.tool, call.input);
      if (result.reason) {{
        call.refused = result.reason;
        break;
      }}
      call.input = result.input;
    }}
    await observe(observing("pre_tool_use", call.tool), call.tool, call.input);
    if (call.refused) return;
    const denied = await decide(denying("pre_tool_use", call.tool), call.tool, call.input);
    if (denied) {{
      call.refused = denied.reason;
      return;
    }}
    // An `ask` about a tool that never asks for permission would never reach
    // the person, and the call would run unasked, so it is refused.
    if (!PROMPTING.includes(call.tool)) {{
      call.asked = true;
      const asked = await decide(asking("pre_tool_use", call.tool), call.tool, call.input);
      if (asked) {{
        const unprompted = `OpenCode offers no permission prompt for \`${{call.tool}}\``;
        call.refused = `${{asked.reason}} (${{unprompted}}, so the call is refused rather than run unasked)`;
      }}
    }}
  }}

  // A new session is the only start OpenCode announces; a resumed one
  // publishes nothing, so `startup` is the only source. A denied stop keeps
  // the session going, told why.
  async function follow() {{
    for await (const event of ctx.event.subscribe()) {{
      const data = event.data ?? {{}};
      if (event.type === "session.created") {{
        if (data.parentID) children.add(data.sessionID);
        else await observe(select("session_start", "startup", () => true), undefined, undefined, "startup");
      }} else if (event.type === "session.execution.succeeded" && !children.has(data.sessionID)) {{
        await observe(observing("stop", undefined), undefined, undefined);
        const denied = await decide(deciding("stop", undefined), undefined, undefined);
        if (denied) await ctx.session.synthetic({{ sessionID: data.sessionID, text: denied.reason, resume: true }});
      }}
    }}
  }}

  load();
  await ctx.tool.hook("execute.before", async (event) => {{
    const call = {{ tool: event.tool, input: event.input }};
    calls.set(event.id, call);
    await before(call);
    event.input = call.input;
    if (!call.refused) return;
    // A refused call never reaches `execute.after`; only a tool that asks
    // still needs it, for its permission check to deny.
    refuse(event, call.refused);
    if (!PROMPTING.includes(call.tool)) calls.delete(event.id);
  }});
  // Where a tool asks for permission, an `ask` group asks here, and a call
  // `execute.before` refused stays refused should it run all the same.
  await ctx.permission.hook("evaluate", async (event) => {{
    if (event.source?.type !== "tool") return;
    const call = calls.get(event.source.id);
    if (!call || call.decided) return;
    call.decided = true;
    if (call.refused) {{
      calls.delete(event.source.id);
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
  // The tool already ran; what a denial can still do is tell the model.
  await ctx.tool.hook("execute.after", async (event) => {{
    calls.delete(event.id);
    await observe(observing("post_tool_use", event.tool), event.tool, event.input);
    const denied = await decide(deciding("post_tool_use", event.tool), event.tool, event.input);
    if (denied) await ctx.session.synthetic({{ sessionID: event.sessionID, text: denied.reason, resume: false }});
  }});
  follow().catch((error) => report("hooks", error));
  let watcher = null;
  try {{
    watcher = watch(DOCUMENTS, load);
  }} catch {{}}
  return () => watcher?.close();
}}"#
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
