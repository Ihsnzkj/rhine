# Working on rhine-lock (humans and coding agents)

`rhine-lock` is a screen locker: a mistake can lock someone out of their
machine. Read `CONTRACT.md` (the frontend ⇄ backend protocol) before changing
either side.

## Safety rules

1. **Never test a lock on your live session.** Run every GUI or lock test in
   the private headless compositor: `tools/headless.sh <command…>` starts a
   GPU-backed headless sway with its own `XDG_RUNTIME_DIR` and
   `WAYLAND_DISPLAY` (sway supports ext-session-lock-v1), unsets
   `NIRI_SOCKET`/`DISPLAY`, runs the command and tears everything down.
   Inside: `grim` for screenshots, `wtype` for typing (the first key of every
   `wtype` call is lost — send a sacrificial `-k F13` first),
   `swaymsg create_output` for extra monitors.
2. **Never authenticate as a real user with a wrong password.**
   `pam_faillock` locks accounts after a few failures. End-to-end tests use a
   `cargo build --features mock-auth` build with `--mock-password`; the
   release build contains neither the flag nor the code path. To exercise
   real PAM plumbing, use a user that does not exist and expect failure;
   `rhine-lock --check-pam` checks the account/credential stacks for the
   current user without authenticating.
3. Never install a backend that is still being edited: `install.sh` refuses
   an uncommitted `backend/` unless `--allow-dirty` is given.

## Layout

| path | what |
|---|---|
| `backend/` | Rust · GTK4 · gtk4-layer-shell session lock · WebKitGTK 6 · PAM · zbus — security and the lock |
| `frontend/` | Svelte 5 · TypeScript · Vite · Three.js — everything seen and heard; never decides to unlock |
| `CONTRACT.md` | the protocol between them (single source of truth) |
| `tools/` | headless compositor, screenshot/recording helpers, regression scripts |

## Checks before sending a change

```sh
cargo build  --manifest-path backend/Cargo.toml --release
cargo clippy --manifest-path backend/Cargo.toml --all-targets --all-features -- -D warnings
cargo test   --manifest-path backend/Cargo.toml --all-features
python3 backend/tests/check_pam.py
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET backend/tests/e2e.sh
(cd frontend && npm run check && npm run build)
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET frontend/tests/input.sh
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET tools/standby-key-test.sh
sh -n install.sh             # try real installs in a disposable container or VM
```

UI changes are judged on screenshots and recordings, not on "it builds":
`tools/shot.sh`, `tools/record.sh` (frontend with the in-page mock backend)
and the transition recordings in `backend/tests/e2e.sh` (real backend).
