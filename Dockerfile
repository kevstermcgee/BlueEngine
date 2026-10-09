# BlueEngine rendering-free dedicated server; locked native dependencies.
FROM rust:1.87-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tools ./tools
COPY templates ./templates
COPY assets ./assets
COPY deploy ./deploy
COPY docs ./docs
RUN cargo build --locked --release --no-default-features --bin be2-headless --bin be2-tools

FROM debian:bookworm-slim AS runtime
RUN groupadd -g 1000 blueengine && useradd -u 1000 -g blueengine -m blueengine
WORKDIR /app
COPY --from=builder /build/target/release/be2-headless /usr/local/bin/be2-headless
COPY --from=builder /build/target/release/be2-tools /usr/local/bin/be2-tools
RUN chown blueengine:blueengine /app
USER blueengine
EXPOSE 7777/udp
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD kill -0 1 || exit 1
# Production requires a mounted PKCS#8 key and the matching public DER pin;
# BLUE_TLS_KEY_FILE / BLUE_TLS_CERT_FILE select them, as on the native server.
ENTRYPOINT ["/usr/local/bin/be2-headless"]
CMD ["--server", "0.0.0.0:7777", "--transport", "production"]
