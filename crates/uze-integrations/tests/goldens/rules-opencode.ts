// Generated rules plugin — do not edit; it is written again.
// Hands the agent the rules its project keeps in .agents/rules/ of the
// checkout it is working in, by asking the rules engine: the always-on
// rules and the index of on-demand ones before every model request, and the
// rules whose globs match a path a tool touched after the call. Nothing it
// cannot get or read is ever an error; the request goes on unchanged.

const ENGINE = "/machine/state/runtime/attachments/opencode/rules/engine";
const SESSIONS = "/machine/state/runtime/rules/sessions";
const SECONDS = 10;

// Where the session works: the location of the instance OpenCode set this
// plugin up for, one per directory it serves. Neither the event nor the
// process says it: the events carry no directory, `ctx.worktree` is the
// worktree API rather than a path, and a shared service runs in the home
// directory whatever project it serves. Without a location, nothing.
function directory(ctx) {
  const where = ctx?.location?.directory;
  return typeof where === "string" ? where : "";
}

// The engine's additionalContext for `payload`, or null.
async function ask(payload) {
  let proc;
  try {
    proc = Bun.spawn(["sh", ENGINE, SESSIONS], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "ignore",
    });
    proc.stdin.write(JSON.stringify(payload));
    proc.stdin.end();
  } catch {
    return null;
  }
  let timer;
  const deadline = new Promise((resolve) => {
    timer = setTimeout(() => resolve(null), SECONDS * 1000);
  });
  const text = await Promise.race([new Response(proc.stdout).text(), deadline]);
  clearTimeout(timer);
  if (text === null) {
    try {
      proc.kill();
    } catch {}
    return null;
  }
  try {
    const context = JSON.parse(text)?.hookSpecificOutput?.additionalContext;
    return typeof context === "string" && context !== "" ? context : null;
  } catch {
    return null;
  }
}

export default {
  id: "agents-rules",
  async setup(ctx) {
    await ctx.session.hook("context", async (event) => {
      try {
        if (!Array.isArray(event.system)) return;
        const text = await ask({
          hook_event_name: "SessionStart",
          source: "context",
          session_id: event.sessionID ?? "",
          cwd: directory(ctx),
        });
        if (!text) return;
        const first = event.system[0];
        event.system.push(typeof first === "object" && first !== null ? { ...first, text } : text);
      } catch {}
    });
    await ctx.tool.hook("execute.after", async (event) => {
      try {
        const result = event.result;
        if (!result || !Array.isArray(result.content)) return;
        if (result.status !== undefined && result.status !== "completed") return;
        const text = await ask({
          hook_event_name: "PostToolUse",
          session_id: event.sessionID ?? "",
          cwd: directory(ctx),
          tool_name: event.tool,
          tool_input: event.input ?? {},
        });
        if (!text) return;
        result.content.push({ type: "text", text });
      } catch {}
    });
  },
};
