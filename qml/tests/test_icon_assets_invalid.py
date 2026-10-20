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


def drawn_webp(size, draw_fn):
    """A transparent WebP canvas with one shape drawn on it by draw_fn(ImageDraw)."""
    from PIL import Image, ImageDraw
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw_fn(ImageDraw.Draw(im))
    buf = io.BytesIO()
    im.save(buf, format="WEBP", quality=86, exact=False, alpha_quality=100)
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
        self.assertIn("PASS: 51 icons in 7 sheets", out)

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

    def edit_provenance(self, fn):
        for t in TREES:
            p = self.root / t / "PROVENANCE.md"
            p.write_text(fn(p.read_text()))

    def test_provenance_without_source_sha(self):
        spec = json.loads((self.root / "tools/petart/icon_sheets.json").read_text())
        patch = spec["sheets"][0]["patches"][0]
        self.edit_provenance(lambda t: t.replace(patch["sha256"], "0" * 64))
        self.assertRejected(f"does not record the care-floor source sha256 of {patch['source']}")

    def test_provenance_ships_private_details(self):
        self.edit_provenance(lambda t: t + "\nWrapper nonce `cpjob-1792132667824361107-1003086-17821`\n")
        self.assertRejected("ships a private build detail (wrapper nonce)")
        self.edit_provenance(lambda t: t.replace("cpjob-", "") + "\nRollout `rollout-2026-10-16T02-37-48-01a1`\n")
        self.assertRejected("ships a private build detail (Codex rollout id)")
        self.edit_provenance(lambda t: t.replace("rollout-", "") + "\nSources in `~/Work/runs/x/icons-src/`\n")
        self.assertRejected("ships a private build detail (home path)")
        self.assertRejected("ships a private build detail (run folder)")

    def test_bar_badge_that_turns_to_mush(self):
        # a 1 px outline badge: solid at 64 px, almost nothing left at 20 and 16 px (SPEC §4.1)
        self.replace_icon("bar/badge_ball.webp", drawn_webp(
            64, lambda d: d.ellipse((4, 4, 59, 59), outline=(200, 30, 30, 255), width=1)))
        self.assertRejected("bar/badge_ball.webp: at 20 px its solid share drops to")
        self.assertRejected("(mush)")

    def test_small_subject(self):
        self.replace_icon("status/help.webp", drawn_webp(
            256, lambda d: d.ellipse((78, 78, 178, 178), fill=(40, 90, 220, 255))))
        self.assertRejected("status/help.webp: subject spans less than 80% of the canvas")

    def test_icon_touching_the_edge(self):
        self.replace_icon("ui/egg.webp", drawn_webp(
            256, lambda d: d.ellipse((0, 0, 255, 255), fill=(240, 220, 180, 255))))
        self.assertRejected("ui/egg.webp: alpha box (0, 0, 256, 256) leaves the 2 px margin")

    def test_opaque_square(self):
        self.replace_icon("care-floor/bed.webp", drawn_webp(
            256, lambda d: d.rectangle((1, 1, 254, 254), fill=(180, 160, 220, 255))))
        self.assertRejected("care-floor/bed.webp: opaque share")

    def test_opaque_corners(self):
        def full_with_a_hole(d):        # one clear hole keeps the alpha channel in the WebP
            d.rectangle((0, 0, 255, 255), fill=(180, 160, 220, 255))
            d.rectangle((120, 120, 135, 135), fill=(0, 0, 0, 0))
        self.replace_icon("care-floor/bed.webp", drawn_webp(256, full_with_a_hole))
        self.assertRejected("care-floor/bed.webp: corners are not transparent")

    def test_magenta_key_residue_on_a_magenta_sheet(self):
        self.replace_icon("status/leaf_calm.webp", webp(256, key=(255, 0, 255)))
        self.assertRejected("still look like the #FF00FF key")

    def test_care_icon_set_defaults_from_habitat(self):
        # icons.json: careIconSet is optional; absent resolves from the habitat.
        reg = self.root / "qml/assets/pets/registry.json"
        data = json.loads(reg.read_text())
        del data["species"]["goldfish"]["careIconSet"]
        del data["species"]["cat"]["careIconSet"]
        reg.write_text(json.dumps(data))
        code, out = self.validate()
        self.assertEqual(code, 0, out)
        del data["species"]["robot"]["careIconSet"]          # robot must set 'robot' explicitly
        reg.write_text(json.dumps(data))
        self.assertRejected("robot: careIconSet 'floor' != contract 'robot'")

    def test_care_icon_set_without_icons(self):
        reg = self.root / "qml/assets/pets/registry.json"
        data = json.loads(reg.read_text())
        data["species"]["cat"]["careIconSet"] = "space"
        reg.write_text(json.dumps(data))
        self.assertRejected("cat: careIconSet 'space' != contract 'floor'")
        self.assertRejected("cat: care-space/food.webp missing")


if __name__ == "__main__":
    unittest.main(verbosity=2)
