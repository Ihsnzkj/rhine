<script lang="ts">
  import DecodeText from "./DecodeText.svelte";
  // "PERMISSION AUTHORIZED" reticle (film 0:20): broken hairline rings
  // counter-rotating, orange satellites, PROCESSING bar.
  let { active = 0, label = "VERIFYING IDENTITY", progress = 0 }: { active?: number; label?: string; progress?: number } = $props();
  const arc = (r: number, a0: number, sweep: number) => {
    const p = (a: number) => `${200 + r * Math.cos((a * Math.PI) / 180)} ${200 + r * Math.sin((a * Math.PI) / 180)}`;
    return `M ${p(a0)} A ${r} ${r} 0 ${sweep > 180 ? 1 : 0} 1 ${p(a0 + sweep)}`;
  };
</script>

{#if active > 0.001}
<div class="ring" style:opacity={active} style:transform="translate(-50%, -50%) scale({0.92 + 0.08 * active})">
  <div class="halo"></div>
  <svg viewBox="0 0 400 400" width="400" height="400">
    <g class="r1"><path d={arc(150, -60, 300)} /></g>
    <g class="r2"><path d={arc(118, 200, 70)} class="thick" /></g>
    <g class="r3"><path d={arc(52, 10, 120)} /><path d={arc(52, 190, 120)} /></g>
    <g class="o1"><circle cx="350" cy="200" r="4.5" /><circle cx="50" cy="200" r="3" /></g>
    <g class="o2"><circle cx="200" cy="108" r="3.5" /></g>
  </svg>
  <div class="label"><DecodeText text={label} /></div>
  <div class="proc">
    <span>PROCESSING</span>
    <div class="bar"><i style:width="{Math.max(0, Math.min(1, progress)) * 100}%"></i></div>
  </div>
</div>
{/if}

<style>
  .ring { position: absolute; width: 400px; height: 400px; pointer-events: none; }
  .halo { position: absolute; inset: -30px; border-radius: 50%; background: radial-gradient(closest-side, color-mix(in srgb, var(--paper) 90%, transparent) 45%, transparent); }
  svg { position: absolute; inset: 0; overflow: visible; }
  path { fill: none; stroke: var(--ink); stroke-width: 1.4; }
  .thick { stroke-width: 2.2; }
  circle { fill: var(--orange); }
  g { transform-origin: 200px 200px; }
  .r1 { animation: spin 9s linear infinite; }
  .r2 { animation: spin 3.8s linear infinite reverse; }
  .r3 { animation: spin 2.6s linear infinite reverse; }
  .o1 { animation: spin 7s linear infinite; }
  .o2 { animation: spin 4.2s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .label { position: absolute; left: 0; right: 0; top: 50%; transform: translateY(-50%); text-align: center; font-size: 15px; letter-spacing: 6px; color: var(--ink); }
  .proc { position: absolute; left: 50%; top: 396px; transform: translateX(-50%); display: flex; flex-direction: column; align-items: center; gap: 7px; font-size: 10px; font-weight: 500; letter-spacing: 2px; color: var(--ink); }
  .bar { width: 168px; height: 9px; border: 1px solid var(--ink); padding: 2px; }
  .bar i { display: block; height: 100%; background: var(--ink); transition: width 200ms; }
</style>
