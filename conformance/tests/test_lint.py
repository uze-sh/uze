#!/usr/bin/env python3
"""The Lab's own rules, enforced on its source rather than left to review.

Every rule here is a way the Lab once went green on a harness that did not
do what the check claimed. The conformance-debug skill already asked for
most of them in prose — "gate every absence on a presence" — and the
OpenCode hook phase shipped without it anyway. A rule a reviewer has to
remember is a rule that holds until the first busy week.

Run without the Lab: `python3 conformance/tests/test_lint.py`.
"""

import ast
import os
import re
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

#: Where checks are written: the contracts, every vertical, every experiment.
CHECK_SOURCES = ("contract", "harnesses", "experiments")

#: Text that answers a vendor prompt a user would meet. Each occurrence must
#: carry `# decision: <id>` naming an entry in DECISIONS.md's "Prompts the
#: Lab answers" section — the only place an answered prompt is allowed to
#: live, so it is revisited when the vendor moves.
PROMPT_ANSWERS = (
    "--dangerously",
    "bypassPermissions",
    "--yolo",
    "--skip-git-repo-check",
    "trustedWorkspaces",
    "trust_level",
    "[features]",
    "--auto ",
    "permissions.allow",
)

DECISION = re.compile(r"#\s*decision:\s*([a-z0-9-]+)")


def python_files(*roots):
    for root in roots:
        for directory, _, files in os.walk(os.path.join(ROOT, root)):
            for name in sorted(files):
                if name.endswith(".py"):
                    yield os.path.join(directory, name)


def called(node, name):
    target = node.func
    return (isinstance(target, ast.Name) and target.id == name) or (
        isinstance(target, ast.Attribute) and target.attr == name
    )


def is_literal_bool(node):
    return isinstance(node, ast.Constant) and isinstance(node.value, bool)


def where(path, node):
    return f"{os.path.relpath(path, ROOT)}:{node.lineno}"


def calls(path):
    with open(path) as handle:
        tree = ast.parse(handle.read(), path)
    return [node for node in ast.walk(tree) if isinstance(node, ast.Call)]


def docstring_lines(source):
    """Line numbers inside a module, class or function docstring: prose
    about a flag is not a use of it."""
    lines = set()
    for node in ast.walk(ast.parse(source)):
        if isinstance(
            node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)
        ):
            body = node.body
            if (
                body
                and isinstance(body[0], ast.Expr)
                and isinstance(getattr(body[0], "value", None), ast.Constant)
                and isinstance(body[0].value.value, str)
            ):
                lines.update(range(body[0].lineno, body[0].end_lineno + 1))
    return lines


def decisions():
    """The ids under DECISIONS.md's "Prompts the Lab answers" section."""
    with open(os.path.join(ROOT, "DECISIONS.md")) as handle:
        text = handle.read()
    marker = "## Prompts the Lab answers"
    if marker not in text:
        return set()
    section = text.split(marker, 1)[1].split("\n## ", 1)[0]
    return set(re.findall(r"^### `([a-z0-9-]+)`", section, re.MULTILINE))


class VerdictsAreObservedTest(unittest.TestCase):
    def test_no_check_is_handed_a_literal_verdict(self):
        offenders = [
            where(path, node)
            for path in python_files(*CHECK_SOURCES)
            for node in calls(path)
            if called(node, "check")
            and len(node.args) > 1
            and is_literal_bool(node.args[1])
        ]
        self.assertEqual(
            offenders, [], "a verdict must be computed from what the run saw"
        )

    def test_no_check_carries_a_kind(self):
        offenders = [
            where(path, node)
            for path in python_files(*CHECK_SOURCES)
            for node in calls(path)
            if called(node, "check") and any(k.arg == "kind" for k in node.keywords)
        ]
        self.assertEqual(offenders, [], "a limitation is a measurement: use declare()")

    def test_every_absence_names_its_proof(self):
        offenders = [
            where(path, node)
            for path in python_files(*CHECK_SOURCES)
            for node in calls(path)
            if called(node, "check_absence")
            and not any(k.arg == "proof" for k in node.keywords)
        ]
        self.assertEqual(offenders, [], "an absence holds only beside a presence")

    def test_no_declaration_is_a_constant(self):
        offenders = [
            where(path, node)
            for path in python_files(*CHECK_SOURCES)
            for node in calls(path)
            if called(node, "declare")
            and len(node.args) > 1
            and is_literal_bool(node.args[1])
        ]
        self.assertEqual(offenders, [], "a declaration's `holds` is a measurement")


class MarkersAreProducedTest(unittest.TestCase):
    def provider_markers(self):
        """Every marker string a provider looks for in a request."""
        found = []
        for path in python_files("harnesses", "shared"):
            with open(path) as handle:
                tree = ast.parse(handle.read(), path)
            for node in ast.walk(tree):
                if not isinstance(node, ast.Assign):
                    continue
                names = [t.id for t in node.targets if isinstance(t, ast.Name)]
                if not any(name.endswith("_MARKERS") for name in names):
                    continue
                if isinstance(node.value, ast.List):
                    for item in node.value.elts:
                        if isinstance(item, ast.Constant) and isinstance(
                            item.value, str
                        ):
                            found.append((path, item))
        return found

    def test_no_marker_is_a_common_word(self):
        offenders = [
            f"{where(path, item)} `{item.value}`"
            for path, item in self.provider_markers()
            if re.fullmatch(r"[a-z]+", item.value)
        ]
        self.assertEqual(
            offenders, [], "a common word is in every request; it proves nothing"
        )

    def test_no_marker_appears_in_a_scripted_call(self):
        markers = {
            item.value for _, item in self.provider_markers() if len(item.value) > 3
        }
        offenders = []
        for path in python_files(*CHECK_SOURCES):
            with open(path) as handle:
                tree = ast.parse(handle.read(), path)
            for node in ast.walk(tree):
                if isinstance(node, ast.Constant) and isinstance(node.value, str):
                    # A scripted call's input: a JSON object naming a command.
                    text = node.value.lstrip()
                    if not text.startswith("{") or (
                        '"command"' not in text and "CommandLine" not in text
                    ):
                        continue
                    for marker in markers:
                        if marker in node.value:
                            offenders.append(f"{where(path, node)} `{marker}`")
        self.assertEqual(
            offenders,
            [],
            "a marker the scripted call carries is in the request already",
        )


class PromptsAreDecidedTest(unittest.TestCase):
    def test_every_answered_prompt_names_a_decision(self):
        known = decisions()
        offenders = []
        for directory in ("harnesses", "contract", "experiments", "shared"):
            for path in python_files(directory):
                with open(path) as handle:
                    source = handle.read()
                lines = source.splitlines()
                prose = docstring_lines(source)
                for number, line in enumerate(lines, 1):
                    if number in prose or line.lstrip().startswith("#"):
                        continue
                    if not any(answer in line for answer in PROMPT_ANSWERS):
                        continue
                    context = " ".join(lines[max(0, number - 3) : number])
                    found = DECISION.search(context)
                    if not found or found.group(1) not in known:
                        offenders.append(f"{os.path.relpath(path, ROOT)}:{number}")
        self.assertEqual(
            offenders, [], "a prompt the Lab answers needs a recorded decision"
        )


class LaunchesAreAPersonsTest(unittest.TestCase):
    def test_every_harness_is_launched_as_its_own_binary(self):
        """The contracts' sessions start the harness the way a person who
        only uses the package manager does: no workspace shim on `PATH`, no
        UZE home other than the one the packages went into."""
        import importlib
        import sys

        sys.path.insert(0, ROOT)
        offenders = []
        for harness in ("claude", "codex", "opencode", "antigravity"):
            module = importlib.import_module(f"harnesses.{harness}.bindings")
            for value in vars(module).values():
                launch = getattr(value, "launch", None)
                if isinstance(value, type) and isinstance(launch, str) and launch:
                    if "shims" in launch or "UZE_HOME=" in launch:
                        offenders.append(f"{harness}: {launch}")
        self.assertEqual(
            offenders, [], "a contract session must start the harness bare"
        )


class FixturesAnswerNothingTest(unittest.TestCase):
    def test_no_fixture_seeds_an_answer_without_a_decision(self):
        """A seeded file answers a prompt as surely as a flag does, and
        carries no comment to name its decision: a fixture that does must
        be named, by path, in the decision that allows it."""
        with open(os.path.join(ROOT, "DECISIONS.md")) as handle:
            recorded = handle.read()
        seeds = (
            "trustedWorkspaces",
            "hasTrustDialogAccepted",
            "trust_level",
            '"allow"',
        )
        offenders = []
        for directory, _, files in os.walk(os.path.join(ROOT, "harnesses")):
            if os.path.basename(directory) != "fixtures":
                continue
            for name in files:
                path = os.path.join(directory, name)
                try:
                    with open(path) as handle:
                        text = handle.read()
                except UnicodeDecodeError:
                    continue
                relative = os.path.relpath(path, ROOT)
                if any(seed in text for seed in seeds) and relative not in recorded:
                    offenders.append(relative)
        self.assertEqual(offenders, [], "a fixture seeds an answer no decision names")


if __name__ == "__main__":
    unittest.main()
