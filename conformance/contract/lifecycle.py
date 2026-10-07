"""What updating and removing a plugin must do, on every harness.

Installing is what every other contract exercises; a person also updates
and removes, and those are where delivery leaves things behind. An update
that re-projects nothing keeps the model on the old Skill; a removal that
detaches the ledger but not the harness's own copy keeps offering it. The
deterministic suite proves UZE's ledger moves; only the harness can say
whether it still loads the old thing.

Both scenes read the outcome where the harness shows it — the request it
sends its model, and the files under its home — never UZE's own report.
"""

import os
import subprocess

from contract import declared
from shared.common import (
    check,
    check_absence,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)
from shared.markers import LIFECYCLE

PROJECT = "/work/lifecycle-project"
PLUGIN = "lifecycle-plugin"

#: Printed around the residue listing once the turn has ended.
RESIDUE_BEGIN = "UZE_LIFECYCLE_RESIDUE_BEGIN"
RESIDUE_END = "UZE_LIFECYCLE_RESIDUE_END"

#: The person's turn. It names no marker: the descriptions the harness puts
#: in front of its model are what carry them.
PROMPT = "which probe does this machine keep?"

#: Where a harness keeps what it loads, under the run's home. UZE's own
#: home is not among them: its store keeps bytes a removal may keep.
HARNESS_HOMES = (".claude", ".claude.json", ".codex", ".config/opencode", ".gemini")

#: What a harness keeps after its *own* uninstall, measured against the
#: vendor's native flow rather than assumed: Claude Code 2.1.290 leaves
#: `plugins/cache/<market>/<plugin>/<version>` behind after
#: `claude plugin uninstall`, exactly as after UZE's removal. Anything else
#: under a harness home that names the plugin is UZE's residue.
VENDOR_KEEPS = (".claude/plugins/cache/",)

PROJECT_PRELUDE = f"""
mkdir -p {PROJECT} && cd {PROJECT}
git init -q -b main .
printf '# Lab project\\n' > AGENTS.md
"""

UPDATE = f"""
cd /work/market
sed -i 's/UZE_LIFECYCLE_SKILL_V1/UZE_LIFECYCLE_SKILL_V2/' plugins/{PLUGIN}/skills/probe/SKILL.md
sed -i 's/UZE_LIFECYCLE_AGENT_V1/UZE_LIFECYCLE_AGENT_V2/' plugins/{PLUGIN}/agents/keeper.md
git -c user.name=lab -c user.email=lab@uze.invalid commit -qam 'lifecycle v2'
uze update {PLUGIN} -m >/work/lifecycle-update.log 2>&1 || cat /work/lifecycle-update.log
"""

REMOVE = f"""
uze remove {PLUGIN} -m >/work/lifecycle-remove.log 2>&1 || cat /work/lifecycle-remove.log
trap 'echo {RESIDUE_BEGIN}; cd /work/home && find {" ".join(HARNESS_HOMES)} -path "*{PLUGIN}*" 2>/dev/null; echo {RESIDUE_END}' EXIT
"""


def assert_contract(cfg, prov_ip, bindings):
    with describe("lifecycle"):
        _assert_update(cfg, bindings)
        _assert_removal(cfg, bindings)
    start_provider(cfg, "static")


def _turn(cfg, bindings, tag, prelude):
    prov_ip = start_provider(cfg, "static")
    cmd = bindings.headless(
        cfg,
        prov_ip,
        PROJECT_PRELUDE + prelude,
        PROMPT,
        PROJECT,
        plugins=f"flow {PLUGIN}",
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, f"lifecycle-{tag}.out"), "w") as f:
        f.write(output)
    struct = provider_struct(cfg)
    return (
        observed_markers(struct, "lifecycle_markers"),
        observed_markers(struct, "skill_markers"),
        output,
    )


def _assert_update(cfg, bindings):
    """After `uze update`, the model is offered the new Skill and agent and
    never the old ones."""
    seen, skills, _ = _turn(cfg, bindings, "update", UPDATE)
    # The control: a plugin the update did not touch is still offered, so
    # an absence below is the update's doing and not an empty request.
    control = bool(skills.get("flow:commit"))
    for kind, (old, new) in zip(
        ("skill", "agent"), zip(LIFECYCLE["V1"], LIFECYCLE["V2"])
    ):
        current = bool(seen.get(new))
        reaches = f"lifecycle-update-{kind}-reaches-model"
        declared.presence(
            bindings,
            reaches,
            reaches,
            current,
            f"the updated {kind} is offered to the model"
            if current
            else f"no request carried `{new}`",
        )
        if bindings.unsupported(reaches) is not None:
            # A harness that offers no delivered agent offers neither
            # version; that one absence is the measurement of both claims.
            either = current or bool(seen.get(old))
            declared.presence(
                bindings,
                f"lifecycle-update-{kind}-replaced",
                reaches,
                either,
                f"neither `{old}` nor `{new}` is offered"
                if not either
                else f"offered: old={bool(seen.get(old))} new={current}",
            )
            continue
        check_absence(
            f"lifecycle-update-{kind}-replaced",
            not seen.get(old),
            control,
            proof=current,
            detail=f"`{old}` is no longer offered beside the update",
        )


def _assert_removal(cfg, bindings):
    """After `uze remove -m`, the harness offers nothing of the plugin and
    keeps nothing of it on disk."""
    seen, skills, output = _turn(cfg, bindings, "remove", REMOVE)
    control = bool(skills.get("flow:commit"))
    check(
        "lifecycle-removal-control-offered",
        control,
        "the plugin that stayed is still offered"
        if control
        else "no flow Skill offered",
    )
    for kind, marker in zip(("skill", "agent"), LIFECYCLE["V1"]):
        check_absence(
            f"lifecycle-removal-{kind}-gone",
            not seen.get(marker),
            control,
            proof=control,
            detail=f"`{marker}` is no longer offered",
        )
    start = output.rfind(RESIDUE_BEGIN)
    end = output.rfind(RESIDUE_END)
    listed = start >= 0 and end > start
    residue = [
        path
        for path in (output[start + len(RESIDUE_BEGIN) : end].split() if listed else [])
        if not path.startswith(VENDOR_KEEPS)
    ]
    check_absence(
        "lifecycle-removal-no-residue",
        not residue,
        listed,
        proof=listed,
        detail="nothing of the plugin is left where the harness loads from"
        if not residue
        else f"left behind: {residue}",
    )
