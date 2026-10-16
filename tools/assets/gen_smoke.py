#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Dark smoke plate for the section tiles (film 1:04): domain-warped fBm,
rendered once offline at 2x so CSS can drift it slowly instead of running a
shader per tile."""
import pathlib
import numpy as np
from PIL import Image

W, H = 1200, 840
rng = np.random.default_rng(7)
grid = rng.random((520, 520))


def noise(x, y):
    xi, yi = np.floor(x).astype(int), np.floor(y).astype(int)
    xf, yf = x - xi, y - yi
    u, v = xf * xf * (3 - 2 * xf), yf * yf * (3 - 2 * yf)
    g = lambda i, j: grid[(yi + j) % 512, (xi + i) % 512]
    return (g(0, 0) * (1 - u) + g(1, 0) * u) * (1 - v) + (g(0, 1) * (1 - u) + g(1, 1) * u) * v


def fbm(x, y, oct=6):
    v, a = 0.0, 0.5
    for _ in range(oct):
        v = v + a * noise(x, y)
        x, y, a = x * 2.07 + 5.3, y * 2.07 + 1.7, a * 0.5
    return v


ys, xs = np.mgrid[0:H, 0:W] / H * 3.2
wx, wy = fbm(xs, ys), fbm(xs + 4.1, ys + 2.3)
s = fbm(xs + wx * 2.4, ys + wy * 2.4)
folds = np.clip((s - 0.35) / 0.6, 0, 1) ** 1.5 * 0.17 + np.clip((fbm(xs * 1.3 + wx, ys * 1.3) - 0.55) / 0.07, 0, 1) * 0.04
v = 0.045 + folds
cy = np.mgrid[0:H, 0:W][0] / H - 0.5
cx = np.mgrid[0:H, 0:W][1] / W - 0.5
v *= 1 - (cx * cx + cy * cy) * 0.9
v += (rng.random((H, W)) - 0.5) * 0.035
rgb = np.stack([v * 0.97, v, v * 1.02], -1)
out = pathlib.Path(__file__).resolve().parents[2] / "frontend" / "public" / "scene" / "smoke.jpg"
Image.fromarray((np.clip(rgb, 0, 1) * 255).astype(np.uint8)).save(out, quality=88)
print(out)
