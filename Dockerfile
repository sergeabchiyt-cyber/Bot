# Node 1 — the read-only XAUUSD market-data and calendar feed (C1 v2).
#
# One crate (`xauusd-engine`), one process, no external tools: the image runs
# the feed binary and nothing else. Node 2 (dashboard), Node 3 (strategy) and
# Node 4 (execution) are separate deployments on separate branches.
FROM rust:1.94-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
# The contract version is baked into the binary; the contracts themselves are
# the signed documents in ./contracts (see ci/contract_sync.py).
RUN cargo build --release

FROM debian:trixie-slim
# ca-certificates is all the runtime needs: HTTPS to the history API and to the
# ForexFactory weekly export. No browser, no media tooling.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-engine /usr/local/bin/engine
EXPOSE 10000
CMD ["engine"]
