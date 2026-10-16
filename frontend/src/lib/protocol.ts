// Mirror of CONTRACT.md §4 — keep identical.
export const PROTOCOL_VERSION = 1;

// ───────────── backend → frontend ─────────────
export type ToFrontend =
  | Init | AuthUpdate | PamMessage | KeyboardState | MediaState
  | BatteryState | Unlocking;

export interface Init {
  type: "init";
  protocol: 1;
  version: string;
  output: { name: string; width: number; height: number; scale: number; primary: boolean };
  outputs: number;
  user: { name: string; realName: string; avatar: string | null };
  /** rev 3: this output's pre-lock screenshot, "rhine://screenshot/<connector>" */
  screenshot: string | null;
  host: string;
  session: string;
  lockedAt: number;
  capabilities: {
    suspend: boolean; hibernate: boolean; reboot: boolean; powerOff: boolean;
    media: boolean;
    layouts: boolean;
  };
  config: Record<string, unknown>;
  mode: "lock" | "preview";
  mock: boolean;
}

export interface AuthUpdate {
  type: "auth";
  state: "idle" | "verifying" | "success" | "failure";
  attempts: number;
  message?: string;
  /** rev 2: which PAM step failed ("account"/"credentials" = key accepted, system refused) */
  stage?: "start" | "authenticate" | "account" | "credentials";
}

export interface PamMessage {
  type: "pam";
  kind: "info" | "error";
  text: string;
}

export interface KeyboardState {
  type: "keyboard";
  capsLock: boolean;
  numLock: boolean;
  layout: string;
  layouts: string[];
}

export interface MediaPlayer {
  name: string; title: string; artist: string; album: string;
  playing: boolean; artUrl: string | null;
  canNext: boolean; canPrevious: boolean; canPlayPause: boolean;
}

export interface MediaState {
  type: "media";
  player: null | MediaPlayer;
}

export interface BatteryState {
  type: "battery";
  battery: null | { percent: number; charging: boolean };
}

export interface Unlocking {
  type: "unlocking";
}

// ───────────── frontend → backend ─────────────
export type PowerAction = "suspend" | "hibernate" | "reboot" | "powerOff";

export type ToBackend =
  | { type: "hello"; protocol: 1 }
  | { type: "authenticate"; password: string }
  | { type: "unlock-ready" }
  | { type: "power"; action: PowerAction }
  | { type: "cycle-layout" }
  | { type: "media"; action: "play-pause" | "next" | "previous" }
  | { type: "log"; level: "debug" | "info" | "warn" | "error"; message: string };
