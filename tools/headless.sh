#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Start a private, headless, GPU-accelerated sway (240 Hz output) and run a
# command in it.  Nothing inside can reach your real session:
#   tools/headless.sh <command...>
# The command gets WAYLAND_DISPLAY, SWAYSOCK and its own XDG_RUNTIME_DIR
# (under $TMPDIR); NIRI_SOCKET / DISPLAY are unset and, where
# dbus-run-session exists, it runs on a private session bus.  If the
# compositor does not come up, or the command fails, the compositor log is
# kept and its path printed.
set -u
here=$(cd "$(dirname "$0")" && pwd)
rt=$(mktemp -d "${TMPDIR:-/tmp}/rl-headless.XXXXXX") || exit 1
unset NIRI_SOCKET DISPLAY WAYLAND_DISPLAY WAYLAND_SOCKET SWAYSOCK
export XDG_RUNTIME_DIR="$rt"
WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$here/bench/sway.conf" > "$rt/sway.log" 2>&1 &
spid=$!
keep=0
cleanup() {
    kill "$spid" 2>/dev/null; wait "$spid" 2>/dev/null
    if [ "$keep" = 1 ] && cp "$rt/sway.log" "$rt.sway.log" 2>/dev/null; then
        echo "headless: compositor log kept at $rt.sway.log" >&2
    fi
    rm -rf "$rt"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# ready = alive, with both its Wayland socket and its IPC socket
sock="" ipc=""
for i in $(seq 1 100); do
    kill -0 "$spid" 2>/dev/null || break
    sock=$(ls "$rt" | grep -E '^wayland-[0-9]+$' | head -1)
    ipc=$(ls "$rt" | grep -E '^sway-ipc\..*\.sock$' | head -1)
    [ -n "$sock" ] && [ -n "$ipc" ] && break
    sleep 0.1
done
if [ -z "$sock" ] || [ -z "$ipc" ] || ! kill -0 "$spid" 2>/dev/null; then
    keep=1
    echo "headless: the private compositor did not start" >&2
    tail -n 20 "$rt/sway.log" >&2
    exit 1
fi
export WAYLAND_DISPLAY="$sock" SWAYSOCK="$rt/$ipc"

if command -v dbus-run-session >/dev/null 2>&1; then
    dbus-run-session --config-file="$here/bench/dbus-session.conf" -- "$@"
else
    env -u DBUS_SESSION_BUS_ADDRESS "$@"
fi
rc=$?
[ "$rc" = 0 ] || keep=1
exit "$rc"
