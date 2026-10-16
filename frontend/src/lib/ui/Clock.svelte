<script lang="ts">
  import { lock } from "../lock.svelte";
  // Top-right chronometer with the film's orange step indicator (• •─○ • •).
  const DAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
  const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
  const pad = (n: number) => String(n).padStart(2, "0");
  const d = $derived(new Date(lock.now));
  const step = $derived(lock.phase === "boot" ? 0 : lock.phase === "verifying" ? 3 : lock.phase === "denied" ? 4
    : lock.phase === "granted" ? 4 : lock.passwordLength ? 2 : 1);
</script>

<div class="clock">
  <div class="steps">
    {#each [0, 1, 2, 3, 4] as i}
      <div class="node" class:on={i === step}>
        <i class="dot"></i>{#if i === step}<i class="line"></i><i class="ring"></i>{/if}
      </div>
    {/each}
  </div>
  <div class="time"><span class="hm">{pad(d.getHours())}:{pad(d.getMinutes())}</span><span class="sec">{pad(d.getSeconds())}</span></div>
  <div class="date">{DAYS[d.getDay()]}&nbsp; · &nbsp;{pad(d.getDate())} {MONTHS[d.getMonth()]} {d.getFullYear()}</div>
  <div class="term"><i></i>TERMINAL / {(lock.init?.host ?? "localhost").toUpperCase()}</div>
</div>

<style>
  .clock { display: flex; flex-direction: column; align-items: flex-end; color: var(--ink); }
  .steps { display: flex; gap: 16px; height: 12px; align-items: center; margin-right: 4px; }
  .node { position: relative; width: 7px; height: 12px; transition: width 320ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  .node.on { width: 34px; }
  .dot { position: absolute; left: 0; top: 2.5px; width: 7px; height: 7px; border-radius: 50%; background: var(--orange); }
  .line { position: absolute; left: 7px; right: 12px; top: 5.25px; height: 1.5px; background: var(--orange); }
  .ring { position: absolute; right: 0; top: 0; width: 12px; height: 12px; border-radius: 50%; border: 1.5px solid var(--orange); box-sizing: border-box; }
  .time { display: flex; align-items: flex-start; gap: 6px; margin-top: 10px; }
  .hm { font-size: 92px; font-weight: 300; letter-spacing: -2px; line-height: 1; }
  .sec { font-family: var(--mono); font-size: 17px; color: var(--mute); margin-top: 12px; }
  .date { margin-top: 8px; margin-right: 2px; font-size: 15px; font-weight: 500; letter-spacing: 3.2px; }
  .term { margin-top: 12px; margin-right: 2px; font-family: var(--mono); font-size: 11px; color: var(--mute); display: flex; align-items: center; gap: 8px; }
  .term i { width: 6px; height: 6px; background: var(--ink); display: block; }
</style>
