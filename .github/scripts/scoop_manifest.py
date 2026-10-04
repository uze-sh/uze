"""The Scoop manifest for a release that carries the Windows archives.

Written beside the archives it names, from their own bytes, so the hashes
in it are the ones SHASUMS256.txt lists:

    scoop_manifest.py <assets-dir> <tag>

`scoop install https://github.com/uze-sh/uze/releases/download/<tag>/uze.json`
installs exactly that release; `checkver` and `autoupdate` let a bucket
follow later ones.
"""

import hashlib
import json
import sys
from pathlib import Path

REPOSITORY = "https://github.com/uze-sh/uze"
ARCHITECTURES = {"64bit": "x86_64", "arm64": "aarch64"}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def manifest(assets: Path, tag: str) -> dict:
    version = tag.removeprefix("v")
    download = f"{REPOSITORY}/releases/download/{tag}"
    architecture = {}
    for scoop, arch in ARCHITECTURES.items():
        archive = assets / f"uze-{arch}-windows.zip"
        architecture[scoop] = {
            "url": f"{download}/{archive.name}",
            "hash": sha256(archive),
        }
    return {
        "version": version,
        "description": "The package manager and workspace for coding agents",
        "homepage": "https://uze.sh",
        "license": "Apache-2.0",
        "notes": "uze needs Git for Windows: scoop install git",
        "architecture": architecture,
        "bin": "uze.exe",
        "checkver": {"github": REPOSITORY},
        "autoupdate": {
            "architecture": {
                scoop: {
                    "url": f"{REPOSITORY}/releases/download/v$version/uze-{arch}-windows.zip"
                }
                for scoop, arch in ARCHITECTURES.items()
            },
            "hash": {"url": "$baseurl/SHASUMS256.txt"},
        },
    }


def main(argv: list[str]) -> int:
    assets, tag = Path(argv[1]), argv[2]
    (assets / "uze.json").write_text(json.dumps(manifest(assets, tag), indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
