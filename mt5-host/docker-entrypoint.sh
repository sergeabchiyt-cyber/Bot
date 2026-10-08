#!/bin/sh
# mt5-host container entrypoint.
#
# The only thing that has to happen before the supervisor starts is seeding a
# writable Wine prefix. Everything else — display, prefix boot, unattended
# terminal install, EA compile, startup ini, terminal launch, bridge — is done
# by the mt5-host binary itself, in Rust, so this script stays deliberately dumb
# and never grows business logic.
#
# Two prefix sources are supported, in priority order:
#   1. a prefix baked into the image at $MT5_HOST_BAKED_PREFIX (built by
#      ci/mt5/Dockerfile.bundle-image) — copies in seconds, needs no network;
#   2. nothing: the host runs `mt5setup.exe /auto` itself (or unpacks
#      MT5_HOST_PREFIX_ARCHIVE_URL first).
#
# A free Render instance has no persistent disk, so source (1) is what makes a
# redeploy cheap: the terminal is already installed inside the image.

set -eu

BAKED="${MT5_HOST_BAKED_PREFIX:-/opt/mt5-prefix}"
STATE="${MT5_HOST_STATE_DIR:-/data}"
PREFIX="${WINEPREFIX:-$STATE/wine}"

log() {
    printf 'entrypoint: %s\n' "$*"
}

mkdir -p "$STATE"

if [ -d "$BAKED/drive_c" ] && [ ! -d "$PREFIX/drive_c" ]; then
    log "seeding the Wine prefix from $BAKED into $PREFIX"
    mkdir -p "$(dirname "$PREFIX")"
    started=$(date +%s)
    if cp -a "$BAKED" "$PREFIX" 2>/dev/null; then
        log "prefix copied in $(( $(date +%s) - started ))s"
    else
        # A hard failure here is usually disk space or an unwritable state dir.
        # Wine only needs the prefix to be writable, and the container layer is,
        # so a symlink keeps the deployment usable and says so in the log.
        log "copy failed; linking $PREFIX -> $BAKED instead"
        rm -rf "$PREFIX"
        ln -sfn "$BAKED" "$PREFIX"
    fi
elif [ -d "$BAKED/drive_c" ]; then
    log "using the existing prefix at $PREFIX (image prefix left untouched)"
fi

exec mt5-host "$@"
