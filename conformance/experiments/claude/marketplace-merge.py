"""Moving plugins from a second local marketplace into the first.

Earlier releases of UZE installed packages that ship their own envelope
through `uze-local` and generated ones through `uze-store`
(vendor-stores-and-uninstall, design D5 step 3.2). This measures the move
UZE makes when it merges them, with Claude's own CLI only, no UZE, no
model turn:

- the author-enveloped plugin, uninstalled from `uze-local` and installed
  from `uze-store` (which now lists it beside the generated one), is
  installed and enabled there, whatever its enablement was before: a
  disabled plugin comes back enabled;
- the generated plugin already in `uze-store` stays installed and enabled
  through the whole move;
- `marketplace remove uze-local` then leaves no plugin of it installed.

The plugins are the Lab fixtures `route-explicit` (an envelope written by
its author) and `flow` (one generated from its canonical manifest).

Run: python3 conformance/lab.py --harness claude --experiment claude/marketplace-merge
"""

from experiments.vendor_cli import check_version, json_in, run_steps
from shared import common

FIXTURES = "/opt/uze-conformance-fixtures/marketplace/plugins"
LOCAL, STORE = "/work/uze-local", "/work/uze-store"


def catalogue(name, plugins):
    entries = ",".join(
        f'{{"name":"{plugin}","source":"./{plugin}"}}' for plugin in plugins
    )
    return f'{{"name":"{name}","owner":{{"name":"UZE Lab"}},"plugins":[{entries}]}}'


SETUP = f"""
mkdir -p {LOCAL}/.claude-plugin {LOCAL}/route-explicit/.claude-plugin
cp -r {FIXTURES}/route-explicit/skills {LOCAL}/route-explicit/
printf '%s' '{{"name":"route-explicit","version":"0.1.0","description":"Written by its author."}}' \\
  > {LOCAL}/route-explicit/.claude-plugin/plugin.json
printf '%s' '{catalogue("uze-local", ["route-explicit"])}' > {LOCAL}/.claude-plugin/marketplace.json
mkdir -p {STORE}/.claude-plugin {STORE}/flow/.claude-plugin
cp -r {FIXTURES}/flow/skills {STORE}/flow/
printf '%s' '{{"name":"flow","version":"1.0.0","description":"Generated."}}' \\
  > {STORE}/flow/.claude-plugin/plugin.json
printf '%s' '{catalogue("uze-store", ["flow"])}' > {STORE}/.claude-plugin/marketplace.json
"""

MERGE = (
    f"cp -r {LOCAL}/route-explicit {STORE}/ && printf '%s' "
    f"'{catalogue('uze-store', ['flow', 'route-explicit'])}' > {STORE}/.claude-plugin/marketplace.json"
)

STEPS = [
    ("version", "claude --version"),
    ("add-local", f"claude plugin marketplace add {LOCAL}"),
    ("add-store", f"claude plugin marketplace add {STORE}"),
    ("install-explicit", "claude plugin install route-explicit@uze-local"),
    ("install-generated", "claude plugin install flow@uze-store"),
    ("disable-explicit", "claude plugin disable route-explicit@uze-local"),
    ("merge", MERGE),
    ("readd-store", f"claude plugin marketplace add {STORE}"),
    ("uninstall-local", "claude plugin uninstall route-explicit@uze-local"),
    ("install-store", "claude plugin install route-explicit@uze-store"),
    ("remove-local", "claude plugin marketplace remove uze-local"),
    ("list", "claude plugin list --json"),
    ("marketplaces", "claude plugin marketplace list --json"),
]


def listed(results):
    return {
        entry.get("id"): entry
        for entry in json_in(results.get("list", (1, ""))[1]) or []
        if isinstance(entry, dict)
    }


def _assert_merged_plugin_comes_back_enabled(results):
    plugins = listed(results)
    merged = plugins.get("route-explicit@uze-store", {})
    common.check(
        "claude-merged-plugin-installs-enabled-from-the-one-marketplace",
        results.get("install-store", (1, ""))[0] == 0
        and merged.get("enabled") is True
        and "route-explicit@uze-local" not in plugins,
        f"{sorted(plugins)}; merged {merged}",
    )


def _assert_generated_plugin_untouched(results):
    generated = listed(results).get("flow@uze-store", {})
    common.check(
        "claude-generated-plugin-stays-through-the-merge",
        generated.get("enabled") is True,
        f"{generated}",
    )


def _assert_retired_marketplace_gone(results):
    known = json_in(results.get("marketplaces", (1, ""))[1])
    common.check(
        "claude-retired-marketplace-removed",
        results.get("remove-local", (1, ""))[0] == 0 and "uze-local" not in str(known),
        f"{known}",
    )


def run(cfg, prov_ip):
    results = run_steps(cfg, prov_ip, "marketplace-merge", SETUP, STEPS)
    check_version("claude-marketplace-merge-version", results)
    _assert_merged_plugin_comes_back_enabled(results)
    _assert_generated_plugin_untouched(results)
    _assert_retired_marketplace_gone(results)
