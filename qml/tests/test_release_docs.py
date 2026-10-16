#!/usr/bin/env python3
"""Release documentation for issues #5 and #6: the decision record, the superseded
"no death" text, the changelog, the pet coverage matrix and an honest v0.2 validation
entry (contact-sheet review checklist and the live QA table, each item pass, fail or
untested).

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
        self.assertIn("ruleset `lifecycle-v1`", adr)
        self.assertIn("carried unchanged into", adr)
        # A build prompt and session wording never belong in a public decision record.
        self.assertNotIn("Record these verbatim", adr)
        self.assertNotIn("this run", adr)
        self.assertNotIn("operator", adr)
        for n in range(1, 13):
            self.assertRegex(adr, rf"(?m)^\| D{n} \|", f"D{n}")
        for req in ("evolution is tied to XP and levels", "neglect can permanently end a life",
                    "a replacement starts at level 1"):
            self.assertIn(req, adr)
        self.assertIn("(../lifecycle.md)", adr)

    def test_raster_art_departure_from_issue_5_is_recorded(self):
        adr = read("docs/decisions/0002-v0.2-raster-pet-art.md")
        flat = " ".join(adr.split())
        for phrase in ("issue #5", "SVG", "not in Git", "codepet-art-sources-v0.2.tar",
                       "extract.py --check", "have not been legally reviewed"):
            self.assertIn(phrase, flat, phrase)
        self.assertIn("docs/decisions/0002-v0.2-raster-pet-art.md", read("CHANGELOG.md"))
        readme = read("README.md")
        self.assertIn("docs/decisions/0002-v0.2-raster-pet-art.md", readme)
        # The source tar is the v0.3.0 release asset: every doc names that one download.
        url = "releases/download/v0.3.0/codepet-art-sources-v0.2.tar"
        for rel in ("README.md", "docs/decisions/0002-v0.2-raster-pet-art.md",
                    "qml/assets/pets/PROVENANCE.md", "docs/pets.md"):
            text = read(rel)
            self.assertIn(url, text, rel)
            for stale in ("planned release asset", "until it is attached", "operator's workstation"):
                self.assertNotIn(stale, text, (rel, stale))

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
        if "codepet-overlay-idle.png" in overlay or "codepet-overlay-menu.png" in overlay:
            self.assertIn("legacy v0.1", overlay.lower())
        for shot in ("codepet-overlay-idle.png", "codepet-overlay-menu.png"):
            for line in (l for l in overlay.splitlines() if shot in l):
                self.assertIn("Legacy v0.1", line, shot)

    def test_readme_shows_the_product_and_the_release(self):
        readme = read("README.md")
        version = tomllib.loads(read("Cargo.toml"))["workspace"]["package"]["version"]
        self.assertIn("](docs/screenshots/codepet-v0.2-roster.webp)", readme)
        self.assertIn(f"`codepet --version` prints `codepet {version}`", readme)
        self.assertIn(f"codepet-{version}-x86_64-linux.tar.gz", readme)
        for section in ("## Requirements", "## Quick start", "### First run", "### Uninstall"):
            self.assertIn(section, readme)
        for stale in ("private testing prerelease", "operator", "v0.1"):
            self.assertNotIn(stale, readme)

    def test_validation_has_a_v021_live_qa_placeholder(self):
        v = read("docs/validation.md")
        self.assertLess(v.index("# v0.2.1"), v.index("# v0.2.0"))
        section = v[v.index("# v0.2.1"):v.index("# v0.2.0")]
        self.assertIn("## Live QA (v0.2.1)", section)
        self.assertNotRegex(section, r"(?m)^\| \d \| [^|]+ \| pass \|", "no live result is claimed here")

    def test_release_notes_have_no_relative_links(self):
        import subprocess, tempfile
        version = tomllib.loads(read("Cargo.toml"))["workspace"]["package"]["version"]
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "notes.md"
            r = subprocess.run(["python3", str(REPO / "tools/release/release_docs.py"), "notes",
                                "--version", version, "--out", str(out)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            notes = out.read_text()
        self.assertNotRegex(notes, r"\]\((?!https?://)[^)]+\)")
        self.assertIn(f"/blob/v{version}/", notes)

    def test_validation_has_an_honest_v02_entry(self):
        v = read("docs/validation.md")
        self.assertIn("# v0.2.0", v)
        section = v[v.index("# v0.2.0"):]
        section = section[:section.index("\n---\n")] if "\n---\n" in section else section
        # Contact-sheet human review checklist (spec §17.4 row 5), per wave.
        self.assertIn("Contact-sheet review", section)
        for wave in ("Wave 1", "Wave 2", "Wave 3"):
            self.assertIn(wave, section)
        for item in ("gait", "care poses", "stage readability", "fish in water", "not recolors",
                     "64/96/112 px at 1x–2x", "both directions", "playback"):
            self.assertIn(item, section)
        self.assertIn("extract.py --previews", section)
        # §17.9 live QA: items 1-7, each pass, fail or untested.
        rows = re.findall(r"(?m)^\| (\d) \| [^|]+ \| (pass|fail|untested) \|", section)
        self.assertEqual(sorted(int(n) for n, _ in rows), [1, 2, 3, 4, 5, 6, 7], rows)
        self.assertIn("untested", section)


if __name__ == "__main__":
    unittest.main(verbosity=2)
