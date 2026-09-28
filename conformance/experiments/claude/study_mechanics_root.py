"""Delivery-mechanics study, Claude follow-up: which copy does a plugin run from?

`study_mechanics` showed `${CLAUDE_PLUGIN_ROOT}` and `${CLAUDE_SKILL_DIR}`
resolving to the marketplace's source directory, not to
`~/.claude/plugins/cache`, and a plugin whose files are symlinks losing
its agents and its version. This separates the causes:

- p-edit (copy): after install, the source's SKILL.md and agent are edited
  and a new skill is added; which text reaches the model says which copy
  Claude reads, and when.
- p-gone (copy): after install, the source directory is deleted.
- p-agentlink: everything physical but `agents/pa-agentlink.md` a symlink.
- p-manifestlink: everything physical but `.claude-plugin/plugin.json` a
  symlink.
- p-skilllink: everything physical but `skills/.../SKILL.md` a symlink.

Run: python3 conformance/lab.py --harness claude --experiment claude/study_mechanics_root
"""

from experiments.claude.study_mechanics_lib import (
    FIXTURE,
    done,
    run_container,
    summarize,
    tree,
)
from harnesses.claude.scenarios import generate_certs
from shared import common

HOME = "/work/home"
CACHE = f"{HOME}/.claude/plugins/cache"
NAMES = ["edit", "gone", "agentlink", "manifestlink", "skilllink"]

CLAUDE = "timeout 150 claude --permission-mode bypassPermissions --output-format json"

SETUP = f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME={HOME} CLAUDE_CONFIG_DIR={HOME}/.claude UZE_HOME={HOME}/.uze
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
mkdir -p {HOME}/.claude
cp /app/fixtures/claude.json {HOME}/.claude.json
{FIXTURE}
mkdir -p /work/mkt/.claude-plugin /work/mkt/plugins
PL=""
for n in {" ".join(NAMES)}; do
  T=$(UP $n)
  P=$STORE/pl-$n
  mkdir -p $P/.claude-plugin
  echo '{{"name":"p-'$n'","version":"1.0.0","description":"study plugin '$n'"}}' > $P/.claude-plugin/plugin.json
  skill_src $P/skills/ps-$n ps-$n SKP_$T
  agent_md $P/agents/pa-$n.md pa-$n AGP_$T
  mcp_src $P/mcp
  echo '{{"mcpServers":{{"pm-'$n'":{{"command":"${{CLAUDE_PLUGIN_ROOT}}/mcp/server.sh"}}}}}}' > $P/.mcp.json
  cp -r $P /work/mkt/plugins/p-$n
  PL="$PL{{\\"name\\":\\"p-$n\\",\\"source\\":\\"./plugins/p-$n\\"}},"
done
D=/work/mkt/plugins
rm $D/p-agentlink/agents/pa-agentlink.md; ln -s $STORE/pl-agentlink/agents/pa-agentlink.md $D/p-agentlink/agents/pa-agentlink.md
rm $D/p-manifestlink/.claude-plugin/plugin.json; ln -s $STORE/pl-manifestlink/.claude-plugin/plugin.json $D/p-manifestlink/.claude-plugin/plugin.json
rm $D/p-skilllink/skills/ps-skilllink/SKILL.md; ln -s $STORE/pl-skilllink/skills/ps-skilllink/SKILL.md $D/p-skilllink/skills/ps-skilllink/SKILL.md
echo "{{\\"name\\":\\"study\\",\\"owner\\":{{\\"name\\":\\"lab\\"}},\\"plugins\\":[${{PL%,}}]}}" > /work/mkt/.claude-plugin/marketplace.json
set +e
claude plugin marketplace add /work/mkt >/dev/null 2>&1
echo '=== plugin-install'
for n in {" ".join(NAMES)}; do claude plugin install p-$n@study 2>&1; done
echo '=== turn-before-edit'
{CLAUDE} -p "/p-edit:ps-edit" | head -c 300; echo
echo '=== mutate-source'
sed -i 's/ end$/ edited end/' $D/p-edit/skills/ps-edit/SKILL.md $D/p-edit/agents/pa-edit.md
skill_src $D/p-edit/skills/ps-late ps-late SKP_LATE
rm -rf $D/p-gone
echo '=== plugin-list'
claude plugin list --json 2>&1
echo '=== mcp-list'
timeout 60 claude mcp list 2>&1
echo '=== cache-tree'
{tree(CACHE)}
echo '=== turn-listing'
{CLAUDE} -p 'study-listing' | head -c 300; echo
for n in {" ".join(NAMES)}; do
  echo "=== turn-skill-$n"; {CLAUDE} -p "/p-$n:ps-$n" | head -c 400; echo
  echo "=== turn-agent-$n"; {CLAUDE} --agent p-$n:pa-$n -p 'agent probe' | head -c 400; echo
done
echo '=== turn-skill-late'; {CLAUDE} -p "/p-edit:ps-late" | head -c 400; echo
"""


def run(cfg, prov_ip):
    env = {"DISCOVERY": "1", "RESPONSE_TEXT": "STUDY_DONE", "FINAL_TEXT": "STUDY_DONE"}
    prov_ip = common.start_provider(cfg, "static", env) or prov_ip
    cmd = common.docker_base(cfg, prov_ip, SETUP, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]
    out, log = run_container(cfg, cmd, "claude-root", timeout=1500)
    summarize(cfg, "claude-root", out, log, listing_prompt="study-listing")
    done(cfg)
