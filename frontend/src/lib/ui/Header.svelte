<script lang="ts">
  import { onMount } from "svelte";
  // RHINE LAB / SYNTHESIZE INFORMATION / ANALYSIS  OS — every line spans the
  // width of the first by tracking (line 2) and by size (line 3), as in the
  // film's corner lock-up.  Measured once the fonts are in.
  let { size = 46 }: { size?: number } = $props();
  let l1: HTMLDivElement, l2: HTMLDivElement, l3: HTMLDivElement;
  let track = $state(0), fit = $state(1);
  onMount(() => {
    const measure = () => {
      const W = l1.getBoundingClientRect().width / scaleOf(l1);
      l2.style.letterSpacing = "0px";
      const n = l2.textContent!.length;
      const w2 = l2.scrollWidth;
      track = (W - w2) / (n - 1);
      const gap = size * 0.42;
      l3.style.fontSize = `${size * 0.86}px`;
      const w3 = l3.scrollWidth - gap;
      fit = (W - gap) / w3;
    };
    const scaleOf = (el: HTMLElement) => el.getBoundingClientRect().width / el.offsetWidth || 1;
    document.fonts.ready.then(measure);
  });
</script>

<div class="hdr" style:--s="{size}px">
  <div class="l1" bind:this={l1}>RHINE LAB</div>
  <div class="l2" bind:this={l2} style:letter-spacing="{track}px">SYNTHESIZE INFORMATION</div>
  <div class="l3" bind:this={l3} style:font-size="{size * 0.86 * fit}px" style:gap="{size * 0.42}px"><span class="a">ANALYSIS</span><span class="b">OS</span></div>
</div>

<style>
  .hdr { display: inline-flex; flex-direction: column; color: var(--ink); font-family: var(--display); line-height: 1; }
  .l1 { font-size: var(--s); font-weight: 700; letter-spacing: calc(var(--s) * 0.02); align-self: flex-start; }
  .l2 { font-size: calc(var(--s) * 0.382); font-weight: 500; margin-top: calc(var(--s) * 0.16); white-space: nowrap; align-self: flex-start; }
  .l3 { display: inline-flex; margin-top: calc(var(--s) * 0.06); white-space: nowrap; align-self: flex-start; }
  .a { font-weight: 300; }
  .b { font-weight: 700; }
</style>
