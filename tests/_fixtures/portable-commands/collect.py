"""Collects one-line commands from public repositories, pinned to a commit.

Regenerates `corpus.tsv` beside it: every command keeps the repository,
commit and path it came from, so a row can be checked against its source.
Needs an authenticated `gh`. Large kinds are capped by a stable hash of the
command, so the sample does not depend on the order repositories answer in.

Usage: python3 -I collect.py <out.tsv>
"""

import base64
import hashlib
import json
import re
import subprocess
import sys

REPOS = [
    "tokio-rs/tokio",
    "denoland/deno",
    "vercel/next.js",
    "facebook/react",
    "microsoft/vscode",
    "vitejs/vite",
    "astral-sh/ruff",
    "astral-sh/uv",
    "BurntSushi/ripgrep",
    "sharkdp/bat",
    "nushell/nushell",
    "prettier/prettier",
    "eslint/eslint",
    "withastro/astro",
    "sveltejs/svelte",
    "vuejs/core",
    "pnpm/pnpm",
    "biomejs/biome",
    "tauri-apps/tauri",
    "zed-industries/zed",
    "helix-editor/helix",
    "casey/just",
    "rust-lang/rust-analyzer",
    "microsoft/TypeScript",
    "nodejs/node",
    "oven-sh/bun",
    "remix-run/react-router",
    "tailwindlabs/tailwindcss",
    "vercel/turborepo",
    "evanw/esbuild",
    "rust-lang/cargo",
    "alacritty/alacritty",
    "wezterm/wezterm",
    "starship/starship",
    "microsoft/playwright",
    "storybookjs/storybook",
    "angular/angular",
    "nestjs/nest",
    "trpc/trpc",
    "prisma/prisma",
    "supabase/supabase",
    "getsentry/sentry-javascript",
    "electron/electron",
    "mui/material-ui",
    "facebook/docusaurus",
    "jestjs/jest",
    "vitest-dev/vitest",
    "tokio-rs/axum",
    "serde-rs/serde",
    "clap-rs/clap",
    "ratatui/ratatui",
    "bevyengine/bevy",
    "microsoft/devcontainers-cli",
    "devcontainers/images",
    "github/docs",
    "home-assistant/frontend",
    "excalidraw/excalidraw",
    "n8n-io/n8n",
    "anthropics/claude-code",
    "openai/codex",
    "sst/opencode",
]


def gh(*args):
    result = subprocess.run(
        ["gh", "api", *args], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        return None
    return json.loads(result.stdout)


def content(repo, sha, path):
    data = gh(f"repos/{repo}/contents/{path}?ref={sha}")
    if not data or isinstance(data, list) or data.get("encoding") != "base64":
        return None
    return base64.b64decode(data["content"]).decode("utf-8", "replace")


def listing(repo, sha, path):
    data = gh(f"repos/{repo}/contents/{path}?ref={sha}")
    return data if isinstance(data, list) else []


def clean(command):
    return " ".join(command.split()) if "\n" not in command.strip() else None


def workflow_runs(text):
    # Single-line `run:` values only; a block scalar is a script, not a line.
    for match in re.finditer(r"^\s*-?\s*run:\s*(.+)$", text, re.MULTILINE):
        value = match.group(1).strip()
        if value in ("|", ">", "|-", ">-", "|+") or value.startswith(("|", ">")):
            continue
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
            value = value[1:-1]
        if "${{" in value:
            continue
        yield value


def strip_jsonc(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return re.sub(r"(?m)^\s*//.*$", "", text)


def justfile_lines(text):
    for line in text.splitlines():
        if not line.startswith((" ", "\t")):
            continue
        body = line.strip().lstrip("@-").strip()
        if not body or body.startswith(("#", "{{")) or "{{" in body:
            continue
        yield body


def collect(repo):
    meta = gh(f"repos/{repo}")
    if not meta:
        return
    branch = meta["default_branch"]
    commit = gh(f"repos/{repo}/commits/{branch}")
    if not commit:
        return
    sha = commit["sha"]
    for entry in listing(repo, sha, ".github/workflows")[:8]:
        if entry["name"].endswith((".yml", ".yaml")):
            text = content(repo, sha, entry["path"]) or ""
            for value in workflow_runs(text):
                yield "actions", repo, sha, entry["path"], value
    text = content(repo, sha, "package.json")
    if text:
        try:
            for value in (json.loads(text).get("scripts") or {}).values():
                if isinstance(value, str):
                    yield "package.json", repo, sha, "package.json", value
        except json.JSONDecodeError:
            pass
    for path in (".devcontainer/devcontainer.json", ".devcontainer.json"):
        text = content(repo, sha, path)
        if not text:
            continue
        try:
            data = json.loads(strip_jsonc(text))
        except json.JSONDecodeError:
            continue
        for key in (
            "onCreateCommand",
            "postCreateCommand",
            "postStartCommand",
            "updateContentCommand",
            "postAttachCommand",
        ):
            value = data.get(key)
            if isinstance(value, str):
                yield "devcontainer", repo, sha, path, value
    for path in ("justfile", "Justfile", ".justfile"):
        text = content(repo, sha, path)
        if text:
            for value in justfile_lines(text):
                yield "justfile", repo, sha, path, value
            break


CAPS = {"actions": 500, "package.json": 500}


def sample(rows):
    def stable(row):
        return hashlib.sha256(row[4].encode()).hexdigest()

    kept = []
    for kind in ("actions", "package.json", "devcontainer", "justfile"):
        group = sorted((row for row in rows if row[0] == kind), key=stable)
        kept += group[: CAPS.get(kind, len(group))]
    return sorted(kept, key=lambda row: (row[0], row[1], row[4]))


def main(out):
    seen = set()
    rows = []
    for repo in REPOS:
        for kind, name, sha, path, command in collect(repo):
            line = clean(command)
            if not line or line in seen:
                continue
            seen.add(line)
            rows.append((kind, name, sha, path, line))
        print(f"{repo}: {len(rows)} so far", file=sys.stderr)
    rows = sample(rows)
    with open(out, "w", encoding="utf-8") as handle:
        handle.write("kind\trepository\tcommit\tpath\tcommand\n")
        handle.writelines(
            "\t".join(field.replace("\t", " ") for field in row) + "\n" for row in rows
        )


if __name__ == "__main__":
    main(sys.argv[1])
