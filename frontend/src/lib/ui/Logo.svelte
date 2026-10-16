<script lang="ts">
  // The Rhine Lab ∞ mark, from its construction geometry (sddm tools/gen_logo.py).
  // `draw` reveals the ring like a pen stroke; the + and − fade in after.
  let { draw = 1, color = "var(--ink)", caption = false, width = 240 }:
    { draw?: number; color?: string; caption?: boolean; width?: number } = $props();
  const RING = "M 691.70 199.36 L 431.75 442.13 A 215.0 215.0 0 1 1 431.75 127.87 L 768.25 442.13 A 215.0 215.0 0 1 0 768.25 127.87";
  const GLYPHS = "M 258.0 187.0 h 64.0 v 66.0 h 66.0 v 64.0 h -66.0 v 66.0 h -64.0 v -66.0 h -66.0 v -64.0 h 66.0 Z M 837.0 256.0 h 162.0 v 64.0 h -162.0 Z";
  const LEN = 2813.06;
  const glyphs = $derived(Math.max(0, Math.min(1, (draw - 0.55) / 0.45)));
</script>

<div class="logo" style:width="{width}px">
  <svg viewBox="0 0 1200 570" width={width} height={width * 570 / 1200} aria-hidden="true">
    <path d={RING} fill="none" stroke={color} stroke-width="100" stroke-linecap="butt" stroke-linejoin="miter"
          stroke-dasharray="{LEN} {LEN}" stroke-dashoffset={LEN * (1 - draw)} />
    <path d={GLYPHS} fill={color} opacity={glyphs} />
  </svg>
  {#if caption}
    <div class="cap" style:color={color} style:opacity={glyphs} style:font-size="{Math.max(6, width * 0.062)}px"
         style:letter-spacing="{width * 0.062}px">RHINE·LAB</div>
  {/if}
</div>

<style>
  .logo { display: inline-block; line-height: 0; }
  svg { display: block; overflow: visible; }
  .cap { line-height: 1; margin-top: 0.6em; text-align: center; font-weight: 700; font-family: var(--display); padding-left: 0.5em; }
</style>
