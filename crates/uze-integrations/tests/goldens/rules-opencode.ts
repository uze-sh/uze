// A project's rules, from the `.agents/rules/` of the checkout a session
// works in, by asking the rules engine: the always-on rules and the index of
// on-demand ones before every model request, so a compaction never loses
// them, and the rules whose globs match a path a tool touched after the call.
// Whatever the engine cannot answer adds nothing; the request goes on.
async function rules(ctx) {
  // The words that start the engine on this platform, with its sessions
  // directory.
  const ENGINE = [
    "/machine/state/runtime/attachments/opencode/rules/engine",
    "/machine/state/runtime/rules/sessions",
  ];
  const SECONDS = 10;
  // Where the session works: the location OpenCode set this plugin up for.
  // The events carry no directory, and a shared service runs in the home
  // directory whatever project it serves.
  const cwd = typeof ctx.location?.directory === "string" ? ctx.location.directory : "";

  // The engine's additional context for `payload`, or null.
  async function ask(payload) {
    let proc;
    try {
      proc = Bun.spawn(ENGINE, { stdin: "pipe", stdout: "pipe", stderr: "ignore" });
      proc.stdin.write(JSON.stringify({ ...payload, cwd }));
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

  // A hook that fails adds nothing, and never fails the request.
  const quietly = (hook) => async (event) => {
    try {
      await hook(event);
    } catch {}
  };

  await ctx.session.hook("context", quietly(async (event) => {
    if (!Array.isArray(event.system)) return;
    const text = await ask({ hook_event_name: "SessionStart", source: "context", session_id: event.sessionID ?? "" });
    if (!text) return;
    const first = event.system[0];
    event.system.push(typeof first === "object" && first !== null ? { ...first, text } : text);
  }));
  await ctx.tool.hook("execute.after", quietly(async (event) => {
    const result = event.result;
    if (!result || !Array.isArray(result.content)) return;
    if (result.status !== undefined && result.status !== "completed") return;
    const text = await ask({
      hook_event_name: "PostToolUse",
      session_id: event.sessionID ?? "",
      tool_name: event.tool,
      tool_input: event.input ?? {},
    });
    if (text) result.content.push({ type: "text", text });
  }));
}