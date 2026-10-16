#!/usr/bin/env python3
"""Generate the renderer test pack (two species, WebP with alpha).

This is TEST data for PetCatalog/PetSprite/DesktopPet, not shipped art. It follows
the v0.2 asset contract (SPEC Amendment A): 192x192 transparent WebP frames named
b00..b11 (base sheet cells) and c00..c11 (care sheet cells) per stage, explicit
per-state frame lists, thumb-s<n>.webp, _placeholder.webp and _memorial.webp.

Species:
  cat       floor quadruped (roams, mirrors when walking left)
  goldfish  aquatic, fixed habitat (roams:false, mirror:false, water block)
  ghost     registry-only entry whose files do not exist (missing-asset fallback)

Run: python3 qml/tests/fixtures/pets/make_fixtures.py   (deterministic; --check compares)
Requires Pillow with WebP support. Never run at product runtime.
"""
import hashlib
import json
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
CANVAS = 192
GROUND_Y = 184

# SPEC Amendment A4 state table; priority/interrupt from SPEC §11.2.
STATES = {
    "idle":        (["b00", "b00", "b01", "b00"], 600, True, 0, 0, "any"),
    "locomotion":  (["b08", "b09", "b10", "b11"], 200, True, 0, 20, "any"),
    "food":        (["b03", "c00", "c01", "c00", "c01", "b04"], 400, False, 1, 70, "higher"),
    "water":       (["b03", "c02", "c03", "c02", "c03", "b04"], 400, False, 1, 70, "higher"),
    "play":        (["b03", "c04", "c05", "c04", "c05", "b04"], 400, False, 1, 70, "higher"),
    "wash":        (["b00", "c06", "c07", "c06", "c07", "b04"], 400, False, 1, 70, "higher"),
    "bathroom":    (["b03", "c08", "c08", "b00"], 600, False, 1, 70, "higher"),
    "sleep_enter": (["c09", "b07"], 600, False, 1, 90, "none"),
    "sleep":       (["b07"], 1000, True, 0, 85, "higher"),
    "sleep_exit":  (["c10", "b00"], 600, False, 1, 90, "none"),
    "celebrate":   (["c11", "b04", "c11", "b04"], 400, False, 0, 60, "higher"),
    "working":     (["b04", "b00"], 800, True, 0, 40, "any"),
    "waiting":     (["b02", "b00"], 800, True, 0, 35, "any"),
    "needs_care":  (["b05"], 800, True, 0, 50, "any"),
    "error":       (["b06"], 1000, True, 0, 100, "higher"),
}
CARE_STATES = {"food", "water", "play", "wash", "bathroom", "sleep_enter", "sleep", "sleep_exit"}

SPECIES = {
    "cat": {
        "displayName": "Cat", "defaultName": "Ginger", "personality": "the curious desk inspector",
        "habitat": "floor", "body": (239, 171, 101), "belly": (255, 240, 212),
        "careLabels": {"food": "Kibble", "water": "Fresh water", "play": "Yarn tuft", "wash": "Groom",
                       "bathroom": "Litter screen", "sleep": "Loaf nap"},
        "careHints": {},
        "stages": ["Pounce Kit", "Desk Prowler", "Maned Inspector"],
        "hitBox": [30, 60, 112, 124],
        "locomotion": {"style": "walk", "pxPerTick": 4, "pauseTicks": [40, 110], "airborneFrames": []},
    },
    "goldfish": {
        "displayName": "Goldfish", "defaultName": "Bubbles", "personality": "the dreamy bubble navigator",
        "habitat": "aquatic", "body": (240, 138, 60), "belly": (255, 211, 168),
        "careLabels": {"food": "Surface pellets", "water": "Water refresh", "play": "Plant arch",
                       "wash": "Tank clean", "bathroom": "Filter clean", "sleep": "Plant hover"},
        "careHints": {"water": "Fish don't drink from bowls; this refreshes the tank water."},
        "stages": ["Fry", "Goldfish", "Fantail"],
        "hitBox": [24, 40, 144, 144],
        "water": {"outer": [24, 40, 144, 144], "waterLineY": 56, "swimBounds": [40, 72, 112, 96]},
        "locomotion": {"style": "swim_in_habitat", "pxPerTick": 0, "pauseTicks": [30, 75], "airborneFrames": []},
    },
}


def frame_image(species, stage, cell):
    """A tiny deterministic figure. Cell index is encoded as dots so frames differ."""
    spec = SPECIES[species]
    img = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    scale = (0.82, 0.92, 1.0)[stage - 1]
    if spec["habitat"] == "aquatic":
        x, y, w, h = spec["water"]["outer"]
        d.rectangle([x, y, x + w - 1, y + h - 1], fill=(127, 196, 216, 110), outline=(80, 140, 170, 255))
        cx, cy = 96, 120
        rw, rh = 34 * scale, 20 * scale
        d.ellipse([cx - rw, cy - rh, cx + rw, cy + rh], fill=spec["body"] + (255,))
    else:
        w, h = 100 * scale, 110 * scale
        d.ellipse([96 - w / 2, GROUND_Y - h, 96 + w / 2, GROUND_Y], fill=spec["body"] + (255,))
        d.ellipse([96 - w / 4, GROUND_Y - h * 0.6, 96 + w / 4, GROUND_Y - 4], fill=spec["belly"] + (255,))
    kind, number = cell[0], int(cell[1:])
    for i in range(number + 1):
        d.rectangle([20 + i * 12, 8, 28 + i * 12, 16], fill=(40, 40, 60, 255) if kind == "b" else (200, 60, 120, 255))
    return img


def neutral(kind):
    img = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if kind == "placeholder":
        d.ellipse([46, 64, 146, GROUND_Y], fill=(150, 160, 175, 200))
    else:
        d.rounded_rectangle([60, 70, 132, GROUND_Y], radius=24, fill=(150, 156, 166, 255))
        d.ellipse([136, 150, 156, 170], fill=(240, 160, 190, 255))
    return img


def save(img, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, "WEBP", lossless=True, quality=100, method=6, exact=False)


def build(out):
    files = {}
    order = list(SPECIES)
    registry = {"schemaVersion": 1, "assetRevision": "wp6-fixture", "stateNames": list(STATES),
                "order": order + ["ghost"], "species": {}}
    for sid, spec in SPECIES.items():
        manifest_files = {}
        cells = [f"b{i:02d}" for i in range(12)] + [f"c{i:02d}" for i in range(12)]
        for stage in (1, 2, 3):
            for cell in cells:
                rel = f"s{stage}/{cell}.webp"
                save(frame_image(sid, stage, cell), out / sid / rel)
                manifest_files[rel] = None
            thumb = frame_image(sid, stage, "b00").resize((96, 96), Image.LANCZOS)
            save(thumb, out / sid / f"thumb-s{stage}.webp")
            manifest_files[f"thumb-s{stage}.webp"] = None
        aquatic = spec["habitat"] == "aquatic"
        states = {}
        for name, (frames, ms, loop, reduced, prio, interrupt) in STATES.items():
            states[name] = {"frames": [f + ".webp" for f in frames], "frameMs": ms, "loop": loop,
                            "repeats": 1, "reducedFrame": reduced, "mirror": not aquatic and name != "error",
                            "priority": prio, "interrupt": interrupt,
                            "fallback": None if name in CARE_STATES or name == "idle" else "idle"}
        geometry = {"viewBox": [0, 0, CANVAS, CANVAS], "groundY": GROUND_Y, "hitBox": spec["hitBox"],
                    "roams": not aquatic}
        if aquatic:
            geometry["water"] = spec["water"]
        manifest = {
            "schemaVersion": 1, "id": sid, "displayName": spec["displayName"], "defaultName": spec["defaultName"],
            "personality": spec["personality"], "habitat": spec["habitat"], "assetRevision": "wp6-fixture",
            "license": "MIT", "placeholder": False,
            "careLabels": spec["careLabels"], "careHints": spec["careHints"],
            "stages": [{"stage": i + 1, "key": k, "name": n, "minLevel": lvl}
                       for i, (k, n, lvl) in enumerate(zip(["young", "grown", "mature"], spec["stages"], [1, 2, 4]))],
            "geometry": geometry, "locomotion": spec["locomotion"], "states": states,
            "thumbnailPattern": "thumb-s{stage}.webp",
        }
        for rel in manifest_files:
            manifest_files[rel] = hashlib.sha256((out / sid / rel).read_bytes()).hexdigest()
        manifest["files"] = manifest_files
        (out / sid / "manifest.json").write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n")
        registry["species"][sid] = {k: v for k, v in manifest.items() if k != "files"}
        files.update({f"{sid}/{k}": v for k, v in manifest_files.items()})
    # A registry entry whose pack is missing on disk: renderers must fall back to the
    # placeholder without any backend effect.
    ghost = json.loads(json.dumps(registry["species"]["cat"]))
    ghost.update({"id": "ghost", "displayName": "Ghost", "defaultName": "Boo", "personality": "a missing pack"})
    registry["species"]["ghost"] = ghost
    for kind in ("placeholder", "memorial"):
        save(neutral(kind), out / f"_{kind}.webp")
    (out / "registry.json").write_text(json.dumps(registry, indent=1, sort_keys=True) + "\n")
    (out / "PetRegistry.js").write_text(".pragma library\n// GENERATED test fixture (make_fixtures.py); not the shipped registry.\nvar REGISTRY = "
                                        + json.dumps(registry, sort_keys=True) + ";\n")
    return files


def snapshot_tree(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(root.rglob("*")) if p.is_file() and p.suffix != ".py"}


def main():
    if "--check" in sys.argv:
        with tempfile.TemporaryDirectory(dir=HERE.parent) as tmp:
            build(Path(tmp))
            fresh, current = snapshot_tree(Path(tmp)), snapshot_tree(HERE)
        if fresh != current:
            diff = sorted(set(fresh.items()) ^ set(current.items()))
            print("fixture pack is stale:", diff[:10])
            sys.exit(1)
        print("fixture pack up to date:", len(fresh), "files")
        return
    build(HERE)
    print("wrote fixture pack to", HERE)


if __name__ == "__main__":
    main()
