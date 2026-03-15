FROM rust:1.75-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release --no-default-features -p herald-daemon -p herald-cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates jq && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/heraldd /usr/local/bin/
COPY --from=builder /app/target/release/herald /usr/local/bin/

ENV HERALD_CONTAINER=1
ENV RUST_LOG=info

EXPOSE 7272

ENTRYPOINT ["heraldd"]
