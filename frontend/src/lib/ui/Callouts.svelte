<script lang="ts">
  // Film 3:44 — "SERVICES SECT" leader lines pinned to live points on the
  // rotating band, and the RHINE LAB.LLC. bar at its visual crossing.
  type P = { x: number; y: number };
  let { a, b, centre, reveal = 1, detailA = "", detailB = "" }:
    { a: P; b: P; centre: P; reveal?: number; detailA?: string; detailB?: string } = $props();
  const LA = { x: 742, y: 96 }, LB = { x: 720, y: 852 };
</script>

{#if reveal > 0.01}
<div class="callouts" style:opacity={reveal}>
  <svg width="1920" height="1080">
    <line x1={a.x} y1={a.y} x2={a.x + (LA.x - 10 - a.x) * reveal} y2={a.y + (LA.y + 30 - a.y) * reveal} />
    <line x1={b.x} y1={b.y} x2={b.x + (LB.x - 10 - b.x) * reveal} y2={b.y + (LB.y + 30 - b.y) * reveal} />
    <circle cx={a.x} cy={a.y} r="3.5" /><circle cx={b.x} cy={b.y} r="3.5" />
  </svg>
  <div class="label" style:left="{LA.x}px" style:top="{LA.y}px"><span>SERVICES SECT</span><b>TERMINAL NODE</b><em>{detailA}</em></div>
  <div class="label" style:left="{LB.x}px" style:top="{LB.y}px"><span>SERVICES SECT</span><b>SESSION SEAL</b><em>{detailB}</em></div>
  <div class="llc" style:left="{centre.x - 34}px" style:top="{centre.y - 12}px" style:width="{300 * reveal}px">
    <b>RHINE LAB.LLC.</b><i></i>
  </div>
</div>
{/if}

<style>
  .callouts { position: absolute; inset: 0; pointer-events: none; }
  svg { position: absolute; left: 0; top: 0; overflow: visible; }
  line { stroke: var(--ink); stroke-width: 1.2; }
  circle { fill: var(--ink); }
  .label { position: absolute; display: flex; flex-direction: column; gap: 3px; color: var(--ink); }
  .label span { align-self: flex-start; background: var(--ink); color: var(--ink-inverse); font-size: 7.5px; font-weight: 600; letter-spacing: 0.6px; padding: 1px 5px; }
  .label b { font-size: 17px; font-weight: 700; letter-spacing: 0.3px; }
  .label em { font-style: normal; font-size: 8.5px; font-weight: 500; color: var(--mute); letter-spacing: 0.6px; white-space: nowrap; }
  .llc { position: absolute; height: 24px; overflow: hidden; display: flex; align-items: center; justify-content: space-between;
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--card) 82%, transparent) 12%); }
  .llc b { margin-left: 46px; font-size: 12px; font-weight: 700; letter-spacing: 0.8px; color: var(--ink); white-space: nowrap; }
  .llc i { width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--ink); margin-right: 8px; flex: none; }
</style>
