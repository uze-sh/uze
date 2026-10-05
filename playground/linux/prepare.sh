#!/usr/bin/env bash
# Runs as root inside the fresh distribution (see up.sh): an ordinary user
# with sudo, Git, then uze installed through install.sh from the staged
# release, the playground marketplace registered and a demo project.
set -euo pipefail
stage="$1"
user=person

export DEBIAN_FRONTEND=noninteractive
command -v git >/dev/null && command -v curl >/dev/null || {
  apt-get update -qq
  apt-get install -y -qq git curl ca-certificates >/dev/null
}

id "$user" >/dev/null 2>&1 || useradd --create-home --shell /bin/bash --groups sudo "$user"
echo "$user ALL=(ALL) NOPASSWD:ALL" > "/etc/sudoers.d/$user"
printf '[user]\ndefault=%s\n' "$user" > /etc/wsl.conf

if [ -d "$stage/sessions" ]; then
  cp -R "$stage/sessions/." "/home/$user/"
  chown -R "$user:$user" "/home/$user"
  find "$stage/sessions" -type f -printf '%P\n' | while read -r file; do chmod 600 "/home/$user/$file"; done
  rm -rf "$stage/sessions"
fi

su - "$user" -c "bash -s" <<SH
set -euo pipefail
git config --global user.name 'Playground Person'
git config --global user.email 'person@example.invalid'
git config --global init.defaultBranch main

UZE_BASE_URL='file://${stage}/release' UZE_VERSION='$(cat "$stage/version")' sh '${stage}/install.sh'
export PATH="\$HOME/.local/bin:\$PATH"
install -m 0755 '${stage}/playground-mcp' "\$HOME/.local/bin/playground-mcp"

cp -R '${stage}/market' "\$HOME/playground-market"
git -C "\$HOME/playground-market" init -q
git -C "\$HOME/playground-market" add -A
git -C "\$HOME/playground-market" commit -q -m 'playground marketplace'
uze market add "\$HOME/playground-market"

touch "\$HOME/.hushlogin"
mkdir -p "\$HOME/projects/demo"
git -C "\$HOME/projects/demo" init -q
printf '# Demo\n\nA project to try uze in.\n' > "\$HOME/projects/demo/AGENTS.md"
git -C "\$HOME/projects/demo" add -A
git -C "\$HOME/projects/demo" commit -q -m init

cat >> "\$HOME/.bashrc" <<'RC'
export PATH="\$HOME/.local/bin:\$PATH"
cat <<'WELCOME'
uze from your checkout, with Git and the playground marketplace.

  uze doctor
  uze setup claude-code            (or codex, opencode, antigravity)
  cd ~/projects/demo && uze install -m playground@playground
  uze workspace
WELCOME
RC
SH
