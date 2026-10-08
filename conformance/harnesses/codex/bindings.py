"""How Codex is driven. No assertions live here."""

import json
import shlex
import subprocess
import time

from contract import continuity
from contract.bindings import Bindings, hook_prelude
from contract.tui import Tui

from .scenarios import codex_container, drive_onboarding


class CodexBindings(Bindings):
    harness = "codex"
    display_name = "Codex"
    launch = "exec codex"
    #: The real prompt, not the splash. A marker loose enough to match
    #: onboarding passes every check against a screen that accepts no
    #: input — which it did, once, here.
    ready_markers = ("Ask Codex to do anything",)
    #: One interrupt. Since codex-cli 0.157 an interactive session is served
    #: by the shared app-server daemon (openai/codex#47179), and leaving it
    #: only disconnects — "Any running work continues" — so there is no
    #: second-interrupt offer to wait for. A second key would land on the
    #: shell once the TUI has handed the terminal back, and end it before
    #: the next launch.
    exit_keys = ("\x03",)
    warmup = 6.0

    def session(self, cfg, prov_ip):
        return Tui(cfg, codex_container(cfg, prov_ip, self.launch), "codex-contract")

    def session_in(self, cfg, prov_ip, cwd, prelude):
        final = f"{prelude}\ncd {cwd} && {self.launch}"
        return Tui(cfg, codex_container(cfg, prov_ip, final), "codex-isolation")

    def relaunch_in(self, cfg, prov_ip, cwd, prelude):
        """Two launches in one terminal, back to back."""
        relaunch = continuity.relaunch_command(self.launcher_name())
        final = f"{prelude}\ncd {cwd} && {relaunch}"
        return Tui(cfg, codex_container(cfg, prov_ip, final), "codex-continuity")

    #: The review Codex shows for delivered hooks: "1. Review hooks  2.
    #: Trust all and continue  3. Continue without trusting (hooks won't
    #: run)". Until a person answers it, no hook UZE delivered runs. In
    #: 0.161 it may arrive only after the first turn starts.
    HOOK_REVIEW = "Trust all and continue"

    #: The hook review is an approval in the turn as well as during startup:
    #: Codex 0.161 defers it until the first tool turn. It needs a different
    #: response than ordinary approvals, handled by `approve` below.
    approval_prompts = (
        "Would you like to run",
        "Allow command",
        "MCP server to run tool",
        "Would you like to make the following edits",
        HOOK_REVIEW,
    )

    def prepare(self, tui):
        """Codex opens on an onboarding flow, and — when a package delivered
        hooks — on their review; the prompt only accepts input once both are
        answered, the review the way a person trusts what they installed."""
        _, plain = drive_onboarding(tui.child)
        reviewed = self.HOOK_REVIEW.replace(" ", "") in plain.replace(" ", "")
        if not reviewed and getattr(tui, "expects_hook_review", False):
            _, _, reviewed = tui.wait_for(
                [self.HOOK_REVIEW], tries=6, squash_spaces=True
            )
            reviewed = reviewed == self.HOOK_REVIEW
        if reviewed:
            # Once: the screen a later read returns can still hold the
            # menu's text, and answering it again types into the prompt.
            tui.child.send("2")
            time.sleep(0.5)
            tui.submit()
            plain, _ = tui.until(self.ready_markers, tries=6)
        tui.snapshot("ready", plain)
        return plain, "Ask Codex" in plain

    def skill_catalog(self, tui):
        """`/skills` opens a menu; option 2 is the Enable/Disable list, which
        is the surface that names every Skill a person can invoke."""
        time.sleep(self.warmup)
        tui.type("/skills")
        time.sleep(1)
        tui.submit()
        tui.until(["Choose an action", "skills"])
        tui.child.send("2")
        catalog, _ = tui.until(["Enable/Disable"])
        return catalog

    def lists(self, catalog, skill):
        """Codex names a plugin Skill `<skill> (<plugin>)` in this list, and
        an individually attached one by its namespaced label."""
        squeezed = catalog.replace(" ", "")
        return f"{skill}(flow)" in squeezed or f"flow:{skill}" in squeezed

    def invoke(self, tui, skill):
        """Codex invokes a Skill with `$<label>`."""
        tui.type(f"$flow:{skill}")
        time.sleep(1.2)
        tui.submit()
        time.sleep(1.0)
        tui.submit()
        return tui.collect(reads=10)

    def mcp_inventory(self, tui):
        """`/mcp` lists every configured server."""
        tui.type("/mcp")
        time.sleep(1)
        tui.submit()
        inventory, _ = tui.until(["uze-conformance", "MCP"], tries=8)
        return inventory

    def headless(
        self, cfg, prov_ip, prelude, prompt, cwd, plugins="", delegating=False
    ):
        """`codex exec`: every session's `spawn_agent` tool lists the custom
        agents in its `agent_type` parameter, so `delegating` needs nothing
        more."""
        final = f"""{prelude}
cd {cwd}
set +e
timeout 240 codex exec {shlex.quote(prompt)} 2>&1
"""
        return codex_container(cfg, prov_ip, final, plugins=plugins, tty=False)

    def dispatch(self, label, prompt):
        """Codex 0.158 dispatches with `spawn_agent` in its `collaboration`
        tool namespace, naming the agent in `agent_type`; the call has to
        carry the namespace or it is answered `unsupported call`. `codex
        exec` ends — and the child with it — as soon as the root turn
        answers, so the root waits for the child first (measured,
        `experiments/codex/agents`)."""
        sequence = [
            {
                "name": "spawn_agent",
                "namespace": "collaboration",
                "args": {
                    "task_name": "lab_dispatch",
                    "message": "Run your checks.",
                    "agent_type": label,
                    "fork_turns": "none",
                },
            },
            {
                "name": "wait_agent",
                "namespace": "collaboration",
                "args": {"timeout_ms": 30000},
            },
        ]
        return "toolcall", {
            "TOOL_SEQUENCE": json.dumps(sequence),
            "TOOL_TRIGGER": prompt,
        }

    #: Codex asks before a command leaves its sandbox; answered on screen,
    #: the way a person answers it. The hook review can be in `prepare` or
    #: arrive with the first turn.
    #: What Codex asks before a call, each answered with Enter on its
    #: preselected "Allow": a command's approval, an MCP tool's ("Allow the
    #: <server> MCP server to run tool …?", 0.160.1), and an edit's when its
    #: sandbox could not apply it ("Would you like to make the following
    #: edits? … retry without sandbox?"), which a CI runner whose kernel
    #: refuses the sandbox raises and a workstation never does.
    def approve(self, tui, prompt):
        if prompt == self.HOOK_REVIEW:
            tui.child.send("2")
            time.sleep(0.5)
        tui.submit()

    def mcp_calls(self):
        """The model's default mode is code mode, where an MCP server's tool
        is a deferred nested tool: left out of `exec`'s description, but on
        the global `tools` object and listed in `ALL_TOOLS` (0.160.1,
        `experiments/codex/mcp-offer`). A model finds it there and calls it
        inside `exec`, as this script does."""
        script = (
            'const found = ALL_TOOLS.find((tool) => tool.name.includes("uze_conformance"));\n'
            'if (!found) { text("no uze_conformance in ALL_TOOLS"); exit(); }\n'
            "const result = await tools[found.name]({});\n"
            "text(JSON.stringify(result));"
        )
        return [{"tool": "functions.exec", "args": script}]

    #: Codex's provider answers the request after a scripted sequence with
    #: its canned turn text.
    final_markers = ("UZE_CONFORMANCE_OK",)

    def hook_review_recorded(self, cfg):
        """Codex records a person's trust as `[hooks.state."<key>"]` tables
        in `~/.codex/config.toml` (measured, `experiments/codex/trust-store`)."""
        config = subprocess.run(
            [
                "docker",
                "exec",
                cfg.harness_container,
                "cat",
                "/work/home/.codex/config.toml",
            ],
            capture_output=True,
            text=True,
            errors="replace",
        ).stdout
        return "[hooks.state." in config

    def hook_session(self, cfg, prov_ip, plugin, tag, before=""):
        final = f"{hook_prelude(self.hook_project)}\n{before}\ncd {self.hook_project} && {self.launch}"
        tui = Tui(
            cfg,
            codex_container(cfg, prov_ip, final, plugins=plugin),
            f"codex-hooks-{tag}",
        )
        tui.expects_hook_review = True
        return tui

    def sequence(self, calls, trigger):
        """Codex's provider scripts `{"name", "namespace", "args"}` steps,
        with arguments as the JSON string the Responses API carries. A
        namespaced tool is declared as `<namespace>.<name>`
        (`capture.declared_tools`). A call whose input is text is a freeform
        tool's — code mode's `exec` — and is scripted as one."""
        steps = []
        for c in calls:
            namespace, _, name = c["tool"].rpartition(".")
            custom = isinstance(c["args"], str)
            args = c["args"] if custom else json.dumps(c["args"])
            steps.append(
                {"name": name, "namespace": namespace, "args": args, "custom": custom}
            )
        return "toolcall", {"TOOL_SEQUENCE": json.dumps(steps), "TOOL_TRIGGER": trigger}

    def unsupported(self, prop):
        """Codex documents no way to disable explicit `$skill` invocation, so
        a canonical `user: false` cannot be enforced here.

        The product already says this rather than inventing it: the exposure
        plan routes model-only as `Degraded`. Declaring it here keeps the
        contract asking every harness the same question and getting an
        honest answer, instead of the check quietly not existing in this
        vertical — which is how the previous suite hid divergence.
        """
        if prop in (
            "model-only-is-not-user-invocable",
            # The same limitation, now measured rather than read: the
            # invocation check typed `$flow:analyze` and its body reached
            # the model. Codex's own documentation says as much of the one
            # control it has — with
            # `agents/openai.yaml` `policy.allow_implicit_invocation:
            # false`, "explicit `$skill` invocation still works".
            "model-only-is-not-invocable",
        ):
            return (
                "Codex has no documented way to disable explicit `$skill` "
                "invocation — its own docs say explicit invocation still "
                "works with allow_implicit_invocation: false; the product "
                "routes this as Degraded"
            )
        return None
