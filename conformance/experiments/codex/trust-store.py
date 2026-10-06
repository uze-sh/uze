"""Where does Codex record the trust a person gives a project and its hooks?

UZE must tell a person which delivered hooks Codex still holds back and
whether Codex will load a project's `AGENTS.md` (`prove-plugin-delivery`,
group 7). It may only *read* the vendor's record of those decisions — never
write it — so the record's location and shape have to be measured, not
guessed.

A real session, answered the way a person answers it: the folder-trust
dialog and the hook review ("Trust all and continue"). Every file under
Codex's home is listed and the text ones printed before and after each
answer, so the diff names what each answer wrote.

Run:
  UZE_MARKETPLACE_MOUNT=$PWD/conformance/_fixtures/marketplace \\
    python3 conformance/lab.py --harness codex --experiment codex/trust-store
"""

import os
import subprocess

from contract.bindings import hook_prelude
from contract.tui import Tui
from harnesses.codex.bindings import CodexBindings
from harnesses.codex.scenarios import codex_container
from shared import common

PROJECT = "/work/project"

#: Every file under Codex's home, then each small text file's content.
SNAPSHOT = r"""
cd /work/home/.codex 2>/dev/null || exit 0
find . -type f | sort
for f in $(find . -type f -size -64k | sort); do
  case "$f" in *.sqlite*|*.db*|*.jsonl) continue ;; esac
  printf '\n===== %s =====\n' "$f"; cat "$f"
done
"""


def snapshot(cfg, name):
    out = subprocess.run(
        ["docker", "exec", cfg.harness_container, "sh", "-c", SNAPSHOT],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(os.path.join(cfg.outdir, f"trust-store-{name}.txt"), "w") as f:
        f.write(out)
    return out


def answer(tui, dialog, keys):
    """Waits for `dialog` on screen, answers it with `keys` as a person
    would, and returns whether it appeared."""
    _, _, seen = tui.wait_for(
        [dialog], tries=40, stop_on_death=True, squash_spaces=True
    )
    if seen:
        tui.child.send(keys)
    return bool(seen)


def run(cfg, prov_ip):
    prov_ip = common.start_provider(cfg, "static")
    final = f"{hook_prelude(PROJECT)}\ncd {PROJECT} && exec codex"
    cmd = codex_container(cfg, prov_ip, final, plugins="hook-rows")
    with Tui(cfg, cmd, "codex-trust-store") as tui:
        # Folder trust first (0.156+ wording), then the hook review.
        folder = answer(tui, "Trust this folder?", "")
        untouched = snapshot(cfg, "1-untouched")
        tui.submit()
        review = answer(tui, CodexBindings.HOOK_REVIEW, "")
        folder_trusted = snapshot(cfg, "2-folder-trusted")
        tui.child.send("2")
        tui.submit()
        tui.until(CodexBindings.ready_markers, tries=8)
        hooks_trusted = snapshot(cfg, "3-hooks-trusted")
        common.check(
            "trust-store-folder-asked", folder, "the folder-trust dialog appeared"
        )
        common.check("trust-store-review-asked", review, "the hook review appeared")
        common.check(
            "trust-store-folder-answer-recorded",
            folder_trusted != untouched,
            "trusting the folder changed Codex's home (see trust-store-*.txt)",
        )
        common.check(
            "trust-store-review-answer-recorded",
            hooks_trusted != folder_trusted,
            "trusting the hooks changed Codex's home (see trust-store-*.txt)",
        )
