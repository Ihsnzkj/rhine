# rhine-lock — frontend ⇄ backend contract  (protocol v1, revision 4)

> **Revision 4 (release review)** — no message changes, still protocol 1:
> keys are limited to 4096 UTF-8 bytes on both sides (§5); PAM runs in a
> disposable worker with a 60 s deadline (§5); readiness of `--daemonize` and
> of a concurrent instance is confirmed by the locking process, per
> compositor (§7); an unresponsive page, a panic or Ctrl+Shift+Esc lead to
> the native fallback (§8); the executable's own installation is preferred
> when looking for the frontend (§7); invalid config is conservative (§7).
>
> **Revision 3 (transitions)** — additive, still protocol 1: pre-lock
> screenshots (§1, §2, `init.screenshot`), no frame other than the
> screenshot/paper before the page paints (§1), exit timing (§5).
>
> **Revision 2 (after the first real-session test)** — additive, still
> protocol 1: `auth.stage` (§4/§5), `WebsitePolicies` autoplay (§8),
> `--check-pam` (§7), PAM service file content (§5).

`rhine-lock` is a Wayland screen locker (ext-session-lock-v1: niri, sway,
Hyprland, river, …) in the Arknights *Rhine Lab* style, matching its companion SDDM greeter theme.

| part | originally written by | stack | lives in |
|---|---|---|---|
| backend  | Codex  | Rust · GTK4 · gtk4-layer-shell (ext-session-lock-v1) · WebKitGTK 6 · PAM · zbus | `backend/` |
| frontend | Claude | Svelte 5 · TypeScript · Vite · Three.js | `frontend/` → built to `frontend/dist/` |
| installer | Claude | shell | `install.sh` |

The backend owns **security and the session lock**. The frontend owns
**everything you see and hear**, and never decides whether to unlock.

---

## 1. Process & windows

* One process. It requests an `ext-session-lock-v1` lock through
  gtk4-layer-shell's `GtkSessionLockInstance` and creates **one GTK window
  with one `WebKitWebView` per monitor**, including monitors hot-plugged
  while locked (and drops windows of removed monitors).
* Every WebView loads `rhine://app/index.html?output=<connector>`.
* Window / WebView background before the page paints: **`#e7e5e1`** (Rhine
  paper). Never white, never black.
* **rev 3 — seamless entry.** *Before* requesting the lock, capture every
  monitor exactly as it is on screen (`grim -o <connector> -t png -l 1 -`
  or equivalent wlr-screencopy; ≤ 800 ms for all outputs together, in
  parallel; on any failure/timeout that output simply has no screenshot).
  Bytes stay in memory only — never written to disk, never logged — and are
  dropped when the process exits. Each lock window then shows, from its very
  first committed frame until the page has painted, a `GtkPicture` of its
  own screenshot (scaled 1:1 to the output, no letterboxing) under a
  WebView with a **transparent** background; without a screenshot the
  window shows paper `#e7e5e1`. No black, white or other frame may ever be
  presented between "desktop" and the page's first paint (the first
  real-session recording showed ~300 ms of black there).
* The webviews of all monitors run the same frontend. Exactly one is
  `primary` (focused output from `niri msg --json focused-output` when
  `NIRI_SOCKET` is set, else the first monitor); the frontend uses it for
  layout richness only — **any** output may submit a password.

## 2. The `rhine://` scheme (backend serves, read-only)

| URL | content |
|---|---|
| `rhine://app/<path>` | files from the frontend dist dir (see §7), correct MIME types (`.js` `text/javascript`, `.wasm`, `.opus` `audio/ogg`, `.hdr`, `.woff2`, `.ttf`, `.svg`, `.png`, `.json`, `.wav`) |
| `rhine://screenshot/<connector>` | rev 3: that output's pre-lock screenshot (`image/png`); 404 if none. Served only from memory. |
| `rhine://avatar` | the user's avatar (AccountsService icon `/var/lib/AccountsService/icons/<user>`, else `~/.face`, else `~/.face.icon`); 404 if none |

Register `rhine` as **secure** and **CORS-enabled** (`WebKitSecurityManager`)
so `fetch()`, ES modules, WebAudio decoding and WebGL texture loads work.
Reject path traversal (`..`, absolute paths, symlinks leaving the dist dir).
**Every other navigation is refused** — `http(s)`, `file`, `data:` top-level,
`window.open`, link clicks, drag-and-drop navigation.

## 3. Message transport

* **frontend → backend**:
  `window.webkit.messageHandlers.rhine.postMessage(<object>)`
  (register the handler `rhine` on the `WebKitUserContentManager`; the payload
  arrives as a JSC value — serialise it to JSON on the Rust side).
* **backend → frontend**: `webview.evaluate_javascript(
  "window.__rhine && window.__rhine.receive(<json>)")`.
* Every message is a JSON object with a string `type`. Unknown types are
  ignored (logged in `--debug`), never fatal. Field names are camelCase.
* The frontend sends `hello` once its script is running. Until a webview has
  said `hello`, the backend **queues** messages for it and flushes them in
  order right after sending `init`. If the page is reloaded, the next `hello`
  gets a fresh `init` plus the current `auth`, `keyboard`, `media` and
  `battery` state. The first `media` snapshot is sent only after the initial
  system poll completes; until then the player state is unknown, and optional
  frontend music waits. `player: null` means the poll found no player.

## 4. Messages

TypeScript is the reference; the frontend has the identical file at
`frontend/src/lib/protocol.ts`.

```ts
export const PROTOCOL_VERSION = 1;

// ───────────── backend → frontend ─────────────
export type ToFrontend =
  | Init | AuthUpdate | PamMessage | KeyboardState | MediaState
  | BatteryState | Unlocking;

export interface Init {
  type: "init";
  protocol: 1;
  version: string;                 // backend version, e.g. "0.1.0"
  output: { name: string; width: number; height: number; scale: number; primary: boolean };
  outputs: number;                 // monitors currently locked
  user: { name: string; realName: string; avatar: string | null }; // avatar = "rhine://avatar" or null
  screenshot: string | null;       // rev 3: "rhine://screenshot/<this output's connector>" or null
  host: string;                    // hostname
  session: string;                 // $XDG_CURRENT_DESKTOP, e.g. "niri"
  lockedAt: number;                // epoch ms when the lock was acquired
  capabilities: {
    suspend: boolean; hibernate: boolean; reboot: boolean; powerOff: boolean;
    media: boolean;                // MPRIS available
    layouts: boolean;              // cycle-layout supported
  };
  config: Record<string, unknown>; // the [frontend] table of the config file, verbatim
  mode: "lock" | "preview";        // preview = windowed, no real session lock
  mock: boolean;                   // true when built with the mock-auth feature
}

export interface AuthUpdate {
  type: "auth";
  state: "idle" | "verifying" | "success" | "failure";
  attempts: number;                // failed attempts since the lock started
  message?: string;                // PAM error text on failure (may be empty)
  // rev 2 — on failure, the PAM step that failed. "authenticate" = the key
  // was wrong (or the account is faillocked); "account" / "credentials" = the
  // key was ACCEPTED but pam_acct_mgmt / pam_setcred refused — a system
  // configuration problem the user must be told about, not "wrong password";
  // "start" = pam_start failed.
  stage?: "start" | "authenticate" | "account" | "credentials";
}

export interface PamMessage {      // PAM_TEXT_INFO / PAM_ERROR_MSG during verification
  type: "pam";
  kind: "info" | "error";
  text: string;
}

export interface KeyboardState {   // send on start and on every change
  type: "keyboard";
  capsLock: boolean;
  numLock: boolean;
  layout: string;                  // active layout short name, e.g. "us" ("" if unknown)
  layouts: string[];
}

export interface MediaState {      // active MPRIS player; send on change, null when none
  type: "media";
  player: null | {
    name: string; title: string; artist: string; album: string;
    playing: boolean; artUrl: string | null;   // only "rhine://media-art" (backend-proxied) or null
    canNext: boolean; canPrevious: boolean; canPlayPause: boolean;
  };
}

export interface BatteryState {    // UPower DisplayDevice; null on desktops
  type: "battery";
  battery: null | { percent: number; charging: boolean };
}

export interface Unlocking {       // sent to all webviews right before unlock
  type: "unlocking";
}

// ───────────── frontend → backend ─────────────
export type ToBackend =
  | { type: "hello"; protocol: 1 }
  | { type: "authenticate"; password: string }
  | { type: "unlock-ready" }                     // exit animation finished
  | { type: "power"; action: "suspend" | "hibernate" | "reboot" | "powerOff" }
  | { type: "cycle-layout" }
  | { type: "media"; action: "play-pause" | "next" | "previous" }
  | { type: "log"; level: "debug" | "info" | "warn" | "error"; message: string };
```

### Media art
If `artUrl` from MPRIS is a `file://` path or an `http(s)` URL, the backend
fetches it itself (≤ 4 MiB, images only) and serves it at
`rhine://media-art?v=<n>` (bump `n` on change). The webview never touches the
network.

## 5. Authentication & unlock

1. `authenticate` while a verification is running, or after `success`, is
   ignored.
2. Backend sends `auth {state:"verifying"}` to **all** webviews, then runs PAM
   (service from config, default **`rhine-lock`**; user = the session user) in
   a worker thread: `pam_start → pam_authenticate → pam_acct_mgmt →
   pam_setcred(PAM_REFRESH_CRED) → pam_end`.
   Conversation: `PAM_PROMPT_ECHO_OFF` → the password; `PAM_PROMPT_ECHO_ON` →
   the user name; `PAM_TEXT_INFO` / `PAM_ERROR_MSG` → `pam` messages. One
   password per attempt: conversations that need more (one-time codes,
   second factors, changing an expired password) are not supported.
   The password buffer is zeroised after use and never logged.
   Rev 4: keys longer than **4096 UTF-8 bytes** (or containing NUL) fail
   without reaching PAM; the frontend refuses them first, visibly, and never
   sends an empty key. Each attempt runs in a disposable PAM worker process
   with a **60 s** deadline; a timeout or a dead worker is a normal failure
   (stage `authenticate`), and a late result is never accepted.
   The service file must provide **both** `auth` and `account` (the
   installer writes both with `include`, from `common-auth`/`common-account`,
   `login` or `system-auth`, whichever the distribution provides); a
   service without an `account` section falls through to
   `/etc/pam.d/other`, which denies — a correct key then cannot unlock.
3. **Failure** → `auth {state:"failure", attempts, message}` to all. The
   backend enforces a minimum of **1.2 s** between the request and the failure
   reply (pads PAM's own delay) so brute force is slow and the frontend's
   verify animation can play.
4. **Success** → `auth {state:"success"}` to all. The backend waits for the
   first `unlock-ready` from any webview **or 1600 ms**, whichever comes
   first (rev 3: the frontend's exit animation ends on a frame identical to
   the pre-lock screenshot and lasts ≤ 1.3 s, so the cap is a safety net), then sends `unlocking`, calls `unlock` on the lock instance,
   destroys the windows and exits 0.
5. The frontend may show anything it likes, but the only thing that unlocks
   is step 4.

## 6. Other behaviour

* `power`: logind over D-Bus (`Suspend`, `Hibernate`, `Reboot`, `PowerOff`,
  `interactive=false`). Ignore actions whose capability is false or that
  config disables (`[lock] allow_power = false` → reboot/powerOff false).
* `cycle-layout`: `niri msg action switch-layout next` (only when
  `NIRI_SOCKET` is set; else capability false).
* `keyboard`: from the GDK seat keyboard (`caps-lock-state`,
  `num-lock-state`, `layout-names`, `active-layout-index`).
* `media`: MPRIS over the session bus; prefer a playing player, else the most
  recently active one.

## 7. Command line & config

```
rhine-lock                 lock now (foreground; exits 0 after unlock)
rhine-lock --daemonize     fork; the parent exits 0 only once the session is
                           actually locked (like swaylock -f) — used by
                           swayidle before-sleep
rhine-lock --preview       normal toplevel window, no session lock (dev)
rhine-lock --frontend DIR  serve this dist dir instead of the default
rhine-lock --debug         WebKit inspector enabled + JS console → stderr
rhine-lock --version
rhine-lock --check-pam     rev 2: pam_start + pam_acct_mgmt + pam_setcred(REFRESH)
                           for the current user with the configured service, NO
                           authentication (cannot count towards faillock); prints
                           each step's result, exit 0 only if all succeed. The
                           installer rolls back an installation that fails it.
```
Test builds only (`cargo build --features mock-auth`):
`--mock-password PW` replaces PAM with a constant-time compare. The release
build must not contain this flag or code path.

Default frontend dir (rev 4): `$RHINE_LOCK_FRONTEND`, else
`../share/rhine-lock/frontend` relative to the executable (its own
installation), else `/usr/share/rhine-lock/frontend`, else `frontend/dist`
relative to the repo when running from `backend/target/*`.

Config: `$XDG_CONFIG_HOME/rhine-lock/config.toml`, falling back to
`/etc/rhine-lock/config.toml`; all keys optional. Rev 4: a file that cannot
be read or parsed, an unknown `[lock]` key or an invalid PAM service name is
reported (without its contents) and the lock runs with the default service
and `allow_power = false`.

```toml
[lock]
pam_service = "rhine-lock"
allow_power = true

[frontend]            # passed through untouched as init.config
quality = "ultra"     # ultra | high | low; anything else → ultra
music = true          # false: the score starts paused
music_volume = 0.68   # 0..1, clamped; non-numbers ignored
sfx_volume = 0.7      # 0..1, clamped
standby_seconds = 45  # 0 = never
```

Single instance, per compositor (rev 4): coordination lives in a private,
UID-owned directory under `$XDG_RUNTIME_DIR`, keyed by the Wayland socket. A
lock-mode launch with `WAYLAND_SOCKET` is refused: an inherited connection can
override the named `WAYLAND_DISPLAY` used for coordination. Unset it and select
the compositor with `WAYLAND_DISPLAY` instead. A
second `rhine-lock` (or `--daemonize`) for the same compositor waits for a
live confirmation from the locking process that the session is locked, then
exits 0 — never earlier, and no readiness bit is kept on disk. If the
compositor refuses the lock (another locker active), exit 1.

## 8. Failure safety (backend)

* If a web process crashes (`web-process-terminated`), reload that webview.
  After 3 crashes within 30 s, or if the dist dir is missing/invalid, replace
  that window's content with a **native GTK fallback**: a centred password
  entry styled in the Rhine palette (paper `#e7e5e1`, ink `#111214`, orange
  `#ff7b00`, lime focus `#b9e432`, font Montserrat→MiSans→sans) that drives the
  same PAM path. The session must always be unlockable.
* Rev 4: after `hello`, the backend probes each page; one that does not
  answer within 8 s gets the native fallback, as does a page without
  `hello` after 15 s. **Ctrl+Shift+Esc** requests the native fallback at any
  time — handled by GTK, so it works while the page is hung; it cannot
  bypass authentication. (Not Ctrl+Alt+F*n*: compositors keep those for VT
  switching even while locked.)
* Rev 4: a panic restarts the binary once, with the same arguments, straight
  into the native fallback; a second panic exits and the compositor keeps
  the session locked (recovery from a TTY, see README).
* The WebView must not be able to break the lock: no navigation, no new
  windows, no inspector unless `--debug`, no context menu, no file chooser,
  no permission requests (deny all), no clipboard read.
* Media autoplay is allowed (`media-playback-requires-user-gesture = false`
  **and**, rev 2, a `WebKitWebsitePolicies` with `autoplay = ALLOW` on every
  view — WebKitGTK 6 otherwise keeps the AudioContext suspended and refuses
  audible playback until the first key press);
  WebGL and hardware acceleration are on (`hardware-acceleration-policy =
  ALWAYS`, `enable-webgl = true`).

## 9. Changing this contract

The frontend and backend were built in parallel by two agents against this
document, so it is the single source of truth: a change to a message, a URL
or a timing is made here first (as an additive revision where possible, see
the revision notes at the top), then in `frontend/src/lib/protocol.ts` and
the backend.
