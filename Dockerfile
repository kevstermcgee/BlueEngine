# ------------------------------------------------------------------------------
# BlueEngine Headless Dedicated Server Dockerfile
# Multi-stage minimal production container
# ------------------------------------------------------------------------------

# Stage 1: Build
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release --bin be2-headless --no-default-features || true
RUN rm -rf src

# Copy source tree
COPY src ./src
COPY tools/FEATURES.json ./tools/FEATURES.json

# Build optimized production binary with no windowing/graphics dependencies
RUN cargo build --release --bin be2-headless --bin be2-tools --no-default-features

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

RUN groupadd -g 1000 blueengine && \
    useradd -u 1000 -g blueengine -s /bin/bash -m blueengine

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /build/target/release/be2-headless /usr/local/bin/be2-headless
COPY --from=builder /build/target/release/be2-tools /usr/local/bin/be2-tools

# Set permissions
RUN chown -R blueengine:blueengine /app

USER blueengine

# Default authoritative server port (UDP)
EXPOSE 7777/udp

# Healthcheck
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD pidof be2-headless || exit 1

ENTRYPOINT ["/usr/local/bin/be2-headless"]
CMD ["0.0.0.0:7777"]
