FROM rust:slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/xauvp /usr/local/bin/xauvp
COPY config.toml /config.toml
EXPOSE 8080
CMD ["xauvp"]
