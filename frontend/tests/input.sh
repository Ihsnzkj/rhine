#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Input-path regressions of the built frontend in the real lock (mock-auth
# backend, private headless compositor, real keys).  Exit 0 = all passed.
set -eu
cd "$(dirname "$0")/../.."
if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}${NIRI_SOCKET:-}" ]; then
    echo "Refusing: run with WAYLAND_DISPLAY, DISPLAY and NIRI_SOCKET unset." >&2
    exit 1
fi
cargo build --manifest-path backend/Cargo.toml --features mock-auth
(cd frontend && npm run build >/dev/null)
mkdir -p backend/test-results
exec tools/headless.sh env RHINE_HEADLESS_TEST=1 python3 frontend/tests/input.py "$@"
