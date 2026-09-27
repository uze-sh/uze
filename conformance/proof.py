#!/usr/bin/env python3
"""Whether a vertical has already been proven against exactly these inputs.

A canonical run of one vertical is a function of two things: the harness
binary the Lab image installed, and the part of this repository that vertical
reads. When neither moved since a run that passed, running it again can only
fail for reasons of its own, so CI reuses that verdict instead of spending the
most expensive minutes in the pipeline to repeat it. A harness release, or a
change the vertical reaches, is a new key and therefore a real run.

The repository half is content, not paths touched: the key hashes the blob of
every tracked file the vertical reads, so a pull request that reverts itself
is the same key as the commit it started from, and a change to another
vendor's integration is not this vertical's input at all.

    proof.py versions                       probe every harness in the image
    proof.py key --harness h --part p --versions J
                                            the key this leg would prove

A harness whose version could not be probed has no key: an `unknown` version
cannot be the same as anything, so it always runs.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

#: Bump to invalidate every recorded proof, e.g. when what a run proves
#: changes in a way the inputs below cannot see.
SCHEME = "v1"

HARNESSES = ("antigravity", "claude", "codex", "opencode")

#: What the Lab image bakes, or what a vertical reads at run time: the same
#: surface `ci.yml`'s `changes` job calls `lab_surface`, plus the toolchain
#: files the image build reads.
READS = (
    "src/",
    "crates/",
    "plugins/",
    "tests/_fixtures/",
    "conformance/",
    "marketplace.json",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    ".github/workflows/conformance.yml",
)

#: Compiled into the image but never reached by a vertical. The terminal UI
#: and its extensions are what the journeys prove; the Lab drives the CLI.
NEVER_REACHED = (
    "src/ui/",
    "src/ui.rs",
    "crates/uze-extensions/",
    "conformance/tests/",
    "conformance/discovery/",
    "conformance/**/*.md",
)


def vendor_paths(harness):
    return (
        f"crates/uze-integrations/src/{harness}.rs",
        f"crates/uze-integrations/src/{harness}/",
        f"conformance/harnesses/{harness}/",
        f"conformance/experiments/{harness}/",
    )


def pathspecs(harness):
    others = [p for h in HARNESSES if h != harness for p in vendor_paths(h)]
    excluded = [*NEVER_REACHED, *others]
    return [
        *READS,
        *(f":(exclude,glob){p}" if "*" in p else f":(exclude){p}" for p in excluded),
    ]


def tree_digest(harness, repo):
    """The content of every tracked file this vertical reads: mode, blob and
    path, as the index records them — independent of the working tree."""
    listing = subprocess.run(
        ["git", "-C", repo, "ls-files", "--stage", "-z", "--", *pathspecs(harness)],
        capture_output=True,
        check=True,
    ).stdout
    return hashlib.sha256(listing).hexdigest()


def key(harness, version, digest, part="all"):
    if not version or version == "unknown":
        return ""
    identity = hashlib.sha256(
        f"{harness}\0{part}\0{version}\0{digest}".encode()
    ).hexdigest()
    return f"conformance-proof-{SCHEME}-{harness}-{part}-{identity[:40]}"


def probe_versions():
    from shared import common

    class Probe:
        def __init__(self, harness):
            self.harness = harness

    return {h: common.probe_harness_version(Probe(h)) for h in HARNESSES}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("versions", help="probe every harness version in the Lab image")
    key_parser = commands.add_parser(
        "key", help="print the proof key as GitHub outputs"
    )
    key_parser.add_argument("--harness", required=True, choices=HARNESSES)
    key_parser.add_argument(
        "--part", default="all", help="the half of the vertical the leg runs"
    )
    key_parser.add_argument(
        "--versions", required=True, help="JSON from `proof.py versions`"
    )
    key_parser.add_argument(
        "--repo", default=os.path.join(os.path.dirname(__file__), "..")
    )
    args = parser.parse_args(argv)

    if args.command == "versions":
        print(json.dumps(probe_versions(), separators=(",", ":")))
        return 0

    version = json.loads(args.versions or "{}").get(args.harness, "unknown")
    print(f"version={version}")
    digest = tree_digest(args.harness, args.repo)
    print(f"key={key(args.harness, version, digest, args.part)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
