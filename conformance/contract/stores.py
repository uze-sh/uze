"""What a harness keeps when the parts of `$UZE_HOME` it does not read are
gone, and what it is left with once UZE is uninstalled.

UZE's promise (`transparent-harness-attachment`, `persisted-state`) is that
a harness reads only its own vendor store, `$UZE_HOME/store/<vendor>`.
The rest of the home is UZE's: `runtime/` and `cache/` are disposable,
`sources/` holds the bytes builds are made from, and the default
`uze uninstall` takes the program away and keeps the stores. If any of it
were read by a harness, deleting it would cost a delivery, and only the
real harness can say whether it does.

So each condition is made after an ordinary install, the harness is then
used the way a person uses it, and the outcome is read where the harness
shows it:

- one headless turn in a project with an `always_on` rule: the plugin's
  Skill and the rule's body both reach the model;
- one hook scene: a `shell` call the package's deny guard refuses is
  refused, and the tool never runs.

The conditions are `runtime/` and `cache/` moved away, `sources/` moved
away, and the default `uze uninstall --yes`. After `uze uninstall --yes
--purge`, no configuration or discovery file of a harness names
`$UZE_HOME`, and the vendor's own registry lists nothing of UZE's.
"""

import os
import subprocess

from contract import hooks, lifecycle, rules
from shared import vocabulary
from shared.common import (
    check,
    check_absence,
    concurrently,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)
from shared.markers import RULES, RULES_TURNS

UZE_HOME = "/work/home/.uze"
MOVED = "/work/moved-away"

#: Printed around the home's top-level entries once a condition was made,
#: so the run's evidence says the condition held, not that it was asked.
HOME_BEGIN = "UZE_STORES_HOME_BEGIN"
HOME_END = "UZE_STORES_HOME_END"


def _away(*tiers):
    return "\n".join(
        [f"mkdir -p {MOVED}"]
        + [f"[ -e {UZE_HOME}/{t} ] && mv {UZE_HOME}/{t} {MOVED}/{t}" for t in tiers]
        + ["true"]
    )


UNINSTALL = "uze uninstall --yes >/work/uninstall.log 2>&1 || cat /work/uninstall.log"
PURGE = (
    "uze uninstall --yes --purge >/work/uninstall.log 2>&1 || cat /work/uninstall.log"
)

#: (tag, the shell that makes the condition, the top-level entries of the
#: home that must be gone once it ran).
CONDITIONS = (
    ("runtime-away", _away("runtime", "cache"), ("runtime", "cache")),
    ("sources-away", _away("sources"), ("sources",)),
    ("uninstalled", UNINSTALL, ("runtime", "cache")),
)

SHOW_HOME = f"echo {HOME_BEGIN}; ls -A {UZE_HOME} 2>/dev/null | tr '\\n' ' '; echo; echo {HOME_END}"

#: Printed around every harness file, under the harness homes, that names
#: `$UZE_HOME`, and around what the vendor's registry lists, once purged.
NAMING_BEGIN = "UZE_STORES_NAMING_BEGIN"
NAMING_END = "UZE_STORES_NAMING_END"
REGISTRY_BEGIN = "UZE_STORES_REGISTRY_BEGIN"
REGISTRY_END = "UZE_STORES_REGISTRY_END"

HARNESS_HOMES = lifecycle.HARNESS_HOMES

#: What a vendor keeps after its *own* uninstall, which UZE's purge runs:
#: measured, not assumed (`lifecycle.VENDOR_KEEPS`). Claude Code 2.1.316
#: leaves `plugins/cache/<market>/<plugin>/<version>` behind, a copy it no
#: longer loads, whose Skills quote the expanded package root.
VENDOR_KEEPS = lifecycle.VENDOR_KEEPS

#: The words a vendor registry lists a UZE entry by: its marketplaces, the
#: Lab's own marketplace every installed plugin is named after, and the
#: bundled plugin.
UZE_ENTRIES = ("uze-store", "uze-local", "uze-lab", "uze@uze", "uze--uze")


def assert_contract(cfg, prov_ip, bindings):
    expected = vocabulary.load_expectation(bindings.harness)
    with describe("stores"):
        found = concurrently(
            cfg,
            {
                f"stores-{tag}": (lambda world, make=make: _turn(world, bindings, make))
                for tag, make, _ in CONDITIONS
            }
            | {"stores-purged": lambda world: _purged(world, bindings)},
        )
        for tag, _, gone in CONDITIONS:
            _assert_turn(bindings, tag, gone, *found[f"stores-{tag}"])
        _assert_purged(bindings, found["stores-purged"])
        if "deny" in set(expected["effects"]):
            for tag, make, gone in CONDITIONS:
                _deny(cfg, bindings, expected, tag, make)
    start_provider(cfg, "static")


def _prelude(project, make):
    """The project with its rules, the flow plugin installed and the
    harness set up, then the condition made."""
    return "\n".join(
        [
            "uze add flow@uze-lab -m >/work/stores-add.log 2>&1 || cat /work/stores-add.log",
            rules.prelude(project),
            "cd /work",
            make,
            SHOW_HOME,
            f"cd {project}",
        ]
    )


def _turn(world, bindings, make):
    prov_ip = start_provider(world, "static")
    project = bindings.hook_project
    prompt = f"{RULES_TURNS[0]} which probe does this machine keep?"
    output = bindings.project_turn(
        world, prov_ip, _prelude(project, make), prompt, project
    )
    struct = provider_struct(world)
    with open(os.path.join(world.outdir, "stores-turn.out"), "w") as f:
        f.write(output)
    return (
        observed_markers(struct, "skill_markers"),
        observed_markers(struct, "rule_markers"),
        output,
    )


def _home_after(output):
    """The home's top-level entries the condition left, or `None` when the
    output does not carry them."""
    if HOME_BEGIN not in output:
        return None
    listed = output.rsplit(HOME_BEGIN, 1)[1].split(HOME_END, 1)[0]
    return listed.split()


def _assert_turn(bindings, tag, gone, skills, rule_markers, output):
    home = _home_after(output)
    made = home is not None and not any(tier in home for tier in gone)
    check(
        f"stores-{tag}-made",
        made,
        f"the home holds {home}" if home is not None else output[-300:],
    )
    if not made:
        return
    reached = bool(rule_markers.get(RULES_TURNS[0]))
    check(
        f"stores-{tag}-turn-reached-model",
        reached,
        "the turn reached the model" if reached else output[-300:],
    )
    loaded = bool(skills.get("flow:commit"))
    check(
        f"stores-{tag}-plugin-loads",
        loaded,
        "the installed plugin's Skill is offered to the model"
        if loaded
        else "no request offered `flow:commit`",
    )
    always = bool(rule_markers.get(RULES["always"]))
    check(
        f"stores-{tag}-always-on-rule-reaches-model",
        always,
        "the always_on rule's body reached the model"
        if always
        else "no request carried the always_on rule",
    )


def _deny(cfg, bindings, expected, tag, make):
    """The package's deny guard still refuses a marked `shell` call, and the
    tool never runs, once the condition is made after the install."""
    shell = expected["aliases"]["shell"]
    denied = bindings.call(
        shell["call"], mark=hooks.MARKED, side=f"{hooks.SIDE}/denied"
    )
    calls = [{"tool": vocabulary.call_tool(shell), "args": denied}]
    with describe(f"{tag}-deny"):
        found, side, turn = hooks._scene(
            cfg, bindings, f"stores-{tag}-deny", "hook-effects", calls, before=make
        )
        settled = turn is not None and turn.settled
        guard = [
            r
            for r in found.get("effect-guard", [])
            if hooks.MARKED in r.get("HOOK_COMMAND", "")
        ]
        reason = hooks.relayed(cfg, f"{hooks.DENIED}effect-guard")
        check(
            f"stores-{tag}-deny-guard-ran",
            bool(guard),
            "the guard ran on the marked command" if guard else f"records: {found}",
        )
        check(
            f"stores-{tag}-deny-reason-relayed",
            reason,
            "the denial reached the conversation"
            if reason
            else "no request carried it",
        )
        check_absence(
            f"stores-{tag}-deny-tool-blocked",
            "denied" not in side,
            settled,
            proof=bool(guard) and reason,
            detail="the denied command never ran" if "denied" not in side else "it ran",
        )


def _purged(world, bindings):
    """A machine with plugins, hooks and rules delivered, then purged; what
    the harness homes and the vendor's registry still hold afterwards."""
    project = bindings.hook_project
    listing = bindings.registry_listing or "true"
    script = "\n".join(
        [
            "uze add flow@uze-lab -m >/work/stores-add.log 2>&1 || cat /work/stores-add.log",
            "uze add hook-effects@uze-lab -m >>/work/stores-add.log 2>&1 || cat /work/stores-add.log",
            rules.prelude(project),
            "cd /work",
            PURGE,
            SHOW_HOME,
            f"echo {NAMING_BEGIN}",
            "cd /work/home && grep -rlsI -F "
            + UZE_HOME
            + " "
            + " ".join(HARNESS_HOMES)
            + " 2>/dev/null || true",
            f"echo {NAMING_END}",
            f"echo {REGISTRY_BEGIN}",
            f"( {listing} ) 2>&1 || true",
            f"echo {REGISTRY_END}",
        ]
    )
    # The turn after it is only what the binding can start; the evidence is
    # what the script printed before any session ran.
    prov_ip = start_provider(world, "static")
    cmd = bindings.headless(world, prov_ip, script, "lab purge probe", "/work")
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(world.outdir, "stores-purged.out"), "w") as f:
        f.write(output)
    return output


def _between(output, begin, end):
    if begin not in output:
        return None
    return output.rsplit(begin, 1)[1].split(end, 1)[0]


def _assert_purged(bindings, output):
    home = _home_after(output)
    gone = home is None or home == []
    named = _between(output, NAMING_BEGIN, NAMING_END)
    listed = _between(output, REGISTRY_BEGIN, REGISTRY_END)
    check(
        "stores-purged-home-gone",
        HOME_BEGIN in output and gone,
        "the home is gone"
        if HOME_BEGIN in output and gone
        else f"the home holds {home}",
    )
    configuration = [
        path for path in (named or "").split() if not path.startswith(VENDOR_KEEPS)
    ]
    check_absence(
        "stores-purged-no-file-names-the-home",
        not configuration,
        named is not None,
        proof=named is not None,
        detail="no configuration or discovery file names the home"
        if not configuration
        else f"still naming it: {configuration}",
    )
    if bindings.registry_listing is None:
        return
    entries = [word for word in UZE_ENTRIES if word in (listed or "")]
    check_absence(
        "stores-purged-registry-lists-nothing-of-uze",
        not entries,
        listed is not None,
        proof=listed is not None,
        detail="the vendor's registry lists nothing of UZE's"
        if not entries
        else f"still listed: {entries}: {listed.strip()[-300:]}",
    )
