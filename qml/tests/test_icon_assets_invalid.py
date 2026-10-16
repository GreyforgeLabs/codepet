#!/usr/bin/env python3
"""Broken icon sets must fail qml/tests/test_icon_assets.py.

Each case copies the icon trees and the files the validator reads into a temporary root,
breaks one thing, and runs the validator with CODEPET_ASSET_ROOT. Needs Pillow.

Run: python3 qml/tests/test_icon_assets_invalid.py
"""
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
VALIDATOR = REPO / "qml/tests/test_icon_assets.py"
COPY = ("qml/assets/icons", "omarchy-plugin/assets/icons", "docs/v0.3/contracts/icons.json",
        "tools/petart/icon_sheets.json", "qml/assets/pets/registry.json")
TREES = ("qml/assets/icons", "omarchy-plugin/assets/icons")


def webp(size, rgba=(200, 120, 40, 255), alpha=True, key=None):
    from PIL import Image
    im = Image.new("RGBA" if alpha else "RGB", (size, size), (0, 0, 0, 0) if alpha else (0, 0, 0))
    inner = Image.new("RGBA", (size - 8, size - 8), key + (255,) if key else rgba)
    if alpha:
        im.alpha_composite(inner, (4, 4))
    else:
        im.paste(inner.convert("RGB"), (4, 4))
    buf = io.BytesIO()
    im.save(buf, format="WEBP", quality=86)
    return buf.getvalue()


class InvalidIcons(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="codepet-icons-")
        self.root = Path(self.tmp.name)
        for rel in COPY:
            src, dst = REPO / rel, self.root / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            (shutil.copytree if src.is_dir() else shutil.copyfile)(src, dst)

    def tearDown(self):
        self.tmp.cleanup()

    def validate(self):
        env = dict(os.environ, CODEPET_ASSET_ROOT=str(self.root))
        r = subprocess.run([sys.executable, str(VALIDATOR)], env=env, capture_output=True, text=True, timeout=300)
        return r.returncode, r.stdout + r.stderr

    def replace_icon(self, rel, data):
        """Replace one icon in both trees and keep index.json consistent, so only the defect fails."""
        for t in TREES:
            (self.root / t / rel).write_bytes(data)
            ip = self.root / t / "index.json"
            idx = json.loads(ip.read_text())
            idx["icons"][rel[:-len(".webp")]]["sha256"] = hashlib.sha256(data).hexdigest()
            ip.write_text(json.dumps(idx, indent=1) + "\n")

    def assertRejected(self, needle):
        code, out = self.validate()
        self.assertNotEqual(code, 0, out)
        self.assertIn(needle, out, out)

    def test_unmodified_copy_passes(self):
        code, out = self.validate()
        self.assertEqual(code, 0, out)
        self.assertIn("PASS: 49 icons in 7 sheets", out)

    def test_trees_differ(self):
        (self.root / TREES[1] / "care-floor/food.webp").write_bytes(
            (self.root / TREES[1] / "care-floor/water.webp").read_bytes())
        self.assertRejected("the two icon trees differ")

    def test_missing_and_unexpected_icon(self):
        for t in TREES:
            (self.root / t / "status/moon.webp").rename(self.root / t / "status/moon2.webp")
        self.assertRejected("missing status/moon.webp")
        self.assertRejected("unexpected file status/moon2.webp")

    def test_wrong_size(self):
        self.replace_icon("care-floor/heart.webp", webp(128))
        self.assertRejected("care-floor/heart.webp: must be a 256x256 WebP with alpha")

    def test_no_alpha(self):
        self.replace_icon("bar/badge_star.webp", webp(64, alpha=False))
        self.assertRejected("bar/badge_star.webp: must be a 64x64 WebP with alpha")

    def test_key_residue(self):
        self.replace_icon("care-floor/star.webp", webp(256, key=(0, 255, 0)))
        self.assertRejected("still look like the #00FF00 key")

    def test_index_sha_mismatch(self):
        for t in TREES:
            ip = self.root / t / "index.json"
            idx = json.loads(ip.read_text())
            idx["icons"]["apps/app_codex"]["sha256"] = "0" * 64
            ip.write_text(json.dumps(idx, indent=1) + "\n")
        self.assertRejected("index.json apps/app_codex: sha256 mismatch")

    def test_provenance_without_nonce(self):
        for t in TREES:
            p = self.root / t / "PROVENANCE.md"
            p.write_text(p.read_text().replace("cpjob-", "job-"))
        self.assertRejected("PROVENANCE.md must record the wrapper nonce")

    def test_care_icon_set_without_icons(self):
        reg = self.root / "qml/assets/pets/registry.json"
        data = json.loads(reg.read_text())
        data["species"]["cat"]["careIconSet"] = "space"
        reg.write_text(json.dumps(data))
        self.assertRejected("cat: careIconSet 'space' != contract 'floor'")
        self.assertRejected("cat: care-space/food.webp missing")


if __name__ == "__main__":
    unittest.main(verbosity=2)
