"""Moving plugins from a second local marketplace into the first.

Earlier releases of UZE installed packages that ship their own envelope
through `uze-local` and generated ones through `uze-store`
(vendor-stores-and-uninstall, design D5 step 3.2). This measures the move
UZE makes when it merges them, with Codex's own CLI only, no UZE, no
model turn:

- the explicit plugin, removed as `<id>@uze-local` and added as
  `<id>@uze-store` (whose catalogue now lists it beside the generated
  one), is installed and enabled there;
- the generated plugin already in `uze-store` stays installed and enabled
  through the whole move;
- `marketplace remove uze-local` then leaves `config.toml` with no key of
  it.

The plugins are the Lab fixtures `route-explicit` (its own `.codex-plugin`)
and `flow` (an envelope generated as UZE writes one).

Run: python3 conformance/lab.py --harness codex --experiment codex/marketplace-merge
"""

from experiments.vendor_cli import check_version, json_in, run_steps
from shared import common

FIXTURES = "/opt/uze-conformance-fixtures/marketplace/plugins"
LOCAL, STORE = "/work/uze-local", "/work/uze-store"
CONFIG = "/work/home/.codex/config.toml"


def entry(name):
    return (
        f'{{"name":"{name}","source":{{"source":"local","path":"./{name}"}},'
        '"policy":{"installation":"AVAILABLE","authentication":"ON_INSTALL"},'
        '"category":"Developer tools"}'
    )


def catalogue(name, plugins):
    entries = ",".join(entry(plugin) for plugin in plugins)
    return f'{{"name":"{name}","interface":{{"displayName":"{name}"}},"plugins":[{entries}]}}'


SETUP = f"""
mkdir -p {LOCAL}/.agents/plugins
cp -r {FIXTURES}/route-explicit {LOCAL}/route-explicit
printf '%s' '{catalogue("uze-local", ["route-explicit"])}' > {LOCAL}/.agents/plugins/marketplace.json
mkdir -p {STORE}/.agents/plugins {STORE}/flow/.codex-plugin
cp -r {FIXTURES}/flow/skills {STORE}/flow/
printf '%s' '{{"name":"flow","version":"1.0.0","description":"Generated.","skills":"./skills/"}}' \\
  > {STORE}/flow/.codex-plugin/plugin.json
printf '%s' '{catalogue("uze-store", ["flow"])}' > {STORE}/.agents/plugins/marketplace.json
"""

MERGE = (
    f"cp -r {LOCAL}/route-explicit {STORE}/ && printf '%s' "
    f"'{catalogue('uze-store', ['flow', 'route-explicit'])}' > {STORE}/.agents/plugins/marketplace.json"
)

STEPS = [
    ("version", "codex --version"),
    ("add-local", f"codex plugin marketplace add {LOCAL}"),
    ("add-store", f"codex plugin marketplace add {STORE}"),
    ("install-explicit", "codex plugin add route-explicit@uze-local"),
    ("install-generated", "codex plugin add flow@uze-store"),
    ("merge", MERGE),
    ("remove-local-plugin", "codex plugin remove route-explicit@uze-local"),
    ("install-store", "codex plugin add route-explicit@uze-store"),
    ("remove-local", "codex plugin marketplace remove uze-local"),
    ("list", "codex plugin list --json --marketplace uze-store"),
    ("config", f"cat {CONFIG}"),
]


def installed(results):
    listing = json_in(results.get("list", (1, ""))[1]) or {}
    return {
        entry.get("pluginId"): entry
        for entry in listing.get("installed", [])
        if isinstance(entry, dict)
    }


def _assert_merged_plugin_installs_enabled(results):
    plugins = installed(results)
    merged = plugins.get("route-explicit@uze-store", {})
    common.check(
        "codex-merged-plugin-installs-enabled-from-the-one-marketplace",
        results.get("install-store", (1, ""))[0] == 0 and merged.get("enabled") is True,
        f"{sorted(plugins)}; merged {merged}",
    )


def _assert_generated_plugin_untouched(results):
    generated = installed(results).get("flow@uze-store", {})
    common.check(
        "codex-generated-plugin-stays-through-the-merge",
        generated.get("enabled") is True,
        f"{generated}",
    )


def _assert_retired_marketplace_leaves_no_key(results):
    config = results.get("config", (1, ""))[1]
    common.check(
        "codex-retired-marketplace-leaves-no-key",
        results.get("remove-local", (1, ""))[0] == 0 and "uze-local" not in config,
        config[-300:],
    )


def run(cfg, prov_ip):
    results = run_steps(cfg, prov_ip, "marketplace-merge", SETUP, STEPS)
    check_version("codex-marketplace-merge-version", results)
    _assert_merged_plugin_installs_enabled(results)
    _assert_generated_plugin_untouched(results)
    _assert_retired_marketplace_leaves_no_key(results)
