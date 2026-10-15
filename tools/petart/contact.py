"""QA contact sheets for extracted pets (not shipped).

Per species `<id>.png`: for each stage, every state's frame sequence on a light and a
dark background (ground line and centre marked faintly), a mirrored locomotion strip, and
a 96 px / 64 px row. Plus `roster.png`: b00 of every species and stage on light and dark,
and (`gallery`) the same roster at 96 px for the committed docs gallery.

Previews (`render_previews`): per species `<id>-scales.webp` (64/96/112 px at 1x, 1.25x,
1.5x and 2x, facing right and mirrored left) and `<id>-s<stage>-playback.webp`, an animated
WebP stepping through every state at its frameMs in both directions.
"""

import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

import roster

LIGHT = (236, 236, 240, 255)
DARK = (34, 36, 44, 255)
INK_L = (40, 40, 48, 255)
INK_D = (220, 222, 230, 255)
CELL = 112
LABEL_W = 180
PAD = 6


def _font(size):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:
        return ImageFont.load_default()


def _frame(path, size, bg, mirror=False, guides=True):
    tile = Image.new("RGBA", (size, size), bg)
    if path is None or not Path(path).is_file():
        d = ImageDraw.Draw(tile)
        d.line([(0, 0), (size, size)], fill=(200, 60, 60, 255), width=2)
        return tile
    im = Image.open(path).convert("RGBA")
    if mirror:
        im = im.transpose(Image.FLIP_LEFT_RIGHT)
    im = im.resize((size, size), Image.LANCZOS)
    if guides:
        d = ImageDraw.Draw(tile)
        gy = round(roster.GROUND_Y * size / roster.CANVAS)
        d.line([(0, gy), (size, gy)], fill=(200, 80, 80, 90))
        d.line([(size // 2, 0), (size // 2, size)], fill=(80, 80, 200, 50))
    tile.alpha_composite(im)
    return tile


def species_sheet(pets_dir, sid, out_path):
    m = json.loads((pets_dir / sid / "manifest.json").read_text())
    if m.get("placeholder"):
        return None
    maxf = max(len(s["frames"]) for s in m["states"].values())
    half = LABEL_W + maxf * (CELL + PAD)
    width = 2 * half + PAD * 3
    rows = []  # (kind, payload)
    for st in (1, 2, 3):
        rows.append(("title", f"{m['displayName']} ({sid}) stage {st}: {m['stages'][st - 1]['name']}"))
        for name in roster.STATE_NAMES:
            rows.append(("state", (st, name)))
        rows.append(("mirror", st))
        rows.append(("small", st))
    heights = {"title": 34, "state": CELL + PAD, "mirror": CELL + PAD, "small": 96 + PAD}
    height = sum(heights[k] for k, _ in rows) + PAD
    sheet = Image.new("RGBA", (width, height), (128, 128, 134, 255))
    d = ImageDraw.Draw(sheet)
    f_title, f_label = _font(20), _font(13)
    y = PAD
    sdir = pets_dir / sid
    for kind, payload in rows:
        if kind == "title":
            d.text((PAD, y + 6), payload, fill=(255, 255, 255, 255), font=f_title)
        else:
            for side, (bg, ink) in enumerate(((LIGHT, INK_L), (DARK, INK_D))):
                x0 = PAD + side * (half + PAD)
                d.rectangle([x0, y, x0 + half - 1, y + heights[kind] - PAD - 1], fill=bg)
                if kind == "state":
                    st, name = payload
                    meta = m["states"][name]
                    d.text((x0 + 6, y + 8), name, fill=ink, font=f_label)
                    d.text((x0 + 6, y + 26), f"{meta['frameMs']} ms {meta['mode']}", fill=ink, font=f_label)
                    d.text((x0 + 6, y + 44), " ".join(f.split('.')[0] for f in meta["frames"]), fill=ink, font=f_label)
                    for i, fr in enumerate(meta["frames"]):
                        sheet.alpha_composite(_frame(sdir / f"s{st}" / fr, CELL, bg),
                                              (x0 + LABEL_W + i * (CELL + PAD), y))
                elif kind == "mirror":
                    st = payload
                    meta = m["states"]["locomotion"]
                    d.text((x0 + 6, y + 8), "locomotion", fill=ink, font=f_label)
                    d.text((x0 + 6, y + 26), "mirrored (walk left)" if meta["mirror"] else "no mirror (aquatic)",
                           fill=ink, font=f_label)
                    for i, fr in enumerate(meta["frames"]):
                        sheet.alpha_composite(_frame(sdir / f"s{st}" / fr, CELL, bg, mirror=meta["mirror"]),
                                              (x0 + LABEL_W + i * (CELL + PAD), y))
                else:
                    st = payload
                    d.text((x0 + 6, y + 8), "96 px / 64 px", fill=ink, font=f_label)
                    x = x0 + LABEL_W
                    for fr in ("b00", "b03", "b08", "c11"):
                        for size in (96, 64):
                            sheet.alpha_composite(_frame(sdir / f"s{st}" / f"{fr}.webp", size, bg, guides=False),
                                                  (x, y + 96 - size))
                            x += size + PAD
        y += heights[kind]
    sheet.convert("RGB").save(out_path, optimize=True)
    return out_path


def roster_sheet(pets_dir, out_path, size=128):
    label_h = 40
    cols = len(roster.ORDER)
    width = cols * (size + PAD) + PAD
    height = 2 * (3 * size + label_h) + 3 * PAD
    sheet = Image.new("RGBA", (width, height), (128, 128, 134, 255))
    d = ImageDraw.Draw(sheet)
    f = _font(13)
    for side, (bg, ink) in enumerate(((LIGHT, INK_L), (DARK, INK_D))):
        y0 = PAD + side * (3 * size + label_h + PAD)
        for c, sid in enumerate(roster.ORDER):
            x = PAD + c * (size + PAD)
            m = json.loads((pets_dir / sid / "manifest.json").read_text())
            d.rectangle([x, y0, x + size - 1, y0 + 3 * size + label_h - 1], fill=bg)
            d.text((x + 4, y0 + 4), sid, fill=ink, font=f)
            d.text((x + 4, y0 + 20), "placeholder" if m.get("placeholder") else m["displayName"], fill=ink, font=f)
            for st in (1, 2, 3):
                p = pets_dir / sid / f"s{st}" / "b00.webp"
                if m.get("placeholder"):
                    p = pets_dir / "_placeholder.webp"
                sheet.alpha_composite(_frame(p, size, bg, guides=False), (x, y0 + label_h + (st - 1) * size))
    out_path = Path(out_path)
    if out_path.suffix == ".webp":
        sheet.convert("RGB").save(out_path, "WEBP", quality=82, method=6)
    else:
        sheet.convert("RGB").save(out_path, optimize=True)
    return out_path


def gallery(pets_dir, out_path):
    """The committed v0.2 preview gallery (docs/screenshots): b00 of every species and
    stage at 96 px on light and dark, as a small lossy WebP."""
    return roster_sheet(pets_dir, out_path, size=96)


PREVIEW_SIZES = (64, 96, 112)          # display sizes in logical px (spec §17.4 row 5)
PREVIEW_SCALES = (1.0, 1.25, 1.5, 2.0)  # output scales


def _scaled(path, px, bg, mirror):
    """One frame resampled straight from the 192 px source to `px` device pixels, as a
    scaled overlay would draw it (no intermediate logical-size step)."""
    tile = Image.new("RGBA", (px, px), bg)
    im = Image.open(path).convert("RGBA")
    if mirror:
        im = im.transpose(Image.FLIP_LEFT_RIGHT)
    tile.alpha_composite(im.resize((px, px), Image.LANCZOS))
    return tile


def preview_sheet(pets_dir, sid, out_path):
    """Fractional-scale and direction preview: for every stage and display size (64/96/112
    px), the idle pose and the first locomotion frame at 1x/1.25x/1.5x/2x, facing right
    and (floor species) mirrored to face left, on the dark background."""
    m = json.loads((pets_dir / sid / "manifest.json").read_text())
    if m.get("placeholder"):
        return None
    mirror_ok = m["states"]["locomotion"]["mirror"]
    loco0 = m["states"]["locomotion"]["frames"][0]
    poses = (("b00.webp", "idle"), (loco0, "walk"))
    col_w = [int(round(max(PREVIEW_SIZES) * sc)) for sc in PREVIEW_SCALES]
    cells = [(sc, w, d, f, label) for sc, w in zip(PREVIEW_SCALES, col_w)
             for d in ("right", "left") for f, label in poses]
    width = LABEL_W + sum(w + PAD for _, w, _, _, _ in cells) + PAD
    row_h = [int(round(sz * max(PREVIEW_SCALES))) + 30 for sz in PREVIEW_SIZES]
    height = 40 + 3 * (34 + sum(h + PAD for h in row_h)) + PAD
    sheet = Image.new("RGBA", (width, height), DARK)
    d = ImageDraw.Draw(sheet)
    f_title, f_label = _font(20), _font(13)
    d.text((PAD, 10), f"{m['displayName']} ({sid}): scale and direction preview "
                      f"({'mirrors when walking left' if mirror_ok else 'aquatic: never mirrors'})",
           fill=INK_D, font=f_title)
    y = 40
    sdir = pets_dir / sid
    for st in (1, 2, 3):
        d.text((PAD, y + 8), f"stage {st}: {m['stages'][st - 1]['name']}", fill=INK_D, font=f_title)
        y += 34
        for sz, h in zip(PREVIEW_SIZES, row_h):
            d.text((PAD, y + 8), f"{sz} px", fill=INK_D, font=f_label)
            x = LABEL_W
            for sc, w, direction, frame, label in cells:
                px = int(round(sz * sc))
                d.text((x, y + 2), f"{sc:g}x {direction} {label}", fill=INK_D, font=f_label)
                mirror = direction == "left" and mirror_ok
                sheet.alpha_composite(_scaled(sdir / f"s{st}" / frame, px, DARK, mirror), (x, y + 20))
                x += w + PAD
            y += h + PAD
    sheet.convert("RGB").save(out_path, "WEBP", quality=85, method=6)
    return out_path


def playback(pets_dir, sid, stage, out_path, size=112):
    """A short animated WebP for one stage: every state's frame sequence in order, each
    frame held for the state's frameMs, facing right and (floor species) mirrored left
    side by side, with the state name above. Plays twice for loops, once otherwise."""
    m = json.loads((pets_dir / sid / "manifest.json").read_text())
    if m.get("placeholder"):
        return None
    sdir = pets_dir / sid / f"s{stage}"
    f_label = _font(13)
    frames, durations = [], []
    for name in roster.STATE_NAMES:
        meta = m["states"][name]
        repeats = 2 if meta["mode"] in ("loop", "while_moving") else 1
        for _ in range(repeats):
            for fr in meta["frames"]:
                canvas = Image.new("RGBA", (2 * size + 3 * PAD, size + 24), DARK)
                d = ImageDraw.Draw(canvas)
                d.text((PAD, 4), f"{sid} s{stage} {name} ({meta['frameMs']} ms {meta['mode']})",
                       fill=INK_D, font=f_label)
                for side in (0, 1):
                    mirror = side == 1 and meta["mirror"]
                    canvas.alpha_composite(_scaled(sdir / fr, size, DARK, mirror),
                                           (PAD + side * (size + PAD), 22))
                frames.append(canvas.convert("RGB"))
                durations.append(meta["frameMs"])
    frames[0].save(out_path, "WEBP", save_all=True, append_images=frames[1:], duration=durations,
                   loop=0, quality=80, method=4)
    return out_path


def render_previews(pets_dir, out_dir, species=None):
    out_dir = Path(out_dir)
    paths = []
    for sid in species or roster.ORDER:
        p = preview_sheet(pets_dir, sid, out_dir / f"{sid}-scales.webp")
        if p is None:
            continue
        paths.append(p)
        for st in (1, 2, 3):
            paths.append(playback(pets_dir, sid, st, out_dir / f"{sid}-s{st}-playback.webp"))
    return paths


def render_all(pets_dir, out_dir, species=None):
    out_dir = Path(out_dir)
    paths = []
    for sid in species or roster.ORDER:
        p = species_sheet(pets_dir, sid, out_dir / f"{sid}.png")
        if p:
            paths.append(p)
    paths.append(roster_sheet(pets_dir, out_dir / "roster.png"))
    return paths
