FROM rust:1.94-slim AS builder
WORKDIR /app
COPY node3-execution/Cargo.toml ./
COPY node3-execution/src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-node3-execution /usr/local/bin/node3-execution
EXPOSE 10000
ENV PORT=10000
CMD ["node3-execution"]
