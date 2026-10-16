<script lang="ts">
  import Logo from "./Logo.svelte";
  // Black section card from the film (CMPT CTRL, STRU, NRG…): smoky plate, a
  // vowel-less code with the ∞ set into one letter, and a caption.
  let { code = "SYS", caption = "", tab = "SECTION", width = 376, height = 263, showTab = true }:
    { code?: string; caption?: string; tab?: string; width?: number; height?: number; showTab?: boolean } = $props();
  const lig = $derived(code.length === 4 ? 2 : Math.min(1, code.length - 1));
  const px = $derived(Math.round(Math.min(height * 0.4, (width * 1.25) / Math.max(3, code.length))));
</script>

<div class="tile" style:width="{width}px" style:height="{height}px">
  {#if showTab}<div class="tab" style:height="{Math.max(14, height * 0.12)}px" style:font-size="{Math.max(9, height * 0.074)}px">{tab}</div>{/if}
  <div class="plate">
    <div class="smoke"></div>
    <i class="reg"></i>
    <div class="code" style:font-size="{px}px" style:letter-spacing="{-px * 0.045}px">
      {#each code.split("") as ch, i}
        <span class="ch" class:lig={i === lig}>{ch}{#if i === lig}<span class="mark" style:width="{px * 0.5}px"><Logo width={px * 0.5} color="#ffffff" /></span>{/if}</span>
      {/each}
    </div>
    {#if caption}<div class="cap" style:font-size="{Math.max(8, Math.round(height * 0.05))}px">{caption}</div>{/if}
    <div class="edge"></div>
  </div>
</div>

<style>
  .tile { position: relative; }
  .tab { position: absolute; left: 14%; bottom: calc(100% - 1px); padding: 0 0.9em; display: flex; align-items: center; background: var(--tab); color: color-mix(in srgb, #3c3c3c, #bdbdbd calc(var(--denied) * 100%)); letter-spacing: 0.5px; }
  .plate { position: absolute; inset: 0; overflow: hidden; background: #101112; display: flex; flex-direction: column; align-items: center; justify-content: center; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, calc(var(--denied) * 0.22)); }
  .smoke { position: absolute; inset: -6%; background: url("/scene/smoke.jpg") center / cover; animation: drift 38s ease-in-out infinite alternate; }
  @keyframes drift { from { transform: translate(-2%, -1%) scale(1.04); } to { transform: translate(2%, 1.5%) scale(1.08); } }
  .reg { position: absolute; right: 6.5%; top: 12%; width: 4.2%; aspect-ratio: 1; border-radius: 50%; background: #6c6c6c; opacity: 0.85; }
  .code { position: relative; color: #fff; font-weight: 600; line-height: 1; display: flex; margin-top: -4%; }
  .ch { position: relative; }
  .mark { position: absolute; left: 50%; top: 60%; transform: translate(-50%, -50%); line-height: 0; padding: 0.12em 0.05em; background: #101112; border-radius: 999px; box-shadow: 0 0 0 0.02em #101112; }
  .cap { position: relative; color: #d9d9d9; margin-top: 0.9em; letter-spacing: 0.04em; text-transform: uppercase; max-width: 86%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .edge { position: absolute; inset: 0; border: 2px solid var(--lime); opacity: 0; transition: opacity 180ms; }
</style>
