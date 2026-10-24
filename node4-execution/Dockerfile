# Node 4 execution service image (execution + MT5 bridge plumbing only).
#   docker build -f node4-execution/Dockerfile -t node4-execution node4-execution
# (build context is the crate directory, so ./Cargo.toml and ./src are its own)
FROM rust:1.94-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-node4-execution /usr/local/bin/node4-execution
RUN ln -s /usr/local/bin/node4-execution /usr/local/bin/node3-execution \
    && ln -s /usr/local/bin/node4-execution /usr/local/bin/xauusd-node4-execution
EXPOSE 10000
ENV PORT=10000 \
    RUST_LOG=info
WORKDIR /app
CMD ["node4-execution"]
