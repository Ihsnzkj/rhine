// Transport for CONTRACT.md §3.  Inside rhine-lock the backend injects
// window.webkit.messageHandlers.rhine and calls window.__rhine.receive();
// anywhere else (vite dev, MiniBrowser screenshots) a mock backend answers.
import type { Init, ToBackend, ToFrontend } from "./protocol";

type Listener = (msg: ToFrontend) => void;

declare global {
  interface Window {
    webkit?: { messageHandlers?: { rhine?: { postMessage(msg: unknown): void } } };
    __rhine?: { receive(msg: ToFrontend | string): void };
    __mock?: MockBackend;
  }
}

const listeners = new Set<Listener>();

function dispatch(msg: ToFrontend) {
  for (const l of listeners) {
    try { l(msg); } catch (e) { console.error("rhine: listener failed", e); }
  }
}

window.__rhine = {
  receive(msg) {
    const m = typeof msg === "string" ? (JSON.parse(msg) as ToFrontend) : msg;
    if (m && typeof m === "object" && typeof m.type === "string") dispatch(m);
  },
};

export const native = !!window.webkit?.messageHandlers?.rhine;

export function onMessage(l: Listener) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function send(msg: ToBackend) {
  if (native) window.webkit!.messageHandlers!.rhine!.postMessage(msg);
  else mock.handle(msg);
}

export function log(level: "debug" | "info" | "warn" | "error", message: string) {
  if (native) send({ type: "log", level, message });
  else console.log(`[${level}] ${message}`);
}

// ───────────────────────── mock backend ─────────────────────────
// Password "rhine" unlocks.  URL knobs: ?media=0 ?caps=1 ?attempts=3 ?user= ?host=
// ?session= ?snap=<image> ?stage=account ?hold=1 ?primary=0 ?q=low|high|ultra
class MockBackend {
  private params = new URLSearchParams(location.search);
  private attempts = Number(this.params.get("attempts") ?? 0);
  private busy = false;
  unlocked = false;

  handle(msg: ToBackend) {
    switch (msg.type) {
      case "hello": return void setTimeout(() => this.boot(), 30);
      case "authenticate": return this.authenticate(msg.password);
      case "unlock-ready": return this.unlock();
      case "cycle-layout": return this.keyboard(false, ["us", "cn"][Math.floor(Math.random() * 2)]);
      case "media": return this.toggleMedia(msg.action);
      default: console.log("mock ←", JSON.stringify(msg));
    }
  }

  private boot() {
    const init: Init = {
      type: "init", protocol: 1, version: "mock",
      output: { name: "DP-1", width: innerWidth, height: innerHeight, scale: devicePixelRatio, primary: this.params.get("primary") !== "0" },
      outputs: 1,
      user: { name: this.params.get("user") ?? "operator", realName: "", avatar: null },
      screenshot: this.params.get("snap"),
      host: this.params.get("host") ?? "rhine-terminal", session: this.params.get("session") ?? "niri",
      lockedAt: Date.now() - Number(this.params.get("lockedAgo") ?? 0) * 1000,
      capabilities: { suspend: true, hibernate: false, reboot: true, powerOff: true, media: true, layouts: true },
      config: { quality: this.params.get("q") ?? "ultra", music: this.params.get("music") !== "0", music_volume: 0.5, sfx_volume: 0.7 },
      mode: "preview", mock: true,
    };
    dispatch(init);
    dispatch({ type: "auth", state: "idle", attempts: this.attempts });
    this.keyboard(this.params.get("caps") === "1", "us");
    if (this.params.get("media") !== "0") {
      dispatch({ type: "media", player: {
        name: "spotify", title: "Rhine Lab.LLC", artist: "塞壬唱片-MSR", album: "孤星OST",
        playing: this.params.get("playing") !== "0", artUrl: null, canNext: true, canPrevious: true, canPlayPause: true,
      } });
    } else dispatch({ type: "media", player: null });
    dispatch({ type: "battery", battery: null });
  }

  keyboard(capsLock: boolean, layout: string) {
    dispatch({ type: "keyboard", capsLock, numLock: true, layout, layouts: ["us", "cn"] });
  }

  private playing = true;
  private toggleMedia(action: string) {
    if (action === "play-pause") this.playing = !this.playing;
    dispatch({ type: "media", player: {
      name: "spotify", title: action === "next" ? "Control's Wishes" : "Rhine Lab.LLC", artist: "塞壬唱片-MSR",
      album: "孤星OST", playing: this.playing, artUrl: null, canNext: true, canPrevious: true, canPlayPause: true,
    } });
  }

  private authenticate(password: string) {
    if (this.busy || this.unlocked) return;
    this.busy = true;
    if (this.params.get("hold") === "1") { dispatch({ type: "auth", state: "verifying", attempts: this.attempts }); return; }
    dispatch({ type: "auth", state: "verifying", attempts: this.attempts });
    const ok = password === "rhine";
    setTimeout(() => {
      this.busy = false;
      if (ok) dispatch({ type: "auth", state: "success", attempts: this.attempts });
      else {
        this.attempts++;
        // ?stage=account reproduces "key accepted, account refused"
        const stage = (this.params.get("stage") ?? "authenticate") as "authenticate";
        dispatch({ type: "auth", state: "failure", attempts: this.attempts, message: "Authentication failure", stage });
      }
    }, ok ? 700 : 1200);
  }

  private unlock() {
    if (this.unlocked) return;
    this.unlocked = true;
    dispatch({ type: "unlocking" });
    console.log("MOCK UNLOCKED");
  }

  /** test hook: push any message as if it came from the backend */
  inject(msg: ToFrontend) { dispatch(msg); }
}

export const mock = new MockBackend();
if (!native) window.__mock = mock;
