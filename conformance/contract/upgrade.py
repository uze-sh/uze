"""What an install the previous release made is worth once UZE is upgraded.

A person installs plugins with the release they have, then upgrades. The
new build moves the package bytes, renders every build into the vendor
store and repoints each marketplace at it (`vendor-stores-and-uninstall`,
design D5). The promise is that nothing the person had stops working: every
plugin is still installed, still enabled, and still loads.

The previous release is the real one (`shared/previous.py`), not a fixture,
and makes the install on a machine nothing else touched: the run's own
setup is purged first, so what the harness holds is what that release
gave it. Then this build is set up and installs again, the way an upgraded
machine meets it, and the outcome is read where the harness shows it: the
vendor's own listing, and the request its model is sent.
"""

import os
import subprocess

from shared import previous
from shared.common import (
    check,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)

PROJECT = "/work/upgrade-project"
PLUGIN = "flow"

#: Printed around what each binary answered to `--version`, and around the
#: vendor's listing once upgraded, so the evidence says which release made
#: the install.
VERSIONS_BEGIN = "UZE_UPGRADE_VERSIONS_BEGIN"
VERSIONS_END = "UZE_UPGRADE_VERSIONS_END"
LISTING_BEGIN = "UZE_UPGRADE_LISTING_BEGIN"
LISTING_END = "UZE_UPGRADE_LISTING_END"
HELD_BEGIN = "UZE_UPGRADE_HELD_BEGIN"
#: Printed around the home's `store/` as the previous release left it, then
#: around `store/` and `sources/` once upgraded.
LAYOUT_BEGIN = "UZE_UPGRADE_LAYOUT_BEGIN"
LAYOUT_END = "UZE_UPGRADE_LAYOUT_END"
SOURCES_HELD = "UZE_UPGRADE_SOURCES_HELD"
HELD_END = "UZE_UPGRADE_HELD_END"


def assert_contract(cfg, prov_ip, bindings):
    with describe("upgrade"):
        binary = previous.binary()
        check(
            "upgrade-previous-release-built",
            binary is not None,
            f"{previous.tag()} at {binary}"
            if binary
            else f"no build of {previous.tag()} could be had",
        )
        if binary is None:
            return
        cfg.previous_uze = binary
        try:
            skills, output = _turn(cfg, bindings)
        finally:
            cfg.previous_uze = None
        _assert(bindings, skills, output)
    start_provider(cfg, "static")


def _prelude(listing):
    earlier = previous.IN_CONTAINER
    return "\n".join(
        [
            # The run's own setup is taken off, so the previous release
            # meets a machine as it found it.
            "uze uninstall --yes --purge >/work/upgrade.log 2>&1 || cat /work/upgrade.log",
            f'echo {VERSIONS_BEGIN}; sha256sum {earlier} "$(command -v uze)" | cut -c1-16; echo {VERSIONS_END}',
            f"{earlier} market add /work/market >>/work/upgrade.log 2>&1 || cat /work/upgrade.log",
            f"{earlier} add {PLUGIN}@uze-lab -m >>/work/upgrade.log 2>&1 || cat /work/upgrade.log",
            f"echo {HELD_BEGIN}; ( {listing} ) 2>&1 || true; echo {HELD_END}",
            f"echo {LAYOUT_BEGIN}; ls -A $UZE_HOME/store | tr '\\n' ' '; echo; echo {LAYOUT_END}",
            "uze setup >>/work/upgrade.log 2>&1 || cat /work/upgrade.log",
            f"uze add {PLUGIN}@uze-lab -m >>/work/upgrade.log 2>&1 || cat /work/upgrade.log",
            f"echo {LISTING_BEGIN}; ( {listing} ) 2>&1 || true; echo {LISTING_END}",
            f"echo {LAYOUT_BEGIN}; ls -A $UZE_HOME/store | tr '\\n' ' '; echo; [ -d $UZE_HOME/sources/plugins ] && echo {SOURCES_HELD}; echo {LAYOUT_END}",
            f"mkdir -p {PROJECT} && cd {PROJECT} && git init -q -b main .",
            "printf '# Lab project\\n' > AGENTS.md",
        ]
    )


def _turn(cfg, bindings):
    prov_ip = start_provider(cfg, "static")
    listing = bindings.plugin_listing or "true"
    cmd = bindings.headless(
        cfg, prov_ip, _prelude(listing), "which probe does this machine keep?", PROJECT
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=600
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, "upgrade.out"), "w") as f:
        f.write(output)
    return observed_markers(provider_struct(cfg), "skill_markers"), output


def _between(output, begin, end):
    if begin not in output:
        return None
    return output.rsplit(begin, 1)[1].split(end, 1)[0]


def _layouts(output):
    """`store/` as the previous release left it, and `store/` with
    `sources/` once upgraded, each as the words `ls` printed."""
    parts = output.split(LAYOUT_BEGIN)[1:]
    return [part.split(LAYOUT_END, 1)[0].split() for part in parts]


def _assert(bindings, skills, output):
    # Two releases may share a version number until one is published, so
    # what says they differ is the bytes of each binary.
    digests = (_between(output, VERSIONS_BEGIN, VERSIONS_END) or "").split()
    distinct = len(digests) == 2 and digests[0] != digests[1]
    check(
        "upgrade-two-releases-met",
        distinct,
        f"installed by {digests[0]}, upgraded to {digests[1]}"
        if distinct
        else f"binary digests: {digests}",
    )
    layouts = _layouts(output)
    moved = (
        len(layouts) == 2
        and "plugins" in layouts[0]
        and "plugins" not in layouts[1]
        and SOURCES_HELD in layouts[1]
    )
    check(
        "upgrade-bytes-moved-to-sources",
        moved,
        "the previous release's `store/plugins` became `sources/`"
        if moved
        else f"layouts: {layouts}",
    )
    if bindings.plugin_listing is not None:
        held = _between(output, HELD_BEGIN, HELD_END)
        listed = _between(output, LISTING_BEGIN, LISTING_END)
        before = held is not None and bindings.plugin_enabled(held, PLUGIN)
        check(
            "upgrade-previous-release-installed-the-plugin",
            before,
            "the previous release installed and enabled it"
            if before
            else f"listed: {(held or '').strip()[-300:]}",
        )
        after = listed is not None and bindings.plugin_enabled(listed, PLUGIN)
        check(
            "upgrade-plugin-installed-and-enabled",
            after,
            "the plugin is still installed and enabled"
            if after
            else f"listed: {(listed or '').strip()[-300:]}",
        )
    loaded = bool(skills.get("flow:commit"))
    check(
        "upgrade-plugin-loads",
        loaded,
        "the plugin's Skill is offered to the model"
        if loaded
        else f"no request offered `flow:commit`: {output[-300:]}",
    )
