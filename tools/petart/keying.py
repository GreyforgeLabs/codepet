"""Chroma keying and cell segmentation for CodePet source sheets (Amendment A3).

Pure functions over numpy arrays; deterministic for identical inputs.

Keying model: the sheets are painted on a flat key colour (#00FF00 or #FF00FF). For each
pixel a *key strength* k is measured (green: G - max(R, B); magenta: min(R, B) - G). The
sheet's own background strength is the median k of its outer border, so a slightly off
key (for example (3, 249, 22)) is handled. Alpha falls on a soft linear ramp between
`lo` (fully opaque) and `hi` (fully transparent, a little below the background). Edge
colour is recovered by un-mixing the background, `fg = (c - (1 - a) * bg) / a`, then a
despill clamp removes residual key tint on the fringe band only, so interior colours
(pink ears, purple props, green frogs on magenta) are never touched. Finally a soft
1-px fringe erode caps every pixel that touches transparency, which removes the
one-pixel key halo without eating thin details.
"""

import numpy as np
from scipy import ndimage

GREEN = (0, 255, 0)
MAGENTA = (255, 0, 255)

# Tuning (documented in docs/pets.md). Values are in 0..255 key-strength units.
KEY_LO = 14            # k at or below this is fully opaque subject
FRINGE_PX = 5          # pixels farther than this from clear background are always opaque
KEY_HI_BELOW_BG = 14   # alpha reaches 0 this far below the measured background strength
ERODE_CAP = 0.45       # a pixel touching transparency keeps at most this much alpha above its weakest neighbour
DESPILL_SLACK = 4.0    # allowed key-channel excess on the fringe band
SPECK_PX = 40          # connected components smaller than this (source px) are key noise
COMPONENT_ALPHA = 0.06
CLEAR_SHARE = 0.7      # k at or above this share of the background strength is background
RESIDUAL_MAX = 0.45    # max distance from the background-to-reference colour line (relative)
SPILL_SAT = 0.35       # unexplained fringe pixels more saturated than this take a tighter despill cap
SPILL_RATIO_SLACK = 0.12  # ... the reference paint's key ratio plus this, instead of neutral 1.0
SPILL_ALPHA = 0.95     # only partly transparent fringe pixels take the tighter cap
POCKET_AREA = 200      # pocket_clear: enclosed key pockets up to this many source px become clear
POCKET_CLEAR_SHARE = 0.35  # ... when every pixel is at least this share of the background strength
GAMUT_BINS = 20        # spill_gamut: bins of min(R,B)/max(R,B) per dominant channel
GAMUT_PCTL = 99.0      # ... percentile of the deep paint's G/max(R,B) taken as the bin's limit
GAMUT_MIN_PX = 400     # ... bins with fewer deep paint pixels stay neutral (no limit)


def parse_hex(h):
    h = h.lstrip("#")
    rgb = tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
    if rgb == GREEN:
        return "green"
    if rgb == MAGENTA:
        return "magenta"
    raise ValueError(f"unsupported chroma key {h!r}; expected #00FF00 or #FF00FF")


def key_strength(rgb, kind):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    if kind == "green":
        return g - np.maximum(r, b)
    return np.minimum(r, b) - g


def _border(a, width=6):
    return np.concatenate([a[:width].reshape(-1, *a.shape[2:]), a[-width:].reshape(-1, *a.shape[2:]),
                           a[:, :width].reshape(-1, *a.shape[2:]), a[:, -width:].reshape(-1, *a.shape[2:])])


def gamut_bin(r, b):
    """Hue-family bin for the green-key gamut: min(R,B)/max(R,B), split by dominant channel."""
    q = np.minimum(r, b) / np.maximum(np.maximum(r, b), 1.0)
    return np.minimum((q * GAMUT_BINS).astype(np.int64), GAMUT_BINS - 1) + np.where(b > r, GAMUT_BINS, 0)


def spill_gamut_lut(rgb, deep):
    """Green-key gamut limit learned from the sheet's own deep paint.

    For each hue family (`gamut_bin`) the GAMUT_PCTL percentile of G/max(R,B) over deep,
    key-free paint is the most green that family is ever painted. Returns the per-bin
    ratio limits (1.0 where a bin has too few samples), so fringe golden fur blended with
    the key (yellow-olive) is pulled back to golden even when its nearest reference pixel
    is lighter cream fur.
    """
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    bins = gamut_bin(r, b)[deep]
    ratio = (g / np.maximum(np.maximum(r, b), 1.0))[deep]
    lut = np.ones(2 * GAMUT_BINS, dtype=np.float32)
    for i in range(2 * GAMUT_BINS):
        sel = ratio[bins == i]
        if sel.size >= GAMUT_MIN_PX:
            lut[i] = max(float(np.percentile(sel, GAMUT_PCTL)), 0.05)
    return lut


def key_sheet(rgb_u8, kind, clear_share=None, pocket_share=None, pocket_clear=False, despill="band",
              spill_gamut=False):
    """rgb_u8: HxWx3 uint8. Returns (rgb float32 HxWx3 in 0..255, alpha float32 HxW, info).

    Per-sheet options (from `overrides.json` `sheets` entries; defaults reproduce the
    global tuning):

    - clear_share: share of the background strength at or above which a pixel is clear
      background (default CLEAR_SHARE). Lower it for sheets whose subject has no paint
      in the key colour, so dark key pockets between legs or hay strands clear.
    - pocket_share: also treat key-tinted pixels with k >= pocket_share * background as
      fringe (eligible for partial alpha and despill) even when they lie deeper than
      FRINGE_PX, which recovers soft glows painted over the key. Never use it on a sheet
      whose subject is intentionally painted in a key-like colour.
    - pocket_clear: small key pockets (<= POCKET_AREA source px, every pixel at least
      POCKET_CLEAR_SHARE of the background strength) count as clear background.
    - despill: "band" (default) despills only the fringe band; "full" despills every
      partly transparent or key-tinted pixel, for subjects with no key-coloured paint.
    - spill_gamut (green key only): also cap fringe G at the sheet's own deep-paint gamut
      for the pixel's hue family (`spill_gamut_lut`). For subjects whose edge colours all
      occur in their interior (golden fur); sparkles drawn only at edge size may dull.
    """
    clear_share = CLEAR_SHARE if clear_share is None else float(clear_share)
    rgb = rgb_u8.astype(np.float32)
    k = key_strength(rgb, kind)
    bg_k = float(np.median(_border(k)))
    bg_rgb = np.median(_border(rgb), axis=0).astype(np.float32)
    if bg_k < 150:
        raise ValueError(f"sheet border is not a flat {kind} key (median key strength {bg_k:.0f})")
    hi = bg_k - KEY_HI_BELOW_BG
    lo = KEY_LO
    # Clear background: anything still strongly keyed, including darker key pockets
    # trapped between fur strands.
    clear = k >= clear_share * bg_k
    if pocket_clear:
        weak = (k >= POCKET_CLEAR_SHARE * bg_k) & ~clear
        lab, n = ndimage.label(weak)
        if n:
            area = ndimage.sum_labels(np.ones_like(k), lab, np.arange(1, n + 1))
            edge = np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))
            small = np.zeros(n + 1, dtype=bool)
            small[1:] = area <= POCKET_AREA
            small[edge] = False
            clear = clear | small[lab]
    dist_clear = ndimage.distance_transform_edt(~clear)
    paint = k <= lo
    # Reference paint: key-free pixels deeper than the fringe zone. Each pixel's
    # nearest reference supplies its expected foreground colour and key strength, so an
    # orange fur edge (k about -80) blended with green is not mistaken for opaque yellow.
    deep = (k <= 0) & (dist_clear > FRINGE_PX)
    if deep.sum() < 64:
        deep = paint & (dist_clear > 2)
    if not deep.any():
        deep = paint
    if not deep.any():
        raise ValueError("sheet has no opaque subject pixels")
    _, (iy, ix) = ndimage.distance_transform_edt(~deep, return_indices=True)
    ref = rgb[iy, ix]
    # Alpha by projecting each pixel onto the line from the background colour to its
    # reference paint colour: c = a * F + (1 - a) * B  =>  a = (c - B).(F - B) / |F - B|^2.
    fb = ref - bg_rgb
    fb2 = np.maximum((fb * fb).sum(-1), 1.0)
    cb = rgb - bg_rgb
    alpha_line = np.clip((cb * fb).sum(-1) / fb2, 0.0, 1.0)
    # When the reference colour does not explain the pixel (a white whisker next to orange
    # fur, a pink ear rim), fall back to the plain key-strength ramp and a neutral despill.
    resid = np.sqrt(((cb - alpha_line[..., None] * fb) ** 2).sum(-1) / fb2)
    explained = resid <= RESIDUAL_MAX
    alpha_key = np.clip((hi - k) / max(hi, 1.0), 0.0, 1.0)
    alpha = np.where(explained, alpha_line, alpha_key).astype(np.float32)
    alpha[clear] = 0.0
    alpha[paint & (alpha > 0.85)] = 1.0
    # Key-tinted pixels deep inside the subject (a purple prop on magenta, a green LED on
    # green) are paint, not spill: only pixels near clear background may become transparent.
    zone = dist_clear <= FRINGE_PX
    if pocket_share is not None:
        zone = zone | (k >= float(pocket_share) * bg_k)
    alpha[~zone] = 1.0

    # Un-mix the background from partially transparent pixels; very faint pixels take
    # the reference colour instead (their un-mixed colour is mostly noise).
    a = alpha[..., None]
    safe = np.maximum(a, 0.2)
    fg = np.clip((rgb - (1.0 - a) * bg_rgb) / safe, 0.0, 255.0)
    w = np.clip((a - 0.15) / 0.35, 0.0, 1.0)
    fg = np.where(a > 0.999, rgb, w * fg + (1.0 - w) * ref)

    # Despill in the fringe zone: the key channel may not exceed the reference paint's
    # own ratio to the other channels (plus a little slack). Un-mixing can also remove too
    # much of the key channel from soft grey or white fringe whose alpha was
    # underestimated, which tints it toward the complementary colour (pink on a green
    # key, green on a magenta key); a floor at the reference paint's own ratio, from
    # below, undoes that without touching paint whose reference is itself key-tinted.
    band = zone & (alpha > 0)
    if despill == "full":
        band = (alpha > 0) & ((alpha < 1.0) | (k > 0))
    elif despill != "band":
        raise ValueError(f"despill must be 'band' or 'full', not {despill!r}")
    r, g, b = fg[..., 0], fg[..., 1], fg[..., 2]
    rr, rg, rb = ref[..., 0], ref[..., 1], ref[..., 2]
    if kind == "green":
        # Saturated, partly transparent fringe the reference does not explain (yellow-olive
        # spill on golden fur) gets a cap near the reference ratio instead of neutral 1.0;
        # low-saturation pixels (white whiskers) keep the neutral cap.
        mx = np.maximum(np.maximum(r, g), b)
        sat = (mx - np.minimum(np.minimum(r, g), b)) / np.maximum(mx, 1.0)
        tight = ~explained & (alpha < SPILL_ALPHA) & (sat > SPILL_SAT)
        ref_ratio = rg / np.maximum(np.maximum(rr, rb), 1.0)
        ratio = np.where(explained, np.minimum(ref_ratio, 1.0),
                         np.where(tight, np.minimum(ref_ratio + SPILL_RATIO_SLACK, 1.0), 1.0))
        cap = np.maximum(r, b) * ratio + np.where(tight, 0.0, DESPILL_SLACK)
        lo_ratio = np.minimum(rg / np.maximum(np.minimum(rr, rb), 1.0), 1.0)
        floor = np.minimum(r, b) * lo_ratio - DESPILL_SLACK
        if spill_gamut:
            lut = spill_gamut_lut(rgb, deep)
            cap = np.minimum(cap, np.maximum(r, b) * lut[gamut_bin(r, b)] + DESPILL_SLACK)
        g = np.where(band, np.clip(g, np.minimum(floor, cap), cap), g)
        fg = np.stack([r, g, b], axis=-1)
    else:
        allowed = np.where(explained, np.maximum(np.minimum(rr, rb) - rg, 0.0), 0.0) + DESPILL_SLACK
        excess = np.minimum(r, b) - g - allowed
        excess = np.where(band & (excess > 0), excess, 0.0)
        r = r - excess
        b = b - excess
        lo_ratio = np.minimum(np.minimum(rr, rb) / np.maximum(rg, 1.0), 1.0)
        floor = g * lo_ratio - DESPILL_SLACK
        r = np.where(band, np.maximum(r, floor), r)
        b = np.where(band, np.maximum(b, floor), b)
        fg = np.stack([r, g, b], axis=-1)

    # Soft 1-px fringe erode.
    weakest = ndimage.minimum_filter(alpha, size=3, mode="constant", cval=0.0)
    alpha = np.minimum(alpha, weakest + ERODE_CAP).astype(np.float32)
    fg = np.where(alpha[..., None] > 0, fg, 0.0).astype(np.float32)
    return fg, alpha, {"bg_strength": round(bg_k, 1), "bg_rgb": [round(float(x), 1) for x in bg_rgb]}


REASSIGN_RADIUS = 12   # px around a `reassign` point searched for its component


def segment(alpha, cols=4, rows=3, reassign=()):
    """Split a keyed sheet into cells by connected-component centroid assignment.

    Returns (masks, warnings): masks[i] is a float32 HxW weight (0/1) for cell i (row-major).
    Every component with at least SPECK_PX pixels goes to the grid cell containing its
    alpha-weighted centroid, so a prop that leans over a gutter stays with its pose.
    Soft fringe pixels below the labelling threshold follow the nearest assigned
    component within 3 px. `reassign` entries {"near": [x, y], "to": cell} move the
    component nearest that sheet point (within REASSIGN_RADIUS) to another cell, for a
    detached piece whose centroid lands across a gutter (a confetti chip).
    """
    h, w = alpha.shape
    cw, ch = w / cols, h / rows
    solid = alpha > COMPONENT_ALPHA
    labels, n = ndimage.label(solid, structure=np.ones((3, 3), dtype=bool))
    warnings = []
    if n == 0:
        return [np.zeros_like(alpha) for _ in range(cols * rows)], ["sheet has no subject pixels"]
    idx = np.arange(1, n + 1)
    areas = ndimage.sum_labels(np.ones_like(alpha), labels, idx)
    cy, cx = np.array(ndimage.center_of_mass(alpha, labels, idx)).T
    objs = ndimage.find_objects(labels)
    cell_of = np.full(n + 1, -1, dtype=np.int64)
    big = 0.002 * h * w
    for j in range(n):
        if areas[j] < SPECK_PX:
            continue
        col = min(cols - 1, max(0, int(cx[j] // cw)))
        row = min(rows - 1, max(0, int(cy[j] // ch)))
        cell_of[j + 1] = row * cols + col
        if areas[j] >= big:
            sl = objs[j]
            x0, x1 = sl[1].start, sl[1].stop
            y0, y1 = sl[0].start, sl[0].stop
            over = max(col * cw - x0, x1 - (col + 1) * cw, row * ch - y0, y1 - (row + 1) * ch)
            if over > 6:
                warnings.append(f"component of {int(areas[j])} px at cell {row * cols + col} "
                                f"crosses its grid cell by {over:.0f} px (kept whole by centroid)")
    for entry in reassign:
        px, py = (int(v) for v in entry["near"])
        to = int(entry["to"])
        if not 0 <= to < cols * rows:
            raise ValueError(f"reassign target cell {to} out of range")
        y0, y1 = max(0, py - REASSIGN_RADIUS), min(h, py + REASSIGN_RADIUS + 1)
        x0, x1 = max(0, px - REASSIGN_RADIUS), min(w, px + REASSIGN_RADIUS + 1)
        win = labels[y0:y1, x0:x1]
        ys, xs = np.nonzero(win)
        if len(xs) == 0:
            raise ValueError(f"reassign: no component within {REASSIGN_RADIUS} px of {entry['near']}")
        j = int(np.argmin((ys + y0 - py) ** 2 + (xs + x0 - px) ** 2))
        lab = int(win[ys[j], xs[j]])
        warnings.append(f"reassign: component of {int(areas[lab - 1])} px near {entry['near']} "
                        f"from cell {int(cell_of[lab])} to cell {to}")
        cell_of[lab] = to
    cell_map = cell_of[labels]                       # -1 for background/specks
    # Soft fringe pixels (0 < alpha <= threshold) join the nearest labelled pixel's cell.
    _, (iy, ix) = ndimage.distance_transform_edt(cell_map < 0, return_indices=True)
    dist = ndimage.distance_transform_edt(cell_map < 0)
    near = cell_map[iy, ix]
    fringe = (cell_map < 0) & (alpha > 0) & (dist <= 3) & ~solid
    owner = np.where(fringe, near, cell_map)
    masks = [(owner == i).astype(np.float32) for i in range(cols * rows)]
    for i, m in enumerate(masks):
        if not m.any():
            warnings.append(f"cell {i} is empty")
    return masks, warnings
