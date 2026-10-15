#!/usr/bin/env python3
"""Negative fixtures for the pet pack validator (SPEC §17.4 row 4: "an invalid-pack
fixture fails validation").

Each case copies the shipped packs (plus the registry module, docs and the Rust constants
the validator reads) into a temporary tree, breaks exactly one thing, and runs
qml/tests/test_pet_assets.py against it with CODEPET_ASSET_ROOT. The validator must exit
non-zero with the expected message. An unmodified copy is checked first, so a failure
means the defect was caught, not that the copy itself was broken.

Run: python3 qml/tests/test_pet_assets_invalid.py
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
VALIDATOR = REPO / "qml/tests/test_pet_assets.py"
COPY = ("qml/assets/pets", "qml/shared/PetRegistry.js", "docs/pets.md",
        "crates/codepet-core/src/species.rs", "crates/codepet-core/src/lifecycle.rs")


def webp_bytes(size, alpha):
    """A small solid WebP, with or without an alpha channel."""
    try:
        from PIL import Image
        mode = "RGBA" if alpha else "RGB"
        im = Image.new(mode, (size, size), (200, 120, 60, 255) if alpha else (200, 120, 60))
        if alpha:
            for x in range(size // 4, size // 2):
                for y in range(size // 4, size - 8):
                    im.putpixel((x, y), (200, 120, 60, 255))
            im.putpixel((0, 0), (0, 0, 0, 0))
        buf = io.BytesIO()
        im.save(buf, "WEBP", lossless=True)
        return buf.getvalue()
    except ImportError:
        from PySide6.QtCore import QBuffer, QByteArray, QIODevice
        from PySide6.QtGui import QColor, QImage
        img = QImage(size, size, QImage.Format_ARGB32 if alpha else QImage.Format_RGB32)
        img.fill(QColor(200, 120, 60, 255))
        if alpha:
            img.setPixelColor(0, 0, QColor(0, 0, 0, 0))
        data = QByteArray()
        buf = QBuffer(data)
        buf.open(QIODevice.WriteOnly)
        img.save(buf, "WEBP")
        return bytes(data)


class InvalidPacks(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="codepet-packs-")
        self.root = Path(self.tmp.name)
        for rel in COPY:
            src, dst = REPO / rel, self.root / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            if src.is_dir():
                shutil.copytree(src, dst)
            else:
                shutil.copyfile(src, dst)
        self.pets = self.root / "qml/assets/pets"

    def tearDown(self):
        self.tmp.cleanup()

    def validate(self):
        env = dict(os.environ, CODEPET_ASSET_ROOT=str(self.root))
        env.pop("CODEPET_ALLOW_PARTIAL_ART", None)
        r = subprocess.run([sys.executable, str(VALIDATOR)], env=env, capture_output=True, text=True, timeout=300)
        return r.returncode, r.stdout + r.stderr

    def manifest(self, sid):
        return json.loads((self.pets / sid / "manifest.json").read_text())

    def write_manifest(self, sid, man, reindex=True):
        (self.pets / sid / "manifest.json").write_text(json.dumps(man, indent=1))
        if reindex:  # keep registry.json and PetRegistry.js consistent, so only the defect fails
            reg = json.loads((self.pets / "registry.json").read_text())
            reg["species"][sid] = {k: v for k, v in man.items() if k not in ("files", "source")}
            (self.pets / "registry.json").write_text(json.dumps(reg))
            js = self.root / "qml/shared/PetRegistry.js"
            js.write_text(".pragma library\nvar REGISTRY = " + json.dumps(reg) + ";\n")

    def replace_frame(self, sid, rel, data):
        path = self.pets / sid / rel
        path.write_bytes(data)
        man = self.manifest(sid)
        man["files"][rel] = hashlib.sha256(data).hexdigest()
        self.write_manifest(sid, man)

    def assertRejected(self, needle):
        code, out = self.validate()
        self.assertNotEqual(code, 0, out)
        self.assertIn(needle, out, out)

    def test_unmodified_copy_passes(self):
        code, out = self.validate()
        self.assertEqual(code, 0, out)
        self.assertIn("15 of 15 pet packs verified (strict)", out)
        # The copy may also be re-indexed without changing the verdict.
        self.write_manifest("cat", self.manifest("cat"))
        self.assertEqual(self.validate()[0], 0)

    def test_missing_state(self):
        man = self.manifest("cat")
        del man["states"]["sleep_exit"]
        self.write_manifest("cat", man)
        self.assertRejected("cat: states must be exactly the 15 state names")

    def test_wrong_sha256(self):
        p = self.pets / "cat/s1/b00.webp"
        data = bytearray(p.read_bytes())
        data[-1] ^= 0xFF
        p.write_bytes(bytes(data))
        self.assertRejected("cat: s1/b00.webp sha256 mismatch")

    def test_webp_without_alpha(self):
        self.replace_frame("dog", "s2/b01.webp", webp_bytes(192, alpha=False))
        self.assertRejected("dog: s2/b01.webp must be WebP with alpha at 192x192")

    def test_wrong_size(self):
        self.replace_frame("owl", "s3/c04.webp", webp_bytes(128, alpha=True))
        self.assertRejected("owl: s3/c04.webp must be WebP with alpha at 192x192")

    def test_not_a_webp(self):
        self.replace_frame("frog", "s1/b02.webp", b"\x89PNG\r\n\x1a\n not really")
        self.assertRejected("frog: s1/b02.webp unreadable")

    def test_missing_water_block(self):
        man = self.manifest("goldfish")
        del man["geometry"]["water"]
        self.write_manifest("goldfish", man)
        self.assertRejected("goldfish: aquatic manifest needs a water block")

    def test_anchor_outside_the_canvas_or_off_the_ground(self):
        man = self.manifest("cat")
        man["geometry"]["pivot"] = [96, 170]
        self.write_manifest("cat", man)
        self.assertRejected("cat: floor geometry.pivot [96, 170] must sit on groundY 184")
        man["geometry"]["pivot"] = [96, 184]
        man["geometry"]["propAnchor"] = [200, 176]
        self.write_manifest("cat", man)
        self.assertRejected("cat: geometry.propAnchor must be an [x,y] point inside the canvas")

    def test_swim_bounds_outside_the_water(self):
        man = self.manifest("goldfish")
        water = man["geometry"]["water"]
        line = water["byStage"]["2"]["waterLineY"]
        water["byStage"]["2"]["swimBounds"][1] = line - 1
        self.write_manifest("goldfish", man)
        self.assertRejected("goldfish: water.byStage.2.swimBounds")
        self.assertRejected(f"must start below its waterLineY {line}")
        man = self.manifest("axolotl")
        sb = man["geometry"]["water"]["swimBounds"]
        sb[0] = 0
        self.write_manifest("axolotl", man)
        self.assertRejected("axolotl: water.swimBounds")
        self.assertRejected("must lie inside its outer box")

    def test_closed_eyed_sleep_frame_for_an_eyes_open_sleeper(self):
        man = self.manifest("goldfish")
        man["states"]["sleep"]["frames"] = ["c11.webp"]
        self.write_manifest("goldfish", man)
        self.assertRejected("goldfish: s1 sleep frame c11.webp is not an open-eyed pose")

    def test_missing_and_unreferenced_files(self):
        (self.pets / "rabbit/s2/c05.webp").unlink()
        self.assertRejected("rabbit: unreferenced or missing files on disk")
        shutil.copyfile(self.pets / "rabbit/s1/c05.webp", self.pets / "rabbit/s2/c05.webp")
        (self.pets / "rabbit/s2/stray.webp").write_bytes(b"x")
        self.assertRejected("rabbit: unreferenced or missing files on disk")

    def test_placeholder_pack_fails_the_release_gate(self):
        man = self.manifest("axolotl")
        man["placeholder"] = True
        self.write_manifest("axolotl", man)
        self.assertRejected("axolotl: placeholder:true (release gate")

    def test_registry_drift(self):
        man = self.manifest("cat")
        man["careLabels"]["food"] = "Something else"
        self.write_manifest("cat", man, reindex=False)
        self.assertRejected("cat: registry entry differs from manifest.json")


if __name__ == "__main__":
    unittest.main(verbosity=2)
