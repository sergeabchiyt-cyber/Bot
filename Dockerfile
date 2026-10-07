# Node 4 execution service image.
#
#   docker build -f Dockerfile -t xauusd-node4-execution .
#   docker run --env-file node4-execution/.env -p 10000:10000 xauusd-node4-execution
#
# This image contains the execution service only: Node 3 intent consumption,
# venue policy, the Deriv / Chelsea / MT5 execution paths, the durable
# idempotency ledger, and the operator resources (`GET /health`,
# `GET /diagnostics`, `GET /mt5/*`, `POST /mt5/control`, `WS /ws`,
# `WS /mt5/bridge`). It holds no market-data engine and no strategy code.
#
# The MT5 demo bridge is a *separate* service that runs on the machine with the
# MT5 terminal (see mt5-bridge/README.md and docs/mt5/EXECUTION_ARCHITECTURE.md)
# and dials out to this one; it is deliberately not part of this image.

FROM rust:1.94-slim AS builder
WORKDIR /app
COPY node4-execution/Cargo.toml ./node4-execution/Cargo.toml
COPY node4-execution/src ./node4-execution/src
RUN cargo build --release --manifest-path node4-execution/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/node4-execution/target/release/xauusd-node4-execution /usr/local/bin/node4-execution

# Public read-only resources + the private bridge/control endpoints.
EXPOSE 10000
ENV PORT=10000 \
    RUST_LOG=info \
    EXECUTION_LEDGER_FILE=/data/execution_ledger.jsonl
VOLUME ["/data"]
WORKDIR /app
CMD ["node4-execution"]
