#!/usr/bin/env python3
"""Release documentation required by spec §16 WP-INT / §18 and issues #5 and #6: the
decision record, the superseded "no death" text, the changelog, the pet coverage matrix
and an honest v0.2 validation entry (contact-sheet review checklist and the §17.9 live QA
table, each item pass, fail or untested).

Run: python3 qml/tests/test_release_docs.py
"""
import json
import re
import tomllib
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DOCS = REPO / "docs"


def read(rel):
    return (REPO / rel).read_text()


class ReleaseDocs(unittest.TestCase):
    def test_decision_record_carries_d1_to_d12(self):
        adr = read("docs/decisions/0001-v0.2-lifecycle.md")
        self.assertIn("delegated defaults for a private testing prerelease", adr)
        for n in range(1, 13):
            self.assertRegex(adr, rf"(?m)^\| D{n} \|", f"D{n}")
        for req in ("evolution is tied to XP and levels", "neglect can permanently end a life",
                    "a replacement starts at level 1"):
            self.assertIn(req, adr)
        self.assertIn("(../lifecycle.md)", adr)

    def test_no_death_text_is_superseded_with_pointers(self):
        spec = read("docs/implementation-spec.md")
        for anchor in ("complex pet breeding or death mechanics.", "### 4.1 Decay and pauses"):
            i = spec.index(anchor)
            following = spec[i:i + 900]
            self.assertIn("Superseded in v0.2.0", following, anchor)
            self.assertIn("(lifecycle.md)", following, anchor)
            self.assertIn("(decisions/0001-v0.2-lifecycle.md)", following, anchor)

    def test_changelog_names_the_release(self):
        version = tomllib.loads(read("Cargo.toml"))["workspace"]["package"]["version"]
        log = read("CHANGELOG.md")
        self.assertIn(f"## {version}", log)
        self.assertIn("#5", log)
        self.assertIn("#6", log)
        self.assertIn("docs/decisions/0001-v0.2-lifecycle.md", log)

    def test_pets_doc_covers_every_species(self):
        pets = read("docs/pets.md")
        registry = json.loads(read("qml/assets/pets/registry.json"))
        for sid in registry["order"]:
            self.assertIn(f"`{sid}`", pets, sid)

    def test_v02_gallery_is_committed_and_linked(self):
        gallery = REPO / "docs/screenshots/codepet-v0.2-roster.webp"
        self.assertTrue(gallery.is_file())
        self.assertEqual(gallery.read_bytes()[:4], b"RIFF")
        self.assertEqual(gallery.read_bytes()[8:12], b"WEBP")
        self.assertLess(gallery.stat().st_size, 400_000, "keep the committed gallery small")
        self.assertIn("(screenshots/codepet-v0.2-roster.webp)", read("docs/pets.md"))
        overlay = read("docs/desktop-overlay.md")
        self.assertIn("legacy v0.1", overlay.lower())
        for shot in ("codepet-overlay-idle.png", "codepet-overlay-menu.png"):
            line = next(l for l in overlay.splitlines() if shot in l)
            self.assertIn("Legacy v0.1", line, shot)

    def test_validation_has_an_honest_v02_entry(self):
        v = read("docs/validation.md")
        self.assertIn("# v0.2.0", v)
        section = v[v.index("# v0.2.0"):]
        section = section[:section.index("\n---\n")] if "\n---\n" in section else section
        # Contact-sheet human review checklist (spec §17.4 row 5), per wave.
        self.assertIn("Contact-sheet review", section)
        for wave in ("Wave 1", "Wave 2", "Wave 3"):
            self.assertIn(wave, section)
        for item in ("gait", "care poses", "stage readability", "fish in water", "not recolors"):
            self.assertIn(item, section)
        # §17.9 live QA: items 1-7, each pass, fail or untested.
        rows = re.findall(r"(?m)^\| (\d) \| [^|]+ \| (pass|fail|untested) \|", section)
        self.assertEqual(sorted(int(n) for n, _ in rows), [1, 2, 3, 4, 5, 6, 7], rows)
        self.assertIn("untested", section)


if __name__ == "__main__":
    unittest.main(verbosity=2)
