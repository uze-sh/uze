"""Makes a project to try the uze marketplace's example plugins in.

    python3 demo.py <directory>

The directory must not exist yet, or be empty. It becomes a small Git
repository, a tasks CLI, holding what each example reads:

- twelve weeks of commits by three authors, for `pulse`'s heatmap;
- a milestone in `.planning/`, one phase done and the next one planned, for
  `milestone`, whose drawer then offers to execute it;
- an `AGENTS.md`, so any harness opened there has something to read.

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
    "AGENTS.md": """# Tasks

A small command-line task list, kept to try uze's example plugins.

- `tasks.py` is the CLI: `python3 tasks.py add <text>`, `python3 tasks.py list`.
- `.planning/` holds the milestone the `milestone` plugin shows: plan and
  execute its phases with `/milestone:plan-phase N` and
  `/milestone:execute-phase N`.
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
    ".planning/STATE.md": """# State

Milestone: v1 · a tasks CLI
Phase: 2
Step: planned
""",
    ".planning/ROADMAP.md": """# Roadmap

- [x] **Phase 1: CLI skeleton**
- [ ] **Phase 2: Keep tasks in a file**
- [ ] **Phase 3: Mark a task done**
- [ ] **Phase 4: List only the pending ones**
""",
    ".planning/phases/01-cli-skeleton/01-PLAN.md": """# Phase 1 · CLI skeleton

1. Create `tasks.py` with a `main(argv)` entry.
2. `add <text>` appends a task.
3. `list` prints the tasks, numbered.
""",
    ".planning/phases/01-cli-skeleton/01-SUMMARY.md": """# Phase 1 · done

1. `tasks.py` has `main(argv)`.
2. `add` appends to an in-memory list.
3. `list` prints them numbered.
""",
    ".planning/phases/02-keep-tasks-in-a-file/02-PLAN.md": """# Phase 2 · Keep tasks in a file

1. Create `store.py` with `load()` and `save(tasks)` over `tasks.json`.
2. `add` appends and saves.
3. `list` reads from `tasks.json`.
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
    """Commits on most weekdays of the last twelve weeks, a few on weekends:
    enough for the heatmap to have quiet days, busy days and every tone."""
    chance = random.Random(12)
    today = datetime.date.today()
    day = today - datetime.timedelta(weeks=12)
    log = os.path.join(directory, "CHANGELOG.md")
    while day < today:
        weekend = day.weekday() >= 5
        count = chance.choice([0, 0, 1] if weekend else [0, 1, 2, 3, 4, 6, 9])
        for index in range(count):
            when = datetime.datetime.combine(day, datetime.time(9 + index % 9, chance.randint(0, 59)))
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
