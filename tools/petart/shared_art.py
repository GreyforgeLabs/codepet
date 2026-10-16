"""Shared non-species images, drawn deterministically with Pillow (Amendment A4).

`_placeholder.webp`: a neutral grey companion silhouette shown when a pack or frame
cannot load. `_memorial.webp`: a small memorial stone with a flower, shown when there is
no living pet. Both are 192x192 with the same ground line as the species frames.
"""

from PIL import Image, ImageDraw, ImageFilter

from roster import CANVAS, GROUND_Y

SS = 4  # supersampling factor

LICENSE_TEXT = """MIT License

Copyright (c) 2026 Greyforge Labs

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Scope: every file in this directory tree (qml/assets/pets/**). The species frames are
raster extractions of AI-generated source sheets (OpenAI GPT Image 2); see PROVENANCE.md.
"""


def _canvas():
    return Image.new("RGBA", (CANVAS * SS, CANVAS * SS), (0, 0, 0, 0))


def _s(*v):
    return [x * SS for x in v]


def _finish(im):
    out = im.resize((CANVAS, CANVAS), Image.LANCZOS)
    px = out.load()
    for y in range(CANVAS):
        for x in range(CANVAS):
            if px[x, y][3] == 0:
                px[x, y] = (0, 0, 0, 0)
    return out


def placeholder():
    """Neutral sitting-companion silhouette with a soft dashed outline."""
    im = _canvas()
    d = ImageDraw.Draw(im)
    fill = (150, 158, 170, 235)
    ink = (96, 104, 116, 255)
    g = GROUND_Y
    d.ellipse(_s(56, g - 92, 136, g), fill=fill, outline=ink, width=3 * SS)           # body
    d.ellipse(_s(62, g - 150, 130, g - 82), fill=fill, outline=ink, width=3 * SS)     # head
    d.polygon(_s(68, g - 128, 74, g - 166, 94, g - 140), fill=fill, outline=ink)     # ears
    d.polygon(_s(124, g - 128, 118, g - 166, 98, g - 140), fill=fill, outline=ink)
    d.ellipse(_s(80, g - 124, 90, g - 112), fill=(236, 240, 245, 255))                # eyes
    d.ellipse(_s(102, g - 124, 112, g - 112), fill=(236, 240, 245, 255))
    d.arc(_s(86, g - 110, 106, g - 98), 20, 160, fill=ink, width=2 * SS)             # mouth
    return _finish(im)


def memorial():
    """A rounded memorial stone on a grass tuft with one small flower."""
    im = _canvas()
    d = ImageDraw.Draw(im)
    g = GROUND_Y
    stone = (168, 172, 180, 255)
    stone_dark = (118, 122, 132, 255)
    d.rounded_rectangle(_s(58, g - 118, 134, g - 2), radius=36 * SS, fill=stone,
                        outline=stone_dark, width=3 * SS)
    d.rectangle(_s(60, g - 40, 132, g - 4), fill=stone)
    d.line(_s(58, g - 3, 134, g - 3), fill=stone_dark, width=3 * SS)
    d.line(_s(60, g - 40, 60, g - 4), fill=stone_dark, width=3 * SS)
    d.line(_s(132, g - 40, 132, g - 4), fill=stone_dark, width=3 * SS)
    # engraved heart
    d.ellipse(_s(84, g - 92, 98, g - 78), fill=stone_dark)
    d.ellipse(_s(94, g - 92, 108, g - 78), fill=stone_dark)
    d.polygon(_s(85, g - 84, 107, g - 84, 96, g - 66), fill=stone_dark)
    # grass tuft
    for x0 in range(46, 150, 8):
        d.polygon(_s(x0, g, x0 + 6, g - 12 - (x0 * 7) % 9, x0 + 10, g), fill=(104, 160, 88, 255))
    # flower
    d.line(_s(142, g, 140, g - 34), fill=(84, 140, 70, 255), width=3 * SS)
    d.ellipse(_s(140, g - 20, 152, g - 14), fill=(104, 160, 88, 255))
    for cx, cy in ((140, g - 46), (149, g - 40), (146, g - 30), (134, g - 30), (131, g - 40)):
        d.ellipse(_s(cx - 6, cy - 6, cx + 6, cy + 6), fill=(240, 160, 184, 255))
    d.ellipse(_s(135, g - 43, 145, g - 33), fill=(250, 214, 96, 255))
    im = im.filter(ImageFilter.SMOOTH)
    return _finish(im)
