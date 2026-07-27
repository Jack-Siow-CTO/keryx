# Multi-stage build for the Keryx Worker binary.
# Prefer native install on always-on hosts; Docker is optional.

FROM rust:1.85-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates ./crates
RUN cargo build -p keryx-worker --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home /var/lib/keryx --shell /usr/sbin/nologin keryx \
    && mkdir -p /var/lib/keryx \
    && chown keryx:keryx /var/lib/keryx

COPY --from=builder /src/target/release/keryx /usr/local/bin/keryx
USER keryx
WORKDIR /var/lib/keryx
ENV KERYX_BIND=127.0.0.1:8787 \
    KERYX_DATA_DIR=/var/lib/keryx \
    KERYX_DEFAULT_PROVIDER=fake
# With docker compose network_mode: host, this probes the host loopback.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8787/health || exit 1
ENTRYPOINT ["/usr/local/bin/keryx"]
