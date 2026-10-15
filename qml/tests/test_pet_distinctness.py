#!/usr/bin/env python3
"""Pet silhouettes must be distinct (SPEC §17.5 item 6 as amended by Amendment A5).

- 32x32 alpha masks of each species' stage-2 b00 (the whole 192x192 canvas, so habitat,
  scale and placement count): pairwise IoU < 0.85 across species.
- Within a species, the three stages' b00 masks must differ: IoU < 0.97 or alpha-bbox
  aspect ratios differing by more than 3 %.
- Every care state's interaction frame (frames[1]) differs from idle b00 on > 4 % of
  pixels (premultiplied colour or alpha, so a fish moving inside a fixed tank counts), and no animated state lists only byte-identical images.

Strict by default; CODEPET_ALLOW_PARTIAL_ART=1 skips species that still carry the
placeholder pack (with a reason). Pillow when available, otherwise PySide6.
"""
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PETS = ROOT / "qml/assets/pets"
PARTIAL = os.environ.get("CODEPET_ALLOW_PARTIAL_ART") == "1"
MASK = 32
CARE = ("food", "water", "play", "wash", "bathroom")

failures, skips = [], []


def fail(msg):
    failures.append(msg)


def _alpha_pillow(path, size=None):
    from PIL import Image
    with Image.open(path) as im:
        a = im.convert("RGBA").getchannel("A")
        if size:
            a = a.resize((size, size), Image.BOX)
        return a.size, a.tobytes()


def _alpha_qt(path, size=None):
    from PySide6.QtCore import Qt
    from PySide6.QtGui import QImage
    img = QImage(str(path)).convertToFormat(QImage.Format_RGBA8888)
    if img.isNull():
        raise ValueError(f"unreadable {path}")
    if size:
        img = img.scaled(size, size, Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
    w, h = img.width(), img.height()
    raw = b"".join(bytes(img.constScanLine(y))[: w * 4] for y in range(h))
    return (w, h), raw[3::4]


try:
    import PIL  # noqa: F401
    read_alpha = _alpha_pillow
except ImportError:
    read_alpha = _alpha_qt


def mask(path):
    _, a = read_alpha(path, MASK)
    return [v >= 128 for v in a]


def iou(a, b):
    inter = sum(1 for x, y in zip(a, b) if x and y)
    union = sum(1 for x, y in zip(a, b) if x or y)
    return inter / union if union else 1.0


def aspect(path):
    (w, h), a = read_alpha(path)
    rows = [y for y in range(h) if max(a[y * w:(y + 1) * w]) >= 16]
    cols = [x for x in range(w) if max(a[x::w]) >= 16]
    return (cols[-1] - cols[0] + 1) / (rows[-1] - rows[0] + 1)


def _rgba_pillow(path, size):
    from PIL import Image
    with Image.open(path) as im:
        im = im.convert("RGBa").resize((size, size), Image.BOX)
        return im.tobytes()


def _rgba_qt(path, size):
    from PySide6.QtCore import Qt
    from PySide6.QtGui import QImage
    img = QImage(str(path)).convertToFormat(QImage.Format_RGBA8888_Premultiplied)
    img = img.scaled(size, size, Qt.IgnoreAspectRatio, Qt.SmoothTransformation)
    return b"".join(bytes(img.constScanLine(y))[: size * 4] for y in range(size))


def differs(p, q, size=96):
    """Share of pixels whose premultiplied colour or alpha changes by more than 32/255.

    Colour counts, not only alpha: an aquatic pet's tank silhouette is the same in every
    frame while the fish inside it moves.
    """
    read = _rgba_pillow if read_alpha is _alpha_pillow else _rgba_qt
    a, b = read(p, size), read(q, size)
    changed = 0
    for i in range(0, len(a), 4):
        if (abs(a[i] - b[i]) > 32 or abs(a[i + 1] - b[i + 1]) > 32 or abs(a[i + 2] - b[i + 2]) > 32
                or abs(a[i + 3] - b[i + 3]) > 32):
            changed += 1
    return changed / (size * size)


registry = json.loads((PETS / "registry.json").read_text())
ready = []
for sid in registry["order"]:
    man = json.loads((PETS / sid / "manifest.json").read_text())
    if man.get("placeholder"):
        if PARTIAL:
            skips.append(f"{sid}: placeholder pack (source sheets not extracted yet)")
        else:
            fail(f"{sid}: placeholder pack (release gate)")
        continue
    ready.append((sid, man))

masks = {}
for sid, man in ready:
    d = PETS / sid
    masks[sid] = mask(d / "s2/b00.webp")
    stage_masks = [mask(d / f"s{st}/b00.webp") for st in (1, 2, 3)]
    aspects = [aspect(d / f"s{st}/b00.webp") for st in (1, 2, 3)]
    for i in range(3):
        for j in range(i + 1, 3):
            v = iou(stage_masks[i], stage_masks[j])
            da = abs(aspects[i] - aspects[j]) / max(aspects[i], aspects[j])
            if not (v < 0.97 or da > 0.03):
                fail(f"{sid}: stages {i + 1} and {j + 1} idle silhouettes too similar (IoU {v:.3f}, aspect diff {da:.1%})")
    for st in (1, 2, 3):
        for state in CARE:
            frames = man["states"][state]["frames"]
            if len(frames) > 1:
                frac = differs(d / f"s{st}/{frames[1]}", d / f"s{st}/b00.webp")
                if frac <= 0.04:
                    fail(f"{sid}: s{st} {state} interaction frame differs from idle on only {frac:.1%} of pixels")
        for state, meta in man["states"].items():
            files = meta["frames"]
            if len(set(files)) > 1:
                blobs = {(d / f"s{st}/{f}").read_bytes() for f in files}
                if len(blobs) < 2:
                    fail(f"{sid}: s{st} {state} frames are all byte-identical")

ids = [sid for sid, _ in ready]
worst = (0.0, None)
for i in range(len(ids)):
    for j in range(i + 1, len(ids)):
        v = iou(masks[ids[i]], masks[ids[j]])
        if v > worst[0]:
            worst = (v, (ids[i], ids[j]))
        if v >= 0.85:
            fail(f"{ids[i]} vs {ids[j]}: stage-2 idle IoU {v:.3f} >= 0.85")

for s in skips:
    print(f"SKIP: {s}")
if failures:
    for f in failures:
        print(f"FAIL: {f}")
    sys.exit(1)
pair = f"{worst[1][0]}/{worst[1][1]} {worst[0]:.3f}" if worst[1] else "n/a"
print(f"PASS: {len(ids)} species distinct (max cross-species idle IoU {pair}); stages differ; "
      f"care frames differ from idle")
