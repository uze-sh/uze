//! OpenCode's rules section: the part of UZE's one plugin that asks the
//! `sh` rules engine before every model request and after every tool call,
//! and hands what it answers to the model.

use std::path::Path;

/// The file an earlier release wrote the rules plugin to, by itself, and the
/// first line that proves the file there is still that plugin.
pub(crate) const RETIRED_FILE: &str = "uze-rules.ts";
pub(crate) const HEADER: &str = "// Generated rules plugin — do not edit; it is written again.";

/// The engine is asked for at most this long; past it the section answers
/// nothing and the request or the tool result goes on unchanged.
const ENGINE_SECONDS: u16 = super::ENGINE_TIMEOUT_SECONDS;

/// The section for an engine at `engine`, recording sessions under
/// `sessions`: the runtime plugin's `async function rules(ctx)`,
/// every hook of which fails open — an answer it cannot get or read adds
/// nothing.
///
/// `context` takes the place of `SessionStart`: it runs before every model
/// request, so the always-on rules and the index are never lost to a
/// compaction. `execute.after` is `PostToolUse`, with the matched rules
/// appended to the result the model reads.
pub(crate) fn source(engine: &Path, sessions: &Path) -> String {
    // How this platform runs a script file, decided when the section is
    // generated: the same words the command-hook entries start it with.
    let (program, mut arguments) = uze_platform::shell::script(&engine.display().to_string());
    arguments.push(sessions.display().to_string());
    let argv = crate::opencode::runtime_plugin::literal(
        &serde_json::json!(
            std::iter::once(program)
                .chain(arguments)
                .collect::<Vec<_>>()
        ),
        1,
    );
    let engine = argv;
    let seconds = ENGINE_SECONDS;
    format!(
        r#"// A project's rules, from the `.agents/rules/` of the checkout a session
// works in, by asking the rules engine: the always-on rules and the index of
// on-demand ones before every model request, so a compaction never loses
// them, and the rules whose globs match a path a tool touched after the call.
// Whatever the engine cannot answer adds nothing; the request goes on.
async function rules(ctx) {{
  // The words that start the engine on this platform, with its sessions
  // directory.
  const ENGINE = {engine};
  const SECONDS = {seconds};
  // Where the session works: the location OpenCode set this plugin up for.
  // The events carry no directory, and a shared service runs in the home
  // directory whatever project it serves.
  const cwd = typeof ctx.location?.directory === "string" ? ctx.location.directory : "";

  // The engine's additional context for `payload`, or null.
  async function ask(payload) {{
    let proc;
    try {{
      proc = Bun.spawn(ENGINE, {{ stdin: "pipe", stdout: "pipe", stderr: "ignore" }});
      proc.stdin.write(JSON.stringify({{ ...payload, cwd }}));
      proc.stdin.end();
    }} catch {{
      return null;
    }}
    let timer;
    const deadline = new Promise((resolve) => {{
      timer = setTimeout(() => resolve(null), SECONDS * 1000);
    }});
    const text = await Promise.race([new Response(proc.stdout).text(), deadline]);
    clearTimeout(timer);
    if (text === null) {{
      try {{
        proc.kill();
      }} catch {{}}
      return null;
    }}
    try {{
      const context = JSON.parse(text)?.hookSpecificOutput?.additionalContext;
      return typeof context === "string" && context !== "" ? context : null;
    }} catch {{
      return null;
    }}
  }}

  // A hook that fails adds nothing, and never fails the request.
  const quietly = (hook) => async (event) => {{
    try {{
      await hook(event);
    }} catch {{}}
  }};

  await ctx.session.hook("context", quietly(async (event) => {{
    if (!Array.isArray(event.system)) return;
    const text = await ask({{ hook_event_name: "SessionStart", source: "context", session_id: event.sessionID ?? "" }});
    if (!text) return;
    const first = event.system[0];
    event.system.push(typeof first === "object" && first !== null ? {{ ...first, text }} : text);
  }}));
  await ctx.tool.hook("execute.after", quietly(async (event) => {{
    const result = event.result;
    if (!result || !Array.isArray(result.content)) return;
    if (result.status !== undefined && result.status !== "completed") return;
    const text = await ask({{
      hook_event_name: "PostToolUse",
      session_id: event.sessionID ?? "",
      tool_name: event.tool,
      tool_input: event.input ?? {{}},
    }});
    if (text) result.content.push({{ type: "text", text }});
  }}));
}}"#
    )
}
