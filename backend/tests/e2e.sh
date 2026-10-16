#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
if [[ -n ${WAYLAND_DISPLAY:-} || -n ${DISPLAY:-} || -n ${NIRI_SOCKET:-} ]]; then
  echo 'Refusing: launch this script with WAYLAND_DISPLAY, DISPLAY and NIRI_SOCKET unset.' >&2
  exit 1
fi
cargo build --manifest-path backend/Cargo.toml --features mock-auth --locked
mkdir -p backend/test-results
exec tools/headless.sh env RHINE_HEADLESS_TEST=1 python3 backend/tests/e2e.py "$@"
