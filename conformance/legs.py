"""Every leg of the Lab, on this machine, the way CI runs them.

CI runs each harness's `contract` and `vendor` halves as eight parallel jobs,
and skips a leg already proven against the same inputs
(`.github/scripts/proof.py`). A local run that walks the four verticals one
after the other pays for both: the halves in series, and every leg again
though nothing it reads moved. This runs the legs as parallel processes (the
Lab nonces its networks and providers per process, so they share nothing but
the host) and reuses a local proof by the same key CI uses.

A key hashes the index, not the working tree, so a leg whose inputs have
uncommitted changes is never reused: it runs, and its proof is not recorded.

    python3 conformance/lab.py --all [--jobs N] [--fresh]
"""

import concurrent.futures
import importlib.util
import json
import os
import subprocess
import sys
import time

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
LAB = os.path.join(REPO, "conformance", "lab.py")


def _proof_module():
    path = os.path.join(REPO, ".github", "scripts", "proof.py")
    spec = importlib.util.spec_from_file_location("proof", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _proof_dir():
    cache = os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache")
    return os.path.join(cache, "uze", "lab-proofs")


def _uncommitted(pathspecs):
    listing = subprocess.run(
        ["git", "-C", REPO, "status", "--porcelain", "--", *pathspecs],
        capture_output=True,
        text=True,
    ).stdout
    return bool(listing.strip())


def _plan(proof, fresh):
    versions = proof.probe_versions()
    plan = []
    for harness in proof.HARNESSES:
        pathspecs = proof.lab_pathspecs(harness)
        digest = proof.tree_digest(pathspecs)
        dirty = _uncommitted(pathspecs)
        for part in proof.PARTS:
            leg_key = (
                ""
                if dirty
                else proof.key(
                    "lab", f"{harness}-{part}", versions.get(harness), digest
                )
            )
            recorded = os.path.join(_proof_dir(), leg_key) if leg_key else ""
            reused = bool(recorded) and not fresh and os.path.isfile(recorded)
            plan.append(
                {
                    "harness": harness,
                    "part": part,
                    "key": leg_key,
                    "recorded": recorded,
                    "reused": reused,
                    "why": "uncommitted inputs" if dirty else "",
                }
            )
    return plan


def _run_leg(leg, root):
    outdir = os.path.join(root, f"{leg['harness']}-{leg['part']}")
    os.makedirs(outdir, exist_ok=True)
    log = os.path.join(outdir, "lab.log")
    started = time.time()
    with open(log, "w") as out:
        code = subprocess.run(
            [
                sys.executable,
                LAB,
                "--harness",
                leg["harness"],
                "--part",
                leg["part"],
                "--retry-once",
            ],
            stdout=out,
            stderr=subprocess.STDOUT,
            env={**os.environ, "AGY_OUTDIR": outdir},
        ).returncode
    seconds = int(time.time() - started)
    if code == 0 and leg["recorded"]:
        os.makedirs(_proof_dir(), exist_ok=True)
        with open(leg["recorded"], "w") as f:
            json.dump({"at": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "log": log}, f)
    return code, seconds, log


def run_all(jobs, fresh):
    proof = _proof_module()
    plan = _plan(proof, fresh)
    root = os.path.join("/tmp/harness-conformance", f"all-{os.getpid()}")
    pending = [leg for leg in plan if not leg["reused"]]
    for leg in plan:
        label = f"{leg['harness']}-{leg['part']}"
        if leg["reused"]:
            print(f"=== {label}: already proven ({leg['key']})", flush=True)
        elif leg["why"]:
            print(f"=== {label}: runs ({leg['why']})", flush=True)
    print(f"=== {len(pending)} leg(s) to run, {jobs} at a time, in {root}", flush=True)
    failed = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        running = {pool.submit(_run_leg, leg, root): leg for leg in pending}
        for done in concurrent.futures.as_completed(running):
            leg = running[done]
            code, seconds, log = done.result()
            label = f"{leg['harness']}-{leg['part']}"
            verdict = "PASS" if code == 0 else "FAIL"
            print(f"{verdict} {label} in {seconds}s ({log})", flush=True)
            if code != 0:
                failed.append(label)
    print(
        f"=== {len(plan) - len(failed)}/{len(plan)} legs green"
        + (f"; failed: {', '.join(failed)}" if failed else ""),
        flush=True,
    )
    sys.exit(1 if failed else 0)
