"""How Claude Code is driven. No assertions live here."""

import json
import shlex
import time

from contract import continuity
from contract.bindings import Bindings, hook_prelude
from contract.tui import Tui

from .scenarios import claude_container, drive_onboarding


class ClaudeBindings(Bindings):
    harness = "claude"
    display_name = "Claude Code"
    launch = "exec claude"
    ready_markers = ("Opus", "API Usage Billing", "❯")
    warmup = 6.0

    def session(self, cfg, prov_ip):
        return Tui(cfg, claude_container(cfg, prov_ip, self.launch), "claude-contract")

    def session_in(self, cfg, prov_ip, cwd, prelude):
        final = f"{prelude}\ncd {cwd} && {self.launch}"
        return Tui(cfg, claude_container(cfg, prov_ip, final), "claude-isolation")

    def relaunch_in(self, cfg, prov_ip, cwd, prelude):
        """Two launches in one terminal, back to back."""
        relaunch = continuity.relaunch_command(self.launcher_name())
        final = f"{prelude}\ncd {cwd} && {relaunch}"
        return Tui(cfg, claude_container(cfg, prov_ip, final), "claude-continuity")

    def prepare(self, tui):
        """Claude opens on a chain of first-run dialogs — welcome, security
        guide, API key, theme, folder trust — before the prompt exists."""
        _, plain, _ = drive_onboarding(tui.child)
        if not plain or not any(m in plain for m in self.ready_markers):
            plain, _ = tui.until(self.ready_markers)
        tui.snapshot("ready", plain)
        return plain, any(marker in plain for marker in self.ready_markers)

    def skill_catalog(self, tui):
        """The `/` menu is the surface a person invokes from: typing a
        namespace prefix opens its completions. `/skills` is the management
        view and lists every Skill whatever its policy, so it cannot tell a
        model-only Skill from an invocable one."""
        time.sleep(self.warmup)
        tui.type("/flow:")
        catalog = tui.collect(reads=4)
        tui.child.send("\x1b")
        time.sleep(0.5)
        return catalog

    def lists(self, catalog, skill):
        """Claude names a Skill by its namespaced invocation label."""
        return f"flow:{skill}" in catalog.replace(" ", "")

    def invoke(self, tui, skill):
        """Claude invokes a Skill as a slash command on its namespaced label.

        The label is typed in full and submitted twice: the first Enter is
        eaten by the completion popup that opens while typing, the second
        sends the line. A harness that needed only one gets an empty second
        submit, which is inert.
        """
        tui.type(f"/flow:{skill}")
        time.sleep(1.2)
        tui.submit()
        time.sleep(1.0)
        tui.submit()
        return tui.collect(reads=10)

    def mcp_inventory(self, tui):
        """`/mcp` lists every configured server and its connection state."""
        tui.type("/mcp")
        time.sleep(1)
        tui.submit()
        inventory, _ = tui.until(["uze-conformance", "MCP"], tries=8)
        return inventory

    def headless(
        self, cfg, prov_ip, prelude, prompt, cwd, plugins="", delegating=False
    ):
        """`claude -p`: every session's `Agent` tool lists the agent types
        it may dispatch, so `delegating` needs nothing more."""
        final = f"""{prelude}
cd {cwd}
set +e
# decision: headless-permissions
timeout 240 claude -p {shlex.quote(prompt)} --permission-mode bypassPermissions \\
  --output-format json 2>&1
"""
        return claude_container(cfg, prov_ip, final, plugins=plugins, tty=False)

    def dispatch(self, label, prompt):
        """Claude dispatches through its `Agent` tool, naming the agent in
        `subagent_type` (measured on 2.1.283, `experiments/claude/parity`)."""
        args = {
            "subagent_type": label,
            "description": "lab dispatch",
            "prompt": "Run your checks.",
        }
        return "toolcall", {
            "TOOL_NAME": "Agent",
            "TOOL_ARGS": json.dumps(args),
            "TOOL_TRIGGER": prompt,
        }

    #: What Claude puts in front of a person during a turn, each answered
    #: with Enter as a person does: a tool's approval ("Do you want to
    #: proceed?", "... create", "... make this edit"), whose highlighted
    #: option is "Yes"; and, since auto mode became the default permission
    #: mode (2.1.290), its notice about classifier billing ("Enter to
    #: continue"), which the Lab never saw while it ran with
    #: `--permission-mode bypassPermissions`.
    approval_prompts = ("Do you want to", "Enter to continue")

    def hook_session(self, cfg, prov_ip, plugin, tag, before=""):
        final = f"{hook_prelude(self.hook_project)}\n{before}\ncd {self.hook_project} && {self.launch}"
        return Tui(
            cfg,
            claude_container(cfg, prov_ip, final, plugins=plugin),
            f"claude-hooks-{tag}",
        )

    def sequence(self, calls, trigger):
        """Claude's provider scripts `{"name", "args"}` steps; the trigger
        keeps a dispatched subagent's own first request from starting the
        sequence again."""
        steps = [{"name": c["tool"], "args": c["args"]} for c in calls]
        return "toolcall", {"TOOL_SEQUENCE": json.dumps(steps), "TOOL_TRIGGER": trigger}

    #: Claude names a plugin's MCP tool `mcp__plugin_<plugin>_<server>__<tool>`
    #: (`/mcp` lists it as `plugin:uze-mcp-conformance:uze-conformance`) and
    #: defers it: the model loads it with `ToolSearch` before calling it
    #: (2.1.290).
    MCP_TOOL = "mcp__plugin_uze-mcp-conformance_uze-conformance__uze_conformance"

    def mcp_calls(self):
        return [
            {
                "tool": "ToolSearch",
                "args": {"query": f"select:{self.MCP_TOOL}", "max_results": 1},
            },
            {"tool": self.MCP_TOOL, "args": {}},
        ]

    def unsupported(self, prop):
        """Claude Code documents both halves of the invocation policy:
        `disable-model-invocation: true` and `user-invocable: false` (the
        latter hides a Skill from the `/` menu and refuses `/name`), and UZE
        emits both — nothing to declare there.

        What it does not offer is a way to show a hook's denial as a
        decision: 2.1.290 renders every `PreToolUse` denial as
        `PreToolUse:<tool> hook error: <reason>`, the documented
        `permissionDecision: deny` on stdout with exit 0 as much as exit 2,
        and from a hand-written hook with no UZE in it
        (`experiments/claude/deny-render`). UZE answers in the JSON dialect,
        which at least shows the reason without the handler's path."""
        if prop == "hooks.deny-rendered-as-decision":
            return (
                "Claude Code renders every PreToolUse denial as a hook error, "
                "the documented JSON decision included (measured with a "
                "hand-written hook, experiments/claude/deny-render)"
            )
        return None
