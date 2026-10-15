#!/usr/bin/env python3
"""The QML CI job must run every test in qml/tests (spec §17.1), so a regression in the
life UI, chooser, renderer, bar model or pet assets fails CI and not only a local run.

Run: python3 qml/tests/test_ci_matrix.py
"""
import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TESTS = REPO / "qml/tests"
WORKFLOW = (REPO / ".github/workflows/qml.yml").read_text()
# Helpers or tests that need what CI cannot provide; each with its reason.
NOT_IN_CI = {
    "run_qml.py": "the smoke runner itself",
}


def run_lines():
    return [l.strip() for l in WORKFLOW.splitlines() if not l.strip().startswith("#")]


class CiMatrix(unittest.TestCase):
    def test_every_smoke_runs(self):
        smokes = sorted(p.name for p in TESTS.glob("*Smoke.qml"))
        self.assertGreaterEqual(len(smokes), 9)
        lines = "\n".join(run_lines())
        # Either the glob loop or each smoke by name.
        if 'for smoke in qml/tests/*Smoke.qml' in lines:
            self.assertIn('python3 qml/tests/run_qml.py "$smoke"', lines)
            self.assertIn("set -e", lines, "the smoke loop must stop on the first failure")
        else:
            for s in smokes:
                self.assertIn(s, lines, s)

    def test_every_python_test_runs(self):
        lines = "\n".join(run_lines())
        for p in sorted(TESTS.glob("*.py")):
            if p.name in NOT_IN_CI:
                continue
            self.assertRegex(lines, r"python3 qml/tests/" + re.escape(p.name) + r"\b", p.name)

    def test_every_node_suite_runs(self):
        self.assertTrue(list(TESTS.glob("*.mjs")))
        self.assertIn("node --test qml/tests/*.mjs", "\n".join(run_lines()))

    def test_lint_still_runs(self):
        self.assertIn("pyside6-qmllint -I qml qml/shared/*.qml qml/standalone/*.qml qml/tests/*Smoke.qml",
                      "\n".join(run_lines()))


if __name__ == "__main__":
    unittest.main(verbosity=2)
