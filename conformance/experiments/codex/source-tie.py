"""What ties Codex to a local marketplace's root.

UZE moves what Codex reads (vendor-stores-and-uninstall, design
"Context"), so how Codex holds a local marketplace has to be measured, not
assumed. Codex's own CLI only, no UZE, no model turn:

- one marketplace lists an explicit plugin (the author's `.codex-plugin`)
  beside a generated one, both installed and enabled;
- `codex plugin add <id>` over an installed plugin rewrites its cache in
  place (same version directory, the source's new bytes);
- a same-name `marketplace add` from another root is refused;
- with the root moved away, `codex plugin list` fails as a whole;
- `marketplace remove` keeps `[plugins."<id>"] enabled = true`, and an
  `add` from the new root lists the plugins installed and enabled again;
- `codex plugin remove <id>` drops its `[plugins]` key and its cache;
- a `file://` source is refused.

The plugins are the Lab fixtures `flow` (an envelope generated as UZE
writes one) and `route-explicit` (its own `.codex-plugin`).

Run: python3 conformance/lab.py --harness codex --experiment codex/source-tie
"""

from experiments.vendor_cli import check_version, json_in, run_steps
from shared import common

FIXTURES = "/opt/uze-conformance-fixtures/marketplace/plugins"
FIRST, OTHER, AWAY = "/work/m1", "/work/m2", "/work/m1-away"
PLUGINS = ("flow", "route-explicit")
CONFIG = "/work/home/.codex/config.toml"
CACHE = "/work/home/.codex/plugins/cache/lab-tie"
SKILL = "skills/analyze/SKILL.md"
MARKER = "SOURCE_TIE_REFRESHED"


def entry(name):
    return (
        f'{{"name":"{name}","source":{{"source":"local","path":"./plugins/{name}"}},'
        '"policy":{"installation":"AVAILABLE","authentication":"ON_INSTALL"},'
        '"category":"Developer tools"}'
    )


SETUP = f"""
M={FIRST}
mkdir -p $M/.agents/plugins $M/plugins/flow/.codex-plugin
cp -r {FIXTURES}/flow/skills $M/plugins/flow/
printf '%s' '{{"name":"flow","version":"1.0.0","description":"Generated.","skills":"./skills/"}}' \\
  > $M/plugins/flow/.codex-plugin/plugin.json
cp -r {FIXTURES}/route-explicit $M/plugins/route-explicit
printf '%s' '{{"name":"lab-tie","interface":{{"displayName":"Lab tie"}},"plugins":[{entry("flow")},{entry("route-explicit")}]}}' \\
  > $M/.agents/plugins/marketplace.json
"""

LIST = "codex plugin list --json --marketplace lab-tie"

STEPS = [
    ("version", "codex --version"),
    ("add", f"codex plugin marketplace add {FIRST}"),
    ("install-flow", "codex plugin add flow@lab-tie"),
    ("install-route-explicit", "codex plugin add route-explicit@lab-tie"),
    ("list-installed", LIST),
    ("cache-before", f"ls -d {CACHE}/flow/*"),
    (
        "refresh",
        f"echo {MARKER} >> {FIRST}/plugins/flow/{SKILL} && codex plugin add flow@lab-tie",
    ),
    ("cache-after", f"ls -d {CACHE}/flow/*"),
    ("cache-refreshed", f"grep -l {MARKER} {CACHE}/flow/*/{SKILL}"),
    ("copy-root", f"cp -r {FIRST} {OTHER}"),
    ("same-name-other-root", f"codex plugin marketplace add {OTHER}"),
    ("move", f"mv {FIRST} {AWAY}"),
    ("list-root-gone", "codex plugin list"),
    ("remove-marketplace", "codex plugin marketplace remove lab-tie"),
    ("config-after-remove", f"cat {CONFIG}"),
    ("add-other-root", f"codex plugin marketplace add {OTHER}"),
    ("list-repointed", LIST),
    ("remove-plugin", "codex plugin remove flow@lab-tie"),
    ("config-after-plugin-remove", f"cat {CONFIG}"),
    ("cache-after-plugin-remove", f"ls {CACHE}"),
    ("file-url", f"codex plugin marketplace add file://{OTHER}"),
]


def listed(results, step, root):
    """Each plugin's `(installed, enabled, source path)`, and whether every
    one is installed, enabled and catalogued under `root`."""
    listing = json_in(results.get(step, (1, ""))[1]) or {}
    plugins = {
        p.get("name"): p for p in listing.get("installed", []) if isinstance(p, dict)
    }
    seen = {
        name: (
            plugins.get(name, {}).get("installed"),
            plugins.get(name, {}).get("enabled"),
            plugins.get(name, {}).get("source", {}).get("path"),
        )
        for name in PLUGINS
    }
    held = all(
        installed is True and enabled is True and path == f"{root}/plugins/{name}"
        for name, (installed, enabled, path) in seen.items()
    )
    return held, seen


def enabled_key(name):
    return f'[plugins."{name}@lab-tie"]\nenabled = true'


def _assert_explicit_beside_generated(rc, out, results):
    both, seen = listed(results, "list-installed", FIRST)
    common.check(
        "codex-one-marketplace-lists-explicit-and-generated",
        rc.get("add") == 0 and both,
        f"{seen}",
    )


def _assert_add_rewrites_cache(rc, out, results):
    before, after = out.get("cache-before", ""), out.get("cache-after", "")
    common.check(
        "codex-add-over-installed-rewrites-cache-in-place",
        rc.get("refresh") == 0
        and rc.get("cache-refreshed") == 0
        and bool(before.strip())
        and before == after,
        f"cache {before.strip()!r} -> {after.strip()!r}, "
        f"new bytes in {out.get('cache-refreshed', '').strip()!r}",
    )


def _assert_same_name_other_root_refused(rc, out, results):
    common.check(
        "codex-same-name-add-from-other-root-refused",
        rc.get("same-name-other-root", 0) != 0,
        out.get("same-name-other-root", "").strip()[-200:],
    )


def _assert_list_needs_local_root(rc, out, results):
    gone = out.get("list-root-gone", "")
    common.check(
        "codex-list-fails-without-local-root",
        rc.get("list-root-gone", 0) != 0 and "lab-tie" in gone,
        gone.strip()[-240:],
    )


def _assert_remove_then_add_keeps_enabled(rc, out, results):
    config = out.get("config-after-remove", "")
    common.check(
        "codex-marketplace-remove-keeps-plugins-enabled",
        rc.get("remove-marketplace") == 0
        and all(enabled_key(name) in config for name in PLUGINS)
        and "[marketplaces.lab-tie]" not in config,
        config.strip().replace("\n", " ")[-300:],
    )
    repointed, seen = listed(results, "list-repointed", OTHER)
    common.check(
        "codex-remove-then-add-repoints-enabled",
        rc.get("add-other-root") == 0 and repointed,
        f"{seen}",
    )


def _assert_plugin_remove_drops_key_and_cache(rc, out, results):
    config = out.get("config-after-plugin-remove", "")
    cache = out.get("cache-after-plugin-remove", "").split()
    common.check(
        "codex-plugin-remove-drops-key-and-cache",
        rc.get("remove-plugin") == 0
        and '[plugins."flow@lab-tie"]' not in config
        and enabled_key("route-explicit") in config
        and "flow" not in cache,
        f"cache holds {cache}",
    )


def _assert_file_url_refused(rc, out, results):
    common.check(
        "codex-file-url-source-refused",
        rc.get("file-url", 0) != 0,
        out.get("file-url", "").strip()[-200:],
    )


def run(cfg, prov_ip):
    results = run_steps(cfg, prov_ip, "source-tie", SETUP, STEPS)
    out = {name: text for name, (_, text) in results.items()}
    rc = {name: code for name, (code, _) in results.items()}
    check_version("codex-source-tie-version", results)
    for assertion in (
        _assert_explicit_beside_generated,
        _assert_add_rewrites_cache,
        _assert_same_name_other_root_refused,
        _assert_list_needs_local_root,
        _assert_remove_then_add_keeps_enabled,
        _assert_plugin_remove_drops_key_and_cache,
        _assert_file_url_refused,
    ):
        assertion(rc, out, results)
