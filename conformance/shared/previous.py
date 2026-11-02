"""The `uze` a user upgrades from: the newest release this checkout descends
from, built the way the Lab image builds its own.

Upgrading is two binaries meeting on one disk: what the previous release
left behind, read by this build. A fixture written by a test proves a ladder
step, not the claim, so the previous release itself makes the install here.

It is built from the tag's own sources, read out of this repository with
`git archive` (no download, so no release's download count moves), in the
Lab image's own Rust image so it links against the same C library as the
harness container. The binary is kept per tag under the user's cache, so
the build is paid once per release.
"""

import os
import shutil
import subprocess
import tempfile

from shared.image import repository_root

CACHE = os.path.join(os.path.expanduser("~"), ".cache", "uze-lab", "previous")

#: Where the binary is mounted in a harness container that asks for it.
IN_CONTAINER = "/opt/uze-previous/uze"

DOCKERFILE = """# syntax=docker/dockerfile:1
FROM rust:1.97-bookworm
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/rustup \\
    --mount=type=cache,target=/usr/local/cargo/registry \\
    --mount=type=cache,id=uze-lab-previous-target,target=/src/target \\
    cargo build --locked --release --bin uze \\
    && mkdir -p /out && cp /src/target/release/uze /out/uze
"""


def tag(root=None):
    """The newest release tag this checkout descends from, or `None`."""
    found = subprocess.run(
        [
            "git",
            "-C",
            root or repository_root(),
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            "v*",
        ],
        capture_output=True,
        text=True,
    )
    return found.stdout.strip() or None if found.returncode == 0 else None


def binary(root=None):
    """The previous release's `uze` on the host, built on first use; `None`
    with the reason on stderr when it cannot be had."""
    root = root or repository_root()
    release = tag(root)
    if release is None:
        print("previous release: no `v*` tag reachable from HEAD", flush=True)
        return None
    kept = os.path.join(CACHE, release, "uze")
    if os.path.isfile(kept):
        return kept
    image = f"uze-lab-previous:{release.lstrip('v')}"
    with tempfile.TemporaryDirectory(prefix="uze-lab-previous-") as context:
        archive = subprocess.Popen(
            ["git", "-C", root, "archive", "--format=tar", f"{release}^{{commit}}"],
            stdout=subprocess.PIPE,
        )
        subprocess.run(["tar", "-x", "-C", context], stdin=archive.stdout, check=True)
        if archive.wait() != 0:
            print(f"previous release: cannot read the sources of {release}", flush=True)
            return None
        with open(os.path.join(context, "Dockerfile.lab-previous"), "w") as f:
            f.write(DOCKERFILE)
        built = subprocess.run(
            [
                "docker",
                "build",
                "-f",
                os.path.join(context, "Dockerfile.lab-previous"),
                "-t",
                image,
                context,
            ]
        )
        if built.returncode != 0:
            print(f"previous release: building {release} failed", flush=True)
            return None
    container = subprocess.run(
        ["docker", "create", image], capture_output=True, text=True, check=True
    ).stdout.strip()
    try:
        os.makedirs(os.path.dirname(kept), exist_ok=True)
        partial = kept + ".partial"
        subprocess.run(["docker", "cp", f"{container}:/out/uze", partial], check=True)
        os.chmod(partial, 0o755)
        shutil.move(partial, kept)
    finally:
        subprocess.run(["docker", "rm", "-f", container], capture_output=True)
    return kept
