<script lang="ts">
  import { lock } from "../lock.svelte";
  import Logo from "./Logo.svelte";
  import SectionTile from "./SectionTile.svelte";
  import PasswordBar from "./PasswordBar.svelte";
  // The lock form staged as a Rhine Lab personnel file (film 1:52): a frosted
  // acrylic slab over the live 3D scene, photo, rank tag, name bar, file card,
  // profile table — and the authorization key bar every file ends with.
  let { reveal = 1, code = "SYS", input = $bindable() }: { reveal?: number; code?: string; input?: HTMLInputElement } = $props();
  const st = (i: number) => Math.max(0, Math.min(1, reveal * 1.6 - i * 0.1));
  const name = $derived((lock.init?.user.realName || lock.init?.user.name || "OPERATOR").toUpperCase());
  const busy = $derived(lock.phase === "verifying" || lock.phase === "granted");
  let avatarOk = $state(false);
  const SIL = "M 50 14 C 63 14 71 24 71 38 C 71 50 66 60 59 65 L 60 72 C 72 75 86 80 93 90 C 99 99 101 112 101 124 L -1 124 C -1 112 1 99 7 90 C 14 80 28 75 40 72 L 41 65 C 34 60 29 50 29 38 C 29 24 37 14 50 14 Z";
  const msg = $derived(lock.message || (lock.capsLock ? "CAPS LOCK ENGAGED — KEY IS CASE-SENSITIVE" : lock.pamInfo.toUpperCase()));
  const msgErr = $derived(lock.message ? lock.messageIsError : lock.capsLock);
  const pad = (n: number) => String(n).padStart(2, "0");
</script>

<div class="panel">
  <div class="rear" style:opacity={st(0)}></div>
  <div class="glass" style:opacity={st(0)}>
    <div class="hex"></div>
    <div class="ghost-name">{name}</div>
    <div class="ghost-info">INFO</div>
    <i class="bevel t"></i><i class="bevel l"></i><i class="bevel b"></i><i class="bevel r"></i>
  </div>

  <!-- header tags -->
  <div class="tags-l" style:opacity={st(1)}><i></i>SECTION PROFILE<span class="slot"></span></div>
  <div class="tags-r" style:opacity={st(1)}>
    <span class="outline">GENERAL ACCESS CONTENT</span>
    <span class="llc">RHINE LAB.LLC.</span>
  </div>

  <!-- photograph -->
  <div class="photo" style:opacity={st(2)} style:transform="translateY({(1 - st(2)) * 24}px)">
    <div class="film">
      {#if lock.init?.user.avatar}
        <img src={lock.init.user.avatar} alt="" onload={() => (avatarOk = true)} onerror={() => (avatarOk = false)} class:hide={!avatarOk} />
      {/if}
      {#if !avatarOk}
        <svg class="sil" viewBox="0 0 100 124" preserveAspectRatio="none"><defs>
            <pattern id="ht" width="3.2" height="3.2" patternUnits="userSpaceOnUse"><circle cx="1.6" cy="1.6" r="0.75" fill="#ffffff" fill-opacity="0.16" /></pattern></defs>
          <path d={SIL} fill="#141516" /><path d={SIL} fill="url(#ht)" /></svg>
        <span class="classified">PHOTO CLASSIFIED</span>
      {/if}
      {#if busy}<i class="scan"></i>{/if}
    </div>
    <div class="badge"><Logo width={22} color="var(--ink-inverse)" /></div>
    <i class="tick" style="right:-3px; top:-3px"></i><i class="tick" style="left:-3px; bottom:-3px"></i><i class="tick" style="right:-3px; bottom:-3px"></i>
  </div>

  <!-- identity -->
  <div class="identity" style:opacity={st(3)} style:transform="translateX({(1 - st(3)) * 40}px)">
    <div class="rank"><span class="op">OPERATOR</span><span class="state" class:alert={lock.phase === "denied"}>{lock.phase === "denied" ? "ACCESS DENIED" : lock.phase === "granted" ? "ACCESS GRANTED" : "SESSION LOCKED"}</span></div>
    <div class="name"><span>{name}</span></div>
    <div class="file">
      <SectionTile {code} width={92} height={60} showTab={false} />
      <div class="fileinfo">
        <div class="loaded">PERSONAL INFORMATION LOADED <i></i></div>
        <div class="mono">■ {code}-{pad(lock.attempts + 1)}&nbsp;&nbsp; LOCKED {lock.lockedFor}</div>
      </div>
      <div class="filelogo"><Logo width={64} color="color-mix(in srgb, var(--orange) 85%, transparent)" caption /></div>
    </div>
  </div>

  <!-- status rings -->
  <div class="rings" style:opacity={st(4)}>
    {#each [{ l: "CAPS", on: lock.capsLock, warn: true }, { l: "NUM", on: lock.numLock, warn: false }, { l: "KEY", on: lock.passwordLength > 0, warn: false }] as r}
      <div class="ring"><i class:on={r.on} class:warn={r.warn && r.on}></i><span>{r.l}</span></div>
    {/each}
  </div>

  <!-- profile table -->
  <div class="profile" style:opacity={st(4)} style:transform="translateY({(1 - st(4)) * 20}px)">
    <div class="ph">PROFILE</div>
    <dl>
      <dt>HOST</dt><dd>{(lock.init?.host ?? "LOCAL").toUpperCase()}</dd>
      <dt>SESSION</dt><dd>{(lock.init?.session ?? "—").toUpperCase()}</dd>
      <dt>LOCKED AT</dt><dd>{lock.init ? lock.clockText(lock.init.lockedAt) : "—"}</dd>
      <dt>KEYMAP</dt><dd>{(lock.layout || "—").toUpperCase()}</dd>
    </dl>
    <i class="rule"></i><span class="foot">RHINE LAB LLC.</span>
  </div>

  <!-- authorization -->
  <div class="auth" style:opacity={st(5)} style:transform="translateY({(1 - st(5)) * 24}px)">
    <div class="authhead">
      <span><i></i>AUTHORIZATION KEY</span>
      <span class="attempt" class:hot={lock.attempts > 0}>ATTEMPT {pad(lock.attempts + 1)} · LEVEL-A</span>
    </div>
    <PasswordBar bind:input />
    <div role="status" aria-live="polite">{#if msg}<div class="msg" class:err={msgErr}><i></i>{msg}</div>{/if}</div>
  </div>
</div>

<style>
  .panel { position: relative; width: 680px; height: 636px; color: var(--ink); }
  .rear { position: absolute; inset: 0; transform: translate(18px, -18px); background: color-mix(in srgb, var(--card) 22%, transparent); border: 1px solid color-mix(in srgb, var(--card) calc(75% - var(--denied) * 45%), transparent); }
  .glass { position: absolute; inset: 0; overflow: hidden;
    -webkit-backdrop-filter: blur(26px) saturate(0.75) brightness(1.03); backdrop-filter: blur(26px) saturate(0.75) brightness(1.03);
    background: linear-gradient(180deg, color-mix(in srgb, var(--card) 62%, transparent), color-mix(in srgb, var(--card) 50%, transparent) 55%, color-mix(in srgb, var(--card) 42%, transparent));
    box-shadow: 10px 18px 46px color-mix(in srgb, #3b3226 calc(22% - var(--denied) * 10%), transparent); }
  .hex { position: absolute; right: 0; bottom: 0; width: 62%; height: 55%; background: url("/scene/hex.png"); opacity: calc(0.55 - var(--denied) * 0.35);
    -webkit-mask-image: linear-gradient(90deg, transparent, #000 70%); mask-image: linear-gradient(90deg, transparent, #000 70%); }
  .ghost-name { position: absolute; left: 286px; top: -26px; font-size: 112px; font-weight: 700; letter-spacing: 2px; color: color-mix(in srgb, var(--ink) 6%, transparent); white-space: nowrap; }
  .ghost-info { position: absolute; left: 22px; top: 346px; font-size: 86px; font-weight: 700; letter-spacing: 4px; color: rgba(255, 255, 255, calc(0.78 - var(--denied) * 0.7)); }
  .bevel { position: absolute; display: block; }
  .bevel.t { left: 0; right: 0; top: 0; height: 2px; background: rgba(255, 255, 255, calc(0.9 - var(--denied) * 0.75)); }
  .bevel.l { left: 0; top: 0; bottom: 0; width: 2px; background: rgba(255, 255, 255, calc(0.75 - var(--denied) * 0.6)); }
  .bevel.b { left: 0; right: 0; bottom: 0; height: 1px; background: color-mix(in srgb, var(--ink) 12%, transparent); }
  .bevel.r { right: 0; top: 0; bottom: 0; width: 1px; background: color-mix(in srgb, var(--ink) 10%, transparent); }

  .tags-l { position: absolute; left: 28px; top: 26px; display: flex; align-items: center; gap: 8px; font-size: 9.5px; font-weight: 600; letter-spacing: 1.2px; }
  .tags-l i { width: 4px; height: 4px; background: var(--ink); }
  .slot { width: 120px; height: 9px; background: color-mix(in srgb, var(--card) 90%, transparent); border: 1px solid var(--faint); }
  .tags-r { position: absolute; right: 28px; top: 20px; display: flex; flex-direction: column; align-items: flex-end; gap: 4px; }
  .outline { font-size: 8px; letter-spacing: 0.8px; border: 1px solid color-mix(in srgb, var(--ink) 50%, transparent); padding: 2px 7px; }
  .llc { background: var(--ink); color: var(--ink-inverse); font-size: 10.5px; font-weight: 700; letter-spacing: 1px; padding: 3px 8px; }

  .photo { position: absolute; left: 30px; top: 70px; width: 232px; height: 282px; background: var(--card); border: 1px solid color-mix(in srgb, var(--ink) 8%, transparent); box-shadow: 0 10px 28px -6px color-mix(in srgb, #2c2418 28%, transparent); }
  .film { position: absolute; inset: 9px; overflow: hidden; background: linear-gradient(180deg, color-mix(in srgb, #ecebe7, #1b1c1e calc(var(--denied) * 100%)), color-mix(in srgb, #d5d1c8, #0f1011 calc(var(--denied) * 100%))); }
  .film img { width: 100%; height: 100%; object-fit: cover; filter: saturate(calc(0.88 - var(--denied) * 0.5)) brightness(calc(1 - var(--denied) * 0.2)); }
  .film img.hide { display: none; }
  .sil { position: absolute; inset: 0; width: 100%; height: 100%; }
  .classified { position: absolute; bottom: 12px; left: 50%; transform: translateX(-50%); background: var(--orange); color: #111214; font-size: 9px; font-weight: 700; letter-spacing: 1.4px; padding: 3px 7px; white-space: nowrap; }
  .scan { position: absolute; left: 0; right: 0; height: 2px; background: var(--orange); opacity: 0.85; animation: scan 900ms ease-in-out infinite alternate; }
  @keyframes scan { from { top: 0; } to { top: calc(100% - 2px); } }
  .badge { position: absolute; left: -8px; top: -8px; width: 30px; height: 30px; background: var(--ink); display: grid; place-items: center; }
  .tick { position: absolute; width: 16px; height: 16px; }
  .tick::before, .tick::after { content: ""; position: absolute; background: color-mix(in srgb, var(--ink) 45%, transparent); }
  .tick::before { left: 7.5px; top: 0; width: 1px; height: 16px; }
  .tick::after { top: 7.5px; left: 0; height: 1px; width: 16px; }

  .identity { position: absolute; left: 286px; top: 86px; right: 0; }
  .rank { display: flex; gap: 10px; align-items: center; }
  .op { background: var(--ink); color: var(--ink-inverse); font-size: 16px; font-weight: 500; letter-spacing: 1.2px; padding: 6px 11px; }
  .state { font-family: var(--mono); font-size: 10px; letter-spacing: 1px; color: var(--mute); border: 1px solid var(--faint); padding: 3px 6px; }
  .state.alert { color: var(--orange); border-color: var(--orange); }
  .name { margin-top: 10px; height: 78px; display: inline-flex; align-items: center; background: var(--ink); padding: 0 32px 0 24px; min-width: 260px; max-width: 428px; }
  .name span { color: var(--ink-inverse); font-size: 48px; font-weight: 700; letter-spacing: 1.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding-top: 4px; }
  .file { position: relative; margin-top: 12px; width: 352px; height: 84px; background: var(--card); border: 1px solid color-mix(in srgb, var(--ink) 6%, transparent); display: flex; align-items: center; gap: 14px; padding-left: 12px; }
  .fileinfo { display: flex; flex-direction: column; gap: 7px; }
  .loaded { font-size: 11px; font-weight: 500; letter-spacing: 0.6px; display: flex; align-items: center; gap: 6px; }
  .loaded i { width: 6px; height: 6px; background: var(--ink); }
  .mono { font-family: var(--mono); font-size: 12.5px; font-weight: 600; }
  .filelogo { position: absolute; right: -18px; top: -16px; }

  .rings { position: absolute; left: 40px; top: 446px; display: flex; gap: 16px; }
  .ring { display: flex; flex-direction: column; align-items: center; gap: 5px; font-family: var(--mono); font-size: 9px; color: var(--mute); }
  .ring i { width: 24px; height: 24px; border-radius: 50%; border: 3px solid var(--accent); transition: background 160ms; }
  .ring i.on { background: var(--accent); }
  .ring i.warn { background: var(--orange); border-color: var(--orange); }

  .profile { position: absolute; right: 30px; top: 330px; width: 318px; height: 160px; background: var(--card); box-shadow: 0 6px 18px -6px color-mix(in srgb, #2c2418 16%, transparent); }
  .ph { margin: 10px 10px 0; background: var(--ink); color: var(--ink-inverse); font-size: 10.5px; font-weight: 700; letter-spacing: 1px; padding: 3px 6px; }
  dl { margin: 10px 14px 0; display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; }
  dt { font-size: 13px; font-weight: 500; color: var(--mute); }
  dd { margin: 0; font-size: 14px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-variant-numeric: tabular-nums; }
  .profile .rule { position: absolute; left: 14px; right: 14px; bottom: 28px; height: 1.5px; background: var(--ink); }
  .profile .foot { position: absolute; right: 14px; bottom: 10px; font-size: 9px; color: var(--mute); letter-spacing: 1px; }

  .auth { position: absolute; left: 30px; top: 506px; width: 620px; }
  .authhead { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; font-size: 12px; font-weight: 700; letter-spacing: 3px; }
  .authhead i { display: inline-block; width: 14px; height: 4px; background: var(--ink); margin-right: 10px; vertical-align: middle; }
  .attempt { font-family: var(--mono); font-size: 10.5px; font-weight: 400; letter-spacing: 0.5px; color: var(--mute); }
  .attempt.hot { color: var(--orange); }
  .msg { margin-top: 12px; display: flex; align-items: center; gap: 8px; font-size: 11.5px; font-weight: 600; letter-spacing: 1.6px; }
  .msg i { width: 7px; height: 7px; background: var(--ink); }
  .msg.err { color: var(--orange-deep); }
  .msg.err i { background: var(--orange); }
</style>
