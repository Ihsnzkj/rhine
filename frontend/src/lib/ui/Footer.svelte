<script lang="ts">
  import { lock } from "../lock.svelte";
  import type { PowerAction } from "../protocol";
  // Power controls (two-step: armed, then executed), audio toggle and the
  // film's "POWERED BY RHINE LAB ▬" signature.
  const caps = $derived(lock.init?.capabilities);
  const items = $derived(([
    { key: "suspend", label: "SUSPEND", ok: caps?.suspend },
    { key: "hibernate", label: "HIBERNATE", ok: caps?.hibernate },
    { key: "reboot", label: "REBOOT", ok: caps?.reboot },
    { key: "powerOff", label: "SHUTDOWN", ok: caps?.powerOff },
  ] as { key: PowerAction; label: string; ok?: boolean }[]).filter((i) => i.ok));
</script>

<div class="footer">
  {#each items as it (it.key)}
    <button class="pw" class:armed={lock.armed === it.key} onclick={() => lock.arm(it.key)}>
      <i></i>{lock.armed === it.key ? "CONFIRM " + it.label : it.label}
      {#if lock.armed === it.key}<span class="count"></span>{/if}
    </button>
  {/each}
  <button class="audio" class:off={lock.muted} onclick={() => lock.toggleMute()} aria-label="toggle audio">
    <svg width="16" height="12" viewBox="0 0 16 12"><rect x="0" y="4" width="4" height="4" /><path d="M4 4 L8 1 V11 L4 8 Z" />
      {#if !lock.muted}<rect x="9.5" y="3" width="1.5" height="6" /><rect x="12.5" y="1" width="1.5" height="10" />
      {:else}<rect x="7.5" y="5.25" width="9" height="1.5" transform="rotate(45 12 6)" class="x" />{/if}</svg>
    {lock.muted ? "AUDIO OFF" : "AUDIO"}
  </button>
  <i class="sep"></i>
  <div class="sig"><span>POWERED BY</span><b>RHINE LAB</b><i></i></div>
</div>

<style>
  .footer { display: flex; align-items: center; gap: 26px; color: var(--ink); }
  button { all: unset; cursor: pointer; position: relative; display: inline-flex; align-items: center; gap: 8px; height: 24px; padding: 0 6px; font-size: 12.5px; font-weight: 600; letter-spacing: 1.6px; color: var(--ink); transition: background 140ms, color 140ms; }
  button:hover { background: var(--ink); color: var(--ink-inverse); }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .pw i { width: 6px; height: 6px; background: currentColor; }
  .pw.armed { background: var(--orange); color: #111214; }
  .count { position: absolute; left: 0; bottom: 0; height: 2px; background: #111214; animation: count 3600ms linear forwards; }
  @keyframes count { from { width: 100%; } to { width: 0; } }
  .audio svg { fill: currentColor; }
  .audio .x { fill: var(--orange); }
  .audio.off { color: var(--mute); }
  .sep { width: 1px; height: 18px; background: var(--faint); }
  .sig { display: flex; align-items: center; gap: 6px; font-size: 17px; letter-spacing: 0.4px; }
  .sig b { font-weight: 800; letter-spacing: 1.2px; }
  .sig i { width: 24px; height: 5px; background: var(--ink); margin-left: 14px; }
</style>
