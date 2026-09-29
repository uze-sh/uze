"""How Antigravity CLI is driven. No assertions live here."""

import json
import shlex
import time

from contract import continuity
from contract.bindings import Bindings
from contract.tui import Tui
from shared.common import docker_base

from .scenarios import agy_setup


class AntigravityBindings(Bindings):
    harness = "antigravity"
    #: UZE installs this harness's launcher under the name people type.
    launcher = "agy"
    launch = "exec agy"
    ready_markers = ("Antigravity CLI",)
    warmup = 3.0
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
        try:
            tui.child.expect("Choose your color scheme", timeout=150)
        except Exception as error:
            return f"onboarding never appeared: {error}", None
        tui.child.send("\r")
        time.sleep(3)
        tui.child.send("\t\t")
        time.sleep(0.7)
        tui.child.send("\r")
        time.sleep(5)
        _, plain = tui.screen(3)
        # A directory the harness has not seen before — a linked worktree,
        # for one — adds a folder-trust dialog after the terms, with "Yes,
        # I trust this folder" preselected. Text typed while it is up goes
        # to the dialog, never to the prompt.
        if "trust the contents" in plain:
            tui.child.send("\r")
            time.sleep(5)
            _, plain = tui.screen(3)
        tui.snapshot("ready", plain)
        return plain, "Antigravity CLI" in plain and ">" in plain

    def skill_catalog(self, tui):
        """`/skills` lists every Skill a person can invoke. The leading `/`
        is sent alone: typing it with the rest loses the palette trigger."""
        time.sleep(self.warmup)
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

    #: The agent a delegating turn runs as. agy offers `invoke_subagent` and
    #: the roster of agents only to an agent that lists the tool — the
    #: default one gets neither in a headless turn (measured on 1.2.12,
    #: `experiments/antigravity/agents`). A driver, not a fixture: it is the
    #: seat the model delegates from, and names nothing UZE delivers.
    DISPATCHER = "lab-dispatcher"
    DISPATCHER_PRELUDE = f"""
mkdir -p /work/home/.gemini/antigravity-cli/agents
cat > /work/home/.gemini/antigravity-cli/agents/{DISPATCHER}.md <<'AGENT_EOF'
---
name: {DISPATCHER}
description: Delegates the lab's work to subagents
tools:
  - invoke_subagent
---
Delegate as asked.
AGENT_EOF
"""

    def headless(
        self, cfg, prov_ip, prelude, prompt, cwd, plugins="", delegating=False
    ):
        """`agy --print`, as the dispatcher when the turn delegates."""
        agent = f"--agent {self.DISPATCHER} " if delegating else ""
        setup = agy_setup(
            cfg,
            prov_ip,
            include_mcp=False,
            plugins=plugins,
            prelude=self.DISPATCHER_PRELUDE if delegating else "",
            final_cmd=f"""{prelude}
cd {cwd}
set +e
timeout 240 agy {agent}--print {shlex.quote(prompt)} --dangerously-skip-permissions \\
  --print-timeout 120s 2>&1
""",
        )
        return docker_base(cfg, prov_ip, setup, tty=False)

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
