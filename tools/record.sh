#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Record the frontend (MiniBrowser on the private headless GPU compositor,
# in-page mock backend) at 60 fps.
#   tools/record.sh <out.mkv> <seconds> [query-string]
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
    wf-recorder -y -o HEADLESS-1 -r 60 -c libx264 -p crf=14 -p preset=veryfast -f "$out" \
        > "$XDG_RUNTIME_DIR/recorder.log" 2>&1 &
    rec=$!
    mb=""
    stop_recorder() {
        stopping_rec=$rec
        rec=""
        kill -INT "$stopping_rec" 2>/dev/null || true
        for i in $(seq 1 40); do
            kill -0 "$stopping_rec" 2>/dev/null || break
            sleep 0.05
        done
        # An idle compositor may never deliver the frame wf-recorder is
        # waiting on. Bound teardown even when SIGINT cannot finish it.
        kill -KILL "$stopping_rec" 2>/dev/null || true
        wait "$stopping_rec"
    }
    cleanup() {
        rc=$?
        trap - EXIT INT TERM HUP
        if [ -n "$mb" ]; then kill "$mb" 2>/dev/null || true; wait "$mb" 2>/dev/null || true; fi
        if [ -n "$rec" ]; then stop_recorder 2>/dev/null || true; fi
        if [ "$rc" != 0 ]; then
            cat "$XDG_RUNTIME_DIR/recorder.log" "$XDG_RUNTIME_DIR/minibrowser.log" > "$out.log"
            echo "recording failed; log: $out.log" >&2
        fi
        exit "$rc"
    }
    trap cleanup EXIT
    trap "exit 130" INT TERM HUP
    : > "$XDG_RUNTIME_DIR/minibrowser.log"
    sleep 0.4
    "$tools/minibrowser.sh" --enable-write-console-messages-to-stdout=true --full-screen "$url" \
        > "$XDG_RUNTIME_DIR/minibrowser.log" 2>&1 &
    mb=$!
    sleep "$secs"
    kill -0 "$mb" 2>/dev/null && kill -0 "$rec" 2>/dev/null || exit 1
    stop_recorder
    [ -s "$out" ] || { tail -n 20 "$XDG_RUNTIME_DIR/recorder.log" >&2; exit 1; }' \
    sh "$out" "$secs" "http://127.0.0.1:$port/?$query" "$root/tools"
