"""What every package's hooks see of one call, through the hooks section of
UZE's one OpenCode plugin, on the shared background service.

With one plugin per package (the earlier delivery, measured on 2.1.2),
plugins ran in file-name order, a denial renamed the call to a tool that
does not exist, and a later package's matcher-less observer was handed
that name with no portable fields. The hooks section runs every package's
groups in one pass over the same call (`opencode-single-plugin` D3):

A. `aa-observe` and `zz-observe` observe `shell`, `zz-census` observes every
   tool, and `mm-deny` denies while `/work/deny` exists: each observer sees
   the real call, and the denial reaches the model;
B. `nn-late`, installed while the service runs, denies the next call with no
   restart;
C. with one package's document corrupted, the next call is refused with a
   reason naming it.

Every turn scripts the same `shell` call; what decides is on disk.

Run:
  python3 conformance/lab.py --harness opencode --experiment opencode/hooks-cross-package
"""

import json
import re
import subprocess

from harnesses.opencode.scenarios import opencode_setup
from shared import common

MARK = "UZE_XP_PROBE"
LOG = "/work/observed.log"
DOCUMENTS = "/work/home/.uze/runtime/attachments/opencode/hook-groups"

GIT = (
    "git -c init.defaultBranch=main -c user.name=lab -c user.email=lab@uze.invalid "
    "-c commit.gpgsign=false -C /work/xmarket"
)


def plugin(name, effect, script, matcher="shell"):
    group = {"id": f"{name}-group", "effect": effect}
    if matcher:
        group["matcher"] = matcher
    hooks = {
        "hooks": {
            "PreToolUse": [
                {
                    **group,
                    "hooks": [
                        {
                            "type": "command",
                            "command": "${PLUGIN_ROOT}/scripts/run",
                            "timeout": 10,
                        }
                    ],
                }
            ]
        }
    }
    manifest = {"name": name, "description": f"cross-package probe {name}"}
    return f"""
mkdir -p /work/xmarket/plugins/{name}/scripts
cat > /work/xmarket/plugins/{name}/plugin.json <<'EOF'
{json.dumps(manifest)}
EOF
cat > /work/xmarket/plugins/{name}/hooks.json <<'EOF'
{json.dumps(hooks)}
EOF
cat > /work/xmarket/plugins/{name}/scripts/run <<'EOF'
#!/bin/sh
{script}
EOF
chmod +x /work/xmarket/plugins/{name}/scripts/run
"""


def observe(name):
    return (
        f'printf "{name} phase=%s tool=%s native=%s command=%s\\n" "$(cat /work/phase)" '
        f'"$HOOK_TOOL" "$HOOK_TOOL_NATIVE" "$HOOK_COMMAND" >> {LOG}\nexit 0'
    )


DENY = (
    f'[ -e /work/deny ] || exit 0\nprintf "mm-deny phase=%s\\n" "$(cat /work/phase)" >> {LOG}\n'
    'echo "denied by mm-deny" >&2\nexit 3'
)
LATE = 'echo "denied by nn-late" >&2\nexit 3'

PACKAGES = ("aa-observe", "mm-deny", "zz-observe", "zz-census", "nn-late")
MARKET = {
    "name": "xp",
    "description": "cross-package probe",
    "plugins": [{"name": n, "source": f"./plugins/{n}"} for n in PACKAGES],
}

SCRIPT = f"""
set +e
mkdir -p /work/xmarket
cat > /work/xmarket/marketplace.json <<'EOF'
{json.dumps(MARKET)}
EOF
{plugin("aa-observe", "observe", observe("aa-observe"))}
{plugin("mm-deny", "deny", DENY)}
{plugin("zz-observe", "observe", observe("zz-observe"))}
{plugin("zz-census", "observe", observe("zz-census"), matcher=None)}
{plugin("nn-late", "deny", LATE)}
{GIT} init -q && {GIT} add -A && {GIT} commit -q -m probe
uze market add /work/xmarket >/dev/null 2>&1
for p in aa-observe mm-deny zz-observe zz-census; do uze add $p@xp -m >/dev/null 2>&1; done
echo "### plugins: $(ls /work/home/.config/opencode/plugins/ | tr '\\n' ' ')"
mkdir -p /work/p && cd /work/p && git init -q .
opencode service stop >/dev/null 2>&1
echo A > /work/phase && touch /work/deny
timeout 180 opencode run "UZE_XP_TURN_A shell please"
rm -f /work/deny && echo B > /work/phase
uze add nn-late@xp -m >/dev/null 2>&1
sleep 2
timeout 180 opencode run "UZE_XP_TURN_B shell please"
uze remove nn-late -m >/dev/null 2>&1
echo C > /work/phase
printf '{{' > {DOCUMENTS}/zz-observe@xp.json
sleep 2
timeout 180 opencode run "UZE_XP_TURN_C shell please"
opencode service stop >/dev/null 2>&1
echo '### observed'
cat {LOG}
"""


def _turn(requests, turn):
    return [r for r in requests if turn in r]


def run(cfg, prov_ip):
    call = {"command": f"echo {MARK}", "description": "probe"}
    prov_ip = common.start_provider(
        cfg,
        "toolcall",
        {
            "DISCOVERY": "1",
            "TOOL_NAME": "shell",
            "TOOL_ARGS": json.dumps(call),
            "TOOL_TRIGGER": "UZE_XP_TURN",
        },
    )
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, SCRIPT, plugins=""), tty=False
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=900
    )
    out = proc.stdout + proc.stderr
    with open(f"{cfg.outdir}/probe.out", "w") as f:
        f.write(out)
    raw = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(f"{cfg.outdir}/requests.log", "w") as f:
        f.write(raw)
    requests = re.split(r"\n(?=### (?:POST|GET) )", raw)
    observed = out.split("### observed", 1)[-1].strip().splitlines()
    plugins = next(
        (line for line in out.splitlines() if line.startswith("### plugins:")), ""
    )

    def seen(name, phase):
        return [line for line in observed if line.startswith(f"{name} phase={phase}")]

    def real(lines):
        return any("tool=shell" in line and MARK in line for line in lines)

    def answered(turn, reason):
        return any(reason in r for r in _turn(requests, turn))

    common.check(
        "runtime-plugin-only",
        "uze.ts" in plugins and "hooks-" not in plugins,
        plugins,
    )
    common.check("deny-handler-ran", bool(seen("mm-deny", "A")), f"{observed}")
    common.check(
        "denial-reached-the-model",
        answered("UZE_XP_TURN_A", "denied by mm-deny"),
        "the request after the call carries the denial reason",
    )
    for name in ("aa-observe", "zz-observe", "zz-census"):
        common.check(
            f"{name}-sees-the-real-call",
            real(seen(name, "A")),
            f"{name}: {seen(name, 'A')}",
        )
    common.check(
        "package-installed-while-running-denies",
        answered("UZE_XP_TURN_B", "denied by nn-late"),
        "the package installed after the service started refused the next call",
    )
    common.check(
        "unreadable-document-refuses-the-call",
        answered("UZE_XP_TURN_C", "zz-observe@xp.json cannot be read"),
        "the call after the corruption is refused, naming the document",
    )
