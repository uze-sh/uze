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
/// `sessions`: a JavaScript `async (ctx) => ...` for the runtime plugin,
/// every hook of which fails open — an answer it cannot get or read adds
/// nothing.
///
/// `context` takes the place of `SessionStart`: it runs before every model
/// request, so the always-on rules and the index are never lost to a
/// compaction. `execute.after` is `PostToolUse`, with the matched rules
/// appended to the result the model reads.
pub(crate) fn source(engine: &Path, sessions: &Path) -> String {
    let engine = serde_json::to_string(&engine.display().to_string()).expect("a path serializes");
    let sessions =
        serde_json::to_string(&sessions.display().to_string()).expect("a path serializes");
    format!(
        r#"async (ctx) => {{
    // Hands the agent the rules its project keeps in .agents/rules/ of the
    // checkout it is working in, by asking the rules engine: the always-on
    // rules and the index of on-demand ones before every model request, and
    // the rules whose globs match a path a tool touched after the call.
    const ENGINE = {engine};
    const SESSIONS = {sessions};
    const SECONDS = {ENGINE_SECONDS};
// Where the session works: the location of the instance OpenCode set this
// plugin up for, one per directory it serves. Neither the event nor the
// process says it: the events carry no directory, `ctx.worktree` is the
// worktree API rather than a path, and a shared service runs in the home
// directory whatever project it serves. Without a location, nothing.
function directory(ctx) {{
  const where = ctx?.location?.directory;
  return typeof where === "string" ? where : "";
}}

// The engine's additionalContext for `payload`, or null.
async function ask(payload) {{
  let proc;
  try {{
    proc = Bun.spawn(["sh", ENGINE, SESSIONS], {{
      stdin: "pipe",
      stdout: "pipe",
      stderr: "ignore",
    }});
    proc.stdin.write(JSON.stringify(payload));
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

    await ctx.session.hook("context", async (event) => {{
      try {{
        if (!Array.isArray(event.system)) return;
        const text = await ask({{
          hook_event_name: "SessionStart",
          source: "context",
          session_id: event.sessionID ?? "",
          cwd: directory(ctx),
        }});
        if (!text) return;
        const first = event.system[0];
        event.system.push(typeof first === "object" && first !== null ? {{ ...first, text }} : text);
      }} catch {{}}
    }});
    await ctx.tool.hook("execute.after", async (event) => {{
      try {{
        const result = event.result;
        if (!result || !Array.isArray(result.content)) return;
        if (result.status !== undefined && result.status !== "completed") return;
        const text = await ask({{
          hook_event_name: "PostToolUse",
          session_id: event.sessionID ?? "",
          cwd: directory(ctx),
          tool_name: event.tool,
          tool_input: event.input ?? {{}},
        }});
        if (!text) return;
        result.content.push({{ type: "text", text }});
      }} catch {{}}
    }});
  }}"#
    )
}
