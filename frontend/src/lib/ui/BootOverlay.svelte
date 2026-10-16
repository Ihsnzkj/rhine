<script lang="ts">
  import Logo from "./Logo.svelte";
  import DecodeText from "./DecodeText.svelte";
  // Lock-in sequence: the mark is drawn, TERMINAL LOCKED resolves, the band is
  // drawn in 3D behind it while the mark flies into the lens.
  let { draw = 0, shift = 0, fade = 1, zoom = 0, name = "OPERATOR", at = "" }:
    { draw?: number; shift?: number; fade?: number; zoom?: number; name?: string; at?: string } = $props();
</script>

{#if fade > 0.001}
<div class="boot">
  <div class="access" style:opacity={1 - shift}><DecodeText text="TERMINAL LOCKED · ACCESS PERMISSION REQUIRED" /></div>
  <div class="lockup" style:opacity={fade} style:transform="translate(calc(-50% - {shift * 150}px), -50%) scale({1 + zoom * 0.9})">
    <Logo width={170} {draw} caption />
    <div class="lines" style:opacity={shift}>
      {#if shift > 0.02}
        <div><i></i><DecodeText text={"ID CONFIRMED : " + name} /></div>
        <div><i class="o"></i><DecodeText text={"SESSION SEALED · " + at} /></div>
      {/if}
    </div>
  </div>
</div>
{/if}

<style>
  .boot { position: absolute; inset: 0; pointer-events: none; }
  .access { position: absolute; left: 0; right: 0; top: calc(50% + 70px); text-align: center; font-size: 12px; font-weight: 500; letter-spacing: 3.5px; color: var(--ink); }
  .lockup { position: absolute; left: 50%; top: 50%; transform-origin: 85px 40px; }
  .lines { position: absolute; left: 204px; top: 26px; display: flex; flex-direction: column; gap: 6px; font-size: 11px; font-weight: 500; letter-spacing: 2px; white-space: nowrap; color: var(--ink); }
  .lines div { display: flex; align-items: center; gap: 8px; }
  .lines i { width: 4px; height: 4px; background: var(--ink); }
  .lines i.o { background: var(--orange); }
</style>
