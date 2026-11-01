"""What ties Antigravity to a plugin's source directory.

UZE moves what Antigravity reads (vendor-stores-and-uninstall, design
"Context"), so how `agy plugin install` holds a plugin has to be measured,
not assumed. Antigravity's own CLI only, no UZE, no model turn:

- `agy plugin install <dir>` copies the plugin into
  `~/.gemini/config/plugins/<name>`, and it stays listed, with its skills,
  after the source directory is moved away;
- installing the same name again replaces the earlier installation.

The plugin carries the Lab fixture `flow`'s skills.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/source-tie
"""

from experiments.vendor_cli import check_version, json_in, run_steps
from shared import common

FIXTURES = "/opt/uze-conformance-fixtures/marketplace/plugins"
SOURCE, AWAY = "/work/src/flow", "/work/away/flow"
INSTALLED = "/work/home/.gemini/config/plugins/flow"

SETUP = f"""
export AGY_CLI_DISABLE_AUTO_UPDATE=1
mkdir -p /work/home/.gemini/antigravity-cli {SOURCE}
for f in settings.json jetski_state.pbtxt installation_id; do
  cp /app/fixtures/$f /work/home/.gemini/antigravity-cli/$f
done
cp -r {FIXTURES}/flow/skills {SOURCE}/
printf '%s' '{{"name":"flow","version":"1.0.0","description":"Source tie probe."}}' > {SOURCE}/plugin.json
"""

STEPS = [
    ("version", "agy --version"),
    ("install", f"agy plugin install {SOURCE}"),
    ("move", f"mkdir -p /work/away && mv {SOURCE} {AWAY}"),
    ("list-source-gone", "agy plugin list --json"),
    ("skills-source-gone", f"ls {INSTALLED}/skills"),
    ("reinstall", f"agy plugin install {AWAY}"),
]


def _assert_install_copies(rc, out):
    listing = json_in(out.get("list-source-gone", "")) or []
    flow = next(
        (p for p in listing if isinstance(p, dict) and p.get("name") == "flow"), {}
    )
    skills = sorted(out.get("skills-source-gone", "").split())
    common.check(
        "antigravity-install-copies-and-survives-source-move",
        rc.get("install") == 0
        and rc.get("move") == 0
        and flow.get("path") == INSTALLED
        and flow.get("enabled") is True
        and rc.get("skills-source-gone") == 0
        and sorted(flow.get("skills", [])) == skills,
        f"listed {flow}; on disk {skills}",
    )


def _assert_same_name_install_replaces(rc, out):
    common.check(
        "antigravity-same-name-install-replaces",
        rc.get("reinstall") == 0
        and "replaced existing installation" in out.get("reinstall", ""),
        out.get("reinstall", "").strip()[-200:],
    )


def run(cfg, prov_ip):
    results = run_steps(cfg, prov_ip, "source-tie", SETUP, STEPS)
    out = {name: text for name, (_, text) in results.items()}
    rc = {name: code for name, (code, _) in results.items()}
    check_version("antigravity-source-tie-version", results)
    _assert_install_copies(rc, out)
    _assert_same_name_install_replaces(rc, out)
