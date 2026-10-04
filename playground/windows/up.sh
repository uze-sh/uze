#!/usr/bin/env bash
# Opens a fresh Windows Sandbox with this checkout's uze installed the way a
# person installs it and the playground marketplace registered, for an
# ordinary account (`UZE_PLAYGROUND_USER=admin` for the Sandbox's
# administrator, in Windows Terminal). Run from WSL: `make playground-windows`.
#
# uze.exe is cross-built here: the Windows SDK comes from `xwin` (installed
# into the playground's cache on first use, after you accept Microsoft's
# license) and links with the toolchain's own rust-lld.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
require_wsl

target=x86_64-pc-windows-msvc
[ -x /mnt/c/Windows/System32/WindowsSandbox.exe ] || die "Windows Sandbox is not enabled.
  In an administrator PowerShell, then restart Windows:
    Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All"

# --- the cross toolchain ---------------------------------------------------
rustup target list --installed | grep -qx "$target" || rustup target add "$target"
xwin="$(command -v xwin || echo "${cache_root}/tools/bin/xwin")"
if [ ! -x "$xwin" ]; then
  say "Installing xwin into ${cache_root}/tools"
  cargo install --locked --root "${cache_root}/tools" xwin
fi
sdk="${cache_root}/xwin"
if [ ! -d "${sdk}/crt" ]; then
  if [ "${UZE_PLAYGROUND_ACCEPT_XWIN_LICENSE:-}" != 1 ]; then
    echo "Building for Windows needs Microsoft's CRT and SDK, downloaded by xwin under"
    echo "the Microsoft Software License Terms (https://go.microsoft.com/fwlink/?LinkId=2086102)."
    read -r -p "Accept them and download (about 650 MB, once)? [y/N] " answer
    [ "$answer" = y ] || [ "$answer" = Y ] || die "the Windows build needs the SDK"
  fi
  "$xwin" --accept-license splat --output "$sdk"
fi
lld="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="$lld"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C linker-flavor=lld-link -C target-feature=+crt-static \
-Lnative=${sdk}/crt/lib/x86_64 -Lnative=${sdk}/sdk/lib/um/x86_64 -Lnative=${sdk}/sdk/lib/ucrt/x86_64"

# --- the build -------------------------------------------------------------
say "Building uze and the playground MCP server for Windows"
cd "$repo_root"
cargo build --locked --release --target "$target" --bin uze
cargo build --locked --release --target "$target" --features dev-servers --bin playground-mcp
built="${CARGO_TARGET_DIR:-${repo_root}/target}/${target}/release"

# --- the world -------------------------------------------------------------
stage="$(world_root windows)"
# Windows runs one Sandbox at a time and refuses a second without a word,
# and an open one holds the staged folder, so it is closed first: its world
# is disposable by definition.
if tasklist.exe /FI "IMAGENAME eq WindowsSandboxServer.exe" 2>/dev/null | grep -qi WindowsSandbox; then
  say "Closing the Sandbox already open"
  taskkill.exe /IM WindowsSandboxRemoteSession.exe /F >/dev/null 2>&1 || true
  taskkill.exe /IM WindowsSandboxServer.exe /F >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    tasklist.exe 2>/dev/null | grep -qi vmmemWindowsSandbox || break
    sleep 2
  done
fi

say "Staging the world in ${stage}"
rm -rf "$stage"
mkdir -p "$stage"
python3 - "$built/uze.exe" "$stage/uze-x86_64-windows.zip" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[2], "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(sys.argv[1], "uze.exe")
PY
stage_release "$stage" "$stage/uze-x86_64-windows.zip"
stage_market "$stage"
cp "${repo_root}/install.ps1" "${playground_root}/windows/prepare.ps1" \
  "${playground_root}/windows/setup-user.ps1" "$built/playground-mcp.exe" "$stage/"
printf '%s' "$(version)" > "$stage/version"
stage_sessions "$stage"
# Whose world it is: an ordinary account by default (see prepare.ps1).
user="${UZE_PLAYGROUND_USER:-standard}"
[ "$user" = standard ] || [ "$user" = admin ] || die "UZE_PLAYGROUND_USER is standard or admin"
printf '%s' "$user" > "$stage/user"

host_folder="$(wslpath -w "$stage")"
cat > "$stage/uze-playground.wsb" <<WSB
<Configuration>
  <Networking>Enable</Networking>
  <MemoryInMB>8192</MemoryInMB>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>${host_folder}</HostFolder>
      <SandboxFolder>C:\\playground</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\playground\\prepare.ps1</Command>
  </LogonCommand>
</Configuration>
WSB

say "Opening Windows Sandbox (preparing takes a minute; progress in ${stage}/prepare.log)"
(cd /mnt/c && cmd.exe /C start "" "$(wslpath -w "$stage/uze-playground.wsb")")
