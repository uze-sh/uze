#!/usr/bin/env python3
"""Lends a world the harness sessions signed in on this machine.

Run by up.sh as the world is staged: each harness's credentials are read
from this machine's home at that moment and written under <out>, laid out
as a home directory, for the world to copy into its user's and then
delete. Nothing is kept in the repository.

Only what lets a session start is lent: the access token, never the token
that renews it. Claude and Codex rotate that one on use, so a world that
renewed with its copy would sign this machine out; without it, a lent
session simply ends when its access token does, which outlasts a
playground. API keys, which renew nothing, are lent as they are.

Usage: sessions.py <out>. Prints one line per harness.
"""

import base64
import json
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

HOME = Path.home()


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def left(expires_at):
    """What remains of a token expiring at `expires_at` (epoch seconds)."""
    seconds = expires_at - time.time()
    if seconds <= 0:
        return None
    return f"{int(seconds // 3600)}h{int(seconds % 3600 // 60):02d}m left"


def jwt_expiry(token):
    try:
        payload = token.split(".")[1]
        claims = json.loads(
            base64.urlsafe_b64decode(payload + "=" * (-len(payload) % 4))
        )
        return float(claims["exp"])
    except (IndexError, KeyError, ValueError):
        return None


def rfc3339_expiry(text):
    # Go writes nanoseconds, which Python's parser does not take.
    text = re.sub(r"(\.\d{6})\d+", r"\1", text).replace("Z", "+00:00")
    try:
        return datetime.fromisoformat(text).astimezone(timezone.utc).timestamp()
    except ValueError:
        return None


def claude():
    stored = read_json(HOME / ".claude/.credentials.json") or {}
    oauth = stored.get("claudeAiOauth")
    if not oauth or not oauth.get("accessToken"):
        return None, "not signed in here"
    remaining = left(oauth.get("expiresAt", 0) / 1000)
    if not remaining:
        return None, "its access token has expired: run claude here once to renew it"
    lent = {
        k: v
        for k, v in oauth.items()
        if k not in ("refreshToken", "refreshTokenExpiresAt")
    }
    # The interactive client also asks its state file which account it is
    # signed in as, and opens on the sign-in flow without it; `claude -p`
    # never looks. Only those fields go, none of this machine's history.
    state = read_json(HOME / ".claude.json") or {}
    account = {
        key: state[key]
        for key in ("oauthAccount", "hasCompletedOnboarding", "lastOnboardingVersion")
        if key in state
    }
    return {
        ".claude/.credentials.json": {"claudeAiOauth": lent},
        ".claude.json": account,
    }, remaining


def codex():
    stored = read_json(HOME / ".codex/auth.json")
    tokens = (stored or {}).get("tokens") or {}
    if not tokens.get("access_token"):
        if stored and stored.get("OPENAI_API_KEY"):
            return {".codex/auth.json": stored}, "API key"
        return None, "not signed in here"
    remaining = left(jwt_expiry(tokens["access_token"]) or 0)
    if not remaining:
        return None, "its access token has expired: run codex here once to renew it"
    lent = dict(stored)
    lent["tokens"] = {**tokens, "refresh_token": ""}
    # Codex renews a session it last renewed long ago; this one is new.
    lent["last_refresh"] = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    return {".codex/auth.json": lent}, remaining


def opencode():
    stored = read_json(HOME / ".local/share/opencode/auth.json")
    if not stored:
        return None, "not signed in here"
    if any(
        entry.get("type") == "oauth"
        for entry in stored.values()
        if isinstance(entry, dict)
    ):
        stored = {
            name: {k: v for k, v in entry.items() if k != "refresh"}
            for name, entry in stored.items()
        }
    return {".local/share/opencode/auth.json": stored}, ", ".join(sorted(stored))


def antigravity():
    stored = read_json(HOME / ".gemini/antigravity-cli/antigravity-oauth-token")
    token = (stored or {}).get("token") or {}
    if not token.get("access_token"):
        return None, "not signed in here"
    remaining = left(rfc3339_expiry(token.get("expiry", "")) or 0)
    if not remaining:
        return None, "its access token has expired: run agy here once to renew it"
    lent = {**stored, "token": {**token, "refresh_token": ""}}
    return {".gemini/antigravity-cli/antigravity-oauth-token": lent}, remaining


def main():
    out = Path(sys.argv[1])
    for name, lend in (
        ("claude", claude),
        ("codex", codex),
        ("opencode", opencode),
        ("agy", antigravity),
    ):
        files, said = lend()
        if files is None:
            print(f"  {name:<9} not lent: {said}")
            continue
        for relative, content in files.items():
            path = out / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(content))
            path.chmod(0o600)
        print(f"  {name:<9} lent ({said})")


if __name__ == "__main__":
    main()
