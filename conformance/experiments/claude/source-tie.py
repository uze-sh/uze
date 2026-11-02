"""What ties Claude Code to a directory marketplace's source.

UZE moves what Claude reads (vendor-stores-and-uninstall, design
"Context"), so how Claude holds a local marketplace has to be measured,
not assumed. Claude's own CLI only, no UZE, no model turn:

- an installed plugin is read *in place* from the marketplace source, not
  from the copy in `plugins/cache` (`claude plugin list` says "Read from");
- with the source moved away the plugin fails to load;
- a same-name `marketplace add` of the new directory repoints it, and the
  plugin stays installed and enabled;
- a `file://` source is refused;
- one marketplace holds an author-enveloped plugin (its own
  `.claude-plugin/plugin.json`) beside a generated one, and both install.

The plugins are the Lab fixtures `flow` (an envelope generated from its
canonical manifest, as UZE writes one) and `route-explicit` (an envelope
written by hand, as an author would).

Run: python3 conformance/lab.py --harness claude --experiment claude/source-tie
"""

from experiments.vendor_cli import check_version, json_in, run_steps
from shared import common

FIXTURES = "/opt/uze-conformance-fixtures/marketplace/plugins"
FIRST, MOVED, BY_URL = "/work/m1", "/work/m2", "/work/m3"
PLUGINS = ("flow", "route-explicit")

SETUP = f"""
M={FIRST}
mkdir -p $M/.claude-plugin $M/plugins/flow/.claude-plugin $M/plugins/route-explicit/.claude-plugin
cp -r {FIXTURES}/flow/skills {FIXTURES}/flow/agents $M/plugins/flow/
printf '%s' '{{"name":"flow","version":"1.0.0","description":"Generated from the canonical manifest."}}' \\
  > $M/plugins/flow/.claude-plugin/plugin.json
cp -r {FIXTURES}/route-explicit/skills $M/plugins/route-explicit/
printf '%s' '{{"name":"route-explicit","version":"0.1.0","description":"Written by its author.","author":{{"name":"UZE Lab"}}}}' \\
  > $M/plugins/route-explicit/.claude-plugin/plugin.json
printf '%s' '{{"name":"lab-tie","owner":{{"name":"UZE Lab"}},"plugins":[{{"name":"flow","source":"./plugins/flow"}},{{"name":"route-explicit","source":"./plugins/route-explicit"}}]}}' \\
  > $M/.claude-plugin/marketplace.json
"""

STEPS = [
    ("version", "claude --version"),
    ("add", f"claude plugin marketplace add {FIRST}"),
    ("install-flow", "claude plugin install flow@lab-tie"),
    ("install-route-explicit", "claude plugin install route-explicit@lab-tie"),
    ("list-in-place", "claude plugin list"),
    ("list-in-place-json", "claude plugin list --json"),
    ("move", f"mv {FIRST} {MOVED}"),
    ("list-moved", "claude plugin list"),
    ("readd", f"claude plugin marketplace add {MOVED}"),
    ("list-repointed", "claude plugin list --json"),
    (
        "file-url",
        f'cp -r {MOVED} {BY_URL} && sed -i \'s/"lab-tie"/"lab-file"/\' '
        f"{BY_URL}/.claude-plugin/marketplace.json && "
        f"claude plugin marketplace add file://{BY_URL}",
    ),
    ("marketplaces", "claude plugin marketplace list --json"),
]


def by_id(listing):
    return {
        entry.get("id"): entry for entry in listing or [] if isinstance(entry, dict)
    }


def read_from(results, step, root):
    """Each plugin's `(enabled, readFromFolder)`, and whether every one is
    enabled and read from its directory under `root`."""
    plugins = by_id(json_in(results.get(step, (1, ""))[1]))
    seen = {
        name: (
            plugins.get(f"{name}@lab-tie", {}).get("enabled"),
            plugins.get(f"{name}@lab-tie", {}).get("readFromFolder"),
        )
        for name in PLUGINS
    }
    held = all(
        enabled is True and folder == f"{root}/plugins/{name}"
        for name, (enabled, folder) in seen.items()
    )
    return held, seen


def _assert_enveloped_beside_generated(results):
    installs = {name: results.get(f"install-{name}", (1, ""))[0] for name in PLUGINS}
    listed = by_id(json_in(results.get("list-in-place-json", (1, ""))[1]))
    common.check(
        "claude-one-marketplace-holds-enveloped-and-generated",
        results.get("add", (1, ""))[0] == 0
        and not any(installs.values())
        and all(f"{name}@lab-tie" in listed for name in PLUGINS),
        f"install exits {installs}; listed {sorted(listed)}",
    )


def _assert_loads_in_place(results):
    in_place, seen = read_from(results, "list-in-place-json", FIRST)
    text = results.get("list-in-place", (1, ""))[1]
    common.check(
        "claude-loads-directory-marketplace-in-place",
        in_place and f"Read from: {FIRST}/plugins/flow" in text,
        f"{seen}",
    )


def _assert_source_gone_fails(results):
    moved = results.get("list-moved", (1, ""))[1]
    common.check(
        "claude-source-gone-fails-to-load",
        moved.count("failed to load") >= len(PLUGINS),
        " | ".join(line.strip() for line in moved.splitlines() if "rror" in line)
        or moved[-300:],
    )


def _assert_same_name_add_repoints(results):
    repointed, seen = read_from(results, "list-repointed", MOVED)
    common.check(
        "claude-same-name-add-repoints-and-keeps-plugins",
        results.get("readd", (1, ""))[0] == 0 and repointed,
        f"{seen}",
    )


def _assert_file_url_refused(results):
    rc, out = results.get("file-url", (0, ""))
    known = json_in(results.get("marketplaces", (1, ""))[1])
    common.check(
        "claude-file-url-source-refused",
        rc != 0 and "lab-file" not in str(known),
        f"exit {rc}: {out.strip()[-200:]}",
    )


def run(cfg, prov_ip):
    results = run_steps(cfg, prov_ip, "source-tie", SETUP, STEPS)
    check_version("claude-source-tie-version", results)
    _assert_enveloped_beside_generated(results)
    _assert_loads_in_place(results)
    _assert_source_gone_fails(results)
    _assert_same_name_add_repoints(results)
    _assert_file_url_refused(results)
