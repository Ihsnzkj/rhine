#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# rhine-lock installer: build as you, install with sudo, write the PAM
# service, verify it — and roll everything back if the verification fails.
# It never edits your compositor or idle-daemon configuration.
#
#   ./install.sh                  build + install to /usr/local
#   ./install.sh --prefix /usr    install under another absolute prefix
#   ./install.sh --uninstall      remove binary/UI (same --prefix); keep PAM
#   --allow-dirty                 install even if backend/ has uncommitted
#                                 changes (refused by default)
#
# Run it as your normal user; it asks for root (sudo or doas) only for the
# install step.  /etc/pam.d/rhine-lock and /etc/rhine-lock are global
# whatever the prefix; staged installs (DESTDIR) are not supported.
#
# Requirements: a Wayland compositor with ext-session-lock-v1 (niri, sway,
# Hyprland, river, …), GTK >= 4.18, gtk4-layer-shell >= 1.3, WebKitGTK 6.0
# >= 2.40, PAM, Rust >= 1.92, Node.js 20.19+ or 22.12+, pkg-config.
# Optional: grim (seamless entry/exit), ffmpeg + numpy (tools/fetch-bgm.py).
set -eu
ROOT=$(cd "$(dirname "$0")" && pwd)
PREFIX=/usr/local
PAM=/etc/pam.d/rhine-lock
PAM_MARK="# rhine-lock screen locker"
UNINSTALL=0
ALLOW_DIRTY=0

die() { echo "error: $*" >&2; exit 1; }
while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)
            [ $# -ge 2 ] && [ -n "$2" ] || die "--prefix needs a directory"
            PREFIX=$2; shift 2 ;;
        --prefix=*) PREFIX=${1#--prefix=}; shift ;;
        --uninstall|--remove) UNINSTALL=1; shift ;;
        --allow-dirty) ALLOW_DIRTY=1; shift ;;
        -h|--help) sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown option: $1 (see --help)" ;;
    esac
done
case "$PREFIX" in /*) ;; *) die "--prefix must be an absolute path (got '$PREFIX')" ;; esac
PREFIX=${PREFIX%/}
BIN="$PREFIX/bin/rhine-lock"
SHARE="$PREFIX/share/rhine-lock/frontend"

# root only for the install step; building as root leaves root-owned files in the tree
if [ "$(id -u)" = 0 ]; then
    [ -z "${SUDO_USER:-}${DOAS_USER:-}" ] || die "run ./install.sh as your user, not through sudo/doas — it asks for root itself"
    SUDO=""
    echo "note: running as root (no sudo needed)"
elif command -v sudo >/dev/null 2>&1; then SUDO=sudo
elif command -v doas >/dev/null 2>&1; then SUDO=doas
else die "need sudo or doas for the install step (or run as root)"
fi

ours() { [ -f "$1" ] && grep -q "^$PAM_MARK" "$1"; }
exists() { [ -e "$1" ] || [ -L "$1" ]; }

# PAM is shared by every prefix. Serialize installs and removals, and refuse
# stale transaction files rather than deleting the only recoverable copy.
INSTALL_LOCK=/etc/.rhine-lock-install.lock
lock_install() {
    $SUDO mkdir -m700 "$INSTALL_LOCK" 2>/dev/null || \
        die "another install/removal is active, or $INSTALL_LOCK needs recovery"
    trap '$SUDO rmdir "$INSTALL_LOCK"' EXIT
    trap 'exit 130' INT TERM HUP
    for path in "$BIN" "$SHARE" "$PAM"; do
        for suffix in rhine-new rhine-old; do
            ! exists "$path.$suffix" || die "unfinished installation: recover $path.$suffix before retrying"
        done
    done
}

if [ "$UNINSTALL" = 1 ]; then
    lock_install
    $SUDO rm -f "$BIN"
    $SUDO rm -rf "$PREFIX/share/rhine-lock"
    # This service is shared across prefixes and may still be needed by a
    # running locker. Package-style removal keeps authentication config.
    if exists "$PAM"; then echo "note: shared $PAM kept for other installations or running lockers"; fi
    echo "rhine-lock removed from $PREFIX (your compositor configuration still mentions it if you added it there)"
    exit 0
fi

# ── preflight: everything is checked before anything is built or touched ──
need() { command -v "$1" >/dev/null 2>&1 || die "missing $1 — $2"; }
need cargo "Rust >= 1.92 (rustup.rs or your distribution)"
need rustc "Rust >= 1.92"
need npm "Node.js 20.19+ or 22.12+ with npm"
need node "Node.js 20.19+ or 22.12+"
need pkg-config "pkg-config / pkgconf"

ver_ge() {   # ver_ge 1.92.0 1.92  →  true
    [ "$(printf '%s\n%s\n' "$2" "$1" | sort -t. -k1,1n -k2,2n -k3,3n | head -1)" = "$2" ]
}
rv=$(rustc --version | sed -n 's/^rustc \([0-9][0-9.]*\).*/\1/p')
ver_ge "${rv:-0}" 1.92 || die "Rust $rv is too old: the GTK bindings need >= 1.92 (rustup update)"
nv=$(node -p 'process.versions.node')
case "$nv" in
    20.*) ver_ge "$nv" 20.19 || die "Node.js $nv is too old: Vite needs 20.19+ or 22.12+" ;;
    21.*) die "Node.js $nv is not supported by Vite: use 20.19+ or 22.12+" ;;
    22.*) ver_ge "$nv" 22.12 || die "Node.js $nv is too old: Vite needs 20.19+ or 22.12+" ;;
    *) ver_ge "$nv" 23 || die "Node.js $nv is too old: Vite needs 20.19+ or 22.12+" ;;
esac
missing=""
for spec in "gtk4 4.18" "gtk4-layer-shell-0 1.3" "webkitgtk-6.0 2.40"; do
    set -- $spec
    if ! have=$(pkg-config --modversion "$1" 2>/dev/null); then
        missing="$missing\n  $1 >= $2: not found (install its development package)"
    elif ! pkg-config --atleast-version="$2" "$1"; then
        missing="$missing\n  $1 >= $2: found $have"
    fi
done
[ -z "$missing" ] || die "$(printf "build dependencies are missing or too old:$missing\nsee README.md → Requirements")"
command -v grim >/dev/null 2>&1 || \
    echo "note: grim not found — locking works, but without the seamless desktop transition"

# the PAM stack this distribution provides (/etc, or the vendor dir /usr/lib);
# "include", not Debian's "@include", which openSUSE's PAM does not understand
pamfile() { [ -f "/etc/pam.d/$1" ] || [ -f "/usr/lib/pam.d/$1" ]; }
if pamfile common-auth && pamfile common-account; then      # Debian, Ubuntu, openSUSE
    STACK='auth     include  common-auth\naccount  include  common-account\n'
elif pamfile login; then                                    # Arch, Fedora, Gentoo, …
    STACK='auth     include  login\naccount  include  login\n'
elif pamfile system-auth; then                              # minimal Fedora/RHEL without login
    STACK='auth     include  system-auth\naccount  include  system-auth\n'
else
    die "no common-auth/common-account, login or system-auth PAM service found — write $PAM by hand (auth + account) and rerun"
fi

if [ "$ALLOW_DIRTY" = 0 ] && command -v git >/dev/null 2>&1 \
   && git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
   && [ -n "$(git -C "$ROOT" status --porcelain -- backend)" ]; then
    die "backend/ has uncommitted changes — refusing to install a work in progress (--allow-dirty overrides)"
fi

# ── build (as you) ──
echo "── building frontend"
cd "$ROOT/frontend"
npm ci --no-audit --no-fund
[ -f public/audio/bgm-loop.opus ] || \
    echo "note: no score — run tools/fetch-bgm.py for the optional music (personal use)"
npm run build

echo "── building backend"
cd "$ROOT/backend"
cargo build --release --locked

# ── install: stage, swap, verify, roll back on failure ──
echo "── installing to $PREFIX"
[ -n "$SUDO" ] && $SUDO true            # one password prompt, before anything changes
lock_install
STAGED=0
# Snapshot presence before the first rename. A failed/interrupted backup must
# leave its untouched original alone; absence of .rhine-old is not evidence
# that the original did not exist.
BIN_HAD=0 SHARE_HAD=0 PAM_NEW=0
exists "$BIN" && BIN_HAD=1
exists "$SHARE" && SHARE_HAD=1
exists "$PAM" || PAM_NEW=1
rollback() {
    [ "$STAGED" = 1 ] || return 0
    echo "── rolling back" >&2
    if exists "$BIN.rhine-old"; then $SUDO mv -f "$BIN.rhine-old" "$BIN" || return 1
    elif [ "$BIN_HAD" = 0 ]; then $SUDO rm -f "$BIN" || return 1; fi
    if exists "$SHARE.rhine-old"; then
        $SUDO rm -rf "$SHARE" && $SUDO mv "$SHARE.rhine-old" "$SHARE" || return 1
    elif [ "$SHARE_HAD" = 0 ]; then $SUDO rm -rf "$SHARE" || return 1; fi
    if exists "$PAM.rhine-old"; then $SUDO mv -f "$PAM.rhine-old" "$PAM" || return 1
    elif [ "$PAM_NEW" = 1 ]; then $SUDO rm -f "$PAM" || return 1; fi
    echo "the previous installation is back in place" >&2
}
finish_install() {
    rc=$?
    trap - EXIT INT TERM HUP
    if ! rollback; then
        echo "error: rollback failed; preserved backups and $INSTALL_LOCK for recovery" >&2
        exit 1
    fi
    $SUDO rm -rf "$BIN.rhine-new" "$SHARE.rhine-new" "$PAM.rhine-new"
    $SUDO rmdir "$INSTALL_LOCK"
    exit "$rc"
}
trap finish_install EXIT

$SUDO install -Dm755 target/release/rhine-lock "$BIN.rhine-new"
$SUDO mkdir -p "$(dirname "$SHARE")"
$SUDO cp -R "$ROOT/frontend/dist" "$SHARE.rhine-new"
$SUDO chmod -R u=rwX,go=rX "$SHARE.rhine-new"

if exists "$PAM" && ! ours "$PAM"; then
    echo "note: keeping your own $PAM (it was not written by install.sh)"
else
    # rhine-lock calls pam_authenticate, pam_acct_mgmt and pam_setcred: it needs
    # auth AND account stacks (without account, PAM falls through to "other"
    # = deny, and even the right password cannot unlock)
    printf "#%%PAM-1.0\n$PAM_MARK (written by install.sh)\n$STACK" | $SUDO tee "$PAM.rhine-new" >/dev/null
    $SUDO chmod 644 "$PAM.rhine-new"
fi

STAGED=1
[ "$BIN_HAD" = 0 ] || $SUDO mv -f "$BIN" "$BIN.rhine-old"
$SUDO mv -f "$BIN.rhine-new" "$BIN"
[ "$SHARE_HAD" = 0 ] || $SUDO mv "$SHARE" "$SHARE.rhine-old"
$SUDO mv "$SHARE.rhine-new" "$SHARE"
if [ -e "$PAM.rhine-new" ]; then
    # Rename is atomic: an interrupted copy must not become the rollback PAM.
    [ "$PAM_NEW" = 1 ] || $SUDO mv -f "$PAM" "$PAM.rhine-old"
    $SUDO mv -f "$PAM.rhine-new" "$PAM"
fi

# The installed binary's own preflight: the account and credential stacks for
# you, without authenticating (so it can never count towards pam_faillock).
# It cannot prove that your password or a second factor works — lock once
# and unlock to be sure.
if ! "$BIN" --check-pam; then
    echo "error: the rhine-lock PAM service rejects your account — see the output above" >&2
    exit 1                                  # the EXIT trap restores the previous files
fi

STAGED=0
$SUDO rm -rf "$BIN.rhine-old" "$SHARE.rhine-old" "$PAM.rhine-old"

cat <<EOF

rhine-lock is installed: $BIN
Lock now:            rhine-lock
From idle / sleep:   rhine-lock --daemonize   (returns once the screen is locked)

Hook it into your compositor (install.sh never edits these files), e.g.
  niri  (binds.kdl):  Mod+Alt+L { spawn "rhine-lock" "--daemonize"; }
  sway  (config):     bindsym \$mod+Alt+l exec rhine-lock --daemonize
  Hyprland:           bind = SUPER ALT, L, exec, rhine-lock --daemonize
  swayidle:           swayidle -w timeout 300 'rhine-lock --daemonize' \\
                               before-sleep 'rhine-lock --daemonize' lock 'rhine-lock --daemonize'
  hypridle:           general { lock_cmd = rhine-lock --daemonize }
EOF
