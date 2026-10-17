"""The Lab image is the checkout it runs against, or it is rebuilt.

The image carries a release build of `uze` made from the repository at the
time it was built. A run against an image built from another source tests a
binary that is not the one being changed — and reads exactly like a
regression of the change, which is how a stale image has cost whole
debugging sessions more than once. So the image is labelled with a digest
of everything its build reads, and every run compares that label with the
checkout as it is on disk, uncommitted files included, before anything is
driven. A mismatch rebuilds the image; nothing runs on a stale one.

`UZE_LAB_IMAGE` naming another image is the one deliberate exception — the
way to rerun a scenario against an older build — and is said aloud.
"""

import hashlib
import os
import subprocess
import sys

LABEL = "sh.uze.lab.source"

#: Set by the run that checked, so the legs `--all` starts do not each
#: rebuild the image the first one just verified.
CHECKED = "UZE_LAB_IMAGE_CHECKED"

#: Paths the image's build does not read. Everything else the checkout
#: holds is in the digest: a file wrongly left in costs a rebuild, one
#: wrongly left out costs a run against the wrong binary.
_NOT_BUILT_FROM = (
    ".github/",
    "journeys/",
    "openspec/",
    "playground/",
    "web/",
)
#: Of `conformance/`, only what the Dockerfile copies.
_LAB_INPUTS = (
    "conformance/Dockerfile",
    "conformance/entrypoint.sh",
    "conformance/_fixtures/",
)


def repository_root():
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _built_from(path):
    if path.startswith("conformance/"):
        return path.startswith(_LAB_INPUTS)
    return not path.startswith(_NOT_BUILT_FROM)


def source_digest(root=None):
    """A digest of every tracked or untracked, not ignored, file the image
    is built from, by path and content."""
    root = root or repository_root()
    listed = subprocess.run(
        [
            "git",
            "-C",
            root,
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        check=True,
        capture_output=True,
    ).stdout.split(b"\0")
    digest = hashlib.sha256()
    for raw in sorted(path for path in listed if path):
        path = raw.decode("utf-8", "surrogateescape")
        if not _built_from(path):
            continue
        absolute = os.path.join(root, path)
        if not os.path.isfile(absolute):
            continue  # deleted in the working tree: not in the build either
        digest.update(raw + b"\0")
        with open(absolute, "rb") as handle:
            digest.update(hashlib.sha256(handle.read()).digest())
    return digest.hexdigest()[:16]


def image_source(image):
    """The digest the image was built from, or `None` for an image that
    does not exist or carries no label."""
    found = subprocess.run(
        [
            "docker",
            "image",
            "inspect",
            "-f",
            f'{{{{ index .Config.Labels "{LABEL}" }}}}',
            image,
        ],
        capture_output=True,
        text=True,
    )
    value = found.stdout.strip()
    return value if found.returncode == 0 and value and value != "<no value>" else None


def build(image, digest, root=None):
    root = root or repository_root()
    subprocess.run(
        [
            "docker",
            "build",
            "-f",
            os.path.join(root, "conformance", "Dockerfile"),
            "--build-arg",
            f"UZE_SOURCE_DIGEST={digest}",
            "-t",
            image,
            root,
        ],
        check=True,
    )


def ensure_current():
    """Rebuilds the Lab image unless it was built from this checkout."""
    if os.environ.get(CHECKED):
        return
    # Here rather than at the top: CI computes the digest before the Lab's
    # runner dependencies are installed.
    from shared import common

    image = common.HARNESS_IMAGE
    if os.environ.get("UZE_LAB_IMAGE"):
        print(
            f"lab image: {image} pinned by UZE_LAB_IMAGE; not checked against this checkout",
            file=sys.stderr,
        )
        os.environ[CHECKED] = "1"
        return
    wanted = source_digest()
    found = image_source(image)
    if found != wanted:
        reason = "has no source label" if found is None else f"was built from {found}"
        print(
            f"lab image: {image} {reason}; this checkout is {wanted}. Rebuilding.",
            file=sys.stderr,
        )
        build(image, wanted)
        if image_source(image) != wanted:
            raise SystemExit(
                f"lab image: {image} still does not carry {wanted} after a rebuild"
            )
    os.environ[CHECKED] = "1"
