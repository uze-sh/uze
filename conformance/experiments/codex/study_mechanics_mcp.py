"""Delivery-mechanics study, Codex follow-up (observation only).

`study_mechanics` could not see any MCP tool on Codex's wire, and read
plugin skills from the cache. This measures, with a launch log written by
the server script itself (tag, `$0`, cwd, the plugin-root variables):

- whether `codex exec` launches a server delivered by each mechanic
  (direct, `codex mcp add` of an absolute path);
- whether a plugin's `.mcp.json` `${PLUGIN_ROOT}` / relative command is
  resolved, and against which copy (source or cache);
- whether a plugin skill edited in the source after install reaches the
  model (source vs cache at runtime).

Run: python3 conformance/lab.py --harness codex --experiment codex/study_mechanics_mcp
"""

from experiments.claude.study_mechanics_lib import (
    FIXTURE,
    done,
    run_container,
    summarize,
    tree,
)
from harnesses.codex.scenarios import codex_container
from shared import common

HOME = "/work/home"
DIRECT = ["a", "b", "c", "d", "e"]
PLUG = ["a", "c", "d", "e", "f1", "f2"]

LOGGED = r"""
logged_src() { # dir tag
  mkdir -p "$1"
  cat > "$1/server.sh" <<EOF
#!/bin/sh
echo "LAUNCH $2 argv0=\$0 pwd=\$(pwd) pr=\${PLUGIN_ROOT:-unset} cpr=\${CLAUDE_PLUGIN_ROOT:-unset}" >> /work/mcp-launch.log
exec /usr/local/bin/uze-mcp-conformance-fixture "\$@"
EOF
  chmod 755 "$1/server.sh"
}
"""

FINAL = f"""
{FIXTURE}
{LOGGED}
set +e
for m in {" ".join(DIRECT)}; do logged_src $STORE/mcpd-$m MCPD_$(UP $m); done
make_ro $STORE/mcpd-e
for m in {" ".join(DIRECT)}; do
  if [ $m = c ]; then ln -s $STORE/mcpd-c /work/deliv/mcp-c
  else deliver_file $m $STORE/mcpd-$m/server.sh /work/deliv/mcp-$m/server.sh; fi
  codex mcp add study-mcp-$m -- /work/deliv/mcp-$m/server.sh >/dev/null 2>&1 || echo "mcp add $m failed"
done
mkdir -p /work/mkt/.agents/plugins /work/mkt/plugins
PL=""
for m in {" ".join(PLUG)}; do
  P=$STORE/pl-$m; mkdir -p $P/.codex-plugin
  echo '{{"name":"p-'$m'","version":"1.0.0","description":"study","skills":"./skills/","mcpServers":"./.mcp.json"}}' > $P/.codex-plugin/plugin.json
  skill_src $P/skills/ps-$m ps-$m SKP_$(UP $m)
  logged_src $P/mcp MCPP_$(UP $m)
  echo '{{"mcpServers":{{"pm-'$m'":{{"command":"${{PLUGIN_ROOT}}/mcp/server.sh"}},"pmrel-'$m'":{{"command":"./mcp/server.sh"}}}}}}' > $P/.mcp.json
  [ $m = e ] && make_ro $P
  deliver $m $P /work/mkt/plugins/p-$m
  PL="$PL{{\\"name\\":\\"p-$m\\",\\"source\\":{{\\"source\\":\\"local\\",\\"path\\":\\"./plugins/p-$m\\"}},\\"policy\\":{{\\"installation\\":\\"AVAILABLE\\",\\"authentication\\":\\"ON_INSTALL\\"}},\\"category\\":\\"Developer tools\\"}},"
done
echo "{{\\"name\\":\\"study\\",\\"interface\\":{{\\"displayName\\":\\"study\\"}},\\"plugins\\":[${{PL%,}}]}}" > /work/mkt/.agents/plugins/marketplace.json
codex plugin marketplace add /work/mkt >/dev/null 2>&1
for m in {" ".join(PLUG)}; do codex plugin add p-$m@study 2>&1 | tail -1; done
echo '=== mcp-list'
codex mcp list 2>&1
echo '=== mutate-source'
sed -i 's/ end$/ edited end/' /work/mkt/plugins/p-a/skills/ps-a/SKILL.md
skill_src /work/mkt/plugins/p-a/skills/ps-late ps-late SKP_LATE
cd /work
echo '=== turn-listing'
timeout 150 codex exec --skip-git-repo-check 'study-listing' 2>&1 | tail -30
echo '=== turn-skill-edited'
timeout 150 codex exec --skip-git-repo-check '$p-a:ps-a study-skill' 2>&1 | tail -4
echo '=== launch-log'
cat /work/mcp-launch.log 2>&1
echo '=== cache-a'
{tree(HOME + "/.codex/plugins/cache/study/p-a")}
"""


def run(cfg, prov_ip):
    env = {"DISCOVERY": "1", "RESPONSE_TEXT": "STUDY_DONE"}
    prov_ip = common.start_provider(cfg, "static", env) or prov_ip
    cmd = codex_container(cfg, prov_ip, FINAL, plugins="", tty=False)
    out, log = run_container(cfg, cmd, "codex-mcp", timeout=1500)
    summarize(cfg, "codex-mcp", out, log, listing_prompt="study-listing")
    done(cfg)
