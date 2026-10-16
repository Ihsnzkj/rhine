// Lock-screen state machine.  The backend decides *whether* to unlock; this
// decides how every state looks and sounds.
//
//   boot ─► ready ⇄ verifying ─► denied ─► ready
//                       └──────► granted ─► (exit animation) ─► unlock-ready
//   ready ⇄ standby
import { native, onMessage, send } from "./bridge";
import { audio } from "./audio";
import type { Init, MediaPlayer, PowerAction } from "./protocol";
import type { Quality } from "./scene/Stage";

export type Phase = "boot" | "ready" | "verifying" | "denied" | "granted" | "standby";

/** idle time before standby; `[frontend] standby_seconds` in the config (0 disables) */
let standbyAfter = 45_000;
/** the backend refuses longer keys; the field refuses them first, visibly */
export const MAX_KEY_BYTES = 4096;
const DENIAL_HOLD = Number(new URLSearchParams(location.search).get("denyhold") ?? 2600);

function pad(n: number, w = 2) { return String(n).padStart(w, "0"); }
const encoder = new TextEncoder();
export const keyBytes = (v: string) => encoder.encode(v).length;

class Lock {
  init = $state<Init | null>(null);
  phase = $state<Phase>("boot");
  attempts = $state(0);
  /** bumps once per denial: replay key for the WARNING plate and the slab shake */
  denials = $state(0);
  /** rev 2: the PAM step that failed last ("account"/"credentials" = key was accepted) */
  failStage = $state<"start" | "authenticate" | "account" | "credentials" | "">("");
  capsLock = $state(false);
  numLock = $state(false);
  layout = $state("");
  layouts = $state<string[]>([]);
  media = $state<MediaPlayer | null>(null);
  battery = $state<{ percent: number; charging: boolean } | null>(null);
  message = $state("");
  messageIsError = $state(false);
  pamInfo = $state("");
  passwordLength = $state(0);
  lastDelta = $state(0);
  armed = $state<PowerAction | "">("");
  /** the user's F4 choice; secondary outputs are silent regardless */
  muted = $state(false);
  /** event.code of the key that woke standby, until it is released: its
   *  repeats and an IME composition it starts never reach the key field */
  wakeKey = "";
  log = $state<{ id: number; text: string }[]>([]);
  now = $state(Date.now());
  /** blends, animated in App's frame loop */
  uiReveal = $state(0);
  exiting = $state(false);

  password = "";                       // never reactive, never logged
  private logId = 0;
  private denyTimer = 0;
  private idleTimer = 0;
  private armTimer = 0;
  private messageTimer = 0;
  private verifyStarted = 0;
  private interacted = false;
  private mediaKnown = false;
  private musicPending = 0;            // fade (s) of a music start waiting for the media state
  hooks = {
    pulse: (_origin?: number) => {},
    glitch: (_strength: number, _ms: number) => {},
    assemble: () => {},
  };

  readonly displayName = $derived(this.init ? (this.init.user.realName || this.init.user.name) : "OPERATOR");
  readonly primary = $derived(this.init?.output.primary ?? true);

  /** `[frontend] quality`, or null when absent/invalid */
  get quality(): Quality | null {
    const q = this.init?.config?.quality;
    return q === "ultra" || q === "high" || q === "low" ? q : null;
  }

  start() {
    const off = onMessage((m) => {
      switch (m.type) {
        case "init": return this.onInit(m);
        case "auth": return this.onAuth(m.state, m.attempts, m.message, m.stage);
        case "pam": this.pamInfo = m.text; this.addLog(`PAM · ${m.text}`); return;
        case "keyboard":
          if (this.init && m.capsLock !== this.capsLock) audio.play("toggle");
          this.capsLock = m.capsLock; this.numLock = m.numLock;
          this.layout = m.layout; this.layouts = m.layouts;
          return;
        case "media": {
          const was = this.media?.title;
          this.media = m.player;
          if (m.player && m.player.title !== was) this.addLog(`AUDIO FEED · ${m.player.title}`);
          if (m.player?.playing) audio.music(false, 1.2);   // never fight the user's own music
          this.mediaSettled();
          return;
        }
        case "battery": this.battery = m.battery; return;
        case "unlocking": this.exiting = true; return;
      }
    });
    send({ type: "hello", protocol: 1 });
    const tick = setInterval(() => (this.now = Date.now()), 1000);
    return () => {
      off();
      clearInterval(tick);
      for (const t of [this.denyTimer, this.idleTimer, this.armTimer, this.messageTimer]) clearTimeout(t);
    };
  }

  private onInit(i: Init) {
    this.init = i;
    const c = i.config ?? {};
    if (typeof c.standby_seconds === "number" && Number.isFinite(c.standby_seconds) && c.standby_seconds >= 0)
      standbyAfter = Math.min(c.standby_seconds * 1000, 2_000_000_000);   // 0 = never; setTimeout caps at 2^31-1
    audio.init({ sfx: c.sfx_volume, music: c.music_volume, musicEnabled: c.music !== false });
    // one voice per machine: secondary monitors stay silent, whatever F4 says
    audio.setMuted(this.muted || !i.output.primary);
    // Initial capabilities can arrive before the first MPRIS poll. Only an
    // actual media snapshot establishes whether optional music may start.
    this.addLog(`LINK ESTABLISHED · ${i.host.toUpperCase()}`);
    this.addLog(`SESSION ${i.session.toUpperCase()} SEALED · ${i.outputs} DISPLAY${i.outputs > 1 ? "S" : ""}`);
    this.addLog(`LOCKED AT ${this.clockText(i.lockedAt)}`);
    if (!native) this.addLog("MOCK AUTHORITY · KEY = RHINE");
  }

  private mediaSettled() {
    this.mediaKnown = true;
    if (this.musicPending) {
      const s = this.musicPending;
      this.musicPending = 0;
      this.startMusic(s);
    }
  }
  private startMusic(seconds: number) {
    if (this.phase === "standby" || this.phase === "granted" || !this.primary || this.muted) {
      this.musicPending = 0;
      return;
    }
    if (!this.mediaKnown) { this.musicPending = seconds; return; }
    if (!this.media?.playing) audio.music(true, seconds);
  }

  // ── auth ──────────────────────────────────────────────────────────
  submit() {
    if (this.phase === "boot") this.setReadyFromBoot();
    if (this.phase === "standby") this.wake();
    if (this.phase !== "ready") return;
    // an empty key is never sent: it cannot be right, but PAM would still
    // count it towards pam_faillock
    if (!this.password) { this.say("ENTER YOUR AUTHORIZATION KEY FIRST"); audio.play("error"); return; }
    this.message = "";
    this.pamInfo = "";
    this.addLog(`REQUEST · ${this.init?.user.name ?? "operator"} · KEY ${"■".repeat(Math.min(this.password.length, 12))}`);
    audio.play("request");
    this.hooks.glitch(0.22, 160);
    const pw = this.password;
    this.password = "";
    this.passwordLength = 0;
    send({ type: "authenticate", password: pw });
  }

  private onAuth(state: string, attempts: number, message?: string, stage?: Lock["failStage"]) {
    this.attempts = attempts;
    if (state === "verifying") {
      this.verifyStarted = performance.now();
      this.setPhase("verifying");
    } else if (state === "failure") {
      this.failStage = stage ?? "authenticate";
      this.deny(message);
    } else if (state === "success") {
      this.grant();
    } else if (state === "idle" && this.phase === "verifying") {
      this.setPhase("ready");
    }
  }

  private deny(message?: string) {
    this.denials++;
    this.setPhase("denied");
    this.password = "";
    this.passwordLength = 0;
    audio.play("warning");
    audio.play("glitch");
    this.hooks.glitch(1, 300);
    this.hooks.pulse(0.5);
    this.addLog(this.keyAccepted
      ? `KEY ACCEPTED · ${this.failStage.toUpperCase()} REFUSED · ${(message ?? "").toUpperCase()}`
      : `DENIED · ATTEMPT ${pad(this.attempts)}${message ? " · " + message.toUpperCase() : ""}`);
    clearTimeout(this.denyTimer);
    this.denyTimer = window.setTimeout(() => this.recover(), DENIAL_HOLD);
    this.pendingMessage = message ?? "";
  }
  private pendingMessage = "";

  recover() {
    if (this.phase !== "denied") return;
    clearTimeout(this.denyTimer);
    this.setPhase("ready");
    audio.play("recover");
    this.hooks.glitch(0.45, 260);
    const n = this.attempts;
    if (this.keyAccepted) {
      // the key was right; the system refused the session — say so plainly
      this.say(`KEY ACCEPTED — SYSTEM REFUSED (PAM ${this.failStage.toUpperCase()}: ${this.pendingMessage.toUpperCase() || "DENIED"})`, true);
      return;
    }
    this.say(this.pendingMessage && !/^authentication fail/i.test(this.pendingMessage)
      ? this.pendingMessage.toUpperCase()
      : `KEY REJECTED · ${n} FAILED ATTEMPT${n === 1 ? "" : "S"} ON RECORD`, true);
  }

  private grant() {
    this.setPhase("granted");
    this.addLog("PERMISSION AUTHORIZED");
    audio.music(false, 1.0);
    // the exit animation (App) ends on the desktop frame and calls unlockReady();
    // the backend's 1600 ms cap is only a safety net
  }

  private unlockSent = false;
  unlockReady() {
    if (this.unlockSent) return;
    this.unlockSent = true;
    send({ type: "unlock-ready" });
  }

  // ── phases ────────────────────────────────────────────────────────
  setPhase(p: Phase) {
    if (p === this.phase) return;
    const was = this.phase;
    this.phase = p;
    if (!native) console.log("PHASE", p);
    audio.hold("process", p === "verifying");
    audio.duck(p === "denied" ? 0.3 : p === "verifying" ? 0.62 : 1);
    if (p === "standby") { audio.play("standby"); audio.music(false, 3); }
    if (was === "standby") audio.play("wake");
    this.touchIdle();
  }

  /** the entry animation keeps running; input is live from here on */
  setReadyFromBoot() {
    if (this.phase === "boot") this.setPhase("ready");
  }

  wake() {
    if (this.phase === "standby") this.setPhase("ready");
  }

  /** any real user input */
  activity() {
    audio.wake();
    if (!this.interacted) {
      this.interacted = true;
      this.startMusic(3.5);
    } else if (this.phase !== "granted" && this.phase !== "standby") {
      this.startMusic(2.0);
    }
    if (this.phase === "standby") this.wake();
    if (this.phase === "denied") this.recover();
    this.touchIdle();
  }

  private touchIdle() {
    clearTimeout(this.idleTimer);
    if (standbyAfter <= 0) return;
    this.idleTimer = window.setTimeout(() => {
      if (this.phase === "ready" && this.passwordLength === 0) {
        this.setPhase("standby");
        this.addLog("TERMINAL STANDBY");
      } else this.touchIdle();
    }, standbyAfter);
  }

  // ── typing ────────────────────────────────────────────────────────
  /** false (and nothing changes) when the key would exceed MAX_KEY_BYTES */
  setPassword(v: string) {
    if (keyBytes(v) > MAX_KEY_BYTES) {
      this.say(`KEY TOO LONG · ${MAX_KEY_BYTES} BYTES MAXIMUM`, true);
      audio.play("error");
      return false;
    }
    const n = [...v].length;             // characters, not UTF-16 units
    const delta = n - this.passwordLength;
    this.password = v;
    this.lastDelta = delta;
    this.passwordLength = n;
    if (!native) console.log("KEYLEN", n);      // mock/dev builds only — length, never content
    if (delta !== 0) {
      audio.play(delta < 0 ? "erase" : "key");
      if (delta > 0) this.hooks.pulse(0.25 + Math.random() * 0.1);
    }
    return true;
  }
  clearPassword(logIt = true) {
    if (!this.password) return;
    this.password = "";
    this.passwordLength = 0;
    if (logIt) this.addLog("KEY BUFFER CLEARED");
    audio.play("erase");
  }

  // ── controls ──────────────────────────────────────────────────────
  arm(a: PowerAction) {
    if (!this.init?.capabilities[a]) {
      audio.play("error");
      this.addLog(`POWER · ${a.toUpperCase()} UNAVAILABLE`);
      return;
    }
    if (this.armed === a) {
      this.armed = "";
      clearTimeout(this.armTimer);
      audio.play("execute");
      this.addLog(`POWER · ${a.toUpperCase()}`);
      send({ type: "power", action: a });
      return;
    }
    this.armed = a;
    audio.play("arm");
    this.addLog(`ARMED · ${a.toUpperCase()} — REPEAT TO EXECUTE`);
    clearTimeout(this.armTimer);
    this.armTimer = window.setTimeout(() => (this.armed = ""), 3600);
  }
  disarm() { this.armed = ""; clearTimeout(this.armTimer); }

  cycleLayout() {
    if (!this.init?.capabilities.layouts) return;
    send({ type: "cycle-layout" });
    audio.play("toggle");
    this.addLog("KEYMAP · NEXT");
  }

  toggleMute() {
    this.muted = !this.muted;
    audio.setMuted(this.muted || !this.primary);
    this.addLog(this.muted ? "AUDIO · MUTED" : "AUDIO · ONLINE");
    if (!this.muted) { audio.play("toggle"); this.startMusic(1.5); }
  }

  mediaAction(action: "play-pause" | "next" | "previous") {
    send({ type: "media", action });
    audio.play("nav");
  }

  // ── text ──────────────────────────────────────────────────────────
  say(text: string, isError = false) {
    this.message = text;
    this.messageIsError = isError;
    clearTimeout(this.messageTimer);
    this.messageTimer = window.setTimeout(() => (this.message = ""), 6500);
  }

  addLog(text: string) {
    this.log = [...this.log.slice(-8), { id: ++this.logId, text: text.toUpperCase() }];
  }

  clockText(t: number) {
    const d = new Date(t);
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }

  get lockedFor() {
    const s = Math.max(0, Math.floor((this.now - (this.init?.lockedAt ?? this.now)) / 1000));
    return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
  }

  get subtitle(): [string, string] {
    const who = this.displayName;
    switch (this.phase) {
      case "boot": return ["终端已锁定。会话内容已封存。", "TERMINAL LOCKED. SESSION CONTENTS SEALED."];
      case "verifying": return ["正在比对授权口令……", "CROSS-CHECKING AUTHORIZATION KEY."];
      case "granted": return [`权限已授予。欢迎回来，${who}。`, `PERMISSION AUTHORIZED. WELCOME BACK, ${who.toUpperCase()}.`];
      case "denied": return this.keyAccepted
        ? ["口令已通过，但系统拒绝了会话。", "KEY ACCEPTED. THE SYSTEM REFUSED THE SESSION."]
        : ["检测到非法访问，权限已被拒绝。", "ILLICIT ACCESS DETECTED. PERMISSION DENIED."];
      case "standby": return ["终端待机中。按任意键恢复访问。", "TERMINAL ON STANDBY. PRESS ANY KEY TO RESUME."];
    }
    if (this.capsLock) return ["注意：大写锁定已开启。", "NOTE: CAPS LOCK IS ENGAGED."];
    return [`${who}，您的会话已锁定。请出示授权口令。`, `${who.toUpperCase()}, YOUR SESSION IS LOCKED. PRESENT YOUR AUTHORIZATION KEY.`];
  }

  /** the key itself was accepted; a later PAM step refused */
  get keyAccepted() { return this.failStage === "account" || this.failStage === "credentials"; }

  get failReason() {
    switch (this.failStage) {
      case "account": return "KEY ACCEPTED · ACCOUNT REFUSED";
      case "credentials": return "KEY ACCEPTED · CREDENTIALS REFUSED";
      case "start": return "AUTHORITY UNAVAILABLE";
    }
    return this.attempts >= 8 ? "ACCOUNT LOCKOUT IMMINENT" : "AUTHENTICATION FAILED";
  }

  // hidden hooks for the mock/screenshot harness
  verifyElapsed() { return performance.now() - this.verifyStarted; }
}

export const lock = new Lock();
