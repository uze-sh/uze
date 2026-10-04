# Shared by playground/windows/up.sh and playground/linux/up.sh: where things
# live on the Windows side, the version being played with, and the two
# artifacts both worlds install from — a local release and a marketplace.
# Sourced, never run.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
playground_root="${repo_root}/playground"
cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/uze-playground"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

# Both worlds are started through Windows, so both need to be run from WSL.
require_wsl() {
  command -v wslpath >/dev/null 2>&1 && [ -x /mnt/c/Windows/System32/cmd.exe ] ||
    die "run this from WSL: it starts its world through Windows"
}

# A Windows environment variable's value, as WSL reaches the path it names.
windows_path_of() {
  local value
  value="$(cd /mnt/c && cmd.exe /C "echo %$1%" 2>/dev/null | tr -d '\r')"
  [ -n "$value" ] && [ "$value" != "%$1%" ] || die "Windows has no %$1%"
  wslpath -u "$value"
}

# Where a world is staged: on the Windows side, so the Sandbox can map it and
# `wsl.exe` can import from it.
world_root() {
  printf '%s/uze-playground/%s' "$(windows_path_of LOCALAPPDATA)" "$1"
}

version() {
  sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "${repo_root}/Cargo.toml"
}

# `<stage>/release/download/v<version>/<archive>` beside its SHASUMS256.txt:
# the layout install.sh and install.ps1 download from, served from a path.
stage_release() {
  local stage="$1" archive="$2"
  local dir="${stage}/release/download/v$(version)"
  mkdir -p "$dir"
  mv "$archive" "$dir/"
  (cd "$dir" && sha256sum "$(basename "$archive")" > SHASUMS256.txt)
}

# The playground plugin, laid out as a marketplace. The world makes it a Git
# repository itself, with its own Git, since a marketplace is one.
stage_market() {
  local market="$1/market"
  mkdir -p "$market/plugins"
  cp -R "${playground_root}/plugin" "$market/plugins/playground"
  cat > "$market/marketplace.json" <<'JSON'
{
  "name": "playground",
  "owner": { "name": "uze playground" },
  "plugins": [
    { "name": "playground", "source": "./plugins/playground", "category": "development" }
  ]
}
JSON
}
