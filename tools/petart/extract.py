#!/usr/bin/env python3
"""Extract CodePet v0.2 pet frames from GPT Image 2 source sheets (Amendment A3).

Dev-time tool only; nothing here runs inside the app. Python 3, Pillow, numpy, scipy.

    python3 tools/petart/extract.py --status          # which species have six final sheets
    python3 tools/petart/extract.py --species cat,dog # extract those (then re-index)
    python3 tools/petart/extract.py --all             # every ready species + shared assets + index
    python3 tools/petart/extract.py --index           # registry.json, PetRegistry.js, PROVENANCE.md, docs/pets.md
    python3 tools/petart/extract.py --check           # rebuild into a scratch dir and byte-compare
    python3 tools/petart/extract.py --contact         # contact sheets into the run directory

Source sheets live outside the repository (about 200 MB):
`$CODEPET_ART_SRC` or `~/Media/codepet-art/v0.2/<srcdir>/stage{1,2,3}-{base,care}.png`
with `provenance.json`. Files named `*.reject-*.png` are ignored. A species is *ready* when
all six sheets exist and each matches the sha256 recorded in its `provenance.json`;
species that are not ready get a `placeholder: true` manifest so the registry always
lists all 15 companions.
"""

import argparse
import hashlib
import io
import json
import os
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import keying  # noqa: E402
import roster  # noqa: E402
from roster import CANVAS, GROUND_Y, MARGIN  # noqa: E402

REPO = HERE.parents[1]
PETS_REL = Path("qml/assets/pets")
REGISTRY_JS_REL = Path("qml/shared/PetRegistry.js")
DOCS_REL = Path("docs/pets.md")
DEFAULT_SRC = Path(os.environ.get("CODEPET_ART_SRC", "~/Media/codepet-art/v0.2")).expanduser()
DEFAULT_RUN = Path(os.environ.get("CODEPET_RUN_DIR", "~/Work/runs/codepet--2026-10-15--v0.2")).expanduser()
OVERRIDES = HERE / "overrides.json"

WEBP = dict(format="WEBP", quality=86, method=6, lossless=False, exact=False)
THUMB = 96
FOOT_SHARE = 0.30          # lower share of the pose height used for the floor anchor
AIRBORNE_MIN_PX = 4        # hopper elevation below this (canvas px) is treated as grounded
AIRBORNE_MAX_PX = 40
FIT_SLACK = 1              # px reserved against rounding


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


def sha256_file(p):
    return sha256_bytes(Path(p).read_bytes())


def cell_name(kind, i):
    return f"{'b' if kind == 'base' else 'c'}{i:02d}"


# ----------------------------------------------------------------- sources

def load_provenance(src_root, sid):
    d = src_root / roster.source_dir(sid)
    p = d / "provenance.json"
    if not p.is_file():
        return None
    return json.loads(p.read_text())


def stage_names(sid, prov):
    fallback = list(roster.SPECIES[sid]["stageNamesFallback"])
    names = (prov or {}).get("stage_names")
    if isinstance(names, dict):
        got = [names.get(str(i)) or names.get(i) for i in (1, 2, 3)]
    elif isinstance(names, list) and len(names) == 3:
        got = names
    else:
        got = [None, None, None]
    return [g if isinstance(g, str) and g.strip() else f for g, f in zip(got, fallback)], \
        ("provenance.json" if any(isinstance(g, str) and g.strip() for g in got) else "spec §11.5")


def readiness(src_root, sid):
    """(ready, reason, sheets) where sheets = [{stage, kind, file, path, sha256, chroma_hex}]."""
    d = src_root / roster.source_dir(sid)
    if not d.is_dir():
        return False, f"source dir {d} missing", []
    missing = [f"stage{s}-{k}.png" for s in (1, 2, 3) for k in ("base", "care")
               if not (d / f"stage{s}-{k}.png").is_file()]
    if missing:
        return False, "missing " + ", ".join(missing), []
    prov = load_provenance(src_root, sid)
    if prov is None:
        return False, "provenance.json missing", []
    by_file = {s.get("file"): s for s in prov.get("sheets", []) if isinstance(s, dict)}
    sheets = []
    for s in (1, 2, 3):
        for k in ("base", "care"):
            name = f"stage{s}-{k}.png"
            entry = by_file.get(name)
            if entry is None:
                return False, f"provenance.json has no entry for {name}", []
            actual = sha256_file(d / name)
            if entry.get("sha256") != actual:
                return False, f"{name} sha256 differs from provenance.json (sheet still changing?)", []
            chroma = entry.get("chroma_hex") or prov.get("chroma_hex") or "#00FF00"
            keying.parse_hex(chroma)
            sheets.append({"stage": s, "kind": k, "file": name, "path": d / name,
                           "sha256": actual, "chroma_hex": chroma.upper()})
    return True, "six final sheets", sheets


def load_overrides():
    if not OVERRIDES.is_file():
        return {}
    return json.loads(OVERRIDES.read_text()).get("species", {})


def cell_override(ov, stage, name):
    cells = ov.get("cells", {})
    out = {}
    for key in (f"*/{name}", f"s{stage}/{name}"):
        if key in cells:
            out.update({k: v for k, v in cells[key].items() if k != "why"})
    return out


# ----------------------------------------------------------------- per-cell geometry

class Cell:
    __slots__ = ("name", "rgba", "w", "h", "foot_cx", "bottom_abs", "flip", "dx", "dy",
                 "fit_exclude", "source_name", "warnings", "mul")


def cut_cells(sheet, kind):
    img = Image.open(sheet["path"])
    img.load()
    rgb = np.asarray(img.convert("RGB"))
    kkind = keying.parse_hex(sheet["chroma_hex"])
    fg, alpha, info = keying.key_sheet(rgb, kkind)
    masks, warnings = keying.segment(alpha)
    cells = []
    for i, m in enumerate(masks):
        a = alpha * m
        ys, xs = np.nonzero(a > 0.02)
        if len(xs) == 0:
            raise SystemExit(f"{sheet['path']}: cell {i} is empty after keying")
        x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
        crop_a = a[y0:y1, x0:x1]
        crop_rgb = fg[y0:y1, x0:x1]
        rgba = np.dstack([np.clip(np.rint(crop_rgb), 0, 255),
                          np.clip(np.rint(crop_a * 255), 0, 255)]).astype(np.uint8)
        rgba[rgba[..., 3] == 0, :3] = 0
        c = Cell()
        c.name = cell_name(kind, i)
        c.rgba = rgba
        c.h, c.w = rgba.shape[:2]
        c.bottom_abs = int(y1)
        c.warnings = []
        cells.append(c)
    return cells, warnings, info


def foot_centroid(rgba):
    a = rgba[..., 3].astype(np.float64)
    h = a.shape[0]
    lower = a[int(h * (1 - FOOT_SHARE)):]
    s = lower.sum()
    if s <= 0:
        return a.shape[1] / 2.0
    return float((lower.sum(axis=0) * (np.arange(a.shape[1]) + 0.5)).sum() / s)


def prepare(cell, flip, habitat):
    if flip:
        cell.rgba = np.ascontiguousarray(cell.rgba[:, ::-1])
    cell.foot_cx = foot_centroid(cell.rgba) if habitat == "floor" else cell.w / 2.0


def fit_scale(cell, elev_src, habitat):
    usable_h = GROUND_Y - MARGIN - FIT_SLACK
    s_h = usable_h / (cell.h + elev_src)
    # Width only: a pose too lopsided to centre on its feet (a leap, a long tail) is slid
    # sideways just enough to stay on the canvas instead of shrinking every frame.
    s_w = (CANVAS - 2 * MARGIN - 2 * FIT_SLACK) / cell.w
    return min(s_h, s_w) / cell.mul


def render(cell, scale, elev_canvas, habitat):
    scale = scale * cell.mul
    w = max(1, int(round(cell.w * scale)))
    h = max(1, int(round(cell.h * scale)))
    im = Image.fromarray(cell.rgba, "RGBA").convert("RGBa").resize((w, h), Image.LANCZOS).convert("RGBA")
    anchor = cell.foot_cx * (w / cell.w)
    x = int(round(CANVAS / 2 - anchor)) + int(cell.dx)
    x = min(max(x, MARGIN), CANVAS - MARGIN - w)
    y = GROUND_Y - h - int(round(elev_canvas)) + int(cell.dy)
    if x < MARGIN or y < MARGIN or x + w > CANVAS - MARGIN or y + h > CANVAS - MARGIN:
        raise SystemExit(f"{cell.name}: placement {x},{y} {w}x{h} leaves the canvas margins")
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    canvas.alpha_composite(im, dest=(x, y))
    arr = np.array(canvas)
    arr[arr[..., 3] == 0, :3] = 0
    return Image.fromarray(arr, "RGBA")


def encode(im, size=None):
    if size is not None:
        im = im.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA")
        arr = np.array(im)
        arr[arr[..., 3] == 0, :3] = 0
        im = Image.fromarray(arr, "RGBA")
    buf = io.BytesIO()
    im.save(buf, **WEBP)
    return buf.getvalue()


def alpha_bbox(im, threshold=16):
    a = np.asarray(im)[..., 3]
    ys, xs = np.nonzero(a >= threshold)
    if len(xs) == 0:
        return None
    return [int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1]


def union(boxes):
    boxes = [b for b in boxes if b]
    return [min(b[0] for b in boxes), min(b[1] for b in boxes),
            max(b[2] for b in boxes), max(b[3] for b in boxes)]


def xywh(b):
    return [b[0], b[1], b[2] - b[0], b[3] - b[1]]


# ----------------------------------------------------------------- species extraction

def extract_species(sid, src_root, out_root, log=print):
    ready, reason, sheets = readiness(src_root, sid)
    sp = roster.SPECIES[sid]
    habitat = sp["habitat"]
    if not ready:
        log(f"{sid}: not ready ({reason}); writing placeholder manifest")
        write_manifest(out_root, placeholder_manifest(sid, src_root, reason))
        return None
    ov = load_overrides().get(sid, {})
    prov = load_provenance(src_root, sid)
    sdir = out_root / PETS_REL / sid
    files = {}
    outputs = {}
    stage_meta = []
    warnings = []
    stage_boxes = {}
    airborne = set()
    for stage in (1, 2, 3):
        raw = {}
        keyinfo = {}
        for kind in ("base", "care"):
            sheet = next(s for s in sheets if s["stage"] == stage and s["kind"] == kind)
            cells, warn, info = cut_cells(sheet, kind)
            keyinfo[kind] = info
            warnings += [f"s{stage} {kind}: {w}" for w in warn]
            for c in cells:
                raw[c.name] = c
        # Per-cell overrides: source remap, flip, nudge, fit exclusion.
        cells = {}
        applied = {}
        for name in [cell_name("base", i) for i in range(12)] + [cell_name("care", i) for i in range(12)]:
            o = cell_override(ov, stage, name)
            src = raw[o.get("source", name)]
            c = Cell()
            c.name = name
            c.source_name = src.name
            c.rgba, c.w, c.h, c.bottom_abs = src.rgba, src.w, src.h, src.bottom_abs
            c.dx, c.dy = o.get("dx", 0), o.get("dy", 0)
            c.fit_exclude = bool(o.get("fit_exclude", False))
            sheet_key = f"s{stage}/{'base' if name.startswith('b') else 'care'}"
            c.mul = float(ov.get("sheets", {}).get(sheet_key, {}).get("scale", 1.0))
            prepare(c, bool(o.get("flip", False)), habitat)
            cells[name] = c
            if o:
                why = [ov.get("cells", {}).get(k, {}).get("why") for k in (f"*/{name}", f"s{stage}/{name}")]
                applied[name] = dict(o, why="; ".join(w for w in why if w))
        # Hopper elevation: keep the sheet's row-relative lift on locomotion frames.
        elev_src = {n: 0 for n in cells}
        if habitat == "floor" and sp["hopper"]:
            loco = [cells[f"b{i:02d}"] for i in range(8, 12)]
            row_bottom = max(c.bottom_abs for c in loco)
            for c in loco:
                elev_src[c.name] = row_bottom - c.bottom_abs
        fits = [fit_scale(c, elev_src[c.name], habitat) for c in cells.values() if not c.fit_exclude]
        fit = min(fits)
        factor = roster.STAGE_FACTORS[stage - 1]
        scale = fit * factor
        for name, c in cells.items():
            s = scale if not c.fit_exclude else min(scale, fit_scale(c, elev_src[name], habitat) * factor)
            elev = elev_src[name] * s * c.mul
            if elev < AIRBORNE_MIN_PX:
                elev = 0
            elev = min(elev, AIRBORNE_MAX_PX)
            if elev and name.startswith("b") and 8 <= int(name[1:]) <= 11:
                airborne.add(int(name[1:]) - 8)
            im = render(c, s, elev, habitat)
            data = encode(im)
            rel = f"s{stage}/{name}.webp"
            outputs[rel] = data
            files[rel] = sha256_bytes(data)
            if name in ("b00", "b03", "b08", "b09", "b10", "b11"):
                stage_boxes.setdefault(stage, []).append(alpha_bbox(im))
            if name == "b00":
                thumb = encode(im, THUMB)
                outputs[f"thumb-s{stage}.webp"] = thumb
                files[f"thumb-s{stage}.webp"] = sha256_bytes(thumb)
        stage_meta.append({"stage": stage, "fitScale": round(fit, 6), "stageFactor": factor,
                           "scale": round(scale, 6), "key": keyinfo,
                           "overrides": applied,
                           "sheetScales": {k: v for k, v in ov.get("sheets", {}).items()
                                           if k.startswith(f"s{stage}/")}})
    # Write outputs; delete stale generated files in this species dir only.
    sdir.mkdir(parents=True, exist_ok=True)
    keep = set(outputs) | {"manifest.json"}
    for p in sorted(sdir.rglob("*")):
        if p.is_file() and p.relative_to(sdir).as_posix() not in keep:
            p.unlink()
    for rel, data in outputs.items():
        p = sdir / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        if not p.is_file() or p.read_bytes() != data:
            p.write_bytes(data)
    for d in sorted(sdir.glob("s*"), reverse=True):
        if d.is_dir() and not any(d.iterdir()):
            d.rmdir()
    manifest = build_manifest(sid, prov, sheets, files, stage_meta, stage_boxes, sorted(airborne),
                              placeholder=False)
    write_manifest(out_root, manifest)
    for w in warnings:
        log(f"{sid}: warning: {w}")
    log(f"{sid}: {len(files)} files, scales " + ", ".join(f"s{m['stage']}={m['scale']:.4f}" for m in stage_meta))
    return manifest


def state_table(sid, airborne):
    habitat = roster.SPECIES[sid]["habitat"]
    states = {}
    for name in roster.STATE_NAMES:
        t = roster.STATES[name]
        frames = [f + ".webp" for f in t["frames"]]
        mode = t["mode"]
        entry = {
            "frames": frames,
            "frameMs": t["frameMs"],
            "mode": mode,
            "loop": mode in ("loop", "while_moving"),
            "repeats": 1,
            "reducedFrame": t["reducedFrame"],
            "mirror": bool(t.get("mirror", True)) and habitat == "floor",
            "priority": t["priority"],
            "interrupt": t["interrupt"],
            "fallback": None if (name == "idle" or name in roster.CARE_STATES or name.startswith("sleep")) else "idle",
            "airborneFrames": list(airborne) if name == "locomotion" else [],
        }
        if mode == "once":
            entry["totalMs"] = t["frameMs"] * len(frames)
        if "breathing" in t:
            entry["breathing"] = dict(t["breathing"])
        states[name] = entry
    return states


def geometry(sid, stage_boxes):
    sp = roster.SPECIES[sid]
    habitat = sp["habitat"]
    per_stage = {str(s): xywh(union(b)) for s, b in sorted(stage_boxes.items())}
    allb = union([union(b) for b in stage_boxes.values()]) if stage_boxes else [40, 40, 152, GROUND_Y]
    hit = xywh(allb)
    geo = {
        "viewBox": [0, 0, CANVAS, CANVAS],
        "canvas": CANVAS,
        "pivot": [CANVAS // 2, GROUND_Y],
        "groundY": GROUND_Y,
        "hitBox": hit,
        "stageHitBoxes": per_stage,
        "safeBounds": [MARGIN, MARGIN, CANVAS - 2 * MARGIN, CANVAS - 2 * MARGIN],
        "propAnchor": [min(CANVAS - MARGIN, allb[2] - 8), GROUND_Y - 8],
        "badgeAnchor": [min(CANVAS - 14, allb[2] - 6), max(14, allb[1] + 6)],
        "roams": habitat == "floor",
    }
    if habitat == "aquatic":
        x, y, w, h = hit
        water_line = y + int(round(h * 0.16))
        geo["water"] = {
            "outer": hit,
            "waterLineY": water_line,
            "swimBounds": [x + int(round(w * 0.16)), water_line + 6,
                           int(round(w * 0.68)), int(round(h * 0.58))],
            "approximate": True,
        }
    return geo


def base_manifest(sid, prov):
    sp = roster.SPECIES[sid]
    names, names_src = stage_names(sid, prov)
    return {
        "schemaVersion": roster.SCHEMA_VERSION,
        "id": sid,
        "displayName": sp["displayName"],
        "defaultName": sp["defaultName"],
        "personality": sp["personality"],
        "habitat": sp["habitat"],
        "assetRevision": roster.ASSET_REVISION,
        "license": "MIT",
        "provenance": f"../PROVENANCE.md#{sid.replace('_', '-')}",
        "careLabels": dict(sp["careLabels"]),
        "careHints": dict(sp["careHints"]),
        "stages": [{"stage": i + 1, "key": roster.STAGE_KEYS[i], "name": names[i],
                    "minLevel": roster.STAGE_MIN_LEVEL[i]} for i in range(3)],
        "stageNamesSource": names_src,
    }


def build_manifest(sid, prov, sheets, files, stage_meta, stage_boxes, airborne, placeholder):
    sp = roster.SPECIES[sid]
    m = base_manifest(sid, prov)
    m["placeholder"] = placeholder
    m["geometry"] = geometry(sid, stage_boxes)
    loco = dict(sp["locomotion"])
    m["locomotion"] = loco
    m["states"] = state_table(sid, airborne)
    m["framePattern"] = "s{stage}/{frame}"
    m["thumbnailPattern"] = "thumb-s{stage}.webp"
    m["source"] = {
        "dir": roster.source_dir(sid),
        "sheets": [{"stage": s["stage"], "kind": s["kind"], "file": s["file"],
                    "sha256": s["sha256"], "chromaHex": s["chroma_hex"]} for s in sheets],
        "extraction": stage_meta,
        "tool": "tools/petart/extract.py",
    }
    ref = (prov or {}).get("reference")
    if isinstance(ref, dict) and ref.get("sha256"):
        m["source"]["reference"] = {"file": ref.get("file"), "sha256": ref.get("sha256"),
                                    "source": ref.get("source")}
    m["source"]["generator"] = (prov or {}).get("tool")
    m["files"] = dict(sorted(files.items()))
    return m


def placeholder_manifest(sid, src_root, reason):
    prov = load_provenance(src_root, sid) if src_root else None
    m = build_manifest(sid, prov, [], {}, [], {}, [], placeholder=True)
    m["source"]["pending"] = "source sheets not extracted yet"
    return m


def write_manifest(out_root, manifest):
    d = out_root / PETS_REL / manifest["id"]
    d.mkdir(parents=True, exist_ok=True)
    if manifest["placeholder"]:
        for p in sorted(d.rglob("*"), reverse=True):
            if p.is_file() and p.name != "manifest.json":
                p.unlink()
            elif p.is_dir() and not any(p.iterdir()):
                p.rmdir()
    write_text(d / "manifest.json", json.dumps(manifest, indent=1, sort_keys=False) + "\n")


def write_text(p, text):
    p.parent.mkdir(parents=True, exist_ok=True)
    if not p.is_file() or p.read_text() != text:
        p.write_text(text)


def write_bytes(p, data):
    p.parent.mkdir(parents=True, exist_ok=True)
    if not p.is_file() or p.read_bytes() != data:
        p.write_bytes(data)


# ----------------------------------------------------------------- shared assets + index

def shared_assets(out_root):
    import shared_art
    pets = out_root / PETS_REL
    write_bytes(pets / "_placeholder.webp", encode(shared_art.placeholder()))
    write_bytes(pets / "_memorial.webp", encode(shared_art.memorial()))
    write_text(pets / "LICENSE", shared_art.LICENSE_TEXT)


def read_manifests(out_root, src_root):
    out = {}
    for sid in roster.ORDER:
        p = out_root / PETS_REL / sid / "manifest.json"
        if p.is_file():
            out[sid] = json.loads(p.read_text())
        else:
            m = placeholder_manifest(sid, src_root, "not extracted yet")
            write_manifest(out_root, m)
            out[sid] = m
    return out


def index(out_root, src_root):
    import docs_gen
    manifests = read_manifests(out_root, src_root)
    registry = {
        "schemaVersion": roster.SCHEMA_VERSION,
        "assetRevision": roster.ASSET_REVISION,
        "stateNames": list(roster.STATE_NAMES),
        "order": list(roster.ORDER),
        "placeholderImage": "_placeholder.webp",
        "memorialImage": "_memorial.webp",
        "species": {sid: {k: v for k, v in m.items() if k not in ("files", "source")}
                    for sid, m in manifests.items()},
    }
    text = json.dumps(registry, indent=1) + "\n"
    write_text(out_root / PETS_REL / "registry.json", text)
    compact = json.dumps(registry, separators=(",", ":"), ensure_ascii=True)
    write_text(out_root / REGISTRY_JS_REL,
               ".pragma library\n// GENERATED by tools/petart/extract.py --index; do not edit.\n"
               f"var REGISTRY = {compact};\n")
    write_text(out_root / PETS_REL / "PROVENANCE.md", docs_gen.provenance_md(manifests, src_root))
    write_text(out_root / DOCS_REL, docs_gen.pets_md(manifests))
    return manifests


# ----------------------------------------------------------------- check

GENERATED_TOP = ("registry.json", "PROVENANCE.md", "LICENSE", "_placeholder.webp", "_memorial.webp")


def tree_hashes(root):
    out = {}
    pets = root / PETS_REL
    if pets.is_dir():
        for p in sorted(pets.rglob("*")):
            if p.is_file():
                out[(PETS_REL / p.relative_to(pets)).as_posix()] = sha256_file(p)
    for rel in (REGISTRY_JS_REL, DOCS_REL):
        if (root / rel).is_file():
            out[rel.as_posix()] = sha256_file(root / rel)
    return out


def check(src_root):
    problems = []
    for sid in roster.ORDER:
        mp = REPO / PETS_REL / sid / "manifest.json"
        if not mp.is_file():
            problems.append(f"{sid}: manifest missing")
            continue
        m = json.loads(mp.read_text())
        if m.get("placeholder"):
            ready, reason, _ = readiness(src_root, sid)
            if ready:
                problems.append(f"{sid}: six sheets are ready but the committed manifest is a placeholder")
            continue
        for s in m["source"]["sheets"]:
            p = src_root / m["source"]["dir"] / s["file"]
            if not p.is_file():
                problems.append(f"{sid}: source sheet {p} unavailable; cannot rebuild")
            elif sha256_file(p) != s["sha256"]:
                problems.append(f"{sid}: {s['file']} changed since extraction")
    if problems:
        return problems
    cache = Path(os.environ.get("XDG_CACHE_HOME", "~/.cache")).expanduser()
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="codepet-petart-check-", dir=cache) as tmp:
        tmp = Path(tmp)
        run_species(list(roster.ORDER), src_root, tmp, log=lambda *_: None)
        shared_assets(tmp)
        index(tmp, src_root)
        want, have = tree_hashes(tmp), tree_hashes(REPO)
        for k in sorted(set(want) | set(have)):
            if k not in have:
                problems.append(f"missing: {k}")
            elif k not in want:
                problems.append(f"unexpected (not generated): {k}")
            elif want[k] != have[k]:
                problems.append(f"differs: {k}")
    return problems


# ----------------------------------------------------------------- parallel driver

def _worker(args):
    sid, src_root, out_root = args
    lines = []
    extract_species(sid, Path(src_root), Path(out_root), log=lines.append)
    return lines


def run_species(sids, src_root, out_root, log=print):
    """Extract several species, in parallel processes (WebP method 6 dominates the time)."""
    jobs = max(1, int(os.environ.get("CODEPET_PETART_JOBS", "3")))
    if jobs == 1 or len(sids) <= 1:
        for sid in sids:
            extract_species(sid, src_root, out_root, log=log)
        return
    from concurrent.futures import ProcessPoolExecutor
    with ProcessPoolExecutor(max_workers=jobs) as ex:
        for lines in ex.map(_worker, [(sid, str(src_root), str(out_root)) for sid in sids]):
            for line in lines:
                log(line)


# ----------------------------------------------------------------- CLI

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--species", help="comma-separated species ids (then re-index)")
    ap.add_argument("--all", action="store_true", help="every species, shared assets and index")
    ap.add_argument("--index", action="store_true", help="registry, PetRegistry.js, PROVENANCE.md, docs/pets.md")
    ap.add_argument("--check", action="store_true", help="rebuild in a scratch dir and compare bytes")
    ap.add_argument("--contact", action="store_true", help="render contact sheets for extracted species")
    ap.add_argument("--status", action="store_true", help="show which species are ready")
    ap.add_argument("--src", type=Path, default=DEFAULT_SRC, help=f"source sheet root (default {DEFAULT_SRC})")
    ap.add_argument("--contact-dir", type=Path, default=DEFAULT_RUN / "contact-sheets")
    args = ap.parse_args(argv)
    src = args.src.expanduser()
    did = False
    if args.status:
        did = True
        for sid in roster.ORDER:
            ready, reason, _ = readiness(src, sid)
            print(f"{sid:10s} {'READY' if ready else 'pending'}  {reason}")
    targets = []
    if args.all:
        targets = list(roster.ORDER)
    elif args.species:
        targets = [s.strip() for s in args.species.split(",") if s.strip()]
        bad = [s for s in targets if s not in roster.SPECIES]
        if bad:
            ap.error(f"unknown species: {', '.join(bad)} (ids: {', '.join(roster.ORDER)})")
    if targets:
        run_species(targets, src, REPO)
        did = True
    if args.all or targets or args.index:
        shared_assets(REPO)
        index(REPO, src)
        did = True
        print("indexed: registry.json, PetRegistry.js, PROVENANCE.md, docs/pets.md")
    if args.contact:
        import contact
        did = True
        args.contact_dir.mkdir(parents=True, exist_ok=True)
        for p in contact.render_all(REPO / PETS_REL, args.contact_dir, species=targets or None):
            print(f"contact sheet: {p}")
    if args.check:
        did = True
        problems = check(src)
        if problems:
            for p in problems:
                print(f"CHECK FAIL: {p}")
            return 1
        print("CHECK OK: generated pet assets are byte-identical to a fresh extraction")
    if not did:
        ap.print_help()
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
