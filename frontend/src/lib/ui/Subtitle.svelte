<script lang="ts">
  // Bilingual caption strip, set like the film's burnt-in subtitles.
  let { zh = "", en = "" }: { zh?: string; en?: string } = $props();
</script>

{#key zh + en}
  <div class="sub">
    <div class="zh">{zh}</div>
    <div class="en">{en}</div>
  </div>
{/key}

<style>
  .sub { display: flex; flex-direction: column; align-items: center; gap: 4px; animation: in 300ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  .zh { font-family: var(--cjk); font-size: 21px; font-weight: 500; letter-spacing: 1.5px; color: var(--ink); text-shadow: 0 1px 0 color-mix(in srgb, var(--card) 55%, transparent); }
  .en { font-size: 11px; font-weight: 500; letter-spacing: 2.2px; color: var(--mute); }
  @keyframes in { from { opacity: 0; transform: translateY(3px); } to { opacity: 1; transform: none; } }
</style>
