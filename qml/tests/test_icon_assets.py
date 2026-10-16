#!/usr/bin/env python3
"""Validate the v0.3 icon set (SPEC-v0.3 §4.1, docs/v0.3/contracts/icons.json).

Checks, for both install copies (qml/assets/icons/ and omarchy-plugin/assets/icons/):
- the two trees are byte-identical, hold exactly index.json, PROVENANCE.md and one WebP per
  contract icon at `<sheet id>/<icon id>.webp`, and index.json lists every file with its sha256;
- every icon is a square WebP with an alpha channel at its contract size (256 px; the bar
  badges at their 64 px source size), with transparent corners, a non-empty subject inside a
  2 px margin, a subject spanning at least 80% of the canvas, a sane fill share and no
  chroma-key residue (the key is per sheet, or per patch, in tools/petart/icon_sheets.json);
- the bar badges keep at least 75% of their 64 px solid share when scaled to 20 and 16 px;
- every bar badge and bar_cell badge the contract names exists, and every species'
  careIconSet (absent: the habitat default) resolves to the contract and to the four care
  icons of its set;
- PROVENANCE.md records every sheet and patch with its selected source sha256, and carries no
  private build detail (wrapper nonces, Codex rollout ids, local paths): it ships to every
  user inside the plugin. Those records stay with the source sheets.

Images are read with Pillow when available, otherwise with PySide6's QImage. Every check is an
explicit comparison, so `python3 -O` does not disable it.

CODEPET_ASSET_ROOT=<dir> validates another tree laid out like the repository;
qml/tests/test_icon_assets_invalid.py uses it to prove broken icon sets are rejected.

Run: python3 qml/tests/test_icon_assets.py
"""
import hashlib
import json
import os
import re
import sys
from pathlib import Path

ROOT = (Path(os.environ["CODEPET_ASSET_ROOT"]).resolve() if os.environ.get("CODEPET_ASSET_ROOT")
        else Path(__file__).resolve().parents[2])
CONTRACT = ROOT / "docs/v0.3/contracts/icons.json"
SHEET_SPEC = ROOT / "tools/petart/icon_sheets.json"
REGISTRY = ROOT / "qml/assets/pets/registry.json"
DIRS = (ROOT / "qml/assets/icons", ROOT / "omarchy-plugin/assets/icons")
NEEDS = ("food", "water", "play", "wash")
MARGIN = 2
ALPHA_ON = 16
KEY_RESIDUE = 120        # key strength of an opaque pixel that still looks like the key colour
KEY_RESIDUE_SHARE = 0.002

# Build details that stay with the source sheets (they ship to every user otherwise).
PRIVATE = (("wrapper nonce", r"cpjob-[0-9-]+"), ("Codex rollout id", r"rollout-[0-9T-]+"),
           ("home path", r"(?:~|/home/[^/\s`]+)/"), ("run folder", r"Work/runs/"))

failures = []


def fail(msg):
    failures.append(msg)


def _load_pillow(path):
    from PIL import Image
    with Image.open(path) as im:
        fmt = im.format
        has_alpha = im.mode in ("RGBA", "LA", "RGBa", "PA") or "transparency" in im.info
        rgba = im.convert("RGBA")
        return fmt, rgba.size, has_alpha, rgba.tobytes()


def _load_qt(path):
    from PySide6.QtGui import QImage, QImageReader
    reader = QImageReader(str(path))
    fmt = bytes(reader.format()).decode().upper()
    img = reader.read()
    if img.isNull():
        raise ValueError(reader.errorString())
    has_alpha = img.hasAlphaChannel()
    img = img.convertToFormat(QImage.Format_RGBA8888)
    w, h = img.width(), img.height()
    data = bytes(img.constBits())[: w * h * 4] if img.bytesPerLine() == w * 4 else b"".join(
        bytes(img.constScanLine(y))[: w * 4] for y in range(h))
    return ("WEBP" if fmt == "WEBP" else fmt), (w, h), has_alpha, data


try:
    import PIL  # noqa: F401
    load_image = _load_pillow
    HAVE_PIL = True
except ImportError:  # CI may carry only PySide6
    load_image = _load_qt
    HAVE_PIL = False


def alpha_box(size, rgba):
    w, h = size
    alpha = rgba[3::4]
    rows = [y for y in range(h) if max(alpha[y * w:(y + 1) * w]) >= ALPHA_ON]
    if not rows:
        return None
    cols = [x for x in range(w) if max(alpha[x::w]) >= ALPHA_ON]
    return cols[0], rows[0], cols[-1] + 1, rows[-1] + 1


def key_residue(rgba, key):
    """Share of opaque pixels that still read as the chroma key."""
    opaque = residue = 0
    for i in range(0, len(rgba), 4):
        r, g, b, a = rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]
        if a < 128:
            continue
        opaque += 1
        k = g - max(r, b) if key == "#00FF00" else min(r, b) - g
        if k > KEY_RESIDUE:
            residue += 1
    return residue / max(opaque, 1)


def icon_key(sheet, icon):
    """The chroma key an icon was cut with: its patch's key, else its sheet's."""
    for patch in sheet.get("patches", []):
        if icon in patch.get("ids", []):
            return patch.get("key", "#00FF00")
    return sheet.get("key", "#00FF00")


def scaled_fill(path, px):
    """Opaque share of the subject's own box after a Lanczos scale to px (Pillow only)."""
    from PIL import Image
    with Image.open(path) as im:
        small = im.convert("RGBa").resize((px, px), Image.LANCZOS).convert("RGBA")
    a = small.tobytes()[3::4]
    box = alpha_box((px, px), small.tobytes())
    if box is None:
        return 0.0
    solid = sum(1 for v in a if v >= 128)
    return solid / ((box[2] - box[0]) * (box[3] - box[1]))


def main():
    contract = json.loads(CONTRACT.read_text())
    spec = {s["id"]: s for s in json.loads(SHEET_SPEC.read_text())["sheets"]}
    sheets = {s["id"]: s for s in contract["sheets"]}
    if sorted(spec) != sorted(sheets):
        fail(f"icon_sheets.json sheets {sorted(spec)} != contract sheets {sorted(sheets)}")

    expected = {}
    for sid, sheet in sheets.items():
        px = sheet.get("ship_px") or sheet.get("source_px")
        if spec.get(sid, {}).get("ids") != sheet["ids"]:
            fail(f"{sid}: icon_sheets.json ids differ from the contract (order matters)")
        if spec.get(sid, {}).get("px") != px:
            fail(f"{sid}: icon_sheets.json px {spec.get(sid, {}).get('px')} != contract {px}")
        for icon in sheet["ids"]:
            expected[f"{sid}/{icon}.webp"] = (px, sid)

    trees = []
    for d in DIRS:
        rel = d.relative_to(ROOT)
        if not d.is_dir():
            fail(f"{rel} missing")
            trees.append({})
            continue
        trees.append({p.relative_to(d).as_posix(): p.read_bytes() for p in sorted(d.rglob("*")) if p.is_file()})
        have = set(trees[-1])
        want = set(expected) | {"index.json", "PROVENANCE.md"}
        for f in sorted(want - have):
            fail(f"{rel}: missing {f}")
        for f in sorted(have - want):
            fail(f"{rel}: unexpected file {f}")
    if len(trees) == 2 and trees[0] != trees[1]:
        diff = sorted(k for k in set(trees[0]) | set(trees[1]) if trees[0].get(k) != trees[1].get(k))
        fail(f"the two icon trees differ: {diff[:5]}")

    base = DIRS[0]
    index = json.loads((base / "index.json").read_text()) if (base / "index.json").is_file() else {}
    icons = index.get("icons", {})
    if sorted(v.get("file") for v in icons.values()) != sorted(expected):
        fail("index.json does not list exactly the contract icons")
    for key, entry in icons.items():
        f = entry.get("file")
        if f != f"{key}.webp":
            fail(f"index.json {key}: file {f} breaks the <sheet>/<id>.webp pattern")
        p = base / str(f)
        if p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest() != entry.get("sha256"):
            fail(f"index.json {key}: sha256 mismatch")
        if f in expected and entry.get("px") != expected[f][0]:
            fail(f"index.json {key}: px {entry.get('px')} != {expected[f][0]}")

    checked = 0
    for rel, (px, sid) in sorted(expected.items()):
        p = base / rel
        if not p.is_file():
            continue
        try:
            fmt, size, has_alpha, rgba = load_image(p)
        except Exception as e:  # noqa: BLE001
            fail(f"{rel}: unreadable: {e}")
            continue
        if fmt != "WEBP" or not has_alpha or size != (px, px):
            fail(f"{rel}: must be a {px}x{px} WebP with alpha (got {fmt} {size} alpha={has_alpha})")
            continue
        w = px
        corners = [rgba[(y * w + x) * 4 + 3] for x, y in ((0, 0), (w - 1, 0), (0, w - 1), (w - 1, w - 1))]
        if max(corners) != 0:
            fail(f"{rel}: corners are not transparent")
        box = alpha_box(size, rgba)
        if box is None:
            fail(f"{rel}: empty alpha")
            continue
        if box[0] < MARGIN or box[1] < MARGIN or box[2] > w - MARGIN or box[3] > w - MARGIN:
            fail(f"{rel}: alpha box {box} leaves the {MARGIN} px margin")
        if max(box[2] - box[0], box[3] - box[1]) < 0.8 * w:
            fail(f"{rel}: subject spans less than 80% of the canvas ({box})")
        solid = sum(1 for v in rgba[3::4] if v >= 128) / (w * w)
        if not 0.12 <= solid <= 0.95:
            fail(f"{rel}: opaque share {solid:.2f} outside 0.12..0.95")
        key = icon_key(spec.get(sid, {}), rel.split("/", 1)[1][:-len(".webp")])
        res = key_residue(rgba, key)
        if res > KEY_RESIDUE_SHARE:
            fail(f"{rel}: {res:.2%} of opaque pixels still look like the {key} key")
        if sid == "bar" and HAVE_PIL:
            full = scaled_fill(p, px)
            for small in (20, 16):
                fill = scaled_fill(p, small)
                if fill < 0.75 * full:
                    fail(f"{rel}: at {small} px its solid share drops to {fill:.0%} from {full:.0%} (mush)")
        checked += 1

    bar_ids = set(sheets.get("bar", {}).get("ids", []))
    for what, badge in contract.get("bar_badges", {}).items():
        if badge not in bar_ids or f"bar/{badge}.webp" not in trees[0]:
            fail(f"bar badge {badge} for {what} has no file")
    for name, cell in contract.get("bar_cell", {}).items():
        if not isinstance(cell, dict):
            continue
        for k in ("thumb", "badge"):
            v = cell.get(k)
            if isinstance(v, str) and v.startswith("badge_") and f"bar/{v}.webp" not in trees[0]:
                fail(f"bar_cell {name}.{k}: {v} has no file")

    registry = json.loads(REGISTRY.read_text())["species"]
    resolved = contract["care_icon_set"]["resolved"]
    for species, man in registry.items():
        # icons.json default_rule: absent -> 'aquatic' for aquatic habitats, else 'floor'.
        cis = man.get("careIconSet", "aquatic" if man.get("habitat") == "aquatic" else "floor")
        if cis != resolved.get(species):
            fail(f"{species}: careIconSet {cis!r} != contract {resolved.get(species)!r}")
        for need in NEEDS:
            if f"care-{cis}/{need}.webp" not in trees[0]:
                fail(f"{species}: care-{cis}/{need}.webp missing")

    prov = (base / "PROVENANCE.md").read_text() if (base / "PROVENANCE.md").is_file() else ""
    for phrase in ("GPT Image 2", "Codex CLI", "ChatGPT plan", "MIT", "not been legally reviewed",
                   "brand-neutral", "codepet-icon-sources-v0.3.tar"):
        if phrase not in prov:
            fail(f"PROVENANCE.md must state {phrase!r}")
    for sid, s in spec.items():
        if f"## {sid}" not in prov:
            fail(f"PROVENANCE.md has no section for {sid}")
        for part in [s] + s.get("patches", []):
            if part.get("sha256", "-") not in prov:
                fail(f"PROVENANCE.md does not record the {sid} source sha256 of {part.get('source')}")
    for what, pattern in PRIVATE:
        m = re.search(pattern, prov)
        if m:
            fail(f"PROVENANCE.md ships a private build detail ({what}): {m.group(0)!r}")

    if failures:
        for f in failures[:80]:
            print(f"FAIL: {f}")
        if len(failures) > 80:
            print(f"... and {len(failures) - 80} more")
        return 1
    print(f"PASS: {checked} icons in {len(sheets)} sheets verified in both install trees"
          f"{'' if HAVE_PIL else ' (bar 20/16 px solidity skipped: no Pillow)'}; "
          "careIconSet resolution and provenance consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main())
