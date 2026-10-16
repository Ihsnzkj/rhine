# rhine-lock backend

Rust/GTK4/WebKitGTK session locker implementing `../CONTRACT.md` protocol 1, revision 4 (with the startup limitation below).
The frontend directory is read-only. Production authentication uses the session
UID's account and the configured PAM service. The installer must supply
`/etc/pam.d/rhine-lock`; this backend does not install system files.

## Build

Required: Rust >= 1.92 (edition 2024), GTK >= 4.18, gtk4-layer-shell >= 1.3,
WebKitGTK 6 >= 2.40, PAM, pkg-config and the normal C build toolchain.
The supplied machine has newer compatible versions.

```sh
cd backend
cargo build --release
cargo clippy -- -D warnings
cargo test
```

Production binary: `backend/target/release/rhine-lock`. Do not install a binary
built with `mock-auth`. The production CLI does not recognize `--mock-password`.
`Cargo.lock` pins the tested dependency graph.

CLI: `--frontend DIR`, `--debug`, `--preview`, `--daemonize`, `--version`, `--check-pam`.
Preview creates a normal closable window. The default action locks the session;
only successful authentication releases a real lock. The daemon parent returns
success only after the compositor confirms locking. A concurrent locker instance
waits for a live confirmation from that compositor's locker, then returns 0;
refusal by another lock client returns 1. Coordination is scoped to the Wayland
socket, in a private UID-owned runtime directory. No readiness bit is kept on
disk, so a stale file cannot report an unlocked session as locked.
Lock mode rejects `WAYLAND_SOCKET` before coordination: inherited connections
can override the named display. Unset that variable and use `WAYLAND_DISPLAY`.

Config is read from `$XDG_CONFIG_HOME/rhine-lock/config.toml` (or
`~/.config/rhine-lock/config.toml`), then `/etc/rhine-lock/config.toml`.
Missing keys use the contract defaults. `[frontend]` is passed through to `init`.
Frontend lookup: explicit `--frontend`, then `RHINE_LOCK_FRONTEND`, then
`../share/rhine-lock/frontend` relative to the executable, `/usr/share`, and
the repository `frontend/dist` fallback. This prevents a stale system UI from
overriding a newer `/usr/local` installation. See the contract for message shapes.

`rhine-lock --check-pam` checks the configured service for the current UID before
any GUI, display connection, singleton lock or daemonization. The check runs
in a separate process with a 15-second deadline; a stalled module is killed
and reported as an unsuccessful preflight. It calls `pam_start`,
`pam_acct_mgmt`, then `pam_setcred(PAM_REFRESH_CRED)`, prints each attempted
step's PAM result, stops at the first failure, and exits 0 only on success.
It never calls `pam_authenticate` and never supplies a password. The installer
must provide both `auth include login` and `account include login`; a service
with only `auth` can reject a correct key in the account step.

Authentication failures now carry `stage` (`start`, `authenticate`, `account`,
or `credentials`) and the PAM error text. The stage is logged without the key;
the 1.2-second failure floor and password zeroization are unchanged.

## Repeatable verification (never use the real desktop)

Run from the repository root with `WAYLAND_DISPLAY`, `NIRI_SOCKET`, and `DISPLAY`
unset. The wrapper refuses to start otherwise, builds only the mock debug binary,
then runs every GUI operation under the supplied private headless sway wrapper.
The compositor and runtime directory are discarded on exit.

```sh
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET backend/tests/e2e.sh
python3 backend/tests/check_pam.py
cargo test --manifest-path backend/Cargo.toml --all-features
cargo clippy --manifest-path backend/Cargo.toml --all-targets --all-features -- -D warnings
```

Extra test dependencies: sway, wtype, grim, Python >= 3.9 + Pillow + PyGObject
(GTK 4), a C compiler, dbus-daemon, and Linux pidfds (kernel >= 5.3). Harness
cleanup signals pinned process descriptors, so recycled PIDs cannot be killed.
No real user's password is ever tested. The only real PAM authentication test
uses service `swaylock`, nonexistent account `rhine-nobody`, and expects rejection.
The separate `check_pam.py` CLI test uses the release binary and isolated configs:
four deterministic PAM fixtures verify success and start/account/credential
failures, and a fifth fixture verifies the deadline. The fixture exits immediately
if pam_authenticate is ever called. Optional real login/swaylock/missing-service
preflights report observed policy without assuming every distribution has the
same stacks. These checks use the current UID without authentication and with
all display/runtime variables removed. Pass a production binary path as the
optional first argument to test another build.
D-Bus integration tests launch a private daemon with fake logind/UPower/MPRIS
services. Mock builds disable actual power, media and compositor-layout actions.

The end-to-end test verifies the real session-lock protocol, DOM form commands,
GTK fallback commands, focus and Tab via gated wtype input, screenshots, failure
floor, duplicate submission, reload, navigation/CSP restrictions, hot-plug and
removal, web-process termination/recovery, three-crash fallback, daemonization,
single-instance behavior, compositor refusal, preview, and both unlock paths.
It asserts failure stages in backend state and frontend messages and checks
that `AudioContext` reaches `running` before any key event, including after
reload, hot-plug, and web-process recovery. Every WebView receives construct-time
`WebsitePolicies` with autoplay `ALLOW`, alongside the existing gesture setting.
It also requires a working `frontend/dist` smoke test: build the frontend first.
For backend-only development, `--backend-only` explicitly permits a missing
dist; a broken existing dist still fails. Unexpected frontend console errors
fail the suite. Native fallback is tested independently.

Artifacts (ignored): `backend/test-results/`: screenshots, per-process logs,
verification output and isolated config. On failure, `failure.png` and the
relevant process log remain. No screenshots of the user's real session are taken.
The stub frontend displays every received message and acknowledges success after
300 ms. No production networking is allowed from a WebView.

## Test-only semantic interface

A build with `mock-auth`, an explicit `--mock-password`, and
`RHINE_TEST_CONTROL=1` enables inherited stdin/stdout JSON lines. There is no
network listener; the parent harness exclusively owns the pipes. All three gates
are required. Re-run a process with fresh runtime/config directories to reset it.

Request: `{"protocol":1,"id":7,"op":"get_state","params":{}}`.
Response contains the same protocol/id/op, `ok`, a machine-readable `code`, a
`message`, and optional `data`. Operations: `health`, `get_state`, `evaluate`
(WebKit JS evaluation for DOM assertions and actual UI actions), `reload`,
`crash_web` (terminates the actual web process), `set_value` for
`unlock-password`, `invoke_action` for `unlock-submit`, and `close_window`. A mock-only `panic` operation exercises the bounded native
recovery path.
An optional zero-based `params.output` selects the monitor. There is deliberately
no bypass-unlock operation. Finish tests through the real mock authentication
path; a failed harness terminates the child and discards the compositor.

## Architecture and safety

- `ui.rs`: GTK main thread, one window/WebView per output, bridge queues and
  state snapshots, authentication state machine and native fallback.
- `auth.rs`: PAM FFI conversation; zeroized Rust-owned password buffers;
  constant-time padded mock comparison. Successful PAM conversation allocations
  are owned and freed by PAM, beyond Rust's control.
- `auth_worker.rs`: one disposable PAM process per attempt, private inherited
  pipes, bounded messages and a 60-second deadline. A timeout or worker exit
  fails authentication, kills/reaps the worker and permits a fresh attempt.
  Worker stdout is separate from PAM module output; no password enters argv.
- `resources.rs`: secure/CORS-enabled scheme, descriptor-anchored path containment,
  a two-worker bounded file queue and three-second response deadline, MIME
  types, avatar/art proxy and restrictive CSP. App files are limited to 64 MiB;
  avatars to 4 MiB; special files are refused.
- `system.rs`: background D-Bus polling and guarded command dispatch. Art fetches
  are limited to 4 MiB and five seconds; validated raster image signatures only.
- `keyboard.rs`: GDK lock state and XKB display-name to short-code mapping.
- `main.rs`: flock, readiness pipe/fork, panic re-exec and process status.

A failed/crashed UI cannot authorize unlocking. Three web-process crashes within
30 seconds activate GTK fallback. A load failure or absent protocol handshake
for 15 seconds also activates fallback. After the handshake, a lightweight
responsiveness probe switches to native input if JavaScript does not answer
within eight seconds. **Ctrl+Shift+Esc** always requests native input through GTK,
even if the page is responsive but its controls have failed. This shortcut
cannot bypass authentication. The panic hook logs its location without
the potentially sensitive panic payload and permits one re-exec of the same
binary/arguments, into native fallback. A second panic aborts without restarting
again. CLOEXEC releases the old singleton fd and the compositor retains its lock.

System integration uses polling (nominal two-second interval; D-Bus/art work can
extend it), explicit two-second D-Bus method deadlines, bounded command/update
queues and periodic reconnection. Departed player history is pruned. Raster art
supports PNG, JPEG, GIF and WebP; SVG is not proxied. HTTP art has one five-second
budget across at most three redirects. Credentials, HTTPS downgrades and
private/special network destinations are refused; DNS results are checked at
connection time and environment proxies are disabled. Local art must be a
bounded regular file; blocking filesystem calls occur outside GTK. A kernel or
filesystem stall can occupy one bounded worker but cannot hold the GTK loop.
Unknown/custom XKB descriptions become empty strings when absent from the system
rules registry. Live niri layout switching, physical monitor changes, real power
operations and successful real-account PAM authentication are intentionally not
part of the safe automated test environment.

## Revision 3: captures and first frames

Before requesting a lock (including daemon mode), the backend runs one `grim`
per current connector in parallel. All helpers share an 800 ms envelope: reading
stops at 775 ms, reserving 25 ms for kill/reap cleanup. Missing helpers, errors,
invalid PNGs, oversized images (128 MiB per output), and timeouts yield no image
for that output. Captures, decoded textures and served PNGs remain in memory;
only output counts and elapsed milliseconds are logged. Removed outputs lose
cached images; new/reconnected monitors have `init.screenshot = null`.

`rhine://screenshot/<percent-encoded-connector>` serves the PNG with `image/png`,
CORS and no-store headers, or 404. `init.screenshot` contains this URL or null.
A `GtkPicture` fills the lock window underneath a transparent WebView. GTK's GPU
renderer is warmed on an **unmapped** window before requesting the lock. WebKit
construction waits until GTK's first paint has submitted the backdrop. Native
fallback retains its existing paper appearance. Authentication and the 1600 ms
unlock deadline are unchanged.

**Known unmet requirement:** Sway can present its own black lock background
between receiving the lock request and receiving the client's first buffer.
Warming GTK and deferring WebKit remove the long client startup blackout, but a
single compositor frame is still observable intermittently. No first client
buffer is deliberately blank. Do not claim a guaranteed flash-free transition:
round 3 remains partial pending a compositor/lock-integration solution.

The default e2e command runs frame recording, the original regressions, and
release security/recovery regressions. Recordings classify at most one uniformly
black Sway pre-commit frame directly on the desktop-to-lock boundary as XFAIL;
any other foreign frame fails. This narrow classification is not a guarantee
about other compositors. To run the parts independently:

```sh
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET backend/tests/e2e.sh --release-only
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET backend/tests/e2e.sh --regression-only
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET backend/tests/e2e.sh --transitions-only
# One bounded diagnostic case (still a strict assertion):
env -u WAYLAND_DISPLAY -u DISPLAY -u NIRI_SOCKET backend/tests/e2e.sh --transitions-only --case=entry-timeout
```

The recorder uses fast lossless `grim -t ppm` polling against a synthetic GTK
fullscreen desktop, with pixel-exact classification of **every sampled frame**
from before process start through the opaque page paint. A one-pixel diagonal
checks resampling. Browser fetch/PNG signature/MIME checks and SHA-256 of decoded
RGBA pixels verify the scheme against the known desktop. Cases include normal
and daemon entry, absent grim, timeout, two distinct monitor images, mixed scale,
and one failed output while another succeeds. Missing captures must show paper
first. Monitor removal/reconnection is covered by the regression suite.

Each recording attempt keeps PNG frames, sample timestamps, unexpected-frame
indices, process logs and semantic transcripts in a unique ignored
`backend/test-results/transitions/<timestamp>/` directory. Polling is sampling,
not a guarantee about unsampled display refreshes. Recordings contain synthetic
fixtures only; production screenshot data is never written. Set
`RHINE_TRACE_WAYLAND=1` on the recording command for protocol timing diagnostics.

## Credential handling and input limits

Backend and credential-worker processes disable core dumps and dumpability
before reading credentials. The small NUL-terminated PAM buffer is preallocated,
best-effort locked in RAM and zeroized before release. Native GTK temporary
strings and owned bridge fields are scrubbed. The maximum password is 4096
UTF-8 bytes; overlong/NUL input is rejected without contacting PAM and without
truncating it into a different password. Native Escape clears the entry.

This is not a guarantee of erasing JSC, IPC, allocator or PAM-owned copies.
WebKit runs managed code in separate vendor processes; the inherited core-size
limit alone is not a universal guarantee against system crash-collection
policies. Full-disk/swap encryption remains outside this application's control.
The conversation implements one password plus the username, not arbitrary
interactive MFA or password-change conversations.

Invalid/unreadable configuration is diagnosed without echoing its contents and
uses the default PAM service with reboot/power-off disabled. A missing config
uses the normal documented defaults. Mock-password sessions use synthetic
identity/host/session metadata and never load a real user's avatar.

The Rust minimum is checked against the locked dependency metadata, not just
this crate's syntax. GTK 4.18 is required for keyboard layout APIs; layer-shell
1.2 introduced the monitor signal, while the enforced 1.3 minimum includes
the upstream lock-refusal memory fix. No WebKit API newer than the 2.40 baseline
is used. Runtime validation on older libraries still requires a distribution
matrix; compiling against current libraries does not provide that coverage.

Release regressions cover startup contention, post-hello hangs, no handshake,
Ctrl+Shift+Esc and native Escape, bounded panic restarts, PAM worker failures and
retry, UTF-8/NUL limits, malformed bridge traffic, bounded filesystem resources,
all outputs disappearing during authentication, and signal/crash recovery.
Failures retain per-process logs/transcripts and a pre-teardown screenshot when
the process is still live. Existing baseline and real-frontend screenshots also
remain under the ignored test-results directory.

Run `python3 backend/tests/dependency_versions.py` to compare every locked
dependency's declared Rust requirement against this crate's MSRV.
