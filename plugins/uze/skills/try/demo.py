"""Makes a project to try the uze marketplace's example plugins in.

    python3 demo.py <directory>

The directory must not exist yet, or be empty. It becomes a small Git
repository, a tasks CLI, holding what each example reads:

- twelve weeks of commits by three authors, for `contributions`' graph;
- a GSD milestone in `.planning/`, written the way GSD writes it, one phase
  done and the next one planned, for `gsd`, whose drawer then offers to
  execute it (the second plan stops at a checkpoint);
- an `AGENTS.md`, so any harness opened there has something to read;
- an empty `agents.yaml`, so the plugins installed there are declared by
  the project, and the workspace offers their extensions where it opens.

Nothing outside the directory is touched. Git is the only program it runs.
"""

import datetime
import os
import random
import subprocess
import sys

AUTHORS = [("ana", "ana@example.invalid"), ("bruno", "bruno@example.invalid"),
           ("carla", "carla@example.invalid")]

SUBJECTS = [
    "feat(cli): add a --done filter to list",
    "fix(store): keep the file when a write fails",
    "test(store): cover an empty tasks file",
    "docs: say where tasks are kept",
    "refactor(cli): parse arguments in one place",
    "feat(cli): print tasks in the order they were added",
    "fix(cli): refuse an empty task",
    "test(cli): cover add and list",
    "chore: ignore the tasks file",
    "perf(store): read the file once per command",
]

FILES = {
    "agents.yaml": "",
    "AGENTS.md": """# Tasks

A small command-line task list, kept to try uze's example plugins.

- `tasks.py` is the CLI: `python3 tasks.py add <text>`, `python3 tasks.py list`.
- `.planning/` holds a GSD (Get Shit Done) milestone, which the `gsd`
  plugin shows. Its steps are GSD's: install GSD to run them.
""",
    "tasks.py": '''"""A small task list: add a task, list them."""

import sys

TASKS = []


def main(argv):
    if argv[:1] == ["add"] and len(argv) > 1:
        TASKS.append(" ".join(argv[1:]))
    for number, task in enumerate(TASKS, 1):
        print(f"{number}. {task}")


if __name__ == "__main__":
    main(sys.argv[1:])
''',
    ".planning/STATE.md": """---
milestone: v1.0
milestone_name: a tasks CLI
status: ready_to_execute
stopped_at: Phase 2 planned
---

# Project State

Phase 2 of 4 is planned: its two plans are ready to execute.
""",
    ".planning/ROADMAP.md": """# Roadmap: a tasks CLI

## Phases

- [x] **Phase 1: CLI skeleton** - add and list tasks in memory
- [ ] **Phase 2: Keep tasks in a file** - tasks survive between runs
- [ ] **Phase 3: Mark a task done** - `done <n>`
- [ ] **Phase 4: List only the pending ones** - `list --pending`
""",
    ".planning/phases/01-cli-skeleton/01-CONTEXT.md": """# Phase 1: CLI skeleton - Context

One file, `tasks.py`, with `add` and `list`; nothing kept between runs yet.
""",
    ".planning/phases/01-cli-skeleton/01-01-PLAN.md": """---
phase: 01-cli-skeleton
plan: 01
wave: 1
autonomous: true
---

<tasks>
<task type="auto">
  <name>Create tasks.py with add and list</name>
</task>
</tasks>
""",
    ".planning/phases/01-cli-skeleton/01-01-SUMMARY.md": """# Phase 1 Plan 01: Summary

`tasks.py` has `main(argv)`; `add` appends to a list in memory and `list`
prints it numbered.
""",
    ".planning/phases/01-cli-skeleton/01-VERIFICATION.md": """# Phase 1: Verification

status: passed
""",
    ".planning/phases/02-keep-tasks-in-a-file/02-CONTEXT.md": """# Phase 2: Keep tasks in a file - Context

Tasks are kept in `tasks.json` beside the CLI, read once per command.
""",
    ".planning/phases/02-keep-tasks-in-a-file/02-01-PLAN.md": """---
phase: 02-keep-tasks-in-a-file
plan: 01
wave: 1
autonomous: true
---

<tasks>
<task type="auto">
  <name>Create store.py with load() and save(tasks) over tasks.json</name>
</task>
<task type="auto">
  <name>Make add save and list load</name>
</task>
</tasks>
""",
    ".planning/phases/02-keep-tasks-in-a-file/02-02-PLAN.md": """---
phase: 02-keep-tasks-in-a-file
plan: 02
wave: 2
autonomous: false
---

<tasks>
<task type="checkpoint:human-verify" gate="blocking">
  <name>Add a task, run list again: it is still there</name>
  <what-built>Tasks kept in tasks.json</what-built>
  <how-to-verify>
    1. python3 tasks.py add "buy milk"
    2. python3 tasks.py list
  </how-to-verify>
  <resume-signal>Type "approved" or describe issues</resume-signal>
</task>
</tasks>
""",
}


def git(directory, *args, when=None, author=None):
    environment = dict(os.environ)
    if when is not None:
        stamp = when.strftime("%Y-%m-%dT%H:%M:%S")
        environment["GIT_AUTHOR_DATE"] = environment["GIT_COMMITTER_DATE"] = stamp
    if author is not None:
        environment["GIT_AUTHOR_NAME"] = environment["GIT_COMMITTER_NAME"] = author[0]
        environment["GIT_AUTHOR_EMAIL"] = environment["GIT_COMMITTER_EMAIL"] = author[1]
    # A throwaway history: the person's signing key and hooks have no say
    # in it, and would make hundreds of commits take minutes.
    quiet = ["-c", "commit.gpgsign=false", "-c", "core.hooksPath=" + os.devnull]
    subprocess.run(["git", "-C", directory, *quiet, *args], check=True, env=environment,
                   stdout=subprocess.DEVNULL)


def write(directory, path, text):
    target = os.path.join(directory, path)
    os.makedirs(os.path.dirname(target), exist_ok=True)
    with open(target, "w", encoding="utf-8", newline="\n") as file:
        file.write(text)


def history(directory):
    """Commits on most weekdays of the last twelve weeks, a few on weekends,
    and a few earlier today: enough for the heatmap to have quiet days, busy
    days and every tone, and for today to have some."""
    chance = random.Random(12)
    now = datetime.datetime.now().replace(microsecond=0)
    today = now.date()
    day = today - datetime.timedelta(weeks=12)
    log = os.path.join(directory, "CHANGELOG.md")
    while day <= today:
        weekend = day.weekday() >= 5
        count = 3 if day == today else chance.choice([0, 0, 1] if weekend else [0, 1, 2, 3, 4, 6, 9])
        for index in range(count):
            when = (now - datetime.timedelta(minutes=40 * (count - index))) if day == today else \
                datetime.datetime.combine(day, datetime.time(9 + index % 9, chance.randint(0, 59)))
            subject = chance.choice(SUBJECTS)
            with open(log, "a", encoding="utf-8", newline="\n") as file:
                file.write(f"- {day.isoformat()}: {subject}\n")
            git(directory, "add", "CHANGELOG.md")
            git(directory, "commit", "-q", "-m", subject, when=when, author=chance.choice(AUTHORS))
        day += datetime.timedelta(days=1)


def main(argv):
    if len(argv) != 1:
        sys.exit("usage: demo.py <directory>")
    directory = os.path.abspath(os.path.expanduser(argv[0]))
    if os.path.exists(directory) and os.listdir(directory):
        sys.exit(f"{directory} is not empty: give a new directory")
    os.makedirs(directory, exist_ok=True)
    git(directory, "init", "-q", "-b", "main")
    start = datetime.datetime.combine(datetime.date.today() - datetime.timedelta(weeks=12, days=1),
                                      datetime.time(9))
    for path, text in FILES.items():
        write(directory, path, text)
    git(directory, "add", "-A")
    git(directory, "commit", "-q", "-m", "chore: start the tasks CLI", when=start, author=AUTHORS[0])
    history(directory)
    print(directory)


if __name__ == "__main__":
    main(sys.argv[1:])
