// Generated rules plugin — do not edit; it is written again.
// Hands the agent the rules its project keeps in .agents/rules/ of the
// checkout it is working in, by asking the rules engine: the always-on
// rules and the index of on-demand ones before every model request, and the
// rules whose globs match a path a tool touched after the call. Nothing it
// cannot get or read is ever an error; the request goes on unchanged.

const ENGINE = "/machine/state/runtime/attachments/opencode/rules/engine";
const SESSIONS = "/machine/state/runtime/rules/sessions";
const SECONDS = 10;

// Where the session works. The event names it where OpenCode says; the
// plugin context otherwise; the server's own directory last.
function directory(ctx, event) {
  return event?.directory ?? event?.cwd ?? ctx?.directory ?? ctx?.worktree ?? process.cwd();
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
          cwd: directory(ctx, event),
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
          cwd: directory(ctx, event),
          tool_name: event.tool,
          tool_input: event.input ?? {},
        });
        if (!text) return;
        result.content.push({ type: "text", text });
      } catch {}
    });
  },
};
