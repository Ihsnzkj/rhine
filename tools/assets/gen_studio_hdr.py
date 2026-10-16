#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Procedural "white cyclorama" studio probe for the fin sculpture.

The film lights its sculptures like product renders: a huge overhead softbox,
a warm paper floor bounce and a cool strip from camera-left.  Written as a
Radiance RGBE equirectangular image that Qt Quick 3D can use as lightProbe.
"""
import pathlib
import numpy as np

W, H = 1024, 512
phi = (np.arange(W) + 0.5) / W * 2 * np.pi - np.pi          # azimuth, 0 = -Z
th = (np.arange(H) + 0.5) / H * np.pi                         # polar, 0 = up
PHI, TH = np.meshgrid(phi, th)
d = np.stack([np.sin(TH) * np.sin(PHI), np.cos(TH), -np.sin(TH) * np.cos(PHI)], -1)

paper = np.array([0.92, 0.90, 0.87])
img = np.empty((H, W, 3))
up = d[..., 1]
img[:] = paper * (0.55 + 0.25 * np.clip(up, -1, 1))[..., None]  # cyclorama gradient


def softbox(center, size, power, colour, sharp=8.0):
    c = np.array(center, float); c /= np.linalg.norm(c)
    cosang = np.clip((d * c).sum(-1), -1, 1)
    ang = np.arccos(cosang)
    m = 1 / (1 + np.exp((ang - size) * sharp * 10))
    img[:] += m[..., None] * power * np.array(colour)


softbox((0.0, 1.0, 0.15), 0.55, 5.5, (1.0, 0.985, 0.96))       # overhead
softbox((-0.9, 0.35, 0.35), 0.22, 3.2, (0.93, 0.96, 1.0))      # cool strip, camera-left
softbox((0.85, 0.25, -0.6), 0.18, 2.4, (1.0, 0.93, 0.84))      # warm kicker, back-right
softbox((0.0, -1.0, 0.0), 0.9, 0.35, (1.0, 0.92, 0.82), 3.0)   # floor bounce

def write_hdr(path, rgb):
    rgb = np.maximum(rgb, 0).astype(np.float32)
    m = rgb.max(-1)
    e = np.where(m > 1e-32, np.floor(np.log2(np.maximum(m, 1e-32))) + 1, 0)
    scale = np.where(m > 1e-32, 256.0 / np.exp2(e), 0)
    rgbe = np.zeros(rgb.shape[:2] + (4,), np.uint8)
    rgbe[..., :3] = np.clip(rgb * scale[..., None], 0, 255).astype(np.uint8)
    rgbe[..., 3] = np.where(m > 1e-32, e + 128, 0).astype(np.uint8)
    with open(path, "wb") as fh:
        fh.write(b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n")
        fh.write(f"-Y {rgb.shape[0]} +X {rgb.shape[1]}\n".encode())
        fh.write(rgbe.tobytes())

out = pathlib.Path(__file__).resolve().parents[2] / "frontend" / "public" / "scene" / "studio.hdr"
write_hdr(out, img)
print(out, out.stat().st_size)
