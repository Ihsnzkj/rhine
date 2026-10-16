#!/usr/bin/env python3
"""Fetch the lock screen's score — optional, for personal use.

"Rhine Lab.LLC" from the official 「孤星」 (Lone Trail) OST is © Hypergryph /
Monster Siren Records and is NOT distributed with rhine-lock.  This script
downloads it from Monster Siren's public API on your own machine and builds
the gapless loop the frontend plays:

  * the track is a steady 112 BPM groove; the first 32 bars are cut exactly on
    the bar line and the bar that follows is cross-faded (equal power) into
    the head, so sample 0 continues the music after the last sample;
  * loudness-normalised to -18 LUFS, encoded as Opus.

The frontend loops it with AudioBufferSourceNode.loop / loopEnd = 32 bars, so
encoder padding past the end never plays.  Without this file the lock screen
simply has effects but no music.

usage: tools/fetch-bgm.py [--out frontend/public/audio/bgm-loop.opus]
needs: python3 + numpy, ffmpeg (with libopus)
"""
import argparse
import json
import pathlib
import subprocess
import sys
import urllib.request

import numpy as np

SONG = "779491"                     # 孤星OST · Rhine Lab.LLC
SR = 48000
BPM = 112.0
BARS = 32
BAR = int(round(60.0 / BPM * 4 * SR))   # 102857 samples — must match frontend audio.ts


def http(url: str, timeout: int = 120) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "rhine-lock (personal BGM fetch)"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def main() -> int:
    root = pathlib.Path(__file__).resolve().parent.parent
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", type=pathlib.Path, default=root / "frontend/public/audio/bgm-loop.opus")
    args = ap.parse_args()

    meta = json.loads(http(f"https://monster-siren.hypergryph.com/api/song/{SONG}"))["data"]
    print(f"downloading {meta['name']} (© Hypergryph — personal use only)")
    raw = http(meta["sourceUrl"], timeout=600)

    pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", "pipe:0", "-f", "f32le", "-ac", "2", "-ar", str(SR), "pipe:1"],
                         input=raw, capture_output=True, check=True).stdout
    x = np.frombuffer(pcm, dtype=np.float32).reshape(-1, 2).astype(np.float64)
    loop = BARS * BAR
    if len(x) < loop + BAR:
        print("track is shorter than expected; the loop points need re-measuring", file=sys.stderr)
        return 1
    body = x[:loop].copy()
    tail = x[loop:loop + BAR]
    g = np.linspace(0, np.pi / 2, BAR)[:, None]
    body[:BAR] = body[:BAR] * np.sin(g) + tail * np.cos(g)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    out16 = (np.clip(body, -1, 1) * 32767).astype("<i2")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "s16le", "-ar", str(SR), "-ac", "2", "-i", "pipe:0",
                    "-af", "loudnorm=I=-18:TP=-1.5:LRA=7", "-c:a", "libopus", "-b:a", "160k", "-ar", str(SR),
                    str(args.out)], input=out16.tobytes(), check=True)
    print(f"wrote {args.out}  ({loop / SR:.3f} s, {BARS} bars at {BPM:g} BPM)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
