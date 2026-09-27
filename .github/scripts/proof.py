#!/usr/bin/env python3
"""Whether an end-to-end leg has already been proven against exactly these
inputs, and where.

A leg of the Lab or of the journeys is a function of what it reads: the part
of this repository that reaches it, plus whatever outside the repository it
drives — the harness binary the Lab image installed, the runner image the
journeys draw a terminal on. When none of that moved since a leg passed,
running it again can only fail for reasons of its own, so CI reuses the
verdict instead of spending the most expensive minutes in the pipeline on it.

The repository half is content, not paths touched: the key hashes the blob of
every tracked file the leg reads, so a pull request that reverts itself is the
same key as the commit it started from, a change to another vendor's
integration is not this vertical's input, and the push to `main` after a
merge is the key its pull request already proved.

A proof is an Actions artifact named by its key, carrying the run that earned
it. Not the Actions cache: a cache entry written by a pull request is visible
to that pull request alone, so `main` could never read one — and `main` after
a merge is exactly the run that repeats a verdict nobody needs twice. Only an
artifact from a run of this repository counts, never one from a fork.

    proof.py versions                       probe every harness in the Lab image
    proof.py conformance --versions J       key and prior proof of every leg
    proof.py journeys --platform P --shard I/N --runner R
                                            key and prior proof of one leg

`--fresh` skips the lookup, for a run that is itself the record. A harness
whose version could not be probed has no key: an `unknown` version cannot be
the same as anything, so it always runs.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import urllib.request

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

#: Bump to invalidate every recorded proof, e.g. when what a leg proves
#: changes in a way the inputs below cannot see. This file is deliberately
#: not one of those inputs: editing it would otherwise throw away every
#: proof over a comment, and a change to what a key covers already changes
#: every key it touches.
SCHEME = "v2"

HARNESSES = ("antigravity", "claude", "codex", "opencode")
PARTS = ("contract", "vendor")

#: The toolchain and the lockfile reach every leg that builds `uze`.
BUILD = (
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo/config.toml",
)

#: What the Lab image bakes, or what a vertical reads at run time.
LAB_READS = (
    *BUILD,
    "src/",
    "crates/",
    "plugins/",
    "tests/_fixtures/",
    "conformance/",
    "marketplace.json",
    ".github/workflows/conformance.yml",
)

#: Compiled into the image but never reached by a vertical. The terminal UI
#: and its extensions are what the journeys prove; the Lab drives the CLI.
LAB_NEVER_REACHES = (
    "src/ui/",
    "src/ui.rs",
    "crates/uze-extensions/",
    "conformance/tests/",
    "conformance/discovery/",
    "conformance/**/*.md",
)

#: What a journey drives, or drives it with. The documentation pages a
#: journey's `proves:` names are not here: `journey validate` resolves those
#: on every run and is never reused.
JOURNEY_READS = (
    *BUILD,
    "src/",
    "crates/",
    "plugins/",
    "tests/_fixtures/",
    "journeys/",
    "marketplace.json",
    ".github/workflows/journeys.yml",
    ".github/actions/journeys/",
)

JOURNEY_NEVER_REACHES = ("journeys/**/*.md",)


def vendor_paths(harness):
    return (
        f"crates/uze-integrations/src/{harness}.rs",
        f"crates/uze-integrations/src/{harness}/",
        f"conformance/harnesses/{harness}/",
        f"conformance/experiments/{harness}/",
    )


def excluding(paths):
    return [f":(exclude,glob){p}" if "*" in p else f":(exclude){p}" for p in paths]


def lab_pathspecs(harness):
    others = [p for h in HARNESSES if h != harness for p in vendor_paths(h)]
    return [*LAB_READS, *excluding([*LAB_NEVER_REACHES, *others])]


def journey_pathspecs():
    return [*JOURNEY_READS, *excluding(JOURNEY_NEVER_REACHES)]


def tree_digest(pathspecs, repo=REPO):
    """The content of every tracked file a leg reads: mode, blob and path, as
    the index records them — independent of the working tree."""
    listing = subprocess.run(
        ["git", "-C", repo, "ls-files", "--stage", "-z", "--", *pathspecs],
        capture_output=True,
        check=True,
    ).stdout
    return hashlib.sha256(listing).hexdigest()


def key(tier, name, outside, digest):
    """`outside` is what the leg drives that the repository does not hold.
    Unknown is not a value: a leg that cannot say what it ran against has no
    key, and runs."""
    if not outside or outside == "unknown":
        return ""
    identity = hashlib.sha256(f"{tier}\0{name}\0{outside}\0{digest}".encode())
    return f"proof-{SCHEME}-{tier}-{name}-{identity.hexdigest()[:40]}"


def proven_by(proof_key, env=os.environ):
    """The run that recorded this proof, or "" — including when the question
    cannot be asked, which only costs running the leg."""
    token = env.get("GITHUB_TOKEN")
    repository = env.get("GITHUB_REPOSITORY")
    repository_id = env.get("GITHUB_REPOSITORY_ID")
    if not (proof_key and token and repository and repository_id):
        return ""
    api = env.get("GITHUB_API_URL", "https://api.github.com")
    request = urllib.request.Request(
        f"{api}/repos/{repository}/actions/artifacts?name={proof_key}&per_page=20",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            artifacts = json.load(response).get("artifacts", [])
    except (OSError, ValueError) as error:
        print(
            f"::warning::proof lookup failed, running the leg: {error}", file=sys.stderr
        )
        return ""
    return first_proof(artifacts, int(repository_id), env)


def first_proof(artifacts, repository_id, env=os.environ):
    server = env.get("GITHUB_SERVER_URL", "https://github.com")
    for artifact in artifacts:
        run = artifact.get("workflow_run") or {}
        if artifact.get("expired") or run.get("head_repository_id") != repository_id:
            continue
        return f"{server}/{env['GITHUB_REPOSITORY']}/actions/runs/{run['id']}"
    return ""


def probe_versions():
    sys.path.insert(0, os.path.join(REPO, "conformance"))
    from shared import common

    class Probe:
        def __init__(self, harness):
            self.harness = harness

    return {h: common.probe_harness_version(Probe(h)) for h in HARNESSES}


def conformance_plan(versions, fresh, repo=REPO):
    plan = {}
    for harness in HARNESSES:
        digest = tree_digest(lab_pathspecs(harness), repo)
        version = versions.get(harness, "unknown")
        for part in PARTS:
            leg_key = key("lab", f"{harness}-{part}", version, digest)
            plan[f"{harness}-{part}"] = {
                "key": leg_key,
                "version": version,
                "proven": "" if fresh else proven_by(leg_key),
            }
    return plan


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("versions", help="probe every harness version in the Lab image")
    lab = commands.add_parser(
        "conformance", help="the plan of every Lab leg, as outputs"
    )
    lab.add_argument("--versions", required=True, help="JSON from `proof.py versions`")
    lab.add_argument("--fresh", action="store_true", help="never reuse a proof")
    journeys = commands.add_parser("journeys", help="one journeys leg, as outputs")
    journeys.add_argument("--platform", required=True)
    journeys.add_argument("--shard", required=True, help="the I/N slice the leg runs")
    journeys.add_argument(
        "--runner", required=True, help="the runner image and version"
    )
    journeys.add_argument("--fresh", action="store_true", help="never reuse a proof")
    args = parser.parse_args(argv)

    if args.command == "versions":
        print(json.dumps(probe_versions(), separators=(",", ":")))
        return 0

    if args.command == "conformance":
        plan = conformance_plan(json.loads(args.versions or "{}"), args.fresh)
        pending = [leg for leg, entry in plan.items() if not entry["proven"]]
        print(f"plan={json.dumps(plan, separators=(',', ':'))}")
        print(f"pending={len(pending)}")
        return 0

    name = f"{args.platform}-{args.shard.replace('/', 'of')}"
    leg_key = key("journeys", name, args.runner, tree_digest(journey_pathspecs()))
    print(f"key={leg_key}")
    print(f"proven={'' if args.fresh else proven_by(leg_key)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
