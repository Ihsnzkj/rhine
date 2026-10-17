// Score and cues — WebAudio port of the SDDM theme's AudioEngine.qml.
// BGM: "Rhine Lab.LLC" (孤星 OST) bar-aligned loop.  On a lock screen it must
// not blare at an empty room: it only fades in once someone is back at the
// keyboard, never while their own media is playing, and fades out for
// standby and unlock.

const CUES: Record<string, { files: string[]; gain: number }> = {
  key: { files: ["key1", "key2", "key3"], gain: 0.62 },
  erase: { files: ["erase"], gain: 0.6 },
  request: { files: ["request"], gain: 0.85 },
  process: { files: ["process"], gain: 0.8 },
  warning: { files: ["warning"], gain: 0.92 },
  glitch: { files: ["glitch1", "glitch2", "glitch3"], gain: 0.7 },
  recover: { files: ["recover"], gain: 0.75 },
  nav: { files: ["nav"], gain: 0.66 },
  toggle: { files: ["toggle"], gain: 0.7 },
  arm: { files: ["arm"], gain: 0.8 },
  execute: { files: ["execute"], gain: 0.85 },
  standby: { files: ["standby"], gain: 0.7 },
  wake: { files: ["wake"], gain: 0.72 },
  access: { files: ["access"], gain: 0.7 },
  draw: { files: ["draw"], gain: 0.85 },
  blip: { files: ["blip1", "blip2"], gain: 0.7 },
  assemble: { files: ["assemble"], gain: 0.9 },
  reveal: { files: ["reveal"], gain: 0.78 },
  choose: { files: ["choose"], gain: 0.82 },
  error: { files: ["error"], gain: 0.7 },
};

// 32 bars of "Rhine Lab.LLC" at 112 BPM, exactly as cut by tools/fetch-bgm.py
const BGM_LOOP_SECONDS = (102857 * 32) / 48000;
/** that loop as the media card shows it when no player of the user's is around */
export const SCORE = { name: "Local score", title: "Rhine Lab.LLC", artist: "塞壬唱片-MSR", album: "孤星OST" };

// perceptual 0..1 → amplitude over a 42 dB range (same curve as the greeter)
const amp = (v: number) => (v <= 0.001 ? 0 : Math.pow(10, ((v - 1) * 42) / 20));
/** a config volume: finite and within 0..1, else the default */
const volume = (v: unknown, fallback: number) =>
  typeof v === "number" && Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : fallback;

class Audio {
  private ctx: AudioContext | null = null;
  private sfxBus!: GainNode;
  private musicBus!: GainNode;
  private buffers = new Map<string, AudioBuffer>();
  private loops = new Map<string, AudioBufferSourceNode>();
  private bgm: AudioBufferSourceNode | null = null;
  private bgmLoading = false;
  private loading: Promise<void> | null = null;
  muted = false;
  sfxVolume = 0.7;
  musicVolume = 0.68;
  musicEnabled = true;
  private musicTarget = 0;
  private duckLevel = 1;
  /** told whenever the score is set to fade in (true) or out (false) */
  onMusic: (on: boolean) => void = () => {};
  /** whether the score is installed (tools/fetch-bgm.py); false without audio */
  hasScore: Promise<boolean> = Promise.resolve(false);

  /** apply (validated) settings; creates the AudioContext the first time */
  init(cfg: { sfx?: unknown; music?: unknown; musicEnabled?: boolean }) {
    this.sfxVolume = volume(cfg.sfx, this.sfxVolume);
    this.musicVolume = volume(cfg.music, this.musicVolume);
    this.musicEnabled = cfg.musicEnabled ?? true;
    if (this.ctx) {
      // a later init (page reload, output change) re-applies the settings
      this.sfxBus.gain.value = this.muted ? 0 : amp(this.sfxVolume);
      if (!this.musicEnabled) this.setTarget(0);
      this.ramp(0.4);
      return;
    }
    try {
      this.ctx = new AudioContext({ latencyHint: "interactive" });
    } catch {
      return; // no audio stack: stay silent, never break the lock
    }
    this.sfxBus = this.ctx.createGain();
    this.sfxBus.gain.value = amp(this.sfxVolume);
    this.sfxBus.connect(this.ctx.destination);
    this.musicBus = this.ctx.createGain();
    this.musicBus.gain.value = 0;
    this.musicBus.connect(this.ctx.destination);
    this.loading = this.load();
    // the media card offers the score only when it is there; HEAD decodes nothing
    this.hasScore = fetch("./audio/bgm-loop.opus", { method: "HEAD" }).then((r) => r.ok, () => false);
  }

  private async load() {
    const ctx = this.ctx!;
    const names = new Set(Object.values(CUES).flatMap((c) => c.files));
    await Promise.all([...names].map(async (n) => {
      try {
        const res = await fetch(`./audio/${n}.wav`);
        this.buffers.set(n, await ctx.decodeAudioData(await res.arrayBuffer()));
      } catch { /* a missing cue is silent */ }
    }));
  }

  /** resume after the first user gesture (WebKit may start suspended) */
  wake() {
    if (this.ctx && this.ctx.state === "suspended") void this.ctx.resume();
  }

  play(name: string, gainScale = 1) {
    const ctx = this.ctx;
    if (!ctx || this.muted) return;
    const cue = CUES[name];
    const file = cue?.files[Math.floor(Math.random() * cue.files.length)];
    const buf = file && this.buffers.get(file);
    if (!buf) return;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    // tiny pitch spread so repeated keys never sound mechanical
    if (name === "key" || name === "nav") src.playbackRate.value = 0.97 + Math.random() * 0.06;
    const g = ctx.createGain();
    g.gain.value = amp(this.sfxVolume * cue.gain) / Math.max(1e-4, amp(this.sfxVolume)) * gainScale;
    src.connect(g).connect(this.sfxBus);
    src.start();
  }

  hold(name: string, on: boolean) {
    const ctx = this.ctx;
    if (!ctx) return;
    const cur = this.loops.get(name);
    if (!on) {
      if (cur) {
        try { cur.stop(ctx.currentTime + 0.05); } catch { /* already stopped */ }
        this.loops.delete(name);
      }
      return;
    }
    if (cur || this.muted) return;
    const buf = this.buffers.get(CUES[name]?.files[0] ?? "");
    if (!buf) return;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.loop = true;
    const g = ctx.createGain();
    g.gain.value = amp(this.sfxVolume * (CUES[name]?.gain ?? 1)) / Math.max(1e-4, amp(this.sfxVolume));
    src.connect(g).connect(this.sfxBus);
    src.start();
    this.loops.set(name, src);
  }

  /** fade the score toward `on` over `seconds`.
   *  The loop is decoded into WebAudio and looped sample-accurately: an
   *  <audio> element cannot load from the backend's rhine:// scheme in
   *  WebKitGTK (MEDIA_ERR_SRC_NOT_SUPPORTED), fetch + decodeAudioData can. */
  music(on: boolean, seconds = 2.4) {
    if (!this.ctx) return;
    this.setTarget(on && this.musicEnabled && !this.muted ? 1 : 0);
    if (this.musicTarget && !this.bgm && !this.bgmLoading) void this.startLoop();
    this.ramp(seconds);
  }

  private setTarget(t: number) {
    this.musicTarget = t;
    this.onMusic(t > 0);
  }

  private async startLoop() {
    const ctx = this.ctx!;
    this.bgmLoading = true;
    try {
      const res = await fetch("./audio/bgm-loop.opus");
      if (!res.ok) return;
      const buf = await ctx.decodeAudioData(await res.arrayBuffer());
      const src = ctx.createBufferSource();
      src.buffer = buf;
      src.loop = true;
      src.loopStart = 0;
      src.loopEnd = Math.min(buf.duration, BGM_LOOP_SECONDS);   // ignore encoder padding
      src.connect(this.musicBus);
      src.start();
      this.bgm = src;
    } catch {
      /* no score: effects only */
    } finally {
      this.bgmLoading = false;
    }
  }

  /** state ducking (verify / lockdown) applied in amplitude after the curve */
  duck(level: number, seconds = 0.6) {
    this.duckLevel = level;
    this.ramp(seconds);
  }

  private ramp(seconds: number) {
    if (!this.ctx) return;
    const g = this.musicBus.gain;
    const t = this.ctx.currentTime;
    g.cancelScheduledValues(t);
    g.setValueAtTime(g.value, t);
    g.linearRampToValueAtTime(amp(this.musicVolume) * this.musicTarget * this.duckLevel, t + seconds);
  }

  setMuted(m: boolean) {
    this.muted = m;
    if (this.ctx) this.sfxBus.gain.value = m ? 0 : amp(this.sfxVolume);
    if (m) {
      for (const k of [...this.loops.keys()]) this.hold(k, false);
      this.music(false, 0.4);
    }
  }

  ready() { return this.loading ?? Promise.resolve(); }
}

export const audio = new Audio();
