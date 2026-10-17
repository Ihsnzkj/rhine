<script lang="ts">
  import { onMount } from "svelte";
  import { Stage, type Quality } from "./lib/scene/Stage";
  import { lock } from "./lib/lock.svelte";
  import { audio } from "./lib/audio";
  import { native, log } from "./lib/bridge";
  import Header from "./lib/ui/Header.svelte";
  import Clock from "./lib/ui/Clock.svelte";
  import LeftRail from "./lib/ui/LeftRail.svelte";
  import SectionTile from "./lib/ui/SectionTile.svelte";
  import PersonnelPanel from "./lib/ui/PersonnelPanel.svelte";
  import Footer from "./lib/ui/Footer.svelte";
  import Subtitle from "./lib/ui/Subtitle.svelte";
  import Callouts from "./lib/ui/Callouts.svelte";
  import AuthRing from "./lib/ui/AuthRing.svelte";
  import Warning from "./lib/ui/Warning.svelte";
  import MediaCard from "./lib/ui/MediaCard.svelte";
  import BootOverlay from "./lib/ui/BootOverlay.svelte";
  import DecodeText from "./lib/ui/DecodeText.svelte";
  import Logo from "./lib/ui/Logo.svelte";
  import { SnapCard, easeInOutCubic, clamp01, smooth, type Rect } from "./lib/transition";

  const params = new URLSearchParams(location.search);
  let canvas: HTMLCanvasElement;
  let input = $state<HTMLInputElement>();
  let stage: Stage | null = null;

  // design canvas: 1920×1080 scaled to fit, extra space distributed like the greeter
  let vw = $state(innerWidth), vh = $state(innerHeight);
  const k = $derived(Math.min(vw / 1920, vh / 1080));
  const cw = $derived(vw / k), ch = $derived(vh / k);

  // eased blends (driven per frame)
  let denied = $state(0), verify = $state(0), standby = $state(0), glitch = 0;
  let verifyProgress = $state(0);
  let calloutReveal = $state(0);
  let boot = $state({ draw: 0, shift: 0, fade: 1, zoom: 0 });
  let exitFade = $state(0);            // no-screenshot exit only
  let exitUi = $state(0);              // 0..1 UI retract during the exit
  let tileReveal = $state(0);          // the SEALED SECTION tile (the card lands into it)
  let snapMode = $state<"pending" | "snap" | "plain">("pending");
  const snap = new SnapCard();
  let centre = $state({ x: 905, y: 520 }), anchorA = $state({ x: 1080, y: 290 }), anchorB = $state({ x: 600, y: 800 });
  let statsText = $state("");
  let statAcc = { n: 0, sum: 0, max: 0, t: 0 };
  let pointer = $state({ x: 0.5, y: 0.5 });

  const sessionCode = $derived(abbreviate(lock.init?.session ?? "niri"));
  const primary = $derived(lock.primary);

  function abbreviate(name: string) {
    const w = name.toUpperCase().replace(/[^A-Z0-9 ]+/g, " ").trim().split(/\s+/)[0] || "SYS";
    if (w.length <= 5) return w;
    let out = w[0];
    for (let i = 1; i < w.length && out.length < 5; ++i)
      if (!"AEIOU".includes(w[i]) && w[i] !== out[out.length - 1]) out += w[i];
    return out.length >= 3 ? out : w.slice(0, 4);
  }

  // ── entry ──
  // With a pre-lock screenshot (contract rev 3) the first frame is the user's
  // desktop: it shrinks into a specimen card and flies into the left column,
  // becoming the SEALED SECTION tile while the ∞ band is drawn behind it.
  // Without one, the mark is drawn on paper (shorter than before).
  let bootStart = 0, bootSkipped = false, entryT = 0, entrySpeed = 1, entryDone = false;
  // `ready`: input is live; `done`: every interval has finished (the last frame settles them all)
  const PLAIN = { draw: [0, 600], shift: [650, 950], assemble: 450, zoom: [800, 1300], reveal: [700, 1500], ready: 1300, done: 1500 };
  const SNAP = { flight: 1050, assemble: 120, ready: 600, reveal: [480, 1350], tile: [880, 1150], fade: [950, 1200], hide: 1250, done: 1350 };
  const outCubic = (x: number) => 1 - Math.pow(1 - x, 3);
  const bootCues = new Set<string>();
  function bootCue(name: string, at: number, t: number, fn?: () => void) {
    if (t >= at && !bootCues.has(name)) { bootCues.add(name); fn?.(); if (primary && name) audio.play(name); }
  }
  /** design-space tile rect → screen px (the card's landing pose) */
  function tileRect(): Rect {
    if (!primary) {  // secondary outputs: the card settles small, centred, and fades
      const w = vw * 0.3, h = vh * 0.3;
      return { x: (vw - w) / 2, y: (vh - h) / 2, w, h };
    }
    return { x: 300 * k, y: 337 * k, w: 376 * k, h: 263 * k };
  }
  function entryFrame(t: number) {
    if (snapMode === "snap") {
      bootCue("access", 0, t);
      bootCue("blip", 60, t);
      bootCue("assemble", SNAP.assemble, t, () => stage?.playAssemble(1900));
      bootCue("reveal", SNAP.reveal[0], t);
      const e = easeInOutCubic(clamp01(t / SNAP.flight));
      snap.pose(e, tileRect(), {
        opacity: 1 - smooth(SNAP.fade[0], SNAP.fade[1], t),
        scan: (t - 80) / 440,
        corners: 1 - smooth(560, 860, t),
        desat: smooth(60, 600, t),
        caption: "SNAPSHOT · SESSION SEALED " + (lock.init ? lock.clockText(lock.init.lockedAt) : ""),
        captionOpacity: smooth(150, 350, t) * (1 - smooth(760, 960, t)),
      });
      tileReveal = smooth(SNAP.tile[0], SNAP.tile[1], t);
      lock.uiReveal = outCubic(clamp01((t - SNAP.reveal[0]) / (SNAP.reveal[1] - SNAP.reveal[0])));
      if (t >= SNAP.ready) lock.setReadyFromBoot();
      if (t >= SNAP.hide) snap.hide();
      if (t >= SNAP.done) entryDone = true;
    } else {
      const span = (a: number, b: number, e = (x: number) => x) => e(clamp01((t - a) / (b - a)));
      bootCue("access", 0, t);
      bootCue("draw", 60, t);
      bootCue("assemble", PLAIN.assemble, t, () => stage?.playAssemble(1700));
      bootCue("reveal", PLAIN.reveal[0], t);
      boot = {
        draw: span(PLAIN.draw[0], PLAIN.draw[1], easeInOutCubic),
        shift: span(PLAIN.shift[0], PLAIN.shift[1], outCubic),
        fade: 1 - span(PLAIN.zoom[0], PLAIN.zoom[1], (x) => x * x),
        zoom: span(PLAIN.zoom[0], PLAIN.zoom[1], (x) => x * x * x),
      };
      lock.uiReveal = span(PLAIN.reveal[0], PLAIN.reveal[1], outCubic);
      tileReveal = clamp01(lock.uiReveal * 1.8 - 0.2);
      if (t >= PLAIN.ready) lock.setReadyFromBoot();
      if (t >= PLAIN.done) entryDone = true;
    }
  }
  /** a key during the entry fast-forwards it (×3) instead of cutting */
  function hurryBoot() { if (!entryDone) entrySpeed = 3; }
  /** screenshots / tests: jump straight to the settled state */
  function skipBoot() {
    if (bootSkipped) return;
    bootSkipped = true;
    entryDone = true;
    boot = { draw: 1, shift: 1, fade: 0, zoom: 1 };
    snap.hide();
    stage?.setAssembled();
    tileReveal = 1;
    lock.uiReveal = 1;
    lock.setReadyFromBoot();
  }

  // ── exit ──
  // Mirror of the entry: the tile turns back into the desktop card, which
  // grows to exactly full screen; only then does the backend unlock, so the
  // live desktop replaces an identical frame.
  let exitStart = 0;
  const exitCues = new Set<string>();
  function exitFrame(te: number) {
    const cue = (name: string, at: number, fn?: () => void) => {
      if (te >= at && !exitCues.has(name)) { exitCues.add(name); fn?.(); if (primary && name.indexOf(":") < 0) audio.play(name); }
    };
    cue("choose", 0);
    cue("x:dissolve", 220, () => stage?.playDissolve(800));
    exitUi = smooth(180, 600, te);
    if (snapMode === "snap") {
      cue("reveal", 560);
      const e = 1 - easeInOutCubic(clamp01((te - 560) / 660));
      snap.pose(e, tileRect(), {
        opacity: smooth(380, 600, te),
        corners: smooth(700, 950, te) * (1 - smooth(1120, 1220, te)),
        desat: 1 - smooth(600, 1150, te),
        caption: "PERMISSION AUTHORIZED · WELCOME BACK, " + lock.displayName.toUpperCase(),
        captionOpacity: smooth(600, 760, te) * (1 - smooth(1000, 1150, te)),
      });
      tileReveal = 1 - smooth(420, 620, te);
      cue("x:unlock", 1230, () => lock.unlockReady());
    } else {
      cue("reveal", 300);
      exitFade = smooth(300, 650, te);
      tileReveal = 1 - exitUi;
      cue("x:unlock", 1150, () => lock.unlockReady());
    }
  }

  // ── per-frame orchestration ──
  function follow(cur: number, target: number, rate: number, dt: number) {
    return cur + (target - cur) * (1 - Math.exp(-dt * rate));
  }
  let glitchUntil = 0, glitchPeak = 0;
  function frame(dt: number) {
    const now = performance.now();
    // the opening starts once the GPU has actually presented frames, so shader
    // compilation on the first frames can never eat part of it
    if (bootStart === 0 && snapMode !== "pending" && (!stage || stage.stats.frames >= 4)) bootStart = now;
    if (!entryDone && !bootSkipped && bootStart > 0) {
      entryT += dt * 1000 * entrySpeed;
      entryFrame(entryT);
    }
    if (lock.phase === "granted") {
      if (exitStart === 0) exitStart = now;
      exitFrame(now - exitStart);
    }
    if (params.has("stats") && stage) {
      const ms = stage.stats.ms;
      statAcc.n++; statAcc.sum += ms; statAcc.max = Math.max(statAcc.max, ms);
      if (now - statAcc.t > 1000) {
        statsText = `${(1000 / (statAcc.sum / statAcc.n)).toFixed(1)} fps · avg ${(statAcc.sum / statAcc.n).toFixed(1)} ms · max ${statAcc.max.toFixed(1)} ms · ${stage.quality}`;
        statAcc = { n: 0, sum: 0, max: 0, t: now };
      }
    }
    const ph = lock.phase;
    denied = follow(denied, ph === "denied" ? 1 : 0, 28, dt);       // a cut, hidden by the tear
    verify = follow(verify, ph === "verifying" || ph === "granted" ? 1 : 0, 6, dt) * (1 - exitUi);
    standby = follow(standby, ph === "standby" ? 1 : 0, 2.2, dt);
    verifyProgress = ph === "granted" ? follow(verifyProgress, 1, 10, dt)
      : ph === "verifying" ? follow(verifyProgress, 0.92, 1.4, dt) : 0;
    glitch = now < glitchUntil ? glitchPeak : follow(glitch, 0, 14, dt);
    calloutReveal = follow(calloutReveal, lock.uiReveal >= 0.99 && ph === "ready" ? 1 : 0, ph === "granted" ? 14 : 5, dt);

    const se = standby * standby * (3 - 2 * standby);
    document.documentElement.style.setProperty("--denied", denied.toFixed(4));
    document.documentElement.style.setProperty("--standby", se.toFixed(4));

    if (stage) {
      // a settled standby can sit there for hours: half the frames, same look
      stage.maxFps = ph === "standby" && se > 0.98 ? 30 : 0;
      stage.targets.denied = denied;
      stage.targets.verify = verify;
      stage.targets.standby = se;
      stage.targets.glitch = glitch;
      stage.targets.cardsReveal = lock.uiReveal * (1 - se) * (1 - exitUi);
      // damped parallax
      stage.targets.parallaxX = follow(stage.targets.parallaxX, (pointer.x - 0.5) * 2, 3, dt);
      stage.targets.parallaxY = follow(stage.targets.parallaxY, (pointer.y - 0.5) * 2, 3, dt);
      // live anchors: the visual crossing of the two strands, and two callout points
      const p = (u: number) => { const q = stage!.project(u); return { x: q.x / k, y: q.y / k }; };
      const p1 = p(0.225), p2 = p(0.275), q1 = p(0.725), q2 = p(0.775);
      const rx = p2.x - p1.x, ry = p2.y - p1.y, sx = q2.x - q1.x, sy = q2.y - q1.y, den = rx * sy - ry * sx;
      if (Math.abs(den) > 1e-3) {
        const a = ((q1.x - p1.x) * sy - (q1.y - p1.y) * sx) / den;
        centre = { x: p1.x + rx * a, y: p1.y + ry * a };
      }
      anchorA = p(0.65);
      anchorB = p(0.36);
    }
  }

  // ── input ──
  /** a real control other than the key field (Tab can reach these) */
  function isControl(t: EventTarget | null): t is HTMLElement {
    return t instanceof HTMLElement && t !== input && t.matches("button, a[href], [tabindex]");
  }
  /** Tab past either end wraps within the page: leaving it would hand focus
   *  to the GTK window, from where the key field takes it back, and the
   *  controls before the field (the media card) could never be reached */
  function wrapFocus(e: KeyboardEvent) {
    const all = [...document.querySelectorAll<HTMLElement>("input, button:not(:disabled), a[href], [tabindex]:not([tabindex='-1'])")]
      .filter((el) => el.getClientRects().length > 0);
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (!all.length || (e.shiftKey ? i > 0 : i >= 0 && i < all.length - 1)) return;
    e.preventDefault();
    all[e.shiftKey ? all.length - 1 : 0].focus();
  }
  function onKey(e: KeyboardEvent) {
    // A held key repeats its keydown; only a fresh press may act: a held F9
    // must never confirm its own power-off, a held Enter never re-submit, and
    // the key that woke standby keeps waking nothing.
    if (e.repeat && ((lock.wakeKey && (e.code || e.key) === lock.wakeKey) || e.key === "Enter" || e.key === "Escape" || /^F\d+$/.test(e.key))) {
      e.preventDefault();
      return;
    }
    const waking = lock.phase === "standby";
    lock.activity();
    hurryBoot();
    if (waking) {
      // The key that wakes the terminal only wakes it: cancelling the keydown
      // stops the browser's default action, so the character never reaches
      // the key field (and Enter/F-keys do nothing).  The window-level
      // listener runs after the focused input in the bubble phase, but default
      // actions only happen once dispatch has finished, so this is in time.
      // An IME composition the key starts anyway is dropped by PasswordBar.
      lock.wakeKey = e.code || e.key;
      e.preventDefault();
      e.stopPropagation();
      if (document.activeElement !== input) input?.focus();
      return;
    }
    lock.wakeKey = "";
    // while an IME composes, its keys are its own: Enter picks a candidate
    // (it must not submit a half-typed key), Esc cancels the composition
    if (e.isComposing || e.keyCode === 229) return;
    // keyboard navigation: Tab walks the controls, Enter/Space press the
    // focused one, Esc returns to the key field; typing always goes there
    if (e.key === "Tab") { wrapFocus(e); return; }
    if (isControl(e.target)) {
      if (e.key === "Enter" || e.key === " ") return;
      // Escape still cancels power/clears the key after returning focus.
    }
    // typing, Backspace/Delete and Escape go to the key field; a modifier or
    // F-key pressed on a control leaves focus there (Shift of Shift+Tab, F4)
    const edits = e.key.length === 1 || e.key === "Backspace" || e.key === "Delete" || e.key === "Escape";
    if (document.activeElement !== input && (edits || !isControl(e.target))) input?.focus();
    switch (e.key) {
      case "Enter": e.preventDefault(); lock.submit(); return;
      case "Escape": e.preventDefault(); if (lock.armed) lock.disarm(); else if (lock.phase === "ready") { lock.clearPassword(); if (input) input.value = ""; } return;
      case "F3": e.preventDefault(); lock.cycleLayout(); return;
      case "F4": e.preventDefault(); lock.toggleMute(); return;
      case "F7": e.preventDefault(); lock.arm("suspend"); return;
      case "F8": e.preventDefault(); lock.arm("reboot"); return;
      case "F9": e.preventDefault(); lock.arm("powerOff"); return;
    }
  }
  function onKeyUp(e: KeyboardEvent) {
    if (lock.wakeKey && (e.code || e.key) === lock.wakeKey) lock.wakeKey = "";
  }
  function onPointer(e: PointerEvent) {
    pointer = { x: e.clientX / vw, y: e.clientY / vh };
    if (Math.abs(e.movementX) + Math.abs(e.movementY) > 2) lock.activity();
  }

  // the key field holds focus whatever happens on screen — except while the
  // user has moved it to a real control (Tab, or a click on a button)
  $effect(() => {
    const el = input;
    if (!el) return;
    const keep = (e: FocusEvent) => {
      if (isControl(e.relatedTarget)) return;
      setTimeout(() => { if (document.activeElement !== el && !isControl(document.activeElement)) el.focus(); }, 0);
    };
    el.addEventListener("blur", keep);
    el.focus();
    return () => el.removeEventListener("blur", keep);
  });
  // the password field mirrors the store (cleared by deny / submit / Esc)
  $effect(() => { if (lock.passwordLength === 0 && input && input.value) input.value = ""; });

  // ── renderer ──
  // Its cost depends on `[frontend] quality`, which arrives with init (sent
  // right after hello), so it waits for init — briefly: until then the first
  // frame is the screenshot or the paper, painted by index.html.
  const INIT_WAIT = 800;
  let rendererStarted = false, unmounted = false, fallbackRaf = 0, initTimer = 0;
  function wantedQuality(): Quality {
    const q = params.get("q");
    if (q === "ultra" || q === "high" || q === "low") return q;
    const c = lock.init?.config?.quality;
    if (c !== undefined && !lock.quality) log("warn", `[frontend] quality = ${JSON.stringify(c)} is not ultra|high|low; using ultra`);
    return lock.quality ?? "ultra";
  }
  function startRenderer() {
    if (rendererStarted || unmounted) return;
    rendererStarted = true;
    clearTimeout(initTimer);
    try {
      stage = new Stage(canvas, wantedQuality());
      stage.onFrame = frame;
      if (bootSkipped) stage.setAssembled(); else stage.hideBand();   // the band is drawn by the entry
      stage.start();
    } catch (err) {
      // no WebGL: the lock still works on the paper background
      console.error("rhine: stage failed", err);
      stage = null;
      let last = performance.now();
      const loop = (t: number) => { frame(Math.min(0.05, (t - last) / 1000)); last = t; fallbackRaf = requestAnimationFrame(loop); };
      fallbackRaf = requestAnimationFrame(loop);
    }
    (window as any).__stage = stage;
  }
  $effect(() => { if (lock.init) startRenderer(); });

  onMount(() => {
    lock.hooks.pulse = (o) => stage?.pulse(o);
    lock.hooks.glitch = (s, ms) => { glitchPeak = s; glitchUntil = performance.now() + ms; };
    const stopLock = lock.start();
    snap.ready.then((ok) => (snapMode = ok ? "snap" : "plain"));
    initTimer = window.setTimeout(startRenderer, INIT_WAIT);
    if (params.get("boot") === "0" || (params.has("scenario") && params.get("scenario") !== "unlock")) skipBoot();
    (window as any).__lock = lock;
    if (!native && params.has("scenario")) runScenario(params.get("scenario")!);
    const onResize = () => { vw = innerWidth; vh = innerHeight; };
    addEventListener("resize", onResize);
    return () => {
      unmounted = true;
      clearTimeout(initTimer);
      cancelAnimationFrame(fallbackRaf);
      removeEventListener("resize", onResize);
      stopLock();
      stage?.dispose();
    };
  });

  // screenshot scenarios (mock backend only)
  function runScenario(s: string) {
    const typeIn = (txt: string, done?: () => void) => {
      let i = 0;
      const t = setInterval(() => { lock.setPassword(txt.slice(0, ++i)); if (i >= txt.length) { clearInterval(t); done?.(); } }, 140);
    };
    setTimeout(() => {
      switch (s) {
        case "unlock": break;
        case "typing": typeIn("rhinelab0"); break;
        case "verifying": typeIn("rhine-lab-1", () => lock.submit()); break;
        case "denied": typeIn("wrong", () => lock.submit()); break;
        case "granted": typeIn("rhine", () => lock.submit()); break;
        case "standby": lock.setPhase("standby"); break;
        case "armed": lock.arm("powerOff"); break;
      }
    }, 600);
    // natural entry, then the right key: records the whole lock → unlock arc
    if (s === "unlock") setTimeout(() => typeIn("rhine", () => lock.submit()), 2500);
  }
</script>

<svelte:window onkeydown={onKey} onkeyup={onKeyUp} onpointermove={onPointer} onpointerdown={() => { lock.activity(); input?.focus(); }} />

<canvas bind:this={canvas}></canvas>

<div class="design" class:hidden={params.has("noui")} style:width="{cw}px" style:height="{ch}px" style:transform="scale({k})">
  <div class="ui" style:opacity={1 - exitFade}>
    <div class="hdr" style:opacity={Math.min(1, lock.uiReveal * 2.2) * (1 - exitUi)}><Header /></div>

    {#if primary}
      <div class="rail" style:opacity={(1 - standby) * (1 - exitUi)}>
        <LeftRail reveal={lock.uiReveal}
          label={lock.phase === "denied" ? "DEFENSE SECTION · LOCKDOWN" : `${(lock.init?.session ?? "SESSION").toUpperCase()} · SEALED SECTION`} />
      </div>
      <div class="tile" style:opacity={tileReveal * (1 - Math.max(0, standby - 0.85) / 0.15)}
           style:transform="translateX({-standby * standby * (3 - 2 * standby) * 820}px)">
        <SectionTile code={sessionCode} caption="{(lock.init?.session ?? 'session').toUpperCase()} · SEALED SECTION" tab="LOCKED" />
      </div>
      <div class="media" style:opacity={Math.min(1, Math.max(0, lock.uiReveal * 1.8 - 0.5)) * (1 - exitUi)}
           style:transform="translateX({(1 - Math.min(1, lock.uiReveal * 1.4)) * -60 - standby * standby * (3 - 2 * standby) * 820}px)">
        <MediaCard />
      </div>
      <Callouts a={anchorA} b={anchorB} {centre} reveal={calloutReveal * (1 - exitUi)}
        detailA="{(lock.init?.host ?? 'localhost').toUpperCase()} · LOCAL ACCESS · PAM AUTHORITY"
        detailB="LOCKED {lock.lockedFor} · {lock.init?.outputs ?? 1} DISPLAY{(lock.init?.outputs ?? 1) > 1 ? 'S' : ''} SEALED" />
    {/if}

    <div class="panelwrap" style:left="{cw - 680 - 64}px"
         style:transform="translateX({Math.max(standby * standby * (3 - 2 * standby), exitUi * exitUi * exitUi) * 960}px) rotateY({-6 - (pointer.x - 0.5) * 4.4}deg) rotateX({(pointer.y - 0.5) * 2.8}deg)"
         style:opacity={1 - Math.max(0, Math.max(standby, exitUi) - 0.85) / 0.15}>
      <PersonnelPanel reveal={lock.uiReveal} code={sessionCode} bind:input />
    </div>

    <div class="clock" style:right="64px" style:opacity={Math.min(1, lock.uiReveal * 2) * (1 - exitUi)}><Clock /></div>
    {#if primary}<div class="footer" style:opacity={Math.min(1, lock.uiReveal * 2) * (1 - standby) * (1 - exitUi)}><Footer /></div>{/if}
    <div class="subtitle" style:opacity={Math.min(1, lock.uiReveal * 2) * (1 - exitUi)}><Subtitle zh={lock.subtitle[0]} en={lock.subtitle[1]} /></div>

    <div class="anchor" style:left="{centre.x}px" style:top="{centre.y}px">
      <AuthRing active={verify} progress={verifyProgress}
        label={lock.phase === "granted" ? "PERMISSION AUTHORIZED" : "VERIFYING IDENTITY"} />
      <Warning active={denied} attempts={lock.attempts} replay={lock.denials}
        reason={lock.failReason} accepted={lock.keyAccepted} />
    </div>

    {#if standby > 0.01}
      <div class="standby" style:opacity={standby * (0.55 + 0.45 * Math.sin(performance.now() / 455))}>
        <DecodeText text="ACCESS PERMISSION REQUIRED  ·  PRESS ANY KEY" />
      </div>
    {/if}
  </div>

  {#if snapMode === "plain"}
    <BootOverlay draw={boot.draw} shift={boot.shift} fade={boot.fade} zoom={boot.zoom}
      name={lock.displayName.toUpperCase()} at={lock.init ? lock.clockText(lock.init.lockedAt) : ""} />
  {/if}

  {#if params.has("stats")}<div class="stats">{statsText}</div>{/if}

  {#if snapMode !== "snap" && lock.phase === "granted"}
    <div class="exit" style:opacity={exitFade}>
      <Logo width={150} caption />
      <div class="welcome"><DecodeText text={"WELCOME BACK, " + lock.displayName.toUpperCase()} /></div>
      <div class="auth"><DecodeText text="PERMISSION AUTHORIZED · SESSION RESTORED" /></div>
    </div>
  {/if}
</div>

<style>
  canvas { position: fixed; inset: 0; width: 100vw; height: 100vh; display: block; }
  .design { position: fixed; left: 0; top: 0; transform-origin: 0 0; perspective: 2200px; }
  .ui { position: absolute; inset: 0; }
  .design.hidden { display: none; }
  .ui > div, .design > div { position: absolute; }
  .hdr { left: 60px; top: 112px; }
  .rail { left: 0; top: 0; }
  .tile { left: 300px; top: 337px; }
  .media { left: 300px; top: 640px; }
  .panelwrap { top: 262px; transform-origin: 0 318px; transform-style: preserve-3d; }
  .clock { top: 54px; }
  .footer { right: 60px; top: 936px; }
  .subtitle { left: 0; right: 120px; top: 1000px; display: flex; justify-content: center; }
  .anchor { width: 0; height: 0; }
  .standby { left: 0; right: 0; top: 900px; text-align: center; font-size: 12px; font-weight: 500; letter-spacing: 4px; color: var(--ink); }
  .stats { left: 12px; top: 8px; font-family: var(--mono); font-size: 13px; background: #000c; color: #b9e432; padding: 4px 8px; z-index: 99; }
  .exit { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 22px; color: var(--ink); }
  .welcome { font-size: 26px; font-weight: 600; letter-spacing: 6px; }
  .auth { font-size: 11px; font-weight: 500; letter-spacing: 3px; color: var(--mute); }
</style>
