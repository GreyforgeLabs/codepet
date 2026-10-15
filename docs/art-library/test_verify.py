#!/usr/bin/env python3
"""Regression tests for verify.py: corruption must fail even under -O.

Each case corrupts a temporary copy of the catalog, never the checked-in files.
Run from the repository root: python3 -I docs/art-library/test_verify.py
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

CATALOG = Path(__file__).resolve().parent
# Plain, -O, -OO and the environment form that the README's command inherits.
MODES = (
    ("plain", ["-I"], {}),
    ("-O", ["-I", "-O"], {}),
    ("-OO", ["-I", "-OO"], {}),
    ("PYTHONOPTIMIZE", [], {"PYTHONOPTIMIZE": "2"}),
)


class VerifyCorruptionTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="codepet-art-verify-")
        cls.root = Path(cls.temp.name) / "art-library"
        shutil.copytree(CATALOG, cls.root, ignore=shutil.ignore_patterns("__pycache__"))
        cls.manifest_text = (cls.root / "manifest.json").read_text(encoding="utf-8")

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def tearDown(self):
        (self.root / "manifest.json").write_text(self.manifest_text, encoding="utf-8")
        for path in self.restore:
            path.unlink(missing_ok=True)
            shutil.copy2(CATALOG / path.relative_to(self.root), path)
        for path in self.remove:
            path.unlink(missing_ok=True)

    def setUp(self):
        self.restore = []
        self.remove = []

    def run_verify(self, flags, extra_env):
        env = {k: v for k, v in os.environ.items() if not k.startswith("PYTHON")}
        env.update(extra_env)
        return subprocess.run(
            [sys.executable, *flags, str(self.root / "verify.py")],
            cwd=self.root.parent,
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )

    def edit_manifest(self, change):
        manifest = json.loads(self.manifest_text)
        change(manifest)
        (self.root / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    def assert_fails_in_every_mode(self, expected):
        for name, flags, env in MODES:
            with self.subTest(mode=name):
                result = self.run_verify(flags, env)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertNotIn("PASS", result.stdout)
                self.assertIn(expected, result.stderr)

    def test_clean_copy_passes_in_every_mode(self):
        for name, flags, env in MODES:
            with self.subTest(mode=name):
                result = self.run_verify(flags, env)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("PASS: 15 source PNGs", result.stdout)

    def test_zeroed_sha256_fails_under_optimization(self):
        self.edit_manifest(lambda m: m["assets"][0].update(sha256="0" * 64))
        self.assert_fails_in_every_mode("SHA-256 does not match")

    def test_flipped_pixel_byte_fails_under_optimization(self):
        manifest = json.loads(self.manifest_text)
        path = self.root / manifest["assets"][3]["relative_path"]
        self.restore.append(path)
        data = bytearray(path.read_bytes())
        data[len(data) // 2] ^= 0xFF
        path.write_bytes(bytes(data))
        self.assert_fails_in_every_mode("SHA-256 does not match")

    def test_missing_png_fails_without_traceback(self):
        manifest = json.loads(self.manifest_text)
        path = self.root / manifest["assets"][5]["relative_path"]
        self.restore.append(path)
        path.unlink()
        for name, flags, env in MODES:
            with self.subTest(mode=name):
                result = self.run_verify(flags, env)
                self.assertEqual(result.returncode, 1)
                self.assertIn("unreadable", result.stderr)
                self.assertNotIn("Traceback", result.stderr)

    def test_runtime_ready_true_fails_under_optimization(self):
        self.edit_manifest(lambda m: m["assets"][1].update(runtime_ready=True))
        self.assert_fails_in_every_mode("runtime_ready must be false")

    def test_escaping_path_fails_under_optimization(self):
        self.edit_manifest(lambda m: m["assets"][2].update(relative_path="../outside.png"))
        self.assert_fails_in_every_mode("path must be relative")

    def test_unlisted_png_fails_under_optimization(self):
        stray = self.root / "source" / "stray.png"
        self.remove.append(stray)
        shutil.copy2(self.root / json.loads(self.manifest_text)["assets"][0]["relative_path"], stray)
        self.assert_fails_in_every_mode("differ from the manifest")


if __name__ == "__main__":
    unittest.main(verbosity=2)
