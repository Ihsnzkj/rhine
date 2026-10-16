<script lang="ts">
  import DecodeText from "./DecodeText.svelte";
  // Final act of the film: black plate, orange WARNING, and the access log
  // ticking through ILLICIT ACCESS DETECTED / PERMISSION DENIED.
  let { active = 0, attempts = 1, reason = "AUTHENTICATION FAILED", replay = 0, accepted = false }:
    { active?: number; attempts?: number; reason?: string; replay?: number; accepted?: boolean } = $props();
  // a refusal after the key was accepted is a configuration fault, not an intruder
  const rows = $derived(accepted
    ? [reason, "SYSTEM POLICY REFUSED THE SESSION", "CHECK /ETC/PAM.D/RHINE-LOCK", "RETURNING TO ACCESS PORTAL"]
    : [reason, "ILLICIT ACCESS DETECTED", `PERMISSION DENIED  ·  ATTEMPT ${String(attempts).padStart(2, "0")}`, "RETURNING TO ACCESS PORTAL"]);
</script>

{#if active > 0.001}
<div class="warn" style:opacity={Math.min(1, active * 1.4)}>
  <div class="halo"></div>
  <div class="box">
    <svg class="tri" width="30" height="27" viewBox="0 0 30 27"><path d="M15 1 L29 26 L1 26 Z" /><rect x="14" y="9" width="2" height="9" /><rect x="14" y="20" width="2" height="2" /></svg>
    <div class="plate"><i></i><DecodeText text="WARNING" {replay} ghost="color-mix(in srgb, var(--orange) 35%, transparent)" /><i></i></div>
    {#key replay}
      {#each rows as r, i}
        <div class="row" class:last={i === 3} style:animation-delay="{260 + i * 240}ms"><span>{r}</span><b></b></div>
      {/each}
    {/key}
  </div>
</div>
{/if}

<style>
  .warn { position: absolute; width: 420px; height: 260px; transform: translate(-50%, -38%); pointer-events: none; }
  .halo { position: absolute; inset: -150px -170px; background: radial-gradient(closest-side, rgba(5, 5, 6, 0.94) 50%, transparent); }
  .box { position: absolute; left: 32px; right: 32px; top: -26px; height: 250px; background: #060607; border: 1px solid rgba(255, 123, 0, 0.25); display: flex; flex-direction: column; align-items: center; padding-top: 26px; }
  .tri { fill: none; }
  .tri path { stroke: var(--orange); stroke-width: 2; stroke-linejoin: miter; }
  .tri rect { fill: var(--orange); }
  .tri { animation: blink 860ms steps(1) infinite; }
  @keyframes blink { 0%, 70% { opacity: 1; } 71%, 100% { opacity: 0.25; } }
  .plate { margin-top: 15px; width: 300px; height: 46px; border: 2px solid var(--orange); display: flex; align-items: center; justify-content: space-between; color: var(--orange); font-size: 30px; letter-spacing: 3px; }
  .plate i { width: 9px; height: 100%; background: var(--orange); }
  .row { width: 300px; margin-top: 7px; display: flex; justify-content: space-between; align-items: center; font-size: 9.5px; font-weight: 500; letter-spacing: 1px; color: #cfcdc8; opacity: 0; animation: show 1ms forwards; }
  .row:first-of-type { margin-top: 14px; }
  .row b { width: 8px; height: 8px; border: 1px solid currentColor; }
  .row.last { color: var(--orange); }
  .row.last b { background: var(--orange); }
  @keyframes show { to { opacity: 1; } }
</style>
