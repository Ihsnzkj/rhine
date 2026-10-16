<script lang="ts">
  // Text that resolves letter by letter behind a short tail of scrambled grey
  // glyphs — how captions assemble themselves in the film.  Re-plays whenever
  // `text` changes (or `replay` bumps).
  let { text = "", duration = 0, replay = 0, ghost = "var(--faint)", class: cls = "", style = "" }:
    { text?: string; duration?: number; replay?: number; ghost?: string; class?: string; style?: string } = $props();
  const GLYPHS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789#%&*+=/<>";
  const TAIL = 3;
  let shown = $state("");
  let tail = $state("");

  $effect(() => {
    const src = text;
    void replay;
    const total = duration || 32 * src.length + 180;
    const t0 = performance.now();
    let raf = 0;
    const step = (now: number) => {
      const p = Math.min(1, (now - t0) / total);
      const n = Math.floor(p * (src.length + TAIL));
      const solid = Math.max(0, Math.min(src.length, n - TAIL));
      let g = "";
      for (let i = solid; i < Math.min(src.length, n); ++i)
        g += src[i] === " " ? " " : GLYPHS[(i * 7 + Math.floor(now / 45) * 13 + src.charCodeAt(i)) % GLYPHS.length];
      shown = src.slice(0, solid);
      tail = g;
      if (p < 1) raf = requestAnimationFrame(step);
      else { shown = src; tail = ""; }
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });
</script>

<span class={cls} {style}>{shown}<span style:color={ghost}>{tail}</span></span>
