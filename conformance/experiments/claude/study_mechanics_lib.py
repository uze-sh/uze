"""Shared fixture for the delivery-mechanics study (observation only).

Question: can UZE deliver a skill / agent / plugin / MCP script out of a
content-addressed, read-only store by *linking* (pnpm-style) instead of
copying, without any harness losing or refusing anything?

Every artifact is written once into `/work/store/<cell>/`, then delivered
to where the harness reads it by one mechanic:

  a   regular copy (cp -r)
  b   symlink per file (directories physical, every file a symlink)
  c   the whole directory a symlink
  d   hardlink per file (cp -al, same tmpfs)
  e   hardlink per file out of a read-only store (0444 files, 0555 dirs
      and executables) -- the delivered directories keep 0555
  f1  rendered files physical (SKILL.md, agent .md, manifests, .mcp.json),
      every other file a symlink
  f2  rendered files physical, every other file a hardlink

Markers carry the cell: `STUDY_BODY_<tag>` (skill body), `STUDY_AGENT_<tag>`
(agent body), `STUDY_REF_<tag>` (a references/ file), `STUDY_SCRIPT_<tag>`
(a scripts/ file's stdout). A rendered file gets ` gen` before ` end`, so a
body that reached the model says which copy it came from. Bodies name
`${CLAUDE_PLUGIN_ROOT}`, `${PLUGIN_ROOT}` and `${CLAUDE_SKILL_DIR}` so the
wire shows which placeholder each harness substitutes, and with what.
"""

import json
import os
import re
import subprocess

from shared import common

MECHS = ["a", "b", "c", "d", "e", "f1", "f2"]

FIXTURE = r"""
STORE=/work/store
mkdir -p $STORE /work/deliv
GEN_RE='(^|/)(SKILL\.md|plugin\.json|marketplace\.json|\.mcp\.json|mcp_config\.json|[^/]*\.agent\.md)$|/agents/[^/]*\.(md|toml)$'

skill_src() { # dir name tag
  mkdir -p "$1/scripts" "$1/references"
  cat > "$1/SKILL.md" <<EOF
---
name: $2
description: Study skill $2 for delivery-mechanic probing. Use when asked about $2.
---
STUDY_BODY_$3 cpr=\${CLAUDE_PLUGIN_ROOT} pr=\${PLUGIN_ROOT} csd=\${CLAUDE_SKILL_DIR} end
Read references/ref.md and run scripts/run.sh from this skill's directory.
EOF
  echo "STUDY_REF_$3" > "$1/references/ref.md"
  printf '#!/bin/sh\necho STUDY_SCRIPT_%s\n' "$3" > "$1/scripts/run.sh"
  chmod 755 "$1/scripts/run.sh"
}

agent_md() { # file name tag
  mkdir -p "$(dirname "$1")"
  cat > "$1" <<EOF
---
name: $2
description: Study agent $2 for delivery-mechanic probing.
---
STUDY_AGENT_$3 cpr=\${CLAUDE_PLUGIN_ROOT} pr=\${PLUGIN_ROOT} end
You are a study agent.
EOF
}

mcp_src() { # dir
  mkdir -p "$1"
  printf '#!/bin/sh\nexec /usr/local/bin/uze-mcp-conformance-fixture "$@"\n' > "$1/server.sh"
  chmod 755 "$1/server.sh"
}

make_ro() { # the pnpm store: nothing in it is writable
  find "$1" -type f -perm -u+x -exec chmod 555 {} +
  find "$1" -type f ! -perm -u+x -exec chmod 444 {} +
  find "$1" -depth -type d -exec chmod 555 {} +
}

render() { # a rendered copy says so on its marker line
  sed 's/ end$/ gen end/' "$1" > "$2"
  if [ -x "$1" ]; then chmod 755 "$2"; fi
}

deliver() { # mech src dst
  m=$1; s=$2; d=$3
  mkdir -p "$(dirname "$d")"
  case $m in
    a) cp -r "$s" "$d" ;;
    c) ln -s "$s" "$d" ;;
    d|e) cp -al "$s" "$d" ;;
    b|f1|f2)
      (cd "$s" && find . -type d) | while read -r x; do mkdir -p "$d/$x"; done
      (cd "$s" && find . -type f) | while read -r x; do
        x=${x#./}
        if [ "$m" != b ] && echo "/$x" | grep -Eq "$GEN_RE"; then
          render "$s/$x" "$d/$x"
        elif [ "$m" = f2 ]; then
          ln "$s/$x" "$d/$x"
        else
          ln -s "$s/$x" "$d/$x"
        fi
      done ;;
  esac
}

deliver_file() { # mech src dst (a single file: an agent, an MCP script)
  m=$1; s=$2; d=$3
  mkdir -p "$(dirname "$d")"
  case $m in
    a) cp "$s" "$d" ;;
    b) ln -s "$s" "$d" ;;
    d|e) ln "$s" "$d" ;;
    f1|f2) render "$s" "$d" ;;
  esac
}

UP() { echo "$1" | tr a-z A-Z; }
"""

EVIDENCE_TAIL = r"""
echo '=== writes'
find /work/store /work/deliv $WATCH -newer /work/stamp -printf 'mtime %y %m %P %p\n' 2>/dev/null | sort
find /work/store /work/deliv $WATCH -cnewer /work/stamp -printf 'ctime %y %m %p\n' 2>/dev/null | sort
echo '=== store-inodes'
find /work/store -type f -printf '%i %n %m %p\n' | sort -k4
"""


def tree(path):
    """Shell listing a tree with type, mode, link count, inode, link target."""
    return f"find {path} -printf '%y %m n=%n i=%i %p -> %l\\n' 2>/dev/null | sort -k5"


def sections(stdout):
    out, name = {}, "pre"
    for line in stdout.splitlines():
        if line.startswith("=== "):
            name = line[4:].strip()
            out[name] = []
        else:
            out.setdefault(name, []).append(line)
    return {k: "\n".join(v) for k, v in out.items()}


def raw_log(cfg):
    return subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout


def blocks(log):
    return [b for b in re.split(r"^### ", log, flags=re.M)[1:]]


def marker_lines(text, prefix):
    """Every `<prefix>_<TAG> ... end` line seen, deduplicated, per tag."""
    found = {}
    # JSON-escaped bodies turn the newline into `\n`; the line ends at `end`.
    for m in re.finditer(prefix + r"_([A-Z0-9_]+)([^\n\"]{0,400}? end)", text):
        found.setdefault(m.group(1), set()).add(m.group(2).strip())
    return {k: sorted(v) for k, v in found.items()}


def markers(text, prefix):
    return sorted(set(re.findall(prefix + r"_([A-Z0-9_]+)", text)))


def run_container(cfg, cmd, name, timeout=900):
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=timeout
    )
    out = proc.stdout + "\n=== stderr\n" + proc.stderr
    with open(os.path.join(cfg.outdir, f"{name}.out"), "w") as f:
        f.write(out)
    log = raw_log(cfg)
    with open(os.path.join(cfg.outdir, f"{name}.requests.log"), "w") as f:
        f.write(log)
    return out, log


def summarize(cfg, name, out, log, listing_prompt=None, extra=None):
    """What reached the model, read from the wire, never from the harness."""
    listing = [b for b in blocks(log) if listing_prompt and listing_prompt in b]
    summary = {
        "requests": len(blocks(log)),
        "listing_requests": len(listing),
        "listed_study_names": sorted(
            set(
                re.findall(
                    r"(?:p-[a-z0-9]+:)?(?:study|ps|pa|pm)-[a-z0-9-]+", "".join(listing)
                )
            )
        ),
        "skill_bodies": marker_lines(log, "STUDY_BODY"),
        "agent_bodies": marker_lines(log, "STUDY_AGENT"),
        "refs_on_wire": markers(log, "STUDY_REF"),
        "scripts_on_wire": markers(log, "STUDY_SCRIPT"),
        "mcp_tools_on_wire": sorted(set(re.findall(r"mcp__[A-Za-z0-9_-]+", log))),
        "sections": sections(out),
    }
    summary.update(extra or {})
    with open(os.path.join(cfg.outdir, f"{name}.json"), "w") as f:
        json.dump(summary, f, indent=1)
    printable = {k: v for k, v in summary.items() if k != "sections"}
    print(json.dumps(printable, indent=1), flush=True)
    return summary


def done(cfg):
    common.check(
        "study-mechanics-ran", True, f"observation only; evidence in {cfg.outdir}"
    )
