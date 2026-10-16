<script lang="ts">
  import { lock } from "../lock.svelte";
  // Authorization key slab: each keystroke lands as a white cell; REQUEST is
  // a real button above the (invisible) input — the SDDM lesson: the input
  // only needs focus, it must never sit on top of anything clickable.
  let { input = $bindable() }: { input?: HTMLInputElement } = $props();
  const busy = $derived(lock.phase === "verifying" || lock.phase === "granted");
  const denied = $derived(lock.phase === "denied");
  let focused = $state(false);
  const PITCH = 21;
  let fieldW = $state(400);
  const fit = $derived(Math.max(1, Math.floor((fieldW - 46) / PITCH)));   // room for "+n" on the right
  const hidden = $derived(Math.max(0, lock.passwordLength - fit));
  // only the cells that fit are drawn: a pasted novel must not become a
  // million DOM nodes (the newest stays on the right, "+n" counts the rest)
  const cells = $derived(Array.from({ length: lock.passwordLength - hidden }, (_, i) => hidden + i));
  const shakeKey = $derived(lock.denials);

  // Standby: the input that wakes the terminal only wakes it.  App cancels a
  // waking keydown; these cover the paths without one — IME composition,
  // paste, on-screen keyboards — so nothing typed in standby reaches the key.
  let discarding = false;
  let discardInput = false;
  let discardTimer = 0;
  function onBeforeInput(e: InputEvent) {
    if (lock.phase !== "standby" || e.isComposing) return;
    e.preventDefault();
    // Some IME/virtual-keyboard edits cannot be canceled. Keep the wake
    // decision until their matching input event, even though activity wakes
    // the store immediately. Do not swallow a later intentional edit.
    if (!e.cancelable) {
      discardInput = true;
      clearTimeout(discardTimer);
      discardTimer = window.setTimeout(() => { discardInput = false; }, 0);
    }
    lock.activity();
  }
  function onCompositionStart() {
    // started in standby, or by the waking key itself (its keydown cannot
    // stop an IME): the whole composition is dropped when it ends
    if (lock.phase === "standby" || lock.wakeKey) { discarding = true; lock.activity(); }
  }
  function onCompositionEnd() {
    if (!discarding) return;
    // WebKit may deliver the final input after compositionend: keep
    // discarding until the event loop has moved on, then restore the field
    setTimeout(() => {
      discarding = false;
      if (input) input.value = lock.password;
    }, 0);
  }
  function onInput(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    if (discarding) return;
    if (discardInput) {
      discardInput = false;
      clearTimeout(discardTimer);
      el.value = lock.password;
      return;
    }
    if (lock.phase === "standby") { el.value = lock.password; lock.activity(); return; }
    // too long: refused whole and said so, never silently truncated
    if (!lock.setPassword(el.value)) el.value = lock.password;
  }
</script>

<div class="bar" class:focused class:denied>
  <input bind:this={input} type="password" class="sink" autocomplete="off" spellcheck="false" aria-label="authorization key"
         readonly={busy}
         onfocus={() => (focused = true)} onblur={() => (focused = false)}
         onbeforeinput={onBeforeInput} oncompositionstart={onCompositionStart} oncompositionend={onCompositionEnd}
         oninput={onInput} />
  {#key shakeKey}
  <div class="slab" class:shake={shakeKey > 0} role="presentation" onmousedown={(e) => { e.preventDefault(); input?.focus(); }}>
    <div class="field" bind:clientWidth={fieldW}>
      {#if lock.passwordLength === 0}
        <span class="ph">{lock.phase === "standby" ? "PRESS ANY KEY TO RESUME" : "ENTER AUTHORIZATION KEY"}</span>
      {/if}
      <div class="cells">
        {#each cells as i (i)}<i class="cell"></i>{/each}
        {#if focused && !busy}<i class="caret"></i>{/if}
      </div>
      {#if hidden > 0}<span class="more">+{hidden}</span>{/if}
    </div>
    <button class="req" class:busy disabled={busy} onmousedown={(e) => e.preventDefault()} onclick={() => { input?.focus(); lock.submit(); }}>
      <span>{busy ? "PENDING" : "REQUEST"}</span>
      <svg width="16" height="12" viewBox="0 0 16 12"><rect y="5" width="15" height="2" /><rect x="9" y="1" width="7" height="2" transform="rotate(45 9 2)" /><rect x="9" y="9" width="7" height="2" transform="rotate(-45 9 10)" /></svg>
    </button>
    <i class="focusline"></i>
  </div>
  {/key}
</div>

<style>
  .bar { position: relative; height: 60px; }
  .sink { position: absolute; inset: 0; opacity: 0; z-index: -1; border: 0; padding: 0; caret-color: transparent; }
  .slab { position: absolute; inset: 0; background: #111214; display: flex; align-items: stretch; cursor: text; }
  .denied .slab { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--orange) 80%, transparent); }
  .shake { animation: shake 310ms cubic-bezier(0.36, 0.07, 0.19, 0.97); }
  @keyframes shake { 15% { transform: translateX(-14px); } 35% { transform: translateX(12px); } 55% { transform: translateX(-8px); } 75% { transform: translateX(5px); } 100% { transform: none; } }
  .field { position: relative; flex: 1; margin: 0 18px 0 24px; overflow: hidden; display: flex; align-items: center; }
  .ph { position: absolute; left: 0; color: #77756f; font-size: 14px; font-weight: 500; letter-spacing: 3px; opacity: 0.7; }
  .focused .ph { opacity: 1; }
  .cells { display: flex; align-items: center; gap: 8px; transition: transform 120ms; }
  .cell { width: 13px; height: 13px; background: #f4f3f0; animation: arrive 260ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes arrive { 0% { transform: scale(0.2) rotate(45deg); background: var(--lime); } 40% { background: var(--lime); } 100% { transform: none; } }
  .caret { width: 3px; height: 28px; background: var(--accent); margin-left: -5px; animation: caret 1.02s steps(1) infinite; }
  @keyframes caret { 0%, 50% { opacity: 1; } 58%, 100% { opacity: 0; } }
  .more { position: absolute; right: 0; color: #77756f; font-size: 11px; }
  .req { all: unset; cursor: pointer; width: 120px; margin: 6px; display: flex; align-items: center; justify-content: space-between; padding: 0 16px 0 14px; box-sizing: border-box; background: #1d1e20; color: #f4f3f0; font-size: 13px; font-weight: 700; letter-spacing: 2px; transition: background 160ms, color 160ms; }
  .req svg { fill: var(--orange); transition: fill 160ms; }
  .req:hover, .req.busy { background: var(--orange); color: #111214; }
  .req.busy { cursor: default; }
  .req:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .req:hover svg, .req.busy svg { fill: #111214; }
  .focusline { position: absolute; left: 0; bottom: 0; height: 3px; width: 0; background: var(--accent); transition: width 420ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  .focused .focusline { width: 100%; }
</style>
