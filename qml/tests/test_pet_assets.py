#!/usr/bin/env python3
"""Validate the v0.2 raster pet packs (SPEC §17.5 as amended by Amendment A5).

Checks 15 species x 3 stages x 15 states: manifest schema, registry and PetRegistry.js
agreement, every referenced file present with matching sha256 and nothing unreferenced,
WebP with alpha at 192x192 (thumbnails 96x96), non-empty alpha inside a 2 px margin,
ground contact within 6 px of groundY for floor species' standing frames, pairwise
different locomotion frames, finite durations, aquatic water blocks (per stage too), hopper
airborne frames exposed on the species locomotion block, per-stage hit boxes no larger than
1.35x the stage's alpha union, stage minLevels and default names equal to the Rust core,
open-eyed sleep frames for species that rest with their eyes open, and the provenance and
docs coverage.

Strict by default: all 15 species must be extracted (`placeholder: false`). With
CODEPET_ALLOW_PARTIAL_ART=1, species still awaiting source sheets are skipped with a
reason instead of failing; everything else is still checked.

Images are read with Pillow when available, otherwise with PySide6's QImage. Every check
is an explicit comparison (no bare assert), so `python3 -O` does not disable it.

CODEPET_ASSET_ROOT=<dir> validates another tree laid out like the repository
(qml/assets/pets, qml/shared/PetRegistry.js, docs/pets.md, crates/codepet-core/src);
qml/tests/test_pet_assets_invalid.py uses it to prove broken packs are rejected.
"""
import hashlib
import json
import os
import re
import sys
from pathlib import Path

ROOT = (Path(os.environ["CODEPET_ASSET_ROOT"]).resolve() if os.environ.get("CODEPET_ASSET_ROOT")
        else Path(__file__).resolve().parents[2])
PETS = ROOT / "qml/assets/pets"
REGISTRY_JS = ROOT / "qml/shared/PetRegistry.js"
DOCS = ROOT / "docs/pets.md"
CORE = ROOT / "crates/codepet-core/src"

STATE_NAMES = ("idle", "locomotion", "food", "water", "play", "wash", "bathroom",
               "sleep_enter", "sleep", "sleep_exit", "celebrate", "working", "waiting",
               "needs_care", "error")
CARE = ("food", "water", "play", "wash", "bathroom")
NO_FALLBACK = CARE + ("sleep_enter", "sleep", "sleep_exit")
CANVAS, GROUND_Y, THUMB, MARGIN, GROUND_TOL = 192, 184, 96, 2, 6
HOPPER_STYLES = ("hop", "leap")
HIT_SLACK = 1.35          # a stage hit box may cover at most this much the stage's alpha union area
ALPHA_ON = 16
MODES = ("loop", "once", "hold", "while_moving")
INTERRUPTS = ("any", "higher", "none")
FRAME_RE = re.compile(r"^[bc]\d\d\.webp$")
PARTIAL = os.environ.get("CODEPET_ALLOW_PARTIAL_ART") == "1"

failures = []
skips = []


def fail(msg):
    failures.append(msg)


# ----------------------------------------------------------------- image loading

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
except ImportError:  # CI may carry only PySide6
    load_image = _load_qt


def alpha_info(size, rgba):
    w, h = size
    alpha = rgba[3::4]
    rows = [y for y in range(h) if max(alpha[y * w:(y + 1) * w]) >= ALPHA_ON]
    if not rows:
        return None
    cols = [x for x in range(w) if max(alpha[x::w]) >= ALPHA_ON]
    return cols[0], rows[0], cols[-1] + 1, rows[-1] + 1


# ----------------------------------------------------------------- core constants

def rust_list(src, name):
    m = re.search(name + r"\s*:\s*\[[^\]]*\]\s*=\s*\[(.*?)\];", src, re.S)
    return re.findall(r'"([^"]+)"', m.group(1)) if m else None


species_rs = (CORE / "species.rs").read_text()
CORE_IDS = rust_list(species_rs, "SPECIES_IDS")
CORE_NAMES = rust_list(species_rs, "DEFAULT_NAMES")
lifecycle_rs = (CORE / "lifecycle.rs").read_text()
min_levels = {tuple(int(x) for x in m.split(","))
              for m in re.findall(r"stage_min_level:\s*\[\s*([\d,\s]+?)\s*\]", lifecycle_rs)}
if not CORE_IDS or len(CORE_IDS) != 15 or not CORE_NAMES or len(CORE_NAMES) != 15:
    fail("could not read SPECIES_IDS / DEFAULT_NAMES from crates/codepet-core/src/species.rs")
    CORE_IDS = CORE_IDS or []
if len(min_levels) != 1:
    fail(f"expected one RULES_V1 stage_min_level value in lifecycle.rs, found {sorted(min_levels)}")
RULE_MIN = list(next(iter(min_levels))) if min_levels else [1, 2, 4]

# ----------------------------------------------------------------- registry

registry_path = PETS / "registry.json"
if not registry_path.is_file():
    print("FAIL: qml/assets/pets/registry.json missing")
    sys.exit(1)
registry = json.loads(registry_path.read_text())
js = REGISTRY_JS.read_text() if REGISTRY_JS.is_file() else ""
m = re.match(r"\.pragma library\n(?://[^\n]*\n)*var REGISTRY = (.*);\n?$", js, re.S)
if not m:
    fail("qml/shared/PetRegistry.js must be '.pragma library' + 'var REGISTRY = <json>;'")
elif json.loads(m.group(1)) != registry:
    fail("PetRegistry.js does not embed an object identical to registry.json")
if registry.get("schemaVersion") != 1:
    fail("registry schemaVersion must be 1")
if list(registry.get("stateNames", [])) != list(STATE_NAMES):
    fail("registry stateNames must list the 15 states in canonical order")
if registry.get("order") != CORE_IDS:
    fail(f"registry order {registry.get('order')} != core SPECIES_IDS {CORE_IDS}")
if sorted(registry.get("species", {})) != sorted(CORE_IDS):
    fail("registry species keys must be exactly the 15 ids")
for top in ("_placeholder.webp", "_memorial.webp"):
    p = PETS / top
    if not p.is_file():
        fail(f"{top} missing")
        continue
    fmt, size, has_alpha, rgba = load_image(p)
    if fmt != "WEBP" or size != (CANVAS, CANVAS) or not has_alpha or alpha_info(size, rgba) is None:
        fail(f"{top} must be a non-empty 192x192 WebP with alpha")
lic = (PETS / "LICENSE").read_text() if (PETS / "LICENSE").is_file() else ""
if "MIT License" not in lic:
    fail("qml/assets/pets/LICENSE must be the MIT licence")
provenance = (PETS / "PROVENANCE.md").read_text() if (PETS / "PROVENANCE.md").is_file() else ""
for phrase in ("GPT Image 2", "ChatGPT subscription", "Codex CLI", "PR #7", "MIT", "not been legally reviewed"):
    if phrase not in provenance:
        fail(f"PROVENANCE.md must state {phrase!r}")
docs = DOCS.read_text() if DOCS.is_file() else ""
if not docs:
    fail("docs/pets.md missing")

# ----------------------------------------------------------------- per species

checked = 0
for idx, sid in enumerate(CORE_IDS):
    mpath = PETS / sid / "manifest.json"
    if not mpath.is_file():
        fail(f"{sid}: manifest.json missing")
        continue
    man = json.loads(mpath.read_text())
    reg = registry.get("species", {}).get(sid)
    if reg != {k: v for k, v in man.items() if k not in ("files", "source")}:
        fail(f"{sid}: registry entry differs from manifest.json (re-run extract.py --index)")
    pre = f"{sid}:"
    # schema
    for key in ("schemaVersion", "id", "displayName", "defaultName", "personality", "habitat",
                "assetRevision", "license", "provenance", "placeholder", "careLabels", "careHints",
                "stages", "geometry", "locomotion", "states", "framePattern", "thumbnailPattern", "files"):
        if key not in man:
            fail(f"{pre} manifest lacks {key}")
    if man.get("id") != sid or man.get("schemaVersion") != 1 or man.get("license") != "MIT":
        fail(f"{pre} id/schemaVersion/license wrong")
    if man.get("defaultName") != (CORE_NAMES[idx] if idx < len(CORE_NAMES) else None):
        fail(f"{pre} defaultName {man.get('defaultName')!r} != core DEFAULT_NAMES")
    if f"## {sid}" not in provenance:
        fail(f"{pre} PROVENANCE.md has no section")
    habitat = man.get("habitat")
    if habitat not in ("floor", "aquatic"):
        fail(f"{pre} habitat must be floor or aquatic")
    labels = man.get("careLabels", {})
    for action in CARE + ("sleep",):
        v = labels.get(action)
        if not isinstance(v, str) or not v.strip() or len(v) > 24:
            fail(f"{pre} careLabels.{action} must be 1..24 chars")
    stages = man.get("stages", [])
    if [s.get("stage") for s in stages] != [1, 2, 3] or [s.get("key") for s in stages] != ["young", "grown", "mature"]:
        fail(f"{pre} stages must be 1/young, 2/grown, 3/mature")
    if [s.get("minLevel") for s in stages] != RULE_MIN:
        fail(f"{pre} stage minLevels {[s.get('minLevel') for s in stages]} != RULES_V1 {RULE_MIN}")
    for s in stages:
        if not isinstance(s.get("name"), str) or not s["name"].strip():
            fail(f"{pre} stage {s.get('stage')} has no name")
        elif s["name"] not in docs:
            fail(f"{pre} docs/pets.md does not name stage {s['name']!r}")
    if f"`{sid}`" not in docs:
        fail(f"{pre} docs/pets.md does not list the species")
    geo = man.get("geometry", {})
    if geo.get("viewBox") != [0, 0, CANVAS, CANVAS] or geo.get("groundY") != GROUND_Y:
        fail(f"{pre} geometry viewBox/groundY must be [0,0,192,192]/184")
    for name in ("hitBox", "safeBounds"):
        b = geo.get(name)
        if not (isinstance(b, list) and len(b) == 4 and b[0] >= 0 and b[1] >= 0 and b[2] > 0 and b[3] > 0
                and b[0] + b[2] <= CANVAS and b[1] + b[3] <= CANVAS):
            fail(f"{pre} geometry.{name} must be [x,y,w,h] inside the canvas")
    if habitat == "aquatic":
        water = geo.get("water")
        if not isinstance(water, dict) or not all(k in water for k in ("outer", "waterLineY", "swimBounds")):
            fail(f"{pre} aquatic manifest needs a water block")
        elif not (water["outer"][1] < water["waterLineY"] < water["outer"][1] + water["outer"][3]):
            fail(f"{pre} waterLineY must fall inside water.outer")
        if geo.get("roams") is not False:
            fail(f"{pre} aquatic species must not roam")
    elif geo.get("roams") is not True:
        fail(f"{pre} floor species must roam")
    states = man.get("states", {})
    if sorted(states) != sorted(STATE_NAMES):
        fail(f"{pre} states must be exactly the 15 state names")
    for name, meta in states.items():
        frames = meta.get("frames")
        if not (isinstance(frames, list) and 1 <= len(frames) <= 6 and all(isinstance(f, str) and FRAME_RE.match(f) for f in frames)):
            fail(f"{pre} {name}.frames must list 1..6 files like b00.webp")
            continue
        ms = meta.get("frameMs")
        if not (isinstance(ms, int) and 200 <= ms <= 1000 and ms % 200 == 0):
            fail(f"{pre} {name}.frameMs must be a multiple of 200 in [200, 1000]")
        if meta.get("mode") not in MODES or meta.get("interrupt") not in INTERRUPTS:
            fail(f"{pre} {name} mode/interrupt invalid")
        if meta.get("mode") == "once" and ms * len(frames) > 4000:
            fail(f"{pre} {name} finite total exceeds 4000 ms")
        rf = meta.get("reducedFrame")
        if not (isinstance(rf, int) and 0 <= rf < len(frames)):
            fail(f"{pre} {name}.reducedFrame out of range")
        if name in NO_FALLBACK and meta.get("fallback") is not None:
            fail(f"{pre} {name} may not declare a fallback")
        if habitat == "aquatic" and meta.get("mirror") is not False:
            fail(f"{pre} aquatic {name} must not mirror")
    for name in states:          # fallback chains end at idle without cycles
        seen, cur = set(), name
        while cur is not None and cur in states and cur not in seen:
            seen.add(cur)
            cur = states[cur].get("fallback")
        if cur is not None:
            fail(f"{pre} fallback chain from {name} does not end at idle")
    if states and states.get("idle", {}).get("fallback") is not None:
        fail(f"{pre} idle must have no fallback")

    if man.get("placeholder"):
        if PARTIAL:
            skips.append(f"{sid}: placeholder pack (source sheets not extracted yet)")
        else:
            fail(f"{pre} placeholder:true (release gate: all 15 species must be extracted)")
        continue
    if man.get("placeholder") is not False:
        fail(f"{pre} placeholder must be false or true")
        continue

    # files: referenced set == listed set == on-disk set; hashes match
    files = man.get("files", {})
    referenced = {f"thumb-s{st}.webp" for st in (1, 2, 3)}
    for st in (1, 2, 3):
        for meta in states.values():
            for f in meta.get("frames", []):
                referenced.add(f"s{st}/{f}")
    on_disk = {p.relative_to(PETS / sid).as_posix() for p in (PETS / sid).rglob("*")
               if p.is_file() and p.name != "manifest.json"}
    if set(files) != referenced:
        fail(f"{pre} files block != referenced frames (extra {sorted(set(files) - referenced)[:3]}, "
             f"missing {sorted(referenced - set(files))[:3]})")
    if on_disk != set(files):
        fail(f"{pre} unreferenced or missing files on disk: extra {sorted(on_disk - set(files))[:3]}, "
             f"missing {sorted(set(files) - on_disk)[:3]}")
    airborne = set(states.get("locomotion", {}).get("airborneFrames", []))
    loco = man.get("locomotion", {})
    if loco.get("style") in HOPPER_STYLES:
        # Renderers read species.locomotion.airborneFrames to move only while airborne.
        if not loco.get("airborneFrames"):
            fail(f"{pre} hopper species must expose a non-empty locomotion.airborneFrames")
        elif sorted(loco["airborneFrames"]) != sorted(airborne):
            fail(f"{pre} locomotion.airborneFrames {loco['airborneFrames']} != states.locomotion.airborneFrames {sorted(airborne)}")
    # A species whose sleep hint says it rests with its eyes open settles and sleeps only on
    # the reviewed open-eyed poses (b00 idle, b07 asleep) or cells rendered from them: the
    # goldfish c09 "settle to sleep" cell draws closed eyes and is remapped to b00.
    if "eyes open" in str(man.get("careHints", {}).get("sleep", "")):
        for st in (1, 2, 3):
            open_eyed = {files.get(f"s{st}/{f}") for f in ("b00.webp", "b07.webp")} - {None}
            for name in ("sleep_enter", "sleep"):
                for f in states.get(name, {}).get("frames", []):
                    if files.get(f"s{st}/{f}") not in open_eyed:
                        fail(f"{pre} s{st} {name} frame {f} is not an open-eyed pose (b00/b07)")
    loco_frames = states.get("locomotion", {}).get("frames", [])
    airborne_files = {loco_frames[i] for i in airborne if 0 <= i < len(loco_frames)}
    pixels = {}
    for rel in sorted(files):
        p = PETS / sid / rel
        if not p.is_file():
            continue
        data = p.read_bytes()
        if hashlib.sha256(data).hexdigest() != files[rel]:
            fail(f"{pre} {rel} sha256 mismatch")
        try:
            fmt, size, has_alpha, rgba = load_image(p)
        except Exception as e:  # noqa: BLE001
            fail(f"{pre} {rel} unreadable: {e}")
            continue
        want = (THUMB, THUMB) if rel.startswith("thumb-") else (CANVAS, CANVAS)
        if fmt != "WEBP" or not has_alpha or size != want:
            fail(f"{pre} {rel} must be WebP with alpha at {want[0]}x{want[1]} (got {fmt} {size} alpha={has_alpha})")
            continue
        box = alpha_info(size, rgba)
        if box is None:
            fail(f"{pre} {rel} has an empty alpha channel")
            continue
        if rel.startswith("thumb-"):
            continue
        pixels[rel] = rgba
        if box[0] < MARGIN or box[1] < MARGIN or box[2] > CANVAS - MARGIN or box[3] > CANVAS - MARGIN:
            fail(f"{pre} {rel} alpha bbox {box} leaves the 2 px margin")
        frame = rel.split("/", 1)[1]
        if habitat == "floor" and frame not in airborne_files and abs(box[3] - GROUND_Y) > GROUND_TOL:
            fail(f"{pre} {rel} bottom {box[3]} not within {GROUND_TOL} px of groundY {GROUND_Y}")
    stage_union = {}
    for rel, rgba in pixels.items():
        b = alpha_info((CANVAS, CANVAS), rgba)
        u = stage_union.get(rel[:2])
        stage_union[rel[:2]] = b if u is None else (min(u[0], b[0]), min(u[1], b[1]), max(u[2], b[2]), max(u[3], b[3]))
    by_stage = (geo.get("water") or {}).get("byStage") if habitat == "aquatic" else None
    if habitat == "aquatic" and sorted(by_stage or {}) != ["1", "2", "3"]:
        fail(f"{pre} aquatic water block needs byStage entries for stages 1-3")
    for st in (1, 2, 3):
        u = stage_union.get(f"s{st}")
        if u is None:
            continue
        area = (u[2] - u[0]) * (u[3] - u[1])
        boxes = {"stageHitBoxes": (geo.get("stageHitBoxes") or {}).get(str(st))}
        if by_stage:
            wb = by_stage.get(str(st)) or {}
            boxes["water.byStage.outer"] = wb.get("outer")
            if wb.get("outer") and not (wb["outer"][1] < wb.get("waterLineY", -1) < wb["outer"][1] + wb["outer"][3]):
                fail(f"{pre} stage {st} waterLineY must fall inside its water.byStage outer box")
        for name, bx in boxes.items():
            if not (isinstance(bx, list) and len(bx) == 4):
                fail(f"{pre} geometry {name} for stage {st} missing")
            elif bx[2] * bx[3] > HIT_SLACK * area:
                fail(f"{pre} stage {st} {name} {bx} is more than {HIT_SLACK}x its alpha union {list(u)}")
    for st in (1, 2, 3):
        imgs = [pixels.get(f"s{st}/{f}") for f in loco_frames]
        for i in range(len(imgs)):
            for j in range(i + 1, len(imgs)):
                if imgs[i] is not None and imgs[i] == imgs[j]:
                    fail(f"{pre} s{st} locomotion frames {loco_frames[i]} and {loco_frames[j]} are identical")
    checked += 1

for s in skips:
    print(f"SKIP: {s}")
if failures:
    for f in failures[:80]:
        print(f"FAIL: {f}")
    if len(failures) > 80:
        print(f"... and {len(failures) - 80} more")
    sys.exit(1)
mode = "partial (CODEPET_ALLOW_PARTIAL_ART=1)" if PARTIAL else "strict"
print(f"PASS: {checked} of 15 pet packs verified ({mode}); {len(skips)} skipped; "
      f"registry, PetRegistry.js, provenance and docs consistent")
