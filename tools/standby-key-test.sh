#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Regression: the key that wakes the lock from standby must not reach the key
# field.  Real keystrokes (wtype) into the built frontend in WebKitGTK
# (MiniBrowser, in-page mock backend) on the private headless compositor.
# Exit 0 = pass; on failure the browser log is kept and its path printed.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
log=$(mktemp "${TMPDIR:-/tmp}/rl-standby.XXXXXX")
port=$((4300 + $$ % 500))
(cd "$root/frontend" && exec node node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port "$port" --strictPort >/dev/null 2>&1) &
srv=$!
ok=0
trap 'kill $srv 2>/dev/null || true; wait $srv 2>/dev/null || true; if [ "$ok" = 1 ]; then rm -f "$log"; else echo "browser log kept: $log" >&2; fi' EXIT
trap 'exit 130' INT TERM HUP
ready=0
for i in $(seq 1 100); do
    kill -0 "$srv" 2>/dev/null || break
    if curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null; then ready=1; break; fi
    sleep 0.1
done
[ "$ready" = 1 ] && kill -0 "$srv" 2>/dev/null || { echo "preview server did not start" >&2; exit 1; }

"$root/tools/headless.sh" sh -c '
    log=$1 url=$2 tools=$3
    "$tools/minibrowser.sh" --enable-write-console-messages-to-stdout=true --full-screen "$url" > "$log" 2>&1 &
    mb=$!
    phase() { grep -o "PHASE [a-z]*" "$log" | tail -n 1; }
    until_() { for i in $(seq 1 150); do eval "$1" && return 0; sleep 0.1; done; echo "timeout: $1" >&2; return 1; }
    until_ "[ \"\$(phase)\" = \"PHASE standby\" ]" || { kill $mb; exit 1; }
    sleep 0.5                      # let the compositor focus the window
    wtype -k F13 -s 300 x          # wakes the terminal: must be swallowed
    until_ "[ \"\$(phase)\" = \"PHASE ready\" ]" || { kill $mb; exit 1; }
    wtype -k F13 -s 300 ab         # real typing
    until_ "grep -q \"KEYLEN 2\" \"\$log\"" || true
    sleep 0.5                      # a leaked x would show up as KEYLEN 3
    kill $mb' sh "$log" "http://127.0.0.1:$port/?scenario=standby" "$root/tools"

phases=$(grep -o 'PHASE [a-z]*' "$log" | tr '\n' ' ')
last=$(grep -o 'KEYLEN [0-9]*' "$log" | tail -n 1 | cut -d' ' -f2)
echo "phases: $phases"
echo "key length after wake + 'ab': ${last:-0}"
case "$phases" in *"PHASE standby PHASE ready"*) ;; *) echo "FAIL: never woke from standby"; exit 1;; esac
[ "${last:-0}" = 2 ] || { echo "FAIL: expected 2"; exit 1; }
ok=1
echo PASS
