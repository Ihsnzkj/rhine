#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Generate the Rhine Lab infinity mark from its construction geometry.

Two circles joined by internal tangents that cross at the midpoint; the
upper-right tangent is broken, leaving the "minus" loop open.  Traced from
frames of the official special film "莱茵生命：访问" (Rhine Lab: Visit).
"""
import json, math, pathlib

W, H = 1200, 570           # viewBox
C1, C2 = (285.0, 285.0), (915.0, 285.0)
R, STROKE = 215.0, 100.0   # centreline radius, stroke width
X = ((C1[0] + C2[0]) / 2, C1[1])
half = (C2[0] - C1[0]) / 2
theta = math.asin(R / half)                      # tangent slope
t_tan = math.sqrt(half * half - R * R)           # distance from X to tangent points
u = (math.cos(theta), -math.sin(theta))           # "/" direction (up-right)
v = (math.cos(theta), math.sin(theta))            # "\" direction (down-right)
STUB = 0.545 * t_tan                              # where the "/" stroke stops


def at(d, t):
    return (X[0] + d[0] * t, X[1] + d[1] * t)


def f(p):
    return f"{p[0]:.2f} {p[1]:.2f}"


stub = at(u, STUB)
p_l1 = at(u, -t_tan)   # lower-right of the left loop
p_l2 = at(v, -t_tan)   # upper-right of the left loop
p_r2 = at(v, t_tan)    # lower-left of the right loop
p_r1 = at(u, t_tan)    # upper-left of the right loop (open end)

ring = (f"M {f(stub)} L {f(p_l1)} A {R} {R} 0 1 1 {f(p_l2)} "
        f"L {f(p_r2)} A {R} {R} 0 1 0 {f(p_r1)}")

# plus & minus glyphs (filled), measured from the trace
plus_c, arm, bar = (290.0, 285.0), 98.0, 64.0
minus_c, mlen = (918.0, 288.0), 162.0
plus = (f"M {plus_c[0]-bar/2} {plus_c[1]-arm} h {bar} v {arm-bar/2} h {arm-bar/2} v {bar} "
        f"h {-(arm-bar/2)} v {arm-bar/2} h {-bar} v {-(arm-bar/2)} h {-(arm-bar/2)} v {-bar} "
        f"h {arm-bar/2} Z")
minus = f"M {minus_c[0]-mlen/2} {minus_c[1]-bar/2} h {mlen} v {bar} h {-mlen} Z"

# arc lengths for stroke-draw animations (in stroke-width units for Qt dash patterns)
length = (STUB + t_tan) + R * (2 * math.pi - 2 * (math.pi / 2 - theta)) + 2 * t_tan \
    + R * (2 * math.pi - 2 * (math.pi / 2 - theta))

out = pathlib.Path(__file__).resolve().parents[2] / "tools" / "assets" / "build"
out.mkdir(parents=True, exist_ok=True)
for name, col in (("logo-ink", "#111111"), ("logo-white", "#ffffff"), ("logo-orange", "#ff7b00")):
    (out / f"{name}.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">'
        f'<path d="{ring}" fill="none" stroke="{col}" stroke-width="{STROKE}" stroke-linecap="butt" stroke-linejoin="miter"/>'
        f'<path d="{plus} {minus}" fill="{col}"/></svg>\n')

(out / "logo-geometry.json").write_text(json.dumps({
    "viewBox": [W, H], "ring": ring, "plus": plus, "minus": minus,
    "stroke": STROKE, "ringLength": round(length, 2)}, indent=2) + "\n")
print("ring length", round(length, 1), "theta", round(math.degrees(theta), 2))
