// The user's own desktop as the first and last frame of the lock.
//
// index.html paints the pre-lock screenshot (contract rev 3) full-screen
// before the bundle even loads; this module adopts that element — reusing it
// matters, the rhine:// scheme is no-store — and flies it between "the whole
// screen" and the SEALED SECTION tile as one continuous move: log-scale
// shrink, a curved path, tilt / corner radius / shadow peaking mid-flight.

export interface Rect { x: number; y: number; w: number; h: number }

const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
const clamp01 = (t: number) => Math.max(0, Math.min(1, t));
export const smooth = (a: number, b: number, t: number) => {
  const x = clamp01((t - a) / (b - a));
  return x * x * (3 - 2 * x);
};

export class SnapCard {
  private el = document.getElementById("rl-snap");
  private img = document.getElementById("rl-snap-img") as HTMLImageElement | null;
  private shade = this.el?.querySelector<HTMLElement>(".shade") ?? null;
  private scan = this.el?.querySelector<HTMLElement>(".scan") ?? null;
  private vfs = Array.from(this.el?.querySelectorAll<HTMLElement>(".vf") ?? []);
  private caption: HTMLDivElement | null = null;

  /** resolves true when a screenshot is shown, false when there is none */
  readonly ready: Promise<boolean> = new Promise((resolve) => {
    const img = this.img;
    if (!img || !this.el || document.documentElement.classList.contains("no-snap") || !img.getAttribute("src")) return resolve(false);
    if (img.complete) return resolve(img.naturalWidth > 0);
    img.addEventListener("load", () => resolve(true), { once: true });
    img.addEventListener("error", () => resolve(false), { once: true });
    setTimeout(() => resolve(img.complete && img.naturalWidth > 0), 1500);
  });

  /**
   * Place the card. `e` = 0 → full screen, 1 → `target`; the path between is
   * fixed so entry (0→1) and exit (1→0) are exact mirror images.
   */
  pose(e: number, target: Rect, extra: { opacity?: number; scan?: number; corners?: number; desat?: number; caption?: string; captionOpacity?: number } = {}) {
    const el = this.el;
    if (!el) return;
    const vw = innerWidth, vh = innerHeight;
    const sT = Math.min(target.w / vw, target.h / vh);
    const s = Math.exp(Math.log(sT) * e);                         // perceptually even shrink
    // quadratic Bézier from the screen centre to the target centre, bowing up
    const p0 = { x: vw / 2, y: vh / 2 }, p2 = { x: target.x + target.w / 2, y: target.y + target.h / 2 };
    const p1 = { x: p0.x * 0.55 + p2.x * 0.45, y: Math.min(p0.y, p2.y) - vh * 0.16 };
    const u = 1 - e;
    const cx = u * u * p0.x + 2 * u * e * p1.x + e * e * p2.x;
    const cy = u * u * p0.y + 2 * u * e * p1.y + e * e * p2.y;
    const arc = Math.sin(Math.PI * e);
    const tilt = -13 * arc;
    const radius = 18 * arc;
    el.style.display = "block";
    el.style.transformOrigin = "50% 50%";
    el.style.transform = `translate(${cx - vw / 2}px, ${cy - vh / 2}px) perspective(1800px) rotateY(${tilt}deg) scale(${s})`;
    el.style.borderRadius = `${radius / s}px`;
    el.style.overflow = "hidden";
    el.style.boxShadow = arc > 0.01 ? `0 ${26 / s}px ${70 / s}px rgba(44, 36, 24, ${0.34 * arc})` : "none";
    el.style.opacity = String(extra.opacity ?? 1);
    if (this.img) this.img.style.filter = `saturate(${1 - 0.55 * (extra.desat ?? 0)}) brightness(${1 + 0.04 * (extra.desat ?? 0)})`;
    if (this.shade) this.shade.style.opacity = String(0.12 * (extra.desat ?? 0));
    if (this.scan) {
      const sc = extra.scan;
      this.scan.style.opacity = sc === undefined || sc < 0 || sc > 1 ? "0" : String(Math.sin(Math.PI * sc));
      if (sc !== undefined) this.scan.style.transform = `translateY(${clamp01(sc) * vh}px) scaleY(${1 / s})`;
    }
    for (const v of this.vfs) {
      v.style.animation = "none";
      v.style.opacity = String(extra.corners ?? 0);
      v.style.transform = `scale(${1 / s})`;
      v.style.transformOrigin = v.classList.contains("tl") ? "0 0" : v.classList.contains("tr") ? "100% 0"
        : v.classList.contains("bl") ? "0 100%" : "100% 100%";
    }
    // a small archival caption under the card, unscaled
    if (extra.caption) {
      const c = this.caption ?? this.makeCaption();
      c.textContent = extra.caption;
      c.style.opacity = String(extra.captionOpacity ?? 0);
      c.style.transform = `translate(${cx - (vw * s) / 2}px, ${cy + (vh * s) / 2 + 14}px)`;
    } else if (this.caption) this.caption.style.opacity = "0";
  }

  hide() {
    if (this.el) this.el.style.display = "none";
    if (this.caption) this.caption.style.opacity = "0";
  }

  private makeCaption() {
    const c = document.createElement("div");
    c.style.cssText = "position:fixed;left:0;top:0;z-index:1001;pointer-events:none;white-space:nowrap;" +
      "font:500 11px 'JetBrains Mono',monospace;letter-spacing:1.5px;color:#111214;";
    document.body.appendChild(c);
    this.caption = c;
    return c;
  }
}

export const easeInOutCubic = easeInOut;
export { clamp01 };
