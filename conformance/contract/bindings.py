"""How one harness is driven, and what it cannot express.

A binding answers mechanics — the command that launches this TUI, the text
that proves it is ready, how a person invokes a Skill here. It never
asserts: the moment a binding decides whether something is correct, the
contract has stopped being common and this file has become another
vertical.

`unsupported` is the one place a harness may decline part of a contract. It
returns a reason; the contract still takes the measurement, and records it
as a declaration (`contract/declared.py`) that the gate holds against the
registry, pinned to the versions it was observed on. An omitted check is
invisible, and a declaration that was never measured is a constant — the
two ways a suite starts lying.
"""

import subprocess
import time
from dataclasses import dataclass, field

from shared.common import squash


@dataclass
class Turn:
    """What one hook-scene turn produced, as the person driving it saw it."""

    plain: str
    #: The final text arrived and the surface went quiet afterwards.
    settled: bool
    detail: str
    #: The text of every approval the harness put on screen and the driver
    #: accepted, in order.
    approvals: list = field(default_factory=list)


def hook_prelude(project):
    """The project a hook scene opens: a Git repository with a README the
    file aliases read, edit and search."""
    return f"""mkdir -p {project} && cd {project}
git init -q -b main .
printf 'lab project\\n' > README.md
"""


class Bindings:
    #: Registry id, matching the integration's own.
    harness = ""

    #: The name UZE's reports give this harness (`display_name()`).
    display_name = ""

    #: The name UZE's launcher is installed under for this harness, when it
    #: differs from the registry id (`shim_name()` on the integration).
    launcher = ""

    #: The command run inside the container to start the TUI.
    launch = ""

    #: Text that proves the TUI is ready for input. Any one is enough.
    ready_markers = ()

    #: The same, for a process that opened on a conversation instead of on
    #: nothing. Empty means `ready_markers` say it for both — declare this
    #: only where a resumed session genuinely shows something else.
    rejoin_markers = ()

    #: Seconds to wait after `ready_markers` before typing. Prompts render
    #: before the surfaces behind them finish loading, and input sent into
    #: that window is lost — a per-harness fact, measured, not guessed.
    #: With `input_markers` it is the ceiling of that wait, not its length.
    warmup = 0.0

    #: Text only a prompt that accepts input carries — never a dialog, a
    #: splash or a screen still loading what is behind it. Empty means the
    #: harness shows no such thing, and `warmup` is slept in full.
    input_markers = ()

    def await_input(self, tui):
        """Waits until the screen shows the prompt accepts input.

        The warmup was slept in full before every typed turn, whatever the
        screen said — 25 seconds a session on OpenCode. Where the harness
        draws something only its ready prompt carries (`input_markers`),
        this ends on it, read off the screen as it stands now; the warmup
        stays the bound, so a harness that never draws it waits exactly as
        long as before. Never on silence: a harness that pauses before its
        session starts (Codex, after its trust dialog) is quiet while it
        cannot take a turn.
        """
        if not self.input_markers:
            time.sleep(self.warmup)
            return
        wanted = [squash(marker) for marker in self.input_markers]
        deadline = time.monotonic() + self.warmup
        while not any(marker in squash(tui.shown()) for marker in wanted):
            left = deadline - time.monotonic()
            if left <= 0:
                return
            # Reading is what grows the record `shown` renders.
            tui.screen(min(0.5, left), first_byte=min(0.5, left))

    def prepare(self, tui):
        """Anything between launch and a usable prompt — an onboarding flow,
        a first-run consent. Returns the plain screen text.

        Default: wait for `ready_markers`. A harness that needs more
        overrides this, because "the process started" and "the prompt
        accepts input" are different facts, and treating them as one is how
        a loose marker starts passing checks against a splash screen.
        """
        return tui.ready(self.ready_markers)

    def session(self, cfg, prov_ip):
        """A live TUI for this harness, as a context manager."""
        raise NotImplementedError

    def session_in(self, cfg, prov_ip, cwd, prelude):
        """A live TUI started in `cwd` after `prelude` — a shell script the
        contract wrote to lay a scene down — has run in the container."""
        raise NotImplementedError

    def launcher_name(self):
        """The launcher's file name — the id unless the harness declares
        another, the same rule the integration's own `shim_name` follows."""
        return self.launcher or self.harness

    def relaunch_in(self, cfg, prov_ip, cwd, prelude):
        """A terminal in `cwd` that runs this harness, and then runs it
        again when the first one ends — what the terminal runtime does when
        it restores a workspace after a restart.

        Both launches go through UZE's own launcher, because that is where
        the resume-or-start decision is made, and the shell between them is
        `continuity.relaunch_command`, so every vertical announces the first
        process's exit the same way.
        """
        raise NotImplementedError

    #: What this harness is ended by, in the order a person would try:
    #: an interrupt, then whatever it takes if the interrupt was not
    #: enough. Data rather than a method, because the *pacing* is the
    #: contract's — a key sent after the process already exited lands on
    #: the shell, which at that moment is starting the next process, and
    #: that is how an interrupt meant for the first one killed the second
    #: before it drew a frame (measured on OpenCode, `experiments/
    #: relaunch_probe`). The contract sends these one at a time and stops
    #: at the one that worked, so a harness may list more than it needs.
    exit_keys = ("\x03", "\x03")

    #: Seconds between one exit key and the next. Per harness because the
    #: harnesses want opposite things and both were measured: Claude's
    #: "press it again to exit" *expires*, so a slow second interrupt is
    #: read as another first one and it never leaves; Codex before 0.157 had
    #: to render that offer before a second interrupt means anything, and a fast one
    #: is swallowed. There is no value that suits both, which is why this
    #: is not a constant in the contract.
    exit_key_gap = 0.5

    def rejoin(self, tui, already=""):
        """What proves the *next* process reached its prompt.

        Not `prepare`: that one drives a first run — the colour scheme, the
        terms, the API key, the folder trust — and none of it happens
        twice. A second process in the same home opens straight on the
        prompt, so waiting again for a picker that will never come back
        reads as a harness that never started, which is exactly how this
        went red on three verticals while the fourth (whose `prepare`
        happened to fall back to the prompt marker) went green.

        Two things this does that the first launch never needs:

        It accumulates. A screen read returns only what arrived since the
        last one, and nobody is driving this screen into repainting the
        way `prepare` drives the first.

        It asks for a repaint when nothing came, and only then. A harness
        relaunched into a terminal it did not start in paints its frame
        once and then waits — measured on Codex 0.153.4
        (`experiments/relaunch_probe`), which sat silent for two minutes
        and rendered its whole prompt on a form feed. Sent only after the
        plain wait failed, because a key sent at a harness that is already
        fine is a key it has to do something with.

        It starts from `already`: what the read that saw the first process
        end took off the screen after it. A harness that paints its whole
        prompt in that same burst and then waits has already said it is
        ready, and waiting for a frame it will not send again sat out the
        full budget — 45 seconds on Antigravity 1.3.2 — until the form feed
        below brought the prompt back.
        """
        markers = list(self.rejoin_markers or self.ready_markers)
        matched = next((m for m in markers if m in already), None)
        plain = already
        if not matched:
            _, more, matched = tui.wait_for(markers, tries=8, accumulate=True)
            plain = f"{already}\n{more}" if already else more
        if not matched:
            tui.child.send("\x0c")
            _, plain, matched = tui.wait_for(markers, tries=8, accumulate=True)
        tui.snapshot("rejoin", plain)
        return plain, matched

    def skill_catalog(self, tui):
        """Opens this harness's Skill list and returns the screen text."""
        raise NotImplementedError

    def lists(self, catalog, skill):
        """Whether `catalog` offers the canonical Skill named `skill`.

        A harness decides this, because only it knows how its own surface
        spells a Skill — `commit (flow)` on one, `flow:commit` on another.
        It is still mechanics, not judgement: the contract decides what the
        answer means.
        """
        raise NotImplementedError

    def mcp_inventory(self, tui):
        """Opens this harness's MCP surface and returns the screen text."""
        raise NotImplementedError

    def names_server(self, inventory, server):
        """Whether `inventory` shows the MCP server `server` as present.

        Same split as `lists`: only the harness knows how its own surface
        spells a server — an id on one, a display name on another, a
        connection row on a third. The contract decides what the answer
        means, never how to read it.
        """
        return server in inventory.replace(" ", "")

    def invoke_skill(self, tui, label):
        """Invokes `label` the way a person would here, returning the turn's
        screen text."""
        raise NotImplementedError

    def headless(
        self, cfg, prov_ip, prelude, prompt, cwd, plugins="", delegating=False
    ):
        """A non-interactive container running one turn of `prompt` in
        `cwd`, after `plugins` were installed and `prelude` ran.

        `delegating` asks for a session whose model is offered the agents it
        may delegate to. Most harnesses offer that roster to every session;
        one that offers it only to some agent opens the turn as such an
        agent here — how, is this harness's business.
        """
        raise NotImplementedError

    def project_turn(self, cfg, prov_ip, prelude, prompt, cwd, delegating=False):
        """One headless turn of `prompt` in a project of the person's at
        `cwd`, after `prelude` ran, returning everything the container
        printed.

        A harness that asks before it reads a project (a folder trust) is
        answered first, on screen, the way a person who opened that project
        once answered it; how, is this harness's business. Default: the
        harness asks nothing a headless turn would meet, so the turn runs
        as `headless` does.
        """
        cmd = self.headless(cfg, prov_ip, prelude, prompt, cwd, delegating=delegating)
        proc = subprocess.run(
            cmd, capture_output=True, text=True, errors="replace", timeout=480
        )
        return proc.stdout + proc.stderr

    def dispatch(self, label, prompt):
        """The provider mode and environment that make the next headless
        turn — the one carrying `prompt` — dispatch the agent `label`
        through this harness's own dispatch tool, in the shape its request
        declared that tool."""
        raise NotImplementedError

    #: Where a hook scene's session runs: a project of the person's, which
    #: is a Git repository like every project a person opens.
    hook_project = "/work/project"

    #: Text this harness's approval prompts carry. A hook scene accepts
    #: each one the way a person does (`approve`) — never by a flag that
    #: stops the harness from asking.
    approval_prompts = ()

    #: Text the final answer of a scripted turn carries.
    final_markers = ("UZE_CONFORMANCE_PASS",)

    def hook_session(self, cfg, prov_ip, plugin, tag, before=""):
        """A live TUI in `hook_project` with `plugin` installed, after
        `before` — a shell line — ran once the install was done."""
        raise NotImplementedError

    def hook_review_recorded(self, cfg):
        """Whether this harness has recorded the person's trust in the
        delivered hooks, read from its own state in the running container;
        `None` for a harness that runs delivered hooks without a review."""
        return None

    def sequence(self, calls, trigger):
        """The provider mode and environment that make the next turn — the
        one whose prompt carries `trigger` — call each of `calls`
        (`{"tool", "args"}`) in order, one per model step, in the shape this
        harness's provider scripts them."""
        raise NotImplementedError

    def call(self, template, mark="", side="lab-side/call"):
        """A tool's input, from the Lab's template for it (`vocabulary.json`
        `call`): `{project}` is the hook project, `{side}` the file the call
        leaves behind, `{dir}` that file's directory, `{mark}` text the
        effect guards look for."""
        path = f"{self.hook_project}/{side}"
        values = {
            "project": self.hook_project,
            "side": path,
            "dir": path.rsplit("/", 1)[0],
            "mark": mark,
        }

        def render(value):
            if isinstance(value, str):
                for key, text in values.items():
                    value = value.replace("{" + key + "}", text)
                return value
            if isinstance(value, dict):
                return {k: render(v) for k, v in value.items()}
            if isinstance(value, list):
                return [render(v) for v in value]
            return value

        return render(template)

    def approve(self, tui, prompt):
        """Accepts the approval on screen — the one `prompt` matched — as a
        person would."""
        tui.submit()

    def hook_turn(self, tui, prompt, tries=40):
        """Sends `prompt` and drives the turn to its end, accepting every
        approval the harness asks for on the way."""
        self.await_input(tui)
        before = self._finals_shown(tui)
        tui.type(prompt)
        tui.submit()
        seen, approvals, matched = "", [], None
        targets = list(self.final_markers) + list(self.approval_prompts)
        for _ in range(tries):
            _, plain, matched = tui.wait_for(
                targets, tries=2, stop_on_death=True, squash_spaces=True
            )
            seen += plain
            if matched in self.approval_prompts:
                approvals.append(plain)
                self.approve(tui, matched)
                continue
            if matched in self.final_markers:
                break
            # The final text can reach the screen in pieces a read never
            # holds whole: Antigravity 1.3.0 streams `UZE_CONFORMA`, redraws
            # its spinner, then writes `NCE_PASS` by moving the cursor back
            # up the line. The screen is what a person reads, so it is what
            # says the answer arrived.
            if self._finals_shown(tui) > before:
                matched = self.final_markers[0]
                break
        settled = matched in self.final_markers and tui.quiet()
        seen = tui.transcript() or seen
        detail = (
            f"the turn ended after {len(approvals)} approvals"
            if settled
            else f"the turn never ended: {seen[-160:]}".replace("\n", " ")
        )
        return Turn(seen, settled, detail, approvals)

    def _finals_shown(self, tui):
        """How many final texts the session's screen shows. Counted rather
        than found, so a second turn in one session waits for its own."""
        screen = squash(tui.shown())
        return sum(screen.count(squash(marker)) for marker in self.final_markers)

    def mcp_calls(self):
        """The scripted calls that run the delivered server's tool in this
        harness, loading it first where the harness defers MCP tools; `None`
        while the vertical proves execution in a phase of its own."""
        return None

    def hook_error(self, plain):
        """Whether the harness reported a hook as having failed, rather
        than as having decided."""
        return "hook error" in plain.lower()

    def unsupported(self, capability):
        """A reason this harness cannot express `capability`, or `None`.

        Answering with a reason does not skip the check: the measurement
        runs, and a harness that turns out to have the control fails the
        declaration as escalated. Answering `None` when the harness in fact
        cannot is a failed check, which is the honest outcome too.
        """
        return None
