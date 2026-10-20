#!/usr/bin/env python3
"""Extract the CodePet v0.3 icon set from GPT Image 2 icon sheets (SPEC-v0.3 §4).

Dev-time tool only; nothing here runs inside the app. Python 3, Pillow, numpy, scipy.

    python3 tools/petart/icons.py --src <icon source dir>     # extract every sheet
    python3 tools/petart/icons.py --src <dir> --check         # rebuild in a scratch dir, byte-compare
    python3 tools/petart/icons.py --src <dir> --contact <png> # review sheet (64, 40 and 20 px)

`tools/petart/icon_sheets.json` lists, per sheet, the selected source PNG and its sha256, the
chroma key (per sheet: green `#00FF00` by default, magenta `#FF00FF` when a subject is green),
the grid and the icon ids in cell order. A sheet may carry `patches`: smaller source sheets
(same fields) whose cells replace the named ids of the base sheet, so one icon can be
regenerated without re-rolling the reviewed rest of its sheet. A patch may also supply ids
listed in the sheet's `added_ids` (icons the base sheet never had), and a `null` id skips
that cell, so several sheets can share one patch sheet. Source sheets are not in Git; the default
source directory is `art-src/icons-v0.3` at the repository root (`$CODEPET_ICON_SRC` or `--src`
for another location). Keying and cell segmentation reuse `keying.py` (the pet pipeline).

Each cell is keyed, trimmed to its alpha box, centred on a square canvas with the sheet's
padding, scaled in premultiplied alpha with Lanczos and written as WebP (quality 86, method 6,
lossless alpha) to `<sheet>/<id>.webp` under both `qml/assets/icons/` and
`omarchy-plugin/assets/icons/` (identical bytes), together with `index.json`.
"""

import argparse
import hashlib
import io
import json
import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import keying  # noqa: E402

REPO = HERE.parents[1]
SHEETS = HERE / "icon_sheets.json"
OUT_DIRS = (Path("qml/assets/icons"), Path("omarchy-plugin/assets/icons"))
DEFAULT_SRC = Path(os.environ.get("CODEPET_ICON_SRC", REPO / "art-src" / "icons-v0.3")).expanduser()
WEBP = dict(format="WEBP", quality=86, method=6, lossless=False, exact=False, alpha_quality=100)
KEYING_KEYS = ("clear_share", "pocket_share", "pocket_clear", "despill", "spill_gamut")
MARGIN = 2          # px of clear alpha kept on every side of a shipped icon


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def cut_cells(path, key_hex, cols, rows, opts):
    """Key one sheet and return its cells as tight RGBA uint8 arrays (row-major)."""
    img = Image.open(path)
    img.load()
    rgb = np.asarray(img.convert("RGB"))
    kind = keying.parse_hex(key_hex)
    fg, alpha, _info = keying.key_sheet(rgb, kind, **{k: opts[k] for k in KEYING_KEYS if k in opts})
    masks, warnings = keying.segment(alpha, cols=cols, rows=rows, reassign=opts.get("reassign", ()))
    cells = []
    for i, m in enumerate(masks):
        a = alpha * m
        ys, xs = np.nonzero(a > 0.02)
        if len(xs) == 0:
            raise SystemExit(f"{path}: cell {i} is empty after keying")
        x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
        rgba = np.dstack([np.clip(np.rint(fg[y0:y1, x0:x1]), 0, 255),
                          np.clip(np.rint(a[y0:y1, x0:x1] * 255), 0, 255)]).astype(np.uint8)
        rgba[rgba[..., 3] == 0, :3] = 0
        cells.append(rgba)
    return cells, warnings


def square(rgba, px, pad):
    """Centre a tight cell on a square canvas (`pad` share of the side on each edge) at px."""
    h, w = rgba.shape[:2]
    inner = max(1, px - 2 * max(MARGIN, int(round(px * pad))))
    scale = inner / max(w, h)
    nw, nh = max(1, int(round(w * scale))), max(1, int(round(h * scale)))
    im = Image.fromarray(rgba, "RGBA").convert("RGBa").resize((nw, nh), Image.LANCZOS)
    canvas = Image.new("RGBa", (px, px), (0, 0, 0, 0))
    canvas.paste(im, ((px - nw) // 2, (px - nh) // 2))
    arr = np.array(canvas.convert("RGBA"))
    arr[arr[..., 3] == 0, :3] = 0
    return Image.fromarray(arr, "RGBA")


def encode(im):
    buf = io.BytesIO()
    im.save(buf, **WEBP)
    return buf.getvalue()


def load_spec():
    return json.loads(SHEETS.read_text())


def build(src, spec, log=print):
    """Return {relative file: bytes} for every icon plus index.json."""
    files, index = {}, {}
    for sheet in spec["sheets"]:
        cut = {}
        for part in [sheet] + sheet.get("patches", []):
            label = sheet["id"] if part is sheet else f"{sheet['id']} patch {part['source']}"
            path = src / part["source"]
            if not path.is_file():
                raise SystemExit(f"{label}: source sheet {path} missing")
            data = path.read_bytes()
            if sha256_bytes(data) != part["sha256"]:
                raise SystemExit(f"{label}: {path} sha256 differs from icon_sheets.json")
            if part is not sheet and not {i for i in part["ids"] if i} <= set(sheet["ids"]) | set(sheet.get("added_ids", [])):
                raise SystemExit(f"{label}: patches ids that are not on the sheet")
            cols, rows = part["grid"]
            cells, warnings = cut_cells(path, part["key"], cols, rows, part.get("keying", {}))
            for w in warnings:
                log(f"{label}: {w}")
            if len(cells) != len(part["ids"]):
                raise SystemExit(f"{label}: {len(cells)} cells for {len(part['ids'])} ids")
            pad = part.get("pad", sheet.get("pad", 0.06))
            for icon_id, rgba in zip(part["ids"], cells):
                if icon_id:  # null skips a cell of a shared patch sheet
                    cut[icon_id] = (rgba, pad)
        for icon_id in sheet["ids"] + sheet.get("added_ids", []):
            if icon_id not in cut:
                raise SystemExit(f"{sheet['id']}: no cell for {icon_id}")
            rgba, pad = cut[icon_id]
            rel = f"{sheet['id']}/{icon_id}.webp"
            blob = encode(square(rgba, sheet["px"], pad))
            files[rel] = blob
            index[f"{sheet['id']}/{icon_id}"] = {"file": rel, "px": sheet["px"], "sha256": sha256_bytes(blob)}
    files["index.json"] = (json.dumps({
        "schema": "codepet.icons.index.v1",
        "spec": "SPEC-v0.3 §4.1; ids and sheets from docs/v0.3/contracts/icons.json",
        "path_pattern": "<sheet id>/<icon id>.webp",
        "care_path_pattern": "care-<careIconSet>/<need>.webp",
        "generated_by": "tools/petart/icons.py",
        "icons": index,
    }, indent=1) + "\n").encode()
    return files


def write(root, files):
    for d in OUT_DIRS:
        base = root / d
        keep = set(files) | {"PROVENANCE.md"}
        if base.is_dir():
            for p in sorted(base.rglob("*"), reverse=True):
                rel = p.relative_to(base).as_posix()
                if p.is_file() and rel not in keep:
                    p.unlink()
                elif p.is_dir() and not any(p.iterdir()):
                    p.rmdir()
        for rel, blob in files.items():
            p = base / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            if not p.is_file() or p.read_bytes() != blob:
                p.write_bytes(blob)


def contact(files, spec, out):
    """A review sheet: every icon at 64, 40 and 20 px (bar badges at 20 and 16) on dark and light."""
    rows = []
    for sheet in spec["sheets"]:
        sizes = (20, 16) if sheet["id"] == "bar" else (64, 40, 20)
        rows.append((sheet, sizes))
    cell = 76
    width = max(len(s["ids"]) + len(s.get("added_ids", [])) for s, _ in rows) * cell * 2 + 16
    height = sum(sum(sz) + 8 * len(sz) + 8 for _, sz in rows) + 16
    sheet_im = Image.new("RGBA", (width, height), (0, 0, 0, 255))
    y = 8
    for sheet, sizes in rows:
        for sz in sizes:
            for half, bg in enumerate(((32, 34, 44, 255), (236, 236, 228, 255))):
                x0 = 8 + half * (width // 2)
                band = Image.new("RGBA", (width // 2 - 8, sz + 8), bg)
                sheet_im.paste(band, (x0, y))
                for i, icon_id in enumerate(sheet["ids"] + sheet.get("added_ids", [])):
                    im = Image.open(io.BytesIO(files[f"{sheet['id']}/{icon_id}.webp"])).convert("RGBA")
                    im = im.convert("RGBa").resize((sz, sz), Image.LANCZOS).convert("RGBA")
                    sheet_im.alpha_composite(im, (x0 + 4 + i * cell, y + 4))
            y += sz + 8
        y += 8
    sheet_im.convert("RGB").save(out)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--src", type=Path, default=DEFAULT_SRC, help=f"icon source dir (default {DEFAULT_SRC})")
    ap.add_argument("--check", action="store_true", help="rebuild in a scratch dir and byte-compare")
    ap.add_argument("--contact", type=Path, help="also write a review contact sheet PNG here")
    args = ap.parse_args(argv)
    spec = load_spec()
    files = build(args.src.expanduser(), spec)
    if args.contact:
        contact(files, spec, args.contact)
    if args.check:
        problems = []
        for d in OUT_DIRS:
            for rel, blob in files.items():
                p = REPO / d / rel
                if not p.is_file():
                    problems.append(f"missing: {d / rel}")
                elif p.read_bytes() != blob:
                    problems.append(f"differs: {d / rel}")
            have = {p.relative_to(REPO / d).as_posix() for p in (REPO / d).rglob("*") if p.is_file()}
            for rel in sorted(have - set(files) - {"PROVENANCE.md"}):
                problems.append(f"unexpected (not generated): {d / rel}")
        for p in problems:
            print(p)
        print("check: OK" if not problems else f"check: {len(problems)} problem(s)")
        return 1 if problems else 0
    write(REPO, files)
    print(f"wrote {len(files) - 1} icons + index.json into {', '.join(str(d) for d in OUT_DIRS)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
