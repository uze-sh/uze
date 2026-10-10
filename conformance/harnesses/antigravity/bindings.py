"""How Antigravity CLI is driven. No assertions live here."""

import json
import shlex
import time

from contract import continuity
from contract.bindings import Bindings, hook_prelude
from contract.tui import Tui
from shared.common import docker_base

from .scenarios import PERMISSION_PROMPTS, PROMPT_MARKER, agy_setup, answer_first_run


class AntigravityBindings(Bindings):
    harness = "antigravity"
    display_name = "Antigravity"
    #: UZE installs this harness's launcher under the name people type.
    launcher = "agy"
    launch = "exec agy"
    ready_markers = ("Antigravity CLI",)
    warmup = 3.0
    #: The hint under agy's input line; no dialog draws it (1.3.2).
    input_markers = (PROMPT_MARKER,)
    #: An interrupt, then this harness's own exit verb when that was not
    #: enough — `agy` treats a lone interrupt as "clear the line".
    exit_keys = ("\x03", "/exit\r")

    def session(self, cfg, prov_ip):
        setup = agy_setup(cfg, prov_ip, include_mcp=True, final_cmd=self.launch)
        return Tui(cfg, docker_base(cfg, prov_ip, setup), "antigravity-contract")

    def session_in(self, cfg, prov_ip, cwd, prelude):
        setup = agy_setup(
            cfg,
            prov_ip,
            include_mcp=False,
            final_cmd=self.launch,
            prelude=f"{prelude}\ncd {cwd}",
        )
        return Tui(cfg, docker_base(cfg, prov_ip, setup), "antigravity-isolation")

    def relaunch_in(self, cfg, prov_ip, cwd, prelude):
        """Two launches in one terminal, back to back."""
        setup = agy_setup(
            cfg,
            prov_ip,
            include_mcp=False,
            final_cmd=continuity.relaunch_command(self.launcher_name()),
            prelude=f"{prelude}\ncd {cwd}",
        )
        return Tui(cfg, docker_base(cfg, prov_ip, setup), "antigravity-continuity")

    def prepare(self, tui):
        """agy opens on a colour-scheme picker and a terms screen; the prompt
        exists only after both are answered."""
        plain = answer_first_run(tui.child, tui.screen)
        if plain is None:
            return "onboarding never appeared", None
        tui.snapshot("ready", plain)
        return plain, "Antigravity CLI" in plain and ">" in plain

    def skill_catalog(self, tui):
        """`/skills` lists every Skill a person can invoke. The leading `/`
        is sent alone: typing it with the rest loses the palette trigger."""
        self.await_input(tui)
        tui.child.send("/")
        time.sleep(1.2)
        tui.type("skills", per_char=0.15)
        time.sleep(1.2)
        tui.submit()
        catalog, _ = tui.until(["flow:review"], tries=4)
        return catalog

    def lists(self, catalog, skill):
        """Antigravity names a Skill by its namespaced invocation label."""
        return f"flow:{skill}" in catalog.replace(" ", "")

    def invoke(self, tui, skill):
        """Antigravity invokes a Skill as a slash command on its label."""
        tui.type(f"/flow:{skill}")
        time.sleep(1.2)
        tui.submit()
        time.sleep(1.0)
        tui.submit()
        return tui.collect(reads=10)

    def headless(
        self, cfg, prov_ip, prelude, prompt, cwd, plugins="", delegating=False
    ):
        """`agy --print` as a person runs it, on the default agent: whether
        the default agent can delegate to what UZE delivered is the agent
        contract's to measure, not the Lab's to arrange. Since 1.3 it is
        offered `invoke_subagent` once the session's experiment flags enable
        it (`LIST_EXPERIMENTS` in the provider)."""
        setup = agy_setup(
            cfg,
            prov_ip,
            include_mcp=False,
            plugins=plugins,
            final_cmd=f"""{prelude}
cd {cwd}
set +e
# decision: headless-permissions
timeout 240 agy --print {shlex.quote(prompt)} --dangerously-skip-permissions \\
  --print-timeout 120s 2>&1
""",
        )
        return docker_base(cfg, prov_ip, setup, tty=False)

    #: What agy puts in front of a person during a turn: a permission menu
    #: per kind of action ("Run this command?", "Allow creation of this
    #: file?" on 1.2.17), each with "Yes" first and the same navigation
    #: line under it, approved with Enter; and its feedback survey,
    #: dismissed with `0`. The navigation line is what every menu shares —
    #: the "Requesting permission for:" header heads only some of them.
    approval_prompts = (*PERMISSION_PROMPTS, "How's the CLI experience")

    def approve(self, tui, prompt):
        if prompt == "How's the CLI experience":
            tui.type("0")
        tui.submit()

    def hook_session(self, cfg, prov_ip, plugin, tag, before=""):
        setup = agy_setup(
            cfg,
            prov_ip,
            include_mcp=False,
            final_cmd=self.launch,
            plugins=plugin,
            prelude=f"{hook_prelude(self.hook_project)}\n{before}\ncd {self.hook_project}",
        )
        return Tui(cfg, docker_base(cfg, prov_ip, setup), f"antigravity-hooks-{tag}")

    def sequence(self, calls, trigger):
        """Antigravity's provider scripts `{"name", "args"}` steps, with
        arguments as the object a Gemini `functionCall` carries."""
        steps = [{"name": c["tool"], "args": c["args"]} for c in calls]
        return "toolcall", {"TOOL_SEQUENCE": json.dumps(steps)}

    def unsupported(self, prop):
        """An agent's model is not Antigravity's to choose here: its shipped
        docs (1.2.12) describe no agent field, and an agent carrying `model`
        is dropped silently, with an id from its own catalogue as much as
        without one. UZE leaves `harness.antigravity.model` out, so the agent
        still arrives, on the session's model."""
        if prop == "agent-vendor-fields-block-model":
            return (
                "Antigravity drops an agent that carries `model` (measured on "
                "1.2.12), so UZE leaves it out and the agent runs on the "
                "session's model"
            )
        return None

    def dispatch(self, label, prompt):
        """agy dispatches with `invoke_subagent`, naming the agent in a
        `Subagents[].TypeName`; the schema also requires a role, a prompt
        and the tool summary/action every agy tool carries."""
        args = {
            "Subagents": [
                {
                    "TypeName": label,
                    "Role": "Lab Dispatch",
                    "Prompt": "Run your checks.",
                }
            ],
            "toolSummary": "Lab dispatch",
            "toolAction": "Dispatching agent",
        }
        return "toolcall", {"TOOL_NAME": "invoke_subagent", "FC_ARGS": json.dumps(args)}

    def mcp_inventory(self, tui):
        """`/mcp` lists every configured server and enumerates its tools."""
        tui.child.send("/")
        time.sleep(1.2)
        tui.type("mcp", per_char=0.15)
        time.sleep(1.2)
        tui.submit()
        inventory, _ = tui.until(["Tools: uze_conformance", "uze-conformance"], tries=8)
        return inventory
