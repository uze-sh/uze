#!/bin/sh
# Offline fixture test for install.sh.
#
# Serves synthetic release artifacts (fake `uze` binaries, real SHA-256
# sums signed by a throwaway release key, corrupt checksums, missing and
# forged signatures) over localhost HTTP and exercises the installer:
# glibc and musl detection, macOS on both architectures, pinned versions,
# signature and checksum refusal, and unsupported platform fail-closed
# paths. Zero network access required.
#
# The installer under test is install.sh with the throwaway key in place of
# the one it ships with; the shipped one is run too, to prove that a key it
# does not carry installs nothing.
#
# Usage: sh tests/scripts/installer-test.sh

set -eu

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
installer="$repo_root/install.sh"

need() {
  command -v "$1" >/dev/null 2>&1 || { printf 'missing: %s\n' "$1" >&2; exit 1; }
}
need python3
need curl
need tar
need sha256sum
need ssh-keygen

work="$(mktemp -d "${TMPDIR:-/tmp}/uze-installer-test.XXXXXX")"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ]; then
    kill "$server_pid" 2>/dev/null || true
  fi
  rm -rf -- "$work"
}
trap cleanup EXIT HUP INT TERM

# --- release keys ----------------------------------------------------------------
ssh-keygen -q -t ed25519 -N '' -C uze-release-test -f "$work/release-key"
ssh-keygen -q -t ed25519 -N '' -C stranger -f "$work/stranger-key"
release_key="$(cat "$work/release-key.pub")"

# install.sh as it would ship once a release key exists, with the
# throwaway key in that key's place.
keyed="$work/install.sh"
sed "s|^UZE_RELEASE_KEY=.*|UZE_RELEASE_KEY='${release_key}'|" "$installer" >"$keyed"
grep -qF "UZE_RELEASE_KEY='${release_key}'" "$keyed" || {
  printf 'install.sh has no UZE_RELEASE_KEY line to stand a key in\n' >&2
  exit 1
}

sign() { # $1=dir  $2=key
  rm -f "$1/SHASUMS256.txt.sig"
  ssh-keygen -q -Y sign -n uze-release -f "$2" "$1/SHASUMS256.txt" >/dev/null 2>&1
}

# --- fixture tree --------------------------------------------------------------
site="$work/site"
latest="$site/latest/download"
pinned="$site/download/v9.9.9"
bad="$site/bad/latest/download"
unsigned="$site/unsigned/latest/download"
forged="$site/forged/latest/download"
mkdir -p "$latest" "$pinned" "$bad" "$unsigned" "$forged"

make_fake_bin() { # $1=fake dir  $2=version string printed by `uze --version`
  mkdir -p "$1"
  printf '#!/bin/sh\nprintf "%%s\\n" "%s"\n' "uze $2" > "$1/uze"
  chmod +x "$1/uze"
}

mk_tarball() { # $1=dest dir  $2=platform  $3=fake bin dir
  (cd "$3" && tar -czf "$1/uze-$2.tar.gz" uze)
}

# Names are generated above (`uze-*.tar.gz`), so the glob can never match a
# leading dash that GNU sha256sum would misread as an option.
# shellcheck disable=SC2035
mk_sums() { (cd "$1" && sha256sum *.tar.gz > SHASUMS256.txt); }

make_fake_bin "$work/macos-arm/fake-bin" "9.9.9-macos-arm"
make_fake_bin "$work/macos-intel/fake-bin" "9.9.9-macos-intel"
make_fake_bin "$work/glibc/fake-bin" "9.9.9-glibc"
make_fake_bin "$work/musl/fake-bin" "9.9.9-musl"
make_fake_bin "$work/pinned/fake-bin" "9.9.9-pinned"
make_fake_bin "$work/corrupt/fake-bin" "9.9.9-corrupt"

mk_tarball "$latest" x86_64-linux-gnu "$work/glibc/fake-bin"
mk_tarball "$latest" x86_64-linux-musl "$work/musl/fake-bin"
mk_tarball "$pinned" x86_64-linux-gnu "$work/pinned/fake-bin"
mk_tarball "$bad" x86_64-linux-gnu "$work/corrupt/fake-bin"
mk_tarball "$latest" aarch64-macos "$work/macos-arm/fake-bin"
mk_tarball "$latest" x86_64-macos "$work/macos-intel/fake-bin"
mk_tarball "$unsigned" x86_64-linux-gnu "$work/corrupt/fake-bin"
mk_tarball "$forged" x86_64-linux-gnu "$work/corrupt/fake-bin"
mk_sums "$latest"
mk_sums "$pinned"
mk_sums "$bad"
mk_sums "$unsigned"
mk_sums "$forged"

# Corrupt every checksum of the "bad" site, and sign what is left: the
# checksum is what must refuse it.
sed -i 's/^/00/' "$bad/SHASUMS256.txt"

sign "$latest" "$work/release-key"
sign "$pinned" "$work/release-key"
sign "$bad" "$work/release-key"
# Signed by a key that is not the release's: whoever can publish checksums
# can publish a signature, and only the key says whose it is.
sign "$forged" "$work/stranger-key"

# --- local server ---------------------------------------------------------------
# `-u`: the startup line must reach the log file immediately, or the port
# handshake below would stall on Python's block-buffered redirected stderr.
python3 -u -m http.server 0 --bind 127.0.0.1 --directory "$site" \
  >"$work/server.out" 2>&1 &
server_pid=$!

port=""
i=0
while [ "$i" -lt 50 ]; do
  port="$(sed -n 's/.*port \([0-9][0-9]*\).*/\1/p' "$work/server.out" | tail -n 1)"
  [ -n "$port" ] && break
  i=$((i + 1))
  sleep 0.1
done
if [ -z "$port" ]; then
  printf 'http server failed to start\n' >&2
  cat "$work/server.out" >&2
  exit 1
fi
base="http://127.0.0.1:${port}"

# --- assertions -------------------------------------------------------------------
pass=0
fail=0
check() { # $1=description  $2=result (0 = pass)
  if [ "$2" -eq 0 ]; then
    pass=$((pass + 1))
    printf 'ok   - %s\n' "$1"
  else
    fail=$((fail + 1))
    printf 'FAIL - %s\n' "$1" >&2
  fi
}

run_installer() { # $1=log file; rest = KEY=VALUE environment overrides
  log="$1"
  shift
  env "$@" sh "$keyed" >"$log" 2>&1 || return $?
}

fake_uname() { # $1=fake bin dir  $2=os  $3=arch
  mkdir -p "$1"
  # `$1`/`$2`/`$3` here are written into the generated fake uname script.
  # shellcheck disable=SC2016
  printf '#!/bin/sh\ncase "$1" in\n  -m) echo %s ;;\n  *) echo %s ;;\nesac\n' "$3" "$2" > "$1/uname"
  chmod +x "$1/uname"
}

archive_glibc="uze-$(uname -m | sed 's/^amd64$/x86_64/;s/^arm64$/aarch64/')-linux-gnu.tar.gz"

# Syntax door check.
sh -n "$installer"
check "install.sh parses cleanly under /bin/sh" $?

# The installer as committed carries whatever key `release-signing.pub`
# does. Until that is a key, it must install nothing at all.
if ! grep -q "^UZE_RELEASE_KEY=''$" "$installer"; then
  check "the committed installer carries a release key" 0
elif env UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin0" sh "$installer" >"$work/out0.log" 2>&1; then
  check "an installer with no release key refuses to install" 1
else
  check "an installer with no release key refuses to install" 0
  grep -q "no release signing key" "$work/out0.log"
  check "and says it carries none" $?
fi

# Default (glibc, latest) happy path.
run_installer "$work/out1.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin1" UZE_HOME="$work/home1"
check "glibc/latest install succeeds" $?
"$work/bin1/uze" --version | grep -q "9.9.9-glibc"
check "installed binary is the glibc artifact" $?
grep -q "latest/download" "$work/out1.log"
check "latest release URL shape is used" $?
grep -q "Downloaded ${archive_glibc}" "$work/out1.log"
check "each step reports what it settled on" $?
# The updater replaces the file this names and no other, so it has to name
# the one that was actually written, resolved the way `canonicalize` will.
receipt="$work/home1/state/install.json"
grep -qF "\"binary\": \"$(cd "$work/bin1" && pwd -P)/uze\"" "$receipt"
check "the install leaves a receipt naming the file it placed" $?
grep -qF '"version": "9.9.9-glibc"' "$receipt"
check "and the release it placed there" $?
# The install ends on the version the binary itself reports, not on the
# step's own generic wording — the fixture binary prints "uze 9.9.9-glibc".
grep -q "9.9.9-glibc" "$work/out1.log"
check "the last step settles on the version installed" $?
grep -q "Agents come and go. Your work stays." "$work/out1.log"
check "the installer opens with the CLI's own header" $?
grep -q "uze setup" "$work/out1.log"
check "and closes on what to run next" $?
# Redirected output is not a terminal, so the installer owes the log a plain
# transcript: no colour, no spinner frames, nothing a `grep` in CI or a
# pasted-into-an-issue log has to be read around.
if grep -q "$(printf '\033')" "$work/out1.log"; then
  check "a redirected install writes no escape sequences" 1
else
  check "a redirected install writes no escape sequences" 0
fi

# musl detection via a fake `ldd` earlier on PATH.
make_fake_bin "$work/musl-bin" "unused"
printf '#!/bin/sh\necho "musl libc (x86_64) Version 1.2.5"\n' > "$work/musl-bin/ldd"
chmod +x "$work/musl-bin/ldd"
run_installer "$work/out2.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin2" \
  PATH="$work/musl-bin:$PATH"
check "musl install succeeds" $?
"$work/bin2/uze" --version | grep -q "9.9.9-musl"
check "installed binary is the musl artifact" $?
grep -q "linux-musl" "$work/out2.log"
check "the musl platform is selected" $?

# Pinned version resolves the /v<version>/ path.
run_installer "$work/out3.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin3" \
  UZE_VERSION="9.9.9"
check "pinned-version install succeeds" $?
"$work/bin3/uze" --version | grep -q "9.9.9-pinned"
check "installed binary is the pinned artifact" $?
grep -q "download/v9.9.9/" "$work/out3.log"
check "pinned release URL shape is used" $?

# Checksum mismatch must fail closed and leave nothing installed.
if run_installer "$work/out4.log" UZE_BASE_URL="$base/bad" UZE_BIN_DIR="$work/bin4"; then
  check "checksum mismatch is refused" 1
else
  check "checksum mismatch is refused" 0
fi
grep -q "checksum mismatch" "$work/out4.log"
check "checksum mismatch is diagnosed" $?
if [ -e "$work/bin4/uze" ]; then
  check "no binary installed on mismatch" 1
else
  check "no binary installed on mismatch" 0
fi

# A release with no signature, or one signed by any other key, is refused
# before anything is unpacked.
for site_name in unsigned forged; do
  if run_installer "$work/out-${site_name}.log" UZE_BASE_URL="$base/${site_name}" \
    UZE_BIN_DIR="$work/bin-${site_name}"; then
    check "the ${site_name} release is refused" 1
  else
    check "the ${site_name} release is refused" 0
  fi
  if [ -e "$work/bin-${site_name}/uze" ]; then
    check "no binary installed from the ${site_name} release" 1
  else
    check "no binary installed from the ${site_name} release" 0
  fi
done
grep -q "not signed by the uze release key" "$work/out-forged.log"
check "a forged signature is diagnosed" $?

# Unsupported architecture fails closed.
fake_uname "$work/arch-bin" "Linux" "mips"
if run_installer "$work/out5.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin5" \
  PATH="$work/arch-bin:$PATH"; then
  check "unsupported architecture is refused" 1
else
  check "unsupported architecture is refused" 0
fi
grep -q "unsupported architecture: mips" "$work/out5.log"
check "unsupported architecture is diagnosed" $?

# macOS, both architectures. `uname -m` says `arm64` on Apple Silicon where
# the asset says `aarch64`, and that mapping is the whole reason to assert
# the installed binary rather than the exit code: picking the wrong asset
# still exits zero right up until the download 404s.
fake_uname "$work/mac-arm-bin" "Darwin" "arm64"
run_installer "$work/out6.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin6" \
  PATH="$work/mac-arm-bin:$PATH"
check "macOS on Apple Silicon installs" $?
"$work/bin6/uze" --version | grep -q "9.9.9-macos-arm"
check "macOS on Apple Silicon resolved aarch64-macos" $?

fake_uname "$work/mac-intel-bin" "Darwin" "x86_64"
run_installer "$work/out7.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin7" \
  PATH="$work/mac-intel-bin:$PATH"
check "macOS on Intel installs" $?
"$work/bin7/uze" --version | grep -q "9.9.9-macos-intel"
check "macOS on Intel resolved x86_64-macos" $?

# macOS names no libc. A `-linux-` asset reaching a Mac would mean the
# platform string was assembled from the wrong branch.
grep -q "uze-x86_64-macos.tar.gz" "$work/out7.log"
check "macOS asks for a macOS asset, with no libc field" $?

# An OS that genuinely has no build still fails closed, before downloading.
fake_uname "$work/os-bin" "FreeBSD" "x86_64"
if run_installer "$work/out8.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin8" \
  PATH="$work/os-bin:$PATH"; then
  check "unsupported OS is refused" 1
else
  check "unsupported OS is refused" 0
fi
grep -q "unsupported OS: FreeBSD" "$work/out8.log"
check "unsupported OS is diagnosed" $?

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]