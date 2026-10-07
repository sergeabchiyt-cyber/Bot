# Node 4 execution service image.
#   docker build -f node4-execution/Dockerfile -t node4-execution node4-execution
# (build context is the crate directory, so ./Cargo.toml and ./src are its own)
FROM rust:1.94-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-node4-execution /usr/local/bin/node4-execution
EXPOSE 10000
ENV PORT=10000
CMD ["node4-execution"]
