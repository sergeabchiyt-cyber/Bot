# Node1 — market data + volume profile + built-in strategy + demo simulator.
# This image builds the single Node1 crate (`xauusd-engine`). It contains no
# broker credential, no execution service and no MT5 bridge: the demo account
# (1:1000, 0.10 spread, 0.01-1.00 lots) fills locally and can never reach a
# broker. Node3 (AI strategy) and Node4 (live execution/MT5) are separate
# deployments on separate branches.
FROM rust:1.94-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:trixie-slim
# ffmpeg decodes live econ-news streams to 16kHz mono PCM for Node3;
# yt-dlp resolves YouTube/other live stream pages to direct media URLs.
RUN apt-get update \
    && apt-get install -y ca-certificates ffmpeg python3-pip \
    && pip3 install --no-cache-dir --break-system-packages yt-dlp \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-engine /usr/local/bin/engine
EXPOSE 10000
CMD ["engine"]
