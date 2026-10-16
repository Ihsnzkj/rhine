#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Screenshot the built frontend in WebKitGTK (MiniBrowser) on the private
# headless GPU compositor, with the in-page mock backend.
#   tools/shot.sh <out.png> <seconds> [query-string]     e.g. "scenario=denied&q=ultra"
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=$(realpath -m "$1"); secs=$2; query=${3:-}
port=$((4300 + $$ % 500))
(cd "$root/frontend" && exec node node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port "$port" --strictPort >/dev/null 2>&1) &
srv=$!
trap 'kill $srv 2>/dev/null || true; wait $srv 2>/dev/null || true' EXIT
trap 'exit 130' INT TERM HUP
ready=0
for i in $(seq 1 100); do
    kill -0 "$srv" 2>/dev/null || break
    if curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null; then ready=1; break; fi
    sleep 0.1
done
[ "$ready" = 1 ] && kill -0 "$srv" 2>/dev/null || { echo "preview server did not start" >&2; exit 1; }
"$root/tools/headless.sh" sh -c '
    set -eu
    out=$1 secs=$2 url=$3 tools=$4
    "$tools/minibrowser.sh" --enable-write-console-messages-to-stdout=true --full-screen "$url" \
        > "$XDG_RUNTIME_DIR/minibrowser.log" 2>&1 &
    mb=$!
    cleanup() {
        rc=$?
        trap - EXIT INT TERM HUP
        kill "$mb" 2>/dev/null || true; wait "$mb" 2>/dev/null || true
        if [ "$rc" != 0 ]; then
            cp "$XDG_RUNTIME_DIR/minibrowser.log" "$out.log"
            echo "browser failed; log: $out.log" >&2
        fi
        exit "$rc"
    }
    trap cleanup EXIT
    trap "exit 130" INT TERM HUP
    sleep "$secs"
    kill -0 "$mb" 2>/dev/null || exit 1
    grim "$out"' \
    sh "$out" "$secs" "http://127.0.0.1:$port/?$query" "$root/tools"
