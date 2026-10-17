<script lang="ts">
  import { lock } from "../lock.svelte";
  import { SCORE } from "../audio";
  import Logo from "./Logo.svelte";
  // AUDIO FEED: the active MPRIS player as a Rhine Lab specimen card.
  // proxies like playerctld are always on the bus; only show a real track
  const player = $derived(lock.media && (lock.media.title || lock.media.playing) ? lock.media : null);
  // With no player around, the lock's own score — on the primary output
  // only, the one that plays it — once the first media snapshot is in.
  const score = $derived(!player && lock.mediaKnown && lock.scoreAvailable && lock.primary);
  const m = $derived(player ?? (score
    ? { ...SCORE, playing: lock.scorePlaying, artUrl: null, canNext: false, canPrevious: false, canPlayPause: true }
    : null));
  let artOk = $state(false);
  function playPause() {
    if (score) lock.toggleScore();
    else lock.mediaAction("play-pause");
  }
</script>

{#if m}
<div class="feed" class:playing={m.playing} class:score>
  <div class="head"><span>AUDIO FEED</span><em>{m.name.toUpperCase()}</em><i class="live"></i></div>
  <div class="body">
    <div class="art">
      {#if m.artUrl}<img src={m.artUrl} alt="" class:hide={!artOk} onload={() => (artOk = true)} onerror={() => (artOk = false)} />{/if}
      {#if !m.artUrl || !artOk}<div class="ph"><Logo width={46} color="#ffffff" /></div>{/if}
    </div>
    <div class="meta">
      <b>{m.title || "UNTITLED SIGNAL"}</b>
      <span>{[m.artist, m.album].filter(Boolean).join(" · ") || "—"}</span>
      <div class="ctl">
        <button disabled={!m.canPrevious} onclick={() => lock.mediaAction("previous")} aria-label="previous">
          <svg viewBox="0 0 12 10" width="12" height="10"><rect x="0" y="0" width="2" height="10" /><path d="M12 0 L3 5 L12 10 Z" /></svg></button>
        <button class="pp" disabled={!m.canPlayPause} onclick={playPause}
                onpointerdown={(e) => { if (score) e.stopPropagation(); }} aria-label="play or pause">
          {#if m.playing}<svg viewBox="0 0 10 10" width="10" height="10"><rect x="0" y="0" width="3.5" height="10" /><rect x="6.5" y="0" width="3.5" height="10" /></svg>
          {:else}<svg viewBox="0 0 10 10" width="10" height="10"><path d="M0 0 L10 5 L0 10 Z" /></svg>{/if}</button>
        <button disabled={!m.canNext} onclick={() => lock.mediaAction("next")} aria-label="next">
          <svg viewBox="0 0 12 10" width="12" height="10"><path d="M0 0 L9 5 L0 10 Z" /><rect x="10" y="0" width="2" height="10" /></svg></button>
        <div class="levels">{#each [0, 1, 2, 3, 4, 5, 6] as i}<i style:animation-delay="{-i * 173}ms"></i>{/each}</div>
      </div>
    </div>
  </div>
</div>
{/if}

<style>
  .feed { width: 376px; background: var(--card); color: var(--ink); box-shadow: 0 10px 26px -10px color-mix(in srgb, #2c2418 30%, transparent); }
  .head { display: flex; align-items: center; gap: 8px; background: var(--ink); color: var(--ink-inverse); padding: 4px 10px; font-size: 9.5px; font-weight: 700; letter-spacing: 1.2px; }
  .head em { font-style: normal; font-family: var(--mono); font-weight: 400; opacity: 0.7; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .live { width: 6px; height: 6px; border-radius: 50%; background: var(--mute); }
  .playing .live { background: var(--lime); animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.35; } }
  .body { display: flex; gap: 14px; padding: 12px; }
  .art { width: 84px; height: 84px; flex: none; background: #101112 url("/scene/smoke.jpg") center / cover; position: relative; overflow: hidden; }
  .art img { width: 100%; height: 100%; object-fit: cover; filter: saturate(calc(0.85 - var(--denied) * 0.5)); }
  .art img.hide { display: none; }
  .ph { position: absolute; inset: 0; display: grid; place-items: center; }
  .meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .meta b { font-size: 17px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta span { font-size: 10px; font-weight: 500; letter-spacing: 0.6px; color: var(--mute); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ctl { margin-top: auto; display: flex; align-items: center; gap: 6px; }
  button { all: unset; cursor: pointer; width: 26px; height: 22px; display: grid; place-items: center; border: 1px solid var(--ink); color: var(--ink); transition: background 140ms, color 140ms; }
  button svg { fill: currentColor; }
  button:hover:not(:disabled) { background: var(--ink); color: var(--ink-inverse); }
  button:disabled { opacity: 0.3; cursor: default; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .pp { width: 34px; background: var(--ink); color: var(--ink-inverse); }
  .pp:hover:not(:disabled) { background: var(--orange); color: #111214; border-color: var(--orange); }
  .levels { margin-left: auto; display: flex; align-items: flex-end; gap: 3px; height: 18px; }
  .levels i { width: 3px; height: 4px; background: var(--accent); }
  .playing .levels i { animation: lvl 820ms ease-in-out infinite alternate; }
  @keyframes lvl { 0% { height: 3px; } 35% { height: 16px; } 60% { height: 7px; } 100% { height: 13px; } }
</style>
