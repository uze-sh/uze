#!/usr/bin/env bash
# Starts a fresh, throwaway WSL distribution (Ubuntu 24.04) with this
# checkout's uze installed the way a person installs it and the playground
# marketplace registered, and opens a shell in it. Run from WSL:
#
#   make playground-linux          a new distribution, replacing the last one
#   make playground-linux-down     remove it
#
# The distribution is `uze-playground`; your own distributions are untouched.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
require_wsl

distro=uze-playground
wsl=/mnt/c/Windows/System32/wsl.exe
rootfs_url=https://cloud-images.ubuntu.com/wsl/releases/24.04/current/ubuntu-noble-wsl-amd64-wsl.rootfs.tar.gz
world="$(world_root linux)"
# wsl.exe forwards its working directory: from a Windows path, never this
# distribution's own. It also reads whatever stdin it inherits, which would
# swallow the rest of this script: only the final shell is given one.
wsl() { (cd /mnt/c && "$wsl" "$@" </dev/null); }
exists() { wsl --list --quiet 2>/dev/null | tr -d '\r\0' | grep -qx "$distro"; }

if [ "${1:-}" = --down ]; then
  exists && wsl --unregister "$distro" >/dev/null
  rm -rf "${world}/distro"
  say "Removed ${distro}"
  exit 0
fi

say "Building uze and the playground MCP server"
cd "$repo_root"
cargo build --locked --release --bin uze
cargo build --locked --release --features dev-servers --bin playground-mcp
built="${CARGO_TARGET_DIR:-${repo_root}/target}/release"

stage="${world}/stage"
say "Staging the world in ${stage}"
rm -rf "$stage"
mkdir -p "$stage"
archive="uze-$(uname -m)-linux-gnu.tar.gz"
tar -czf "$stage/$archive" -C "$built" uze
stage_release "$stage" "$stage/$archive"
stage_market "$stage"
cp "${repo_root}/install.sh" "${playground_root}/linux/prepare.sh" "$built/playground-mcp" "$stage/"
printf '%s' "$(version)" > "$stage/version"

rootfs="${world}/cache/$(basename "$rootfs_url")"
if [ ! -f "$rootfs" ]; then
  say "Downloading Ubuntu 24.04 for WSL (once)"
  mkdir -p "$(dirname "$rootfs")"
  curl -fL --progress-bar "$rootfs_url" -o "$rootfs.part"
  mv "$rootfs.part" "$rootfs"
fi

if exists; then
  say "Replacing the previous ${distro}"
  wsl --unregister "$distro" >/dev/null
fi
rm -rf "${world}/distro"
mkdir -p "${world}/distro"
say "Importing ${distro}"
# What wsl.exe says on success is about your own .wslconfig (it warns there
# that a sparse disk was not used, for one); only a failure is shown, in the
# UTF-16 it writes, made readable.
if ! imported="$(wsl --import "$distro" "$(wslpath -w "${world}/distro")" "$(wslpath -w "$rootfs")" --version 2 2>&1)"; then
  printf '%s\n' "$imported" | tr -d '\0\r' >&2
  die "could not import ${distro}"
fi

say "Preparing ${distro}"
wsl -d "$distro" --user root -- bash "${stage}/prepare.sh" "$stage"
# The default user is read when the distribution starts.
wsl --terminate "$distro" >/dev/null

say "Ready. Opening a shell in ${distro} (leave with exit; remove with make playground-linux-down)"
[ "${UZE_PLAYGROUND_NO_SHELL:-}" = 1 ] || exec "$wsl" -d "$distro" --cd "~"
