FROM rust:1.94-slim AS builder
WORKDIR /app
COPY node3-strategy/Cargo.toml ./
COPY node3-strategy/src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-node3-strategy /usr/local/bin/node3-strategy
EXPOSE 10000
ENV PORT=10000
CMD ["node3-strategy"]
