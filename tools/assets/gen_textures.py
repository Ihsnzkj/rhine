#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Seamless pattern tiles used by the glass panels and personnel photos."""
import math, pathlib
from PIL import Image, ImageDraw

OUT = pathlib.Path(__file__).resolve().parents[2] / "frontend" / "public" / "scene"
SS = 4  # supersampling


def hex_tile(r=26, line=1.2):
    # flat-topped hexagons; period is (3r, sqrt(3) r) — r=26 keeps it ~integral
    w, h = round(3 * r), round(math.sqrt(3) * r)
    img = Image.new("L", (w * SS * 3, h * SS * 3), 0)
    d = ImageDraw.Draw(img)
    R = r * SS
    for gx in range(-2, 6):
        for gy in range(-2, 6):
            for ox, oy in ((0, 0), (1.5 * R, math.sqrt(3) / 2 * R)):
                cx, cy = gx * 3 * R + ox, gy * math.sqrt(3) * R + oy
                pts = [(cx + R * math.cos(math.pi / 3 * k), cy + R * math.sin(math.pi / 3 * k)) for k in range(7)]
                d.line(pts, fill=255, width=max(1, round(line * SS)))
    img = img.crop((w * SS, h * SS, 2 * w * SS, 2 * h * SS)).resize((w, h), Image.LANCZOS)
    rgba = Image.new("RGBA", (w, h), (255, 255, 255, 0))
    rgba.putalpha(img)
    rgba.save(OUT / "hex.png")


def halftone_tile(pitch=7, radius=1.6):
    n = pitch * SS
    img = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(img)
    rr = radius * SS
    for cx, cy in ((0, 0), (n, 0), (0, n), (n, n), (n / 2, n / 2)):
        d.ellipse((cx - rr, cy - rr, cx + rr, cy + rr), fill=255)
    img = img.resize((pitch, pitch), Image.LANCZOS)
    rgba = Image.new("RGBA", (pitch, pitch), (255, 255, 255, 0))
    rgba.putalpha(img)
    rgba.save(OUT / "halftone.png")


hex_tile()
halftone_tile()
print("ok")


def acrylic_card(logo_png):
    """Face of the frosted ∞ cards that stream through the film's left column."""
    from PIL import ImageFilter
    W, H, R = 512, 368, 26
    img = Image.new("RGBA", (W * SS, H * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, W * SS - 1, H * SS - 1), R * SS, fill=(244, 243, 239, 255))
    d.rounded_rectangle((6 * SS, 6 * SS, W * SS - 6 * SS, H * SS - 6 * SS), (R - 6) * SS,
                        outline=(214, 211, 204, 255), width=2 * SS)
    img = img.resize((W, H), Image.LANCZOS)
    # milky vertical falloff
    shade = Image.linear_gradient("L").resize((W, H)).point(lambda v: 255 - v // 10)
    px = img.load()
    sp = shade.load()
    for y in range(H):
        for x in range(W):
            r, g, b, a = px[x, y]
            k = sp[x, y] / 255
            px[x, y] = (int(r * k), int(g * k), int(b * k), a)
    d = ImageDraw.Draw(img)
    logo = Image.open(logo_png).convert("RGBA")
    tint = Image.new("RGBA", logo.size, (196, 192, 184, 255))
    tint.putalpha(logo.getchannel("A"))
    img.alpha_composite(tint, (60, H // 2 - logo.size[1] // 2 - 8))
    # small print: barcode block and two caption rules, like the film's cards
    for i, w in enumerate((3, 1, 2, 1, 3, 2, 1, 1, 3, 1, 2)):
        x0 = W - 128 + sum((3, 1, 2, 1, 3, 2, 1, 1, 3, 1, 2)[:i]) * 3 + i * 2
        d.rectangle((x0, 40, x0 + w * 3 - 1, 74), fill=(150, 148, 142, 255))
    d.rectangle((W - 128, 84, W - 52, 87), fill=(170, 168, 162, 255))
    d.rectangle((60, H - 70, 230, H - 66), fill=(186, 183, 176, 255))
    d.rectangle((60, H - 58, 170, H - 55), fill=(206, 203, 197, 255))
    d.ellipse((W - 70, H - 72, W - 46, H - 48), outline=(186, 183, 176, 255), width=3)
    img.save(OUT / "card.png")


import sys
if len(sys.argv) > 1:
    acrylic_card(sys.argv[1])
