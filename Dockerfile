FROM rust:1.94-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
COPY static ./static
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xauusd-engine /usr/local/bin/engine
EXPOSE 10000
CMD ["engine"]
