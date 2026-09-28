"""Delivery-mechanics study, Antigravity follow-up (observation only).

`study_mechanics` found agy 1.2.12 moving `~/.gemini/antigravity-cli/skills`
and `agents` to `~/.gemini/config/{skills,agents}` on a session start and
leaving symlinks behind. This measures what that move does to linked
deliveries (inode, link count, symlink targets, modes before and after),
whether delivering straight into `~/.gemini/config/` works for every
mechanic, how a plugin's skill and agent are named, and whether a nested
(physical or symlinked) agents sub-directory is discovered.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/study_mechanics_migrate
"""

from experiments.claude.study_mechanics_lib import (
    FIXTURE,
    MECHS,
    done,
    run_container,
    summarize,
    tree,
)
from harnesses.antigravity.scenarios import agy_setup
from shared import common

HOME = "/work/home"
OLD = f"{HOME}/.gemini/antigravity-cli"
NEW = f"{HOME}/.gemini/config"
PRINT = "timeout 150 agy --dangerously-skip-permissions --print-timeout 100s"

BODY = f"""
{FIXTURE}
MECHS="{" ".join(MECHS)}"
mkdir -p {OLD}/skills {OLD}/agents
for m in $MECHS; do
  T=$(UP $m)
  skill_src $STORE/skd-$m/study-$m study-$m SKD_$T
  skill_src $STORE/sk2-$m/study2-$m study2-$m SK2_$T
  agent_md $STORE/agd-$m/study-agent-$m.md study-agent-$m AGD_$T
  agent_md $STORE/ag2-$m/study2-agent-$m.md study2-agent-$m AG2_$T
done
agent_md $STORE/agd-nest/study-agent-nest.md study-agent-nest AGD_NEST
for p in a b; do
  P=$STORE/pl-$p; mkdir -p $P
  echo '{{"name":"p-'$p'"}}' > $P/plugin.json
  skill_src $P/skills/ps-$p ps-$p SKP_$(UP $p)
  agent_md $P/agents/pa-$p.md pa-$p AGP_$(UP $p)
done
for c in skd-e sk2-e agd-e ag2-e; do make_ro $STORE/$c; done
for m in $MECHS; do deliver $m $STORE/skd-$m/study-$m {OLD}/skills/study-$m; done
for m in a b d e; do deliver_file $m $STORE/agd-$m/study-agent-$m.md {OLD}/agents/study-agent-$m.md; done
ln -s $STORE/agd-c {OLD}/agents/sub-c
mkdir -p {OLD}/agents/sub-p; cp $STORE/agd-nest/study-agent-nest.md {OLD}/agents/sub-p/
set +e
agy plugin install $STORE/pl-a >/dev/null 2>&1; agy plugin install $STORE/pl-b >/dev/null 2>&1
echo '=== before'
{tree(OLD + "/skills")}
{tree(OLD + "/agents")}
cd /work
echo '=== turn-first'
{PRINT} --print 'study-first' 2>&1 | tail -3
echo '=== after'
find {HOME}/.gemini -maxdepth 2 \\( -name skills -o -name agents \\) -printf '%y %p -> %l\\n'
{tree(NEW + "/skills")}
{tree(NEW + "/agents")}
echo '=== deliver-into-config'
for m in $MECHS; do deliver $m $STORE/sk2-$m/study2-$m {NEW}/skills/study2-$m; done
for m in a b d e; do deliver_file $m $STORE/ag2-$m/study2-agent-$m.md {NEW}/agents/study2-agent-$m.md; done
ln -s $STORE/ag2-c {NEW}/agents/sub2-c
{tree(NEW + "/skills")}
echo '=== turn-listing'
{PRINT} --print 'study-listing' 2>&1 | tail -3
for m in $MECHS; do
  echo "=== turn-skill2-$m"; {PRINT} --print "/study2-$m" 2>&1 | tail -2
done
echo '=== turn-plugin-skill-qualified'; {PRINT} --print '/p-a:ps-a' 2>&1 | tail -2
echo '=== turn-plugin-skill-bare'; {PRINT} --print '/ps-b' 2>&1 | tail -2
echo '=== turn-plugin-agent-bare'; {PRINT} --agent pa-a --print 'agent probe' 2>&1 | tail -2
echo '=== turn-plugin-agent-qualified'; {PRINT} --agent p-b:pa-b --print 'agent probe' 2>&1 | tail -2
for a in study2-agent-a study2-agent-b study2-agent-c study2-agent-d study2-agent-e study-agent-nest; do
  echo "=== turn-agent-$a"; {PRINT} --agent $a --print 'agent probe' 2>&1 | tail -2
done
echo '=== turn-agent-missing'; {PRINT} --agent does-not-exist --print 'agent probe' 2>&1 | tail -3
"""


def run(cfg, prov_ip):
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"}) or prov_ip
    setup = agy_setup(cfg, prov_ip, include_mcp=False, plugins="", final_cmd=BODY)
    cmd = common.docker_base(cfg, prov_ip, setup, tty=False)
    out, log = run_container(cfg, cmd, "agy-migrate", timeout=2400)
    summarize(cfg, "agy-migrate", out, log, listing_prompt="study-listing")
    done(cfg)
