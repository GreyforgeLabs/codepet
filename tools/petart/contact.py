"""QA contact sheets for extracted pets (not shipped).

Per species `<id>.png`: for each stage, every state's frame sequence on a light and a
dark background (ground line and centre marked faintly), a mirrored locomotion strip, and
a 96 px / 64 px row. Plus `roster.png`: b00 of every species and stage on light and dark.
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


def roster_sheet(pets_dir, out_path):
    size = 128
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
    sheet.convert("RGB").save(out_path, optimize=True)
    return out_path


def render_all(pets_dir, out_dir, species=None):
    out_dir = Path(out_dir)
    paths = []
    for sid in species or roster.ORDER:
        p = species_sheet(pets_dir, sid, out_dir / f"{sid}.png")
        if p:
            paths.append(p)
    paths.append(roster_sheet(pets_dir, out_dir / "roster.png"))
    return paths
