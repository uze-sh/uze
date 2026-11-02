"""What `uze doctor --repair` puts back, proven by the real harness using it.

The deterministic suite proves the repair against fake harnesses
(`tests/acceptance/repair.rs`). Only the real harness can say whether what
the repair put back is what it reads: a marketplace registered again where
the vendor's own CLI accepts it, bytes the builds are made from, an
endpoint the next workspace can bind.

So the damage is made after an ordinary install, `uze doctor --repair` runs,
and the harness is then used the way a person uses it:

- `sources/` deleted, the package bytes every build is made from;
- where the harness keeps a registry of UZE's marketplace, that marketplace
  registered at a root an earlier release used (the binding says how);
- a terminal server killed outright, leaving its endpoint behind.

Afterwards one headless turn in a project with an `always_on` rule offers
the plugin's Skill and carries the rule's body; a `shell` call the
package's deny guard refuses is refused, and the tool never runs; and every
package's digest is the one recorded at install.
"""

import os
import re

from contract import hooks, rules
from shared import vocabulary
from shared.common import (
    check,
    check_absence,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)
from shared.markers import RULES, RULES_TURNS

UZE_HOME = "/work/home/.uze"

DIGESTS_BEFORE = "UZE_REPAIR_DIGESTS_BEFORE"
DIGESTS_AFTER = "UZE_REPAIR_DIGESTS_AFTER"
DOCTOR_BEFORE = "UZE_REPAIR_DOCTOR_BEFORE"
DOCTOR_AFTER = "UZE_REPAIR_DOCTOR_AFTER"
REPAIRED = "UZE_REPAIR_REPORT"
LISTED = "UZE_REPAIR_PLUGINS"
BEGIN = "_BEGIN"
END = "_END"

#: The findings the damage makes, as `uze doctor --format json` names their
#: subjects; registrations only where the binding misplaces one.
BYTES = "package_bytes"
ENDPOINT = "terminal_endpoint"
REGISTRATION = "registrations"


def _between(output, marker):
    begin = marker + BEGIN
    if begin not in output:
        return None
    return output.rsplit(begin, 1)[1].split(marker + END, 1)[0]


def _printed(marker, command):
    return f"echo {marker}{BEGIN}; ( {command} ) 2>&1 || true; echo {marker}{END}"


DIGESTS = f'grep -o \'"digest": *"[^"]*"\' {UZE_HOME}/state/packages.json | sort'
DOCTOR = "uze doctor --format json"

#: A terminal server, started and killed with no chance to clean up: the
#: endpoint it bound stays behind. Waits for it to answer at most ten
#: seconds; what it left is read back by `doctor`, not assumed here.
STALE_ENDPOINT = "\n".join(
    [
        "uze terminal serve --project /work >/work/serve.log 2>&1 &",
        "SERVE=$!",
        "for _ in $(seq 100); do",
        f"  find {UZE_HOME}/state/terminal /tmp -type s 2>/dev/null | grep -q . && break",
        "  sleep 0.1",
        "done",
        "kill -9 $SERVE 2>/dev/null || true",
        "wait $SERVE 2>/dev/null || true",
    ]
)


def damage(bindings):
    """The shell that breaks the machine after the install, repairs it, and
    prints what doctor saw before and after."""
    return "\n".join(
        [
            "cd /work",
            _printed(DIGESTS_BEFORE, DIGESTS),
            f"rm -rf {UZE_HOME}/sources",
            f"( {bindings.misplace_registration or 'true'} ) || true",
            STALE_ENDPOINT,
            _printed(DOCTOR_BEFORE, DOCTOR),
            _printed(
                REPAIRED,
                "uze doctor --repair >/work/repair.log 2>&1; "
                'echo "repair exit $?"; cat /work/repair.log',
            ),
            _printed(DOCTOR_AFTER, DOCTOR),
            _printed(DIGESTS_AFTER, DIGESTS),
            _printed(LISTED, bindings.plugin_listing or "true"),
        ]
    )


def assert_contract(cfg, prov_ip, bindings):
    expected = vocabulary.load_expectation(bindings.harness)
    with describe("repair"):
        prov_ip = start_provider(cfg, "static")
        project = bindings.hook_project
        prelude = "\n".join(
            [
                "uze add flow@uze-lab -m >/work/repair-add.log 2>&1 || cat /work/repair-add.log",
                rules.prelude(project),
                damage(bindings),
                f"cd {project}",
            ]
        )
        prompt = f"{RULES_TURNS[0]} which probe does this machine keep?"
        output = bindings.project_turn(cfg, prov_ip, prelude, prompt, project)
        with open(os.path.join(cfg.outdir, "repair-turn.out"), "w") as f:
            f.write(output)
        struct = provider_struct(cfg)
        _assert_repaired(bindings, output)
        _assert_turn(
            observed_markers(struct, "skill_markers"),
            observed_markers(struct, "rule_markers"),
            output,
        )
        if "deny" in set(expected["effects"]):
            _deny(cfg, bindings, expected)
    start_provider(cfg, "static")


def _subjects(document):
    """The subjects of a doctor JSON report's health findings, or `None`
    when it printed none."""
    if not document:
        return None
    found = re.findall(r'"subject":\s*"([a-z_]+)"', document)
    return set(found) if '"health"' in document else None


def _assert_repaired(bindings, output):
    before = _subjects(_between(output, DOCTOR_BEFORE))
    wanted = {BYTES, ENDPOINT} | (
        {REGISTRATION} if bindings.misplace_registration else set()
    )
    made = before is not None and wanted <= before
    check(
        "repair-damage-made",
        made,
        f"doctor found {sorted(before)}" if before is not None else output[-300:],
    )
    report = _between(output, REPAIRED) or ""
    exited = "repair exit 0" in report
    check(
        "repair-exits-healthy",
        exited,
        "doctor --repair exited 0" if exited else report.strip()[-400:],
    )
    after = _subjects(_between(output, DOCTOR_AFTER))
    left = None if after is None else sorted(after & wanted)
    check(
        "repair-damage-gone",
        left == [],
        "doctor finds none of it after the repair"
        if left == []
        else f"still found: {left if left is not None else output[-300:]}",
    )
    digests_before = _between(output, DIGESTS_BEFORE)
    digests_after = _between(output, DIGESTS_AFTER)
    same = bool((digests_before or "").strip()) and digests_before == digests_after
    check(
        "repair-digests-unchanged",
        same,
        "every package's digest is the one recorded at install"
        if same
        else f"before {digests_before!r}, after {digests_after!r}",
    )
    if bindings.plugin_listing is not None:
        listing = _between(output, LISTED) or ""
        enabled = bindings.plugin_enabled(listing, "flow")
        check(
            "repair-plugin-installed-and-enabled",
            enabled,
            "the vendor lists the plugin installed and enabled"
            if enabled
            else listing.strip()[-300:],
        )


def _assert_turn(skills, rule_markers, output):
    reached = bool(rule_markers.get(RULES_TURNS[0]))
    check(
        "repair-turn-reached-model",
        reached,
        "the turn reached the model" if reached else output[-300:],
    )
    loaded = bool(skills.get("flow:commit"))
    check(
        "repair-plugin-loads",
        loaded,
        "the installed plugin's Skill is offered to the model"
        if loaded
        else "no request offered `flow:commit`",
    )
    always = bool(rule_markers.get(RULES["always"]))
    check(
        "repair-always-on-rule-reaches-model",
        always,
        "the always_on rule's body reached the model"
        if always
        else "no request carried the always_on rule",
    )


def _deny(cfg, bindings, expected):
    """The package's deny guard still refuses a marked `shell` call, and the
    tool never runs, once the machine was damaged and repaired after the
    install."""
    shell = expected["aliases"]["shell"]
    denied = bindings.call(
        shell["call"], mark=hooks.MARKED, side=f"{hooks.SIDE}/denied"
    )
    calls = [{"tool": vocabulary.call_tool(shell), "args": denied}]
    with describe("repair-deny"):
        found, side, turn = hooks._scene(
            cfg,
            bindings,
            "repair-deny",
            "hook-effects",
            calls,
            before=damage(bindings),
        )
        settled = turn is not None and turn.settled
        guard = [
            r
            for r in found.get("effect-guard", [])
            if hooks.MARKED in r.get("HOOK_COMMAND", "")
        ]
        reason = hooks.relayed(cfg, f"{hooks.DENIED}effect-guard")
        check(
            "repair-deny-guard-ran",
            bool(guard),
            "the guard ran on the marked command" if guard else f"records: {found}",
        )
        check(
            "repair-deny-reason-relayed",
            reason,
            "the denial reached the conversation"
            if reason
            else "no request carried it",
        )
        check_absence(
            "repair-deny-tool-blocked",
            "denied" not in side,
            settled,
            proof=bool(guard) and reason,
            detail="the denied command never ran" if "denied" not in side else "it ran",
        )
