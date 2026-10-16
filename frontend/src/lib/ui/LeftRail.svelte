<script lang="ts">
  import { lock } from "../lock.svelte";
  import DecodeText from "./DecodeText.svelte";
  // The film's left column: hairline rule with the lime cursor block, database
  // micro-copy, the live command log, key legend and section caption.
  let { reveal = 1, label = "" }: { reveal?: number; label?: string } = $props();
  const markerY = $derived(lock.phase === "verifying" ? 520 : lock.phase === "denied" ? 700 : lock.phase === "standby" ? 880 : 347);
  const hints = [
    { k: "ENTER", v: "REQUEST" }, { k: "ESC", v: "CLEAR" }, { k: "F3", v: "KEYMAP" },
    { k: "F4", v: "AUDIO" }, { k: "F7–F9", v: "POWER" },
  ];
</script>

<div class="rail" style:--r={reveal}>
  <div class="rule" style:height="{Math.min(1, reveal * 1.4) * 100}%"></div>
  <div class="markerwrap" style:opacity={Math.min(1, reveal * 2)}>{#key markerY}<div class="marker" style:top="{markerY}px"></div>{/key}</div>
  <div class="body" style:opacity={Math.max(0, Math.min(1, reveal * 2 - 0.6))}>
    <i class="bar" style="top:408px"></i>
    <i class="dash" style="left:46px; top:368px"></i>
    <div class="chip" style="left:46px; top:404px">RHINE LAB LLC DATABASE</div>
    <p class="micro" style="left:60px; top:424px">COLUMBIA'S LARGEST DIGITAL COLLECTION OF CLINICAL RECORDS, BIOLOGICAL INFORMATION AND UP-TO-DATE RESEARCH PROJECTS. ALL VISUALISED THROUGH THE ANALYSIS OS.</p>
    <div class="chip" style="left:46px; top:640px">COMMAND ENTERED</div>
    <div class="log" style="left:46px; top:662px">
      {#each lock.log as e, i (e.id)}
        <div class="row" style:opacity={0.35 + 0.65 * (i + 1) / lock.log.length}><i></i><DecodeText text={e.text} /></div>
      {/each}
    </div>
    <i class="dash" style="left:46px; top:862px"></i>
    <i class="hair" style="left:70px; top:863px"></i>
    <div class="hints" style="left:46px; top:880px">
      {#each hints as h}<span><b>{h.k}</b>{h.v}</span>{/each}
    </div>
    <i class="foot" style="top:955px"></i>
    <div class="label" style="left:73px; top:950px">{label}</div>
  </div>
</div>

<style>
  .rail { position: absolute; left: 0; top: 0; width: 700px; height: 1080px; color: var(--ink); }
  .rule { position: absolute; left: 429px; top: 0; width: 2px; background: color-mix(in srgb, var(--ink) 86%, transparent); }
  .markerwrap { position: absolute; inset: 0; }
  .marker { position: absolute; left: 411px; width: 38px; height: 22px; background: var(--accent); animation: blink 260ms steps(1) 1; transition: top 0s; }
  @keyframes blink { 0% { opacity: 0.2; } 30% { opacity: 1; } 55% { opacity: 0.35; } 75% { opacity: 1; } }
  .body > * { position: absolute; }
  .bar { left: 0; width: 13px; height: 292px; background: var(--ink); }
  .dash { width: 14px; height: 3px; background: var(--ink); }
  .hair { width: 190px; height: 1px; background: color-mix(in srgb, var(--ink) 60%, transparent); }
  .chip { background: var(--ink); color: var(--ink-inverse); font-size: 7.5px; font-weight: 600; letter-spacing: 0.4px; padding: 2px 6px; line-height: 1.1; }
  .micro { margin: 0; width: 196px; font-family: var(--mono); font-size: 7.5px; line-height: 1.6; color: var(--mute); }
  .log { display: flex; flex-direction: column; gap: 4px; font-family: var(--mono); font-size: 8.5px; color: var(--mute); letter-spacing: 0.3px; }
  .row { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
  .row i { width: 2px; height: 2px; background: var(--ink); }
  .hints { width: 360px; display: flex; flex-wrap: wrap; gap: 10px 14px; font-size: 8.5px; font-weight: 500; letter-spacing: 0.8px; color: var(--mute); }
  .hints span { display: inline-flex; align-items: center; gap: 5px; }
  .hints b { font-family: var(--mono); font-weight: 700; color: var(--ink); border: 1px solid var(--ink); padding: 1px 4px; }
  .foot { left: 0; width: 46px; height: 14px; background: var(--ink); }
  .label { font-size: 17px; font-weight: 700; letter-spacing: 0.6px; white-space: nowrap; }
</style>
