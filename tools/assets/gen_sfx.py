#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Synthesise the greeter's signature cues, timed to its animations.

Everything here is generated from scratch (no samples), so it can live in the
repository.  Cues are 48 kHz 16-bit WAV, the format Qt's SoundEffect plays
with the lowest latency.

  draw.wav       1.05 s  pen stroke of the boot logo (BootSequence logoDraw)
  access.wav     0.90 s  decode chatter under ACCESS PERMISSION REQUIRED
  assemble.wav   2.40 s  the ∞ band being drawn in 3D (Shell assembleAnim)
  process.wav    2.143 s one 4/4 bar at 112 BPM — loops seamlessly under the
                         "Rhine Lab.LLC" BGM while a key is being verified
  warning.wav    1.10 s  WARNING lockdown: two-tone alarm over a sub thump
  reveal.wav     0.80 s  interface arrival: airy glass swell
"""
import pathlib
import wave
import numpy as np

SR = 48000
OUT = pathlib.Path(__file__).resolve().parents[2] / "frontend" / "public" / "audio"
rng = np.random.default_rng(1977)


def t_axis(seconds):
    return np.arange(int(seconds * SR)) / SR


def env_adsr(n, a, d, s, r, sustain=0.6):
    """Piecewise-linear envelope with segment lengths in seconds."""
    a, d, r = int(a * SR), int(d * SR), int(r * SR)
    s = max(0, n - a - d - r)
    e = np.concatenate([np.linspace(0, 1, a, endpoint=False),
                        np.linspace(1, sustain, d, endpoint=False),
                        np.full(s, sustain),
                        np.linspace(sustain, 0, r)])
    return np.pad(e, (0, max(0, n - len(e))))[:n]


def onepole_lp(x, cutoff):
    """One-pole low-pass; `cutoff` may be a per-sample array (Hz)."""
    cutoff = np.broadcast_to(np.asarray(cutoff, float), x.shape)
    a = np.exp(-2 * np.pi * cutoff / SR)
    y = np.empty_like(x)
    acc = 0.0
    for i in range(len(x)):
        acc = (1 - a[i]) * x[i] + a[i] * acc
        y[i] = acc
    return y


def bandpass(x, lo, hi):
    return onepole_lp(x, hi) - onepole_lp(x, lo)


def tick(freq, length=0.012, bright=1.0):
    t = t_axis(length)
    body = np.sin(2 * np.pi * freq * t) * np.exp(-t * 520)
    click = rng.standard_normal(len(t)) * np.exp(-t * 2200) * 0.35 * bright
    return body + click


def place(buf, clip, at, gain=1.0):
    i = int(at * SR)
    j = min(len(buf), i + len(clip))
    if i < len(buf):
        buf[i:j] += clip[: j - i] * gain


def stereo_width(mono, amount=0.0012):
    """Haas-style widening: right channel delayed by a hair."""
    d = int(amount * SR)
    right = np.concatenate([np.zeros(d), mono[:-d]]) if d else mono
    return np.stack([mono, right], 1)


def write(name, x, peak_db=-3.0):
    x = np.asarray(x, float)
    if x.ndim == 1:
        x = np.stack([x, x], 1)
    fade = min(len(x), int(0.004 * SR))
    x[:fade] *= np.linspace(0, 1, fade)[:, None]
    x[-fade:] *= np.linspace(1, 0, fade)[:, None]
    x *= 10 ** (peak_db / 20) / max(1e-9, np.abs(x).max())
    pcm = (np.clip(x, -1, 1) * 32767).astype("<i2")
    OUT.mkdir(parents=True, exist_ok=True)
    with wave.open(str(OUT / name), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    print(f"{name:14s} {len(x) / SR:5.2f}s")


def draw():
    # nib on paper: band-limited noise whose centre glides upward with the
    # stroke, plus a faint sine that traces the pen's speed
    t = t_axis(1.05)
    n = rng.standard_normal(len(t))
    prog = 0.5 - 0.5 * np.cos(np.pi * np.clip(t / 1.05, 0, 1))      # InOutCubic-ish
    centre = 1800 + 5200 * prog
    hiss = bandpass(n, centre * 0.6, centre * 1.4)
    speed = np.gradient(prog) * SR / 2.0
    tone = np.sin(2 * np.pi * np.cumsum(420 + 380 * prog) / SR) * 0.08
    x = (hiss * (0.25 + speed) + tone) * env_adsr(len(t), 0.06, 0.2, 0.7, 0.25, 0.8)
    write("draw.wav", stereo_width(x), -9)


def access():
    buf = np.zeros(int(0.95 * SR))
    for k in range(26):
        place(buf, tick(2600 + rng.uniform(-400, 900), bright=0.6), 0.02 + k * 0.034, rng.uniform(0.35, 0.75))
    write("access.wav", stereo_width(buf), -12)


def assemble():
    # a glass harmonic cluster that blooms while the band is drawn, with a
    # granular stream of card "placements" riding on top
    t = t_axis(2.4)
    prog = np.clip(t / 2.3, 0, 1)
    ease = 0.5 - 0.5 * np.cos(np.pi * prog)
    chord = np.zeros_like(t)
    for f, g in ((293.66, 1.0), (440.0, 0.7), (587.33, 0.55), (880.0, 0.35), (1318.5, 0.18)):
        det = 1 + 0.002 * np.sin(2 * np.pi * 0.7 * t + f)
        chord += g * np.sin(2 * np.pi * f * det * t)
    chord *= ease ** 1.4 * (1 - np.clip((t - 2.0) / 0.4, 0, 1)) * 0.22
    air = bandpass(rng.standard_normal(len(t)), 3000, 9000) * ease * 0.05
    buf = chord + air
    k = 0
    pos = 0.0
    while pos < 2.25:
        place(buf, tick(3200 + 1600 * (pos / 2.3), length=0.01, bright=0.4), pos, 0.12 + 0.18 * (pos / 2.3))
        pos += 0.055 - 0.025 * (pos / 2.3)
        k += 1
    swell = np.sin(2 * np.pi * 146.83 * t) * env_adsr(len(t), 1.6, 0.2, 0.1, 0.5, 0.5) * 0.15
    write("assemble.wav", stereo_width(buf + swell, 0.0018), -6)


def process():
    # exactly one bar at 112 BPM so it loops on the beat of the BGM
    bar = 60 / 112 * 4
    buf = np.zeros(int(round(bar * SR)))
    step = bar / 16
    pattern = [1, 0, 0.4, 0, 0.7, 0, 0.4, 0.3, 1, 0, 0.4, 0, 0.7, 0.3, 0.4, 0]
    for i, a in enumerate(pattern):
        if a:
            place(buf, tick(4200 if i % 4 else 2800, length=0.014, bright=0.5), i * step, a * 0.5)
    t = np.arange(len(buf)) / SR
    hum = np.sin(2 * np.pi * 110 * t) * 0.05 + np.sin(2 * np.pi * 220.5 * t) * 0.025
    hum *= 0.6 + 0.4 * np.sin(2 * np.pi * t / bar * 2) ** 2
    data = bandpass(rng.standard_normal(len(t)), 5000, 11000) * 0.02
    write("process.wav", stereo_width(buf + hum + data), -14)


def warning():
    t = t_axis(1.1)
    buf = np.zeros_like(t)
    for k, f in enumerate((988, 740, 988, 740)):
        seg = t_axis(0.13)
        sq = np.sign(np.sin(2 * np.pi * f * seg)) * 0.5 + np.sin(2 * np.pi * f * seg) * 0.5
        sq = onepole_lp(sq, 3200) * env_adsr(len(seg), 0.004, 0.03, 0.06, 0.03, 0.7)
        place(buf, sq, k * 0.16, 0.6)
    sub = np.sin(2 * np.pi * (58 - 20 * t) * t) * np.exp(-t * 4.5)
    crush = np.round(bandpass(rng.standard_normal(len(t)), 200, 6000) * 6) / 6 * np.exp(-t * 18) * 0.25
    write("warning.wav", stereo_width(buf + sub * 0.9 + crush), -3)


def reveal():
    t = t_axis(0.8)
    x = np.zeros_like(t)
    for f, g in ((1174.7, 1.0), (1760.0, 0.6), (2349.3, 0.35)):
        x += g * np.sin(2 * np.pi * f * t) * np.exp(-t * 5.5)
    x *= env_adsr(len(t), 0.12, 0.1, 0.3, 0.3, 0.6)
    air = bandpass(rng.standard_normal(len(t)), 4000, 12000) * env_adsr(len(t), 0.25, 0.1, 0.2, 0.25, 0.4) * 0.08
    write("reveal.wav", stereo_width(x * 0.25 + air, 0.002), -10)


if __name__ == "__main__":
    draw()
    access()
    assemble()
    process()
    warning()
    reveal()
