#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Run WebKitGTK 6.0's MiniBrowser, wherever this distribution installs it
# (override with MINIBROWSER=/path/to/MiniBrowser).
for mb in "${MINIBROWSER:-}" /usr/lib/webkitgtk-6.0/MiniBrowser /usr/libexec/webkitgtk-6.0/MiniBrowser \
          /usr/lib64/webkitgtk-6.0/MiniBrowser /usr/lib/*/webkitgtk-6.0/MiniBrowser; do
    [ -n "$mb" ] && [ -x "$mb" ] && exec "$mb" "$@"
done
echo "MiniBrowser (WebKitGTK 6.0) not found — set MINIBROWSER=/path/to/MiniBrowser" >&2
exit 127
