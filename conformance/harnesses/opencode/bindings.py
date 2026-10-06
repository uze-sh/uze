"""How OpenCode is driven. No assertions live here."""

import json
import shlex
import time

from contract import continuity
from contract.bindings import Bindings, hook_prelude
from contract.tui import Tui

from .scenarios import opencode_container


class OpenCodeBindings(Bindings):
    harness = "opencode"
    display_name = "OpenCode"
    #: What OpenCode V2 puts in front of a person before a call a plugin
    #: asks about ("△ Permission required", then "Allow once", "Always
    #: allow", "Reject"), answered with Enter on the preselected "Allow
    #: once" (2.0.24).
    approval_prompts = ("Permission required",)
    #: Started as its own binary, the way a person who only uses the
    #: package manager starts it: the plugins must reach it with no shim on
    #: `PATH`. What only the workspace's launch carries is the continuity
    #: contract's, which relaunches through the launcher on purpose.
    #: `--standalone` is decision `opencode-standalone`: this container has
    #: no background service for a TUI to attach to.
    launch = "exec opencode --standalone"
    ready_markers = ("Ask anything",)
    #: One interrupt ends it — measured (`experiments/relaunch_probe`). A
    #: second would not be spare: the process is already gone by then, so
    #: the shell has it, and the shell is starting the next process.
    exit_keys = ("\x03",)
    #: What a *resumed* session shows. `Ask anything` is the empty prompt's
    #: placeholder, and a session opened with earlier turns in it has no
    #: empty prompt to place a holder in — measured on beta-19192
    #: (`experiments/relaunch_probe`): the second process rendered its
    #: status bar and nothing else this vertical was looking for.
    rejoin_markers = ("Build", "UZE Conformance Model")
    #: The prompt renders long before the skill and MCP surfaces finish
    #: loading, and input typed into that window is dropped. Measured, not
    #: guessed: 25s is what a working manual probe needed.
    warmup = 25.0

    def session(self, cfg, prov_ip):
        return Tui(
            cfg, opencode_container(cfg, prov_ip, self.launch), "opencode-contract"
        )

    def session_in(self, cfg, prov_ip, cwd, prelude):
        final = f"{prelude}\ncd {cwd} && {self.launch}"
        return Tui(cfg, opencode_container(cfg, prov_ip, final), "opencode-isolation")

    #: The `opencode` this scene's launches resolve to.
    #:
    #: Every other scene here launches `opencode --standalone`, because
    #: this container has no background service for a TUI to attach to.
    #: This one cannot: an argument on the command line is an argument the
    #: *caller* composed, and that is precisely the case where UZE
    #: contributes nothing — measured, the launch went bare and no
    #: conversation was ever recorded. So the flag goes behind the
    #: launcher rather than in front of it, where it is the container's
    #: business and the launch stays bare.
    #:
    #: A subcommand is passed through untouched: UZE reads this harness's
    #: session listing with `api GET /api/session`, and that one does want
    #: the service every other process is talking to.
    STANDALONE_WRAPPER = """
mkdir -p /tmp/lab-bin
cat > /tmp/lab-bin/opencode <<'WRAP_EOF'
#!/bin/sh
real=$(command -v opencode2)
case "$1" in
  ""|-*) exec "$real" --standalone "$@" ;;
  *) exec "$real" "$@" ;;
esac
WRAP_EOF
chmod +x /tmp/lab-bin/opencode
export PATH=/tmp/lab-bin:$PATH
"""

    def relaunch_in(self, cfg, prov_ip, cwd, prelude):
        """Two launches in one terminal, back to back, through the scene's
        own launcher rather than the image's — the task record this contract
        writes lives under the run's `UZE_HOME`, and the launcher has to read
        the same one."""
        relaunch = continuity.relaunch_command(self.launcher_name())
        final = f"{prelude}\n{self.STANDALONE_WRAPPER}\ncd {cwd} && {relaunch}"
        return Tui(cfg, opencode_container(cfg, prov_ip, final), "opencode-continuity")

    def skill_catalog(self, tui):
        time.sleep(self.warmup)
        tui.type("/skills")
        time.sleep(1)
        tui.submit()
        # This surface renders by region, so a name arrives split across
        # repaint frames; accumulating is the only way to see it whole.
        return tui.collect(reads=8)

    def lists(self, catalog, skill):
        """OpenCode names a Skill by its qualified invocation label."""
        return f"flow:{skill}" in catalog.replace(" ", "")

    def invoke(self, tui, skill):
        """OpenCode V2 invokes a Skill as a **mention**, not a slash command.

        Measured on beta-19192: the picker renders skills as `"@" + id` and
        selects them as `{type: "skill", value: {id, mention}}`, and the
        prompt payload carries `skills` as mentions beside `files` and
        `agents`. Typing `/flow:commit` here would prove nothing about this
        harness, which is exactly the confusion a listing-only check let
        stand.
        """
        tui.type(f"@flow:{skill}")
        time.sleep(1.2)
        tui.submit()
        time.sleep(1.0)
        tui.submit()
        return tui.collect(reads=10)

    def mcp_inventory(self, tui):
        """`/mcps` — plural here — opens the MCP toggle surface.

        The warmup applies to every surface, not just the first: each
        contract opens its own session, so each pays the same wait before
        the surfaces behind the prompt have loaded.
        """
        time.sleep(self.warmup)
        tui.type("/mcps")
        time.sleep(1)
        tui.submit()
        return tui.collect(reads=6)

    def headless(
        self, cfg, prov_ip, prelude, prompt, cwd, plugins="", delegating=False
    ):
        """`opencode run`: every session's `subagent` tool lists the agents
        whose mode lets them be one, so `delegating` needs nothing more."""
        final = f"""{prelude}
cd {cwd}
set +e
timeout 240 opencode run {shlex.quote(prompt)} 2>&1
"""
        return opencode_container(cfg, prov_ip, final, plugins=plugins, tty=False)

    def dispatch(self, label, prompt):
        """OpenCode 2.0.18 dispatches with its `subagent` tool (not `task`),
        naming the agent in `agent` (measured, `experiments/opencode/agents`)."""
        args = {
            "agent": label,
            "description": "lab dispatch",
            "prompt": "Run your checks.",
        }
        return "toolcall", {
            "TOOL_NAME": "subagent",
            "TOOL_ARGS": json.dumps(args),
            "TOOL_TRIGGER": prompt,
        }

    def hook_session(self, cfg, prov_ip, plugin, tag, before=""):
        final = f"{hook_prelude(self.hook_project)}\n{before}\ncd {self.hook_project} && {self.launch}"
        return Tui(
            cfg,
            opencode_container(cfg, prov_ip, final, plugins=plugin),
            f"opencode-hooks-{tag}",
        )

    def sequence(self, calls, trigger):
        """OpenCode's provider scripts `{"name", "args"}` steps, with
        arguments as the JSON string chat completions carry."""
        steps = [{"name": c["tool"], "args": json.dumps(c["args"])} for c in calls]
        return "toolcall", {"TOOL_SEQUENCE": json.dumps(steps), "TOOL_TRIGGER": trigger}

    def unsupported(self, prop):
        """What OpenCode V2 cannot express, each re-measured by the run.

        V2 removed `slash` from the skill contract (anomalyco/opencode
        199aabe9e, 2026-09-13; the docs dropped it in 3ddb0cb1d), leaving
        `autoinvoke` as the only control a Skill carries. Nothing hides a
        Skill from the person: the `/skills` browser lists every delivered
        one and a mention (`@id`) expands any of them. Both declarations
        below are measured on that, not on the `slash` behaviour the
        previous reason described.
        """
        if prop == "hooks.ask-shows-reason":
            return (
                "OpenCode V2 asks the person, but its permission prompt shows the "
                "call and never the request's message (tui/src/routes/session/"
                "permission.tsx), so the handler's reason is not on screen"
            )
        if prop == "context-project-agent-reaches-model":
            # Measured, `experiments/opencode/project-agents`: the project
            # roots are inside the checkout, `OPENCODE_CONFIG_DIR` replaces
            # the user's configuration, and the additive variables reach
            # only the server a launch starts, which by default is the
            # shared service every project is served from.
            return (
                "OpenCode 2.0.18 reads no `.agents/agents`, and a launch cannot "
                "hand it one project's agents: its extra-configuration "
                "environment reaches only the background service a launch "
                "starts, which then serves them to every other project"
            )
        if prop == "model-only-is-not-user-invocable":
            return (
                "OpenCode V2 defines no field that hides a Skill from the "
                "person (`slash` was removed): its `/skills` browser lists "
                "every delivered Skill"
            )
        if prop == "model-only-is-not-invocable":
            return (
                "OpenCode V2 expands any Skill a person mentions (`@id`), "
                "whatever its policy: no field it defines withholds one"
            )
        return None

    def names_server(self, inventory, server):
        """OpenCode's `/mcps` surface is a toggle list showing connection
        state; it was not observed to print the server id.

        So presence is read from the connected row rather than the name. A
        weaker signal than an id, and recorded as such in
        `conformance/DECISIONS.md` — the surface is the vendor's, and
        asserting an id it does not render would be asserting fiction.
        """
        squeezed = inventory.replace(" ", "")
        return "Connected" in inventory or "disconnectspace" in squeezed
