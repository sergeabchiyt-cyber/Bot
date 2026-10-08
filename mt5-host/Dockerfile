# mt5-host — the headless MT5 demo terminal host for a Linux container.
#
# Build from the repository root (Render calls this "build context"):
#
#     docker build -f mt5-host/Dockerfile -t mt5-host .
#     docker run --env-file mt5-host/.env -p 10000:10000 mt5-host
#
# One image, two processes plus a virtual display: Xvfb, the MT5 terminal under
# Wine, and mt5-host supervising both and serving /health, /readyz and
# /diagnostics on $PORT. No desktop, no VNC, no inbound port except the health
# surface — the bridge dials *out* to Node 4.
#
# Size matters here (Render free = 0.1 CPU / 512 MB), so the Rust build happens
# in a throwaway stage and the runtime keeps only what Wine and MT5 need.

# ── stage 1: build the bridge and the supervisor ─────────────────────────────
FROM rust:1.94-slim-bookworm AS builder
WORKDIR /src

# The bridge first: it is the process the host spawns, and the EA source that
# mt5-host embeds at compile time lives in the same tree.
COPY mt5-bridge/Cargo.toml mt5-bridge/Cargo.toml
COPY mt5-bridge/src mt5-bridge/src
COPY mt5-bridge/tests mt5-bridge/tests
RUN cargo build --release --manifest-path mt5-bridge/Cargo.toml

COPY mt5-host/Cargo.toml mt5-host/Cargo.toml
COPY mt5-host/src mt5-host/src
# mt5-host/src/templates.rs does include_str!("../../mt5-bridge/mql5/Mt5BridgeEA.mq5"):
# the EA shipped in the image is byte-identical to the one the bridge contract
# tests exercise, because it is compiled into the binary.
COPY mt5-bridge/mql5 mt5-bridge/mql5
RUN cargo build --release --manifest-path mt5-host/Cargo.toml

# ── stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# Which Wine to install:
#   winehq (default) — WineHQ stable for bookworm (Wine 10.x). This is the
#                      combination the working public MT5-in-Docker projects
#                      use; Debian's stock wine 8 has known MT5 crashes.
#   debian           — Debian's own wine64 + wine32:i386. No third-party apt
#                      repo, older Wine. Use it when the WineHQ repo is
#                      unreachable from your builder.
ARG MT5_HOST_WINE_FLAVOR=winehq
ARG MT5_SETUP_URL=https://download.mql5.com/cdn/web/metaquotes.software.corp/mt5/mt5setup.exe

ENV DEBIAN_FRONTEND=noninteractive

# i386 is enabled unconditionally: MT5's installer is a 32-bit executable and
# the terminal it installs is 64-bit, so the prefix must support both. Xvfb is
# the whole "headless" story — the terminal thinks it has a screen, nothing is
# ever rendered to anyone. libGL + mesa give Wine a software GL to fall back on.
RUN set -eux; \
    dpkg --add-architecture i386; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        ca-certificates curl tar \
        xvfb xauth \
        libgl1 libglx-mesa0 \
        fonts-liberation fonts-dejavu-core \
        procps; \
    if [ "$MT5_HOST_WINE_FLAVOR" = "winehq" ]; then \
        mkdir -pm755 /etc/apt/keyrings; \
        curl -fsSL https://dl.winehq.org/wine-builds/winehq.key \
            -o /etc/apt/keyrings/winehq-archive.key; \
        curl -fsSL https://dl.winehq.org/wine-builds/debian/dists/bookworm/winehq-bookworm.sources \
            -o /etc/apt/sources.list.d/winehq-bookworm.sources; \
        apt-get update; \
        apt-get install -y --install-recommends winehq-stable; \
    else \
        apt-get install -y --no-install-recommends wine64 wine32; \
    fi; \
    rm -rf /var/lib/apt/lists/*; \
    wine --version

# The unattended installer, baked in so the container does not have to trust
# download.mql5.com at 0.1 CPU on boot. A failed fetch is not fatal: the host
# falls back to MT5_HOST_INSTALLER_URL at runtime and says so on /readyz.
RUN set -eux; \
    mkdir -p /opt/mt5 /data; \
    if curl -fsSL --retry 3 --max-time 300 "$MT5_SETUP_URL" -o /opt/mt5/mt5setup.exe \
        && [ "$(head -c 2 /opt/mt5/mt5setup.exe)" = "MZ" ] \
        && [ "$(stat -c %s /opt/mt5/mt5setup.exe)" -gt 10000 ]; then \
        chmod 0755 /opt/mt5/mt5setup.exe; \
        echo "baked $(stat -c %s /opt/mt5/mt5setup.exe) bytes of mt5setup.exe"; \
    else \
        rm -f /opt/mt5/mt5setup.exe; \
        echo "warning: could not fetch mt5setup.exe at build time — the host will download it at boot"; \
    fi

COPY --from=builder /src/mt5-host/target/release/mt5-host /usr/local/bin/mt5-host
COPY --from=builder /src/mt5-bridge/target/release/mt5-bridge /usr/local/bin/mt5-bridge
COPY mt5-host/docker-entrypoint.sh /usr/local/bin/mt5-host-entrypoint
RUN chmod 0755 /usr/local/bin/mt5-host-entrypoint

# Wine settings used by every wine/wineboot/metaeditor64/terminal64 invocation.
# WINEDEBUG=-all: on 0.1 CPU the debug channel itself costs measurable time.
# WINEDLLOVERRIDES: never prompt for Mono/Gecko, those dialogs are invisible
# here and would hang an unattended boot.
ENV MT5_HOST_STATE_DIR=/data \
    MT5_HOST_INSTALLER=/opt/mt5/mt5setup.exe \
    MT5_HOST_BRIDGE_BIN=/usr/local/bin/mt5-bridge \
    MT5_HOST_DISPLAY=:99 \
    WINEPREFIX=/data/wine \
    WINEARCH=win64 \
    WINEDEBUG=-all \
    WINEDLLOVERRIDES=mscoree,mshtml= \
    DISPLAY=:99 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    RUST_LOG=info \
    MT5_TRADING_ENABLED=0 \
    PORT=10000

# No VOLUME here on purpose. Two reasons: a declared volume would mask a prefix
# baked into the image under /data, and the free tier has no persistent disk for
# it to point at anyway. /data is simply the state directory.
EXPOSE 10000
WORKDIR /app

# Render's health check should point at /health: it answers 200 as soon as the
# process is up, while /readyz stays 503 until the terminal and the bridge are
# actually connected. First boot can take minutes (Wine + install) and must not
# look like a failed deploy.
ENTRYPOINT ["/usr/local/bin/mt5-host-entrypoint"]
