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
# Each site is a release host: `<site>/latest` redirects to the release page
# of the tag in `<site>/latest-tag`, as GitHub's does, and every asset sits
# under `<site>/download/v<version>/`.
site="$work/site"

release_dir() { # $1=site prefix ('' for the root)  $2=version
  dir="$site${1:+/$1}/download/v$2"
  mkdir -p "$dir"
  printf '%s\n' "$dir"
}

latest_is() { # $1=site prefix  $2=version
  root="$site${1:+/$1}"
  mkdir -p "$root/tag"
  printf 'v%s' "$2" >"$root/latest-tag"
  printf 'release page\n' >"$root/tag/v$2"
}

make_fake_bin() { # $1=fake dir  $2=what `uze --version` prints after "uze "
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

# The binaries say which build they are after the version, so a test can
# tell which asset was installed: `uze 9.9.9 glibc`.
make_fake_bin "$work/macos-arm/fake-bin" "9.9.9 macos-arm"
make_fake_bin "$work/macos-intel/fake-bin" "9.9.9 macos-intel"
make_fake_bin "$work/glibc/fake-bin" "9.9.9 glibc"
make_fake_bin "$work/musl/fake-bin" "9.9.9 musl"
make_fake_bin "$work/pinned/fake-bin" "9.9.8 pinned"
make_fake_bin "$work/corrupt/fake-bin" "9.9.9 corrupt"
# A genuinely signed build of an older release.
make_fake_bin "$work/older/fake-bin" "9.9.7"

latest_is "" 9.9.9
latest="$(release_dir "" 9.9.9)"
pinned="$(release_dir "" 9.9.8)"
mk_tarball "$latest" x86_64-linux-gnu "$work/glibc/fake-bin"
mk_tarball "$latest" x86_64-linux-musl "$work/musl/fake-bin"
mk_tarball "$latest" aarch64-macos "$work/macos-arm/fake-bin"
mk_tarball "$latest" x86_64-macos "$work/macos-intel/fake-bin"
mk_tarball "$pinned" x86_64-linux-gnu "$work/pinned/fake-bin"

for name in bad unsigned forged; do
  latest_is "$name" 9.9.9
  mk_tarball "$(release_dir "$name" 9.9.9)" x86_64-linux-gnu "$work/corrupt/fake-bin"
done
bad="$(release_dir bad 9.9.9)"
unsigned="$(release_dir unsigned 9.9.9)"
forged="$(release_dir forged 9.9.9)"

# ENV-2 of the 2026-10-30 audit: an older, genuinely signed build published
# under a newer tag (and under a pinned one) verifies against its checksums
# and its signature, and only its own version tells it apart.
latest_is rollback 9.9.9
rollback="$(release_dir rollback 9.9.9)"
rollback_pinned="$(release_dir rollback 9.9.8)"
mk_tarball "$rollback" x86_64-linux-gnu "$work/older/fake-bin"
mk_tarball "$rollback_pinned" x86_64-linux-gnu "$work/older/fake-bin"

# A host that does not say which release is the latest.
mkdir -p "$site/untagged"

for dir in "$latest" "$pinned" "$bad" "$unsigned" "$forged" "$rollback" "$rollback_pinned"; do
  mk_sums "$dir"
done

# Corrupt every checksum of the "bad" site, and sign what is left: the
# checksum is what must refuse it.
sed -i 's/^/00/' "$bad/SHASUMS256.txt"

for dir in "$latest" "$pinned" "$bad" "$rollback" "$rollback_pinned"; do
  sign "$dir" "$work/release-key"
done
# Signed by a key that is not the release's: whoever can publish checksums
# can publish a signature, and only the key says whose it is.
sign "$forged" "$work/stranger-key"

# --- local server ---------------------------------------------------------------
# A static server that answers `<site>/latest` the way the release host
# does: a redirect to the release page of the latest tag.
cat >"$work/serve.py" <<'PY'
import http.server, os, sys

root = sys.argv[1]

class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=root, **kwargs)

    def do_GET(self):
        if self.path.endswith("/latest"):
            prefix = self.path[: -len("/latest")]
            marker = os.path.join(root, prefix.lstrip("/"), "latest-tag")
            if os.path.isfile(marker):
                with open(marker) as tag:
                    self.send_response(302)
                    self.send_header("Location", f"{prefix}/tag/{tag.read().strip()}")
                    self.end_headers()
                    return
            self.send_error(404)
            return
        super().do_GET()

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(f"serving on port {server.server_address[1]}", file=sys.stderr, flush=True)
server.serve_forever()
PY
# `-u`: the startup line must reach the log file immediately, or the port
# handshake below would stall on Python's block-buffered redirected stderr.
python3 -u -I "$work/serve.py" "$site" >"$work/server.out" 2>&1 &
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
"$work/bin1/uze" --version | grep -q "9.9.9 glibc"
check "installed binary is the glibc artifact" $?
grep -q "download/v9.9.9/${archive_glibc}" "$work/out1.log"
check "latest is resolved to its release, and downloaded from there" $?
grep -q "Latest release is v9.9.9" "$work/out1.log"
check "and the resolved release is reported" $?
grep -q "Downloaded ${archive_glibc}" "$work/out1.log"
check "each step reports what it settled on" $?
# The updater replaces the file this names and no other, so it has to name
# the one that was actually written, resolved the way `canonicalize` will.
receipt="$work/home1/state/install.json"
grep -qF "\"binary\": \"$(cd "$work/bin1" && pwd -P)/uze\"" "$receipt"
check "the install leaves a receipt naming the file it placed" $?
grep -qF '"version": "9.9.9"' "$receipt"
check "and the release it placed there" $?
# The install reports the version the binary itself reports, not the
# step's own generic wording — the fixture binary prints "uze 9.9.9 glibc".
grep -q "uze 9.9.9 glibc" "$work/out1.log"
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
"$work/bin2/uze" --version | grep -q "9.9.9 musl"
check "installed binary is the musl artifact" $?
grep -q "linux-musl" "$work/out2.log"
check "the musl platform is selected" $?

# Pinned version resolves the /v<version>/ path.
run_installer "$work/out3.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin3" \
  UZE_VERSION="9.9.8"
check "pinned-version install succeeds" $?
"$work/bin3/uze" --version | grep -q "9.9.8 pinned"
check "installed binary is the pinned artifact" $?
grep -q "download/v9.9.8/" "$work/out3.log"
check "pinned release URL shape is used" $?

# ENV-2: a genuinely signed older build under the latest tag, or under a
# pinned one, is refused, and a binary already installed is left alone.
for case_name in latest pinned; do
  bin_dir="$work/bin-rollback-${case_name}"
  mkdir -p "$bin_dir"
  printf 'previous\n' >"$bin_dir/uze"
  if [ "$case_name" = pinned ]; then
    set -- UZE_VERSION=9.9.8
  else
    set --
  fi
  if run_installer "$work/out-rollback-${case_name}.log" UZE_BASE_URL="$base/rollback" \
    UZE_BIN_DIR="$bin_dir" UZE_HOME="$work/home-rollback-${case_name}" "$@"; then
    check "an older build under the ${case_name} release is refused" 1
  else
    check "an older build under the ${case_name} release is refused" 0
  fi
  grep -q "reports 9.9.7, not 9.9." "$work/out-rollback-${case_name}.log"
  check "and both versions are named" $?
  [ "$(cat "$bin_dir/uze")" = previous ] && [ "$(ls -A "$bin_dir")" = uze ]
  check "and the installed binary is left as it was" $?
  if [ -e "$work/home-rollback-${case_name}/state/install.json" ]; then
    check "and no receipt is written" 1
  else
    check "and no receipt is written" 0
  fi
done

# A host that cannot say which release is the latest installs nothing.
if run_installer "$work/out-untagged.log" UZE_BASE_URL="$base/untagged" \
  UZE_BIN_DIR="$work/bin-untagged"; then
  check "an unresolvable latest release is refused" 1
else
  check "an unresolvable latest release is refused" 0
fi
if [ -e "$work/bin-untagged/uze" ]; then
  check "no binary installed when latest is unknown" 1
else
  check "no binary installed when latest is unknown" 0
fi

# A pinned version is a path segment, so it is a release version or nothing.
if run_installer "$work/out-badpin.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin-badpin" \
  UZE_VERSION="../../9.9.9"; then
  check "a pinned version that is not a release is refused" 1
else
  check "a pinned version that is not a release is refused" 0
fi

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
"$work/bin6/uze" --version | grep -q "9.9.9 macos-arm"
check "macOS on Apple Silicon resolved aarch64-macos" $?

fake_uname "$work/mac-intel-bin" "Darwin" "x86_64"
run_installer "$work/out7.log" UZE_BASE_URL="$base" UZE_BIN_DIR="$work/bin7" \
  PATH="$work/mac-intel-bin:$PATH"
check "macOS on Intel installs" $?
"$work/bin7/uze" --version | grep -q "9.9.9 macos-intel"
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

# Uninstall delegates to the binary when it is here, with the same mode.
mkdir -p "$work/bin-un"
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >"%s"\n' "$work/uninstall-args" >"$work/bin-un/uze"
chmod +x "$work/bin-un/uze"
env UZE_BIN_DIR="$work/bin-un" UZE_HOME="$work/home-un" sh "$keyed" --uninstall --purge >"$work/out-un.log" 2>&1
check "uninstall with the binary present succeeds" $?
grep -qx "uninstall --yes --purge" "$work/uninstall-args"
check "and runs uze uninstall --yes --purge" $?
env UZE_BIN_DIR="$work/bin-un" UZE_HOME="$work/home-un" sh "$keyed" --uninstall >"$work/out-un2.log" 2>&1
grep -qx "uninstall --yes" "$work/uninstall-args"
check "the default uninstall runs uze uninstall --yes" $?

# Without the binary, only the installer's own record goes, never ~/.uze.
mkdir -p "$work/home-gone/state" "$work/bin-gone"
printf '{}\n' >"$work/home-gone/state/install.json"
printf 'kept\n' >"$work/home-gone/state/packages.json"
env UZE_BIN_DIR="$work/bin-gone" UZE_HOME="$work/home-gone" sh "$keyed" --uninstall >"$work/out-gone.log" 2>&1
check "uninstall without the binary succeeds" $?
[ ! -e "$work/home-gone/state/install.json" ] && [ -f "$work/home-gone/state/packages.json" ]
check "and removes only the install record" $?
grep -q "uze uninstall --purge" "$work/out-gone.log"
check "and says how to remove every trace" $?
if env UZE_BIN_DIR="$work/bin-gone" sh "$keyed" --purge >"$work/out-purge-alone.log" 2>&1; then
  check "--purge alone is refused" 1
else
  check "--purge alone is refused" 0
fi

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]