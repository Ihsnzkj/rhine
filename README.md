# rhine-lock

**English** · [简体中文](README.zh-CN.md)

[![build](https://github.com/tmc5csxr9m-cpu/rhine-lock/actions/workflows/build.yml/badge.svg)](https://github.com/tmc5csxr9m-cpu/rhine-lock/actions/workflows/build.yml)

A Wayland screen locker in the style of *Rhine Lab* (莱茵生命) from
*Arknights* — a live 3D ∞ sculpture of acrylic cards, a frosted personnel
file, and the film's black-and-orange WARNING lockdown. Your desktop becomes
a specimen card when you lock and turns back into your desktop when you
unlock.

![lock and unlock](docs/media/lock-unlock.webp)

| ready | verifying | access denied |
|---|---|---|
| ![ready](docs/media/ready.jpg) | ![verifying](docs/media/verifying.jpg) | ![denied](docs/media/denied.jpg) |

It runs on any compositor that implements `ext-session-lock-v1` — niri,
sway, Hyprland, river and others — and authenticates through PAM.

> Fan work. Not affiliated with or endorsed by Hypergryph. *Arknights* and
> *Rhine Lab* are trademarks of Hypergryph; the ∞ mark is a fan redrawing.

## How it works

| part | stack | role |
|---|---|---|
| `backend/` | Rust · GTK 4 · gtk4-layer-shell · WebKitGTK 6 · PAM · zbus | takes the session lock on every monitor, serves the UI from memory over a private `rhine://` scheme, runs PAM, and is the **only** thing that can unlock |
| `frontend/` | Svelte 5 · TypeScript · Vite · Three.js | everything you see and hear; it never decides to unlock |
| `CONTRACT.md` | | the message protocol between the two |

- **Seamless entry and exit.** Each monitor is captured *before* the lock
  (with `grim`, kept in memory only) and is the first thing the lock draws;
  on unlock the animation ends on that same image and the live desktop
  replaces an identical frame. (niri keeps your desktop on screen until the
  lock has drawn; sway may show one black frame of its own first.)
- **Fail-safe.** The UI cannot unlock anything — only a PAM success in the
  backend does. A crashed web view is reloaded; after three crashes in 30 s,
  without a frontend, without a handshake within 15 s, or when the page
  stops answering for 8 s, a native GTK password field takes over —
  and **Ctrl+Shift+Esc** asks for it at any time. A PAM module that hangs is
  cut off after 60 s and counts as a failed attempt. A Rust panic restarts
  the locker once, into the native field; a second panic, a native crash or
  a killed process leaves recovery to the user while the compositor keeps
  the screen locked (see *Recovery*).
- **Honest errors.** A wrong key and a key that was accepted but refused by
  the account policy are shown differently. `rhine-lock --check-pam`
  checks the PAM account and credential setup without a password.
- **Careful with the key.** Keys longer than 4096 bytes are refused, never
  truncated; an empty key is never sent (it could only count against
  `pam_faillock`); core dumps are off and the backend wipes its copies.
- **Media and power.** Shows the playing MPRIS track with controls; suspend,
  reboot and power-off appear only when logind allows them without a polkit
  prompt, and need a second, separate press.
- **Sound.** Interface sounds are included; the score is optional and
  fetched by you (see below). Music starts only once you are back at the
  keyboard and never plays over your own music.

## Requirements

- A Wayland compositor with `ext-session-lock-v1`
- GTK ≥ 4.18, gtk4-layer-shell ≥ 1.3, WebKitGTK 6.0 ≥ 2.40, Linux-PAM
- To build: Rust ≥ 1.92, Node.js 20.19+ or 22.12+ with npm, pkg-config,
  a C compiler
- Optional: `grim` (seamless transitions), `ffmpeg` + Python `numpy` (the
  music), niri (keyboard-layout switching, focused-monitor detection),
  systemd-logind or elogind (power actions), UPower (battery), an MPRIS
  player (media card)

| distribution | build dependencies |
|---|---|
| Arch | `sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 pam rust nodejs npm pkgconf grim` |
| Fedora 43+ | `sudo dnf install 'pkgconfig(gtk4)' 'pkgconfig(gtk4-layer-shell-0)' 'pkgconfig(webkitgtk-6.0)' pam-devel cargo nodejs npm grim` |
| openSUSE Tumbleweed | `sudo zypper install 'pkgconfig(gtk4)' 'pkgconfig(gtk4-layer-shell-0)' 'pkgconfig(webkitgtk-6.0)' pam-devel cargo nodejs npm grim` |
| Ubuntu 26.04+, Debian testing | `sudo apt install libgtk-4-dev libgtk4-layer-shell-dev libwebkitgtk-6.0-dev libpam0g-dev pkg-config build-essential nodejs npm grim`, plus Rust from [rustup.rs](https://rustup.rs) |

Debian 13 (trixie) ships gtk4-layer-shell 1.0.4, which predates the
session-lock API: build gtk4-layer-shell ≥ 1.3 from source first, or use
Debian testing. `install.sh` checks every minimum before it builds anything.

Tested: daily use on Arch Linux with niri; install, PAM preflight,
reinstall, rollback and uninstall in clean containers of Fedora 43,
Ubuntu 26.04, Debian sid and openSUSE Tumbleweed; the end-to-end suite
drives real GTK/WebKit lock windows under sway.

## Install

```sh
git clone https://github.com/tmc5csxr9m-cpu/rhine-lock
cd rhine-lock
tools/fetch-bgm.py   # optional: download the score for personal use
./install.sh         # check, build, install to /usr/local, write the PAM service, verify
```

`install.sh` checks every requirement first, builds as your user and asks
for root (sudo or doas) only to install. It writes `/etc/pam.d/rhine-lock`
from your distribution's own stack (`common-auth`/`common-account`, `login`
or `system-auth` — a file you wrote yourself is kept) and then runs
`rhine-lock --check-pam`; if that fails, the previous installation is put
back. It never touches your compositor configuration.
Interrupted swaps also roll back. Concurrent installs/removals and leftover
`.rhine-old`/`.rhine-new` files are refused. If an uncatchable interruption
leaves `/etc/.rhine-lock-install.lock`, recover the saved files before removing
that directory and retrying; the installer preserves them for recovery.
`./install.sh --prefix /usr` installs elsewhere, `./install.sh --uninstall`
removes its binary and frontend again (use the same prefix). The shared PAM
service is kept so another installation or a running locker can still
authenticate; remove `/etc/pam.d/rhine-lock` manually only after neither needs it.

Then hook it up, for example:

```kdl
// niri — binds.kdl
Mod+Alt+L { spawn "rhine-lock" "--daemonize"; }
```
```sh
# sway
bindsym $mod+Alt+l exec rhine-lock --daemonize
# Hyprland
bind = SUPER ALT, L, exec, rhine-lock --daemonize
# swayidle — lock after 5 min, and before sleep
swayidle -w timeout 300 'rhine-lock --daemonize' \
            before-sleep 'rhine-lock --daemonize' lock 'rhine-lock --daemonize'
# hypridle — general { lock_cmd = rhine-lock --daemonize }
```

`--daemonize` returns only once the screen is actually locked — also when
another instance is already locking it — so `before-sleep` hooks are safe.

## Use

| key | action |
|---|---|
| type, Enter | authenticate |
| Esc | clear the key / cancel an armed power action |
| Tab | move between the on-screen controls (Enter or Space presses one; typing returns to the key) |
| F3 | next keyboard layout (niri) |
| F4 | sound on / off |
| F7 / F8 / F9 | suspend / reboot / power off — press twice |
| Ctrl+Shift+Esc | switch to the plain native password field |

## Configuration

`~/.config/rhine-lock/config.toml` (or `/etc/rhine-lock/config.toml`); every
key is optional.

```toml
[lock]
pam_service = "rhine-lock"   # PAM service to authenticate against
allow_power = true           # offer reboot / power off

[frontend]
quality = "ultra"            # ultra | high | low (low: no shadows, AO or depth of field)
music = true
music_volume = 0.68          # perceptual, 0–1
sfx_volume = 0.7             # 0–1
standby_seconds = 45         # dim into standby after this idle time, 0 = never
```

A config file that cannot be read or parsed is reported in the log; the
lock then uses the default PAM service and disables reboot and power-off.

## Limitations

- Lock mode selects the compositor through `WAYLAND_DISPLAY`; inherited
  connections via `WAYLAND_SOCKET` are refused. Unset `WAYLAND_SOCKET` when
  launching the locker from such an environment.
- One password per attempt: PAM conversations that ask for more (a
  one-time code, a second factor, changing an expired password) are not
  supported.
- Keyboard-layout switching (F3) and focused-monitor detection need niri;
  elsewhere the layout is shown but not switched.
- The 3D scene needs WebGL 2; without it the lock shows a flat background
  and works the same.
- The key passes through WebKit's JavaScript engine, whose memory
  rhine-lock cannot wipe. Use disk and swap encryption if that matters to
  you.

## Recovery

- **Correct password refused?** Run `rhine-lock --check-pam` from another
  terminal or over SSH. It tells you which PAM step fails, without
  authenticating.
- **UI frozen or broken?** Press **Ctrl+Shift+Esc** for the native password
  field.
- **Locker crashed?** A Rust panic restarts it once; native crashes and killed
  processes do not restart automatically. If it is gone for good, the
  compositor keeps the screen locked on purpose: switch to a TTY
  (Ctrl+Alt+F2), log in, and start a new locker on your session's display —
  `WAYLAND_DISPLAY=wayland-1 rhine-lock` (check `ls $XDG_RUNTIME_DIR` for
  the socket name) — then unlock normally. sway and niri accept a new lock
  client after the old one died; Hyprland needs
  `misc:allow_session_lock_restore = true`.

## Development

```sh
cd frontend && npm ci && npm run dev   # the UI in a browser with a mock backend; the key is "rhine"
```

Read [AGENTS.md](AGENTS.md) before touching anything: never test a lock on
your live session (`tools/headless.sh` runs everything in a private headless
compositor) and never try wrong passwords against a real account. The
end-to-end suites (`backend/tests/e2e.sh`, `frontend/tests/input.sh`) drive
real GTK/WebKit lock windows with real key presses, record the transitions
frame by frame and check them pixel for pixel.

The backend was written by OpenAI Codex and the frontend by Anthropic's
Claude, in parallel, against `CONTRACT.md`; before release Codex reviewed
the whole project in depth and both sides fixed what it found.

## Licence and credits

- Code: [GNU AGPL-3.0-or-later](LICENSE).
- Fonts: Montserrat and JetBrains Mono, SIL Open Font License 1.1
  (`frontend/public/fonts/OFL-*.txt`).
- Interface sounds: Kenney *Interface Sounds* (CC0); the boot, assembly,
  verification and warning cues are synthesised for this project. Every
  asset's origin is listed in [tools/assets/README.md](tools/assets/README.md).
- Score: "Rhine Lab.LLC" from the 「孤星」 OST © Hypergryph / Monster Siren
  Records — **not distributed**; `tools/fetch-bgm.py` downloads it for your
  personal use.
- Visual reference: *Arknights* special film 《莱茵生命：访问》. The light
  probe, scene textures and synthesis cues are generated by scripts in this
  repository; the preview desktop is a hand-drawn synthetic image.
