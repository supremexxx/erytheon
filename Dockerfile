# syntax=docker/dockerfile:1.7

FROM rust:1.94-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p engine && \
    cp /src/target/release/pyrorisk /tmp/pyrorisk

FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install --yes --no-install-recommends ca-certificates curl gdal-bin && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --create-home --uid 10001 --shell /usr/sbin/nologin pyrorisk && \
    mkdir -p /app/out /data && \
    chown -R pyrorisk:pyrorisk /app /data

WORKDIR /app
COPY --from=builder /tmp/pyrorisk /usr/local/bin/pyrorisk
COPY --chown=pyrorisk:pyrorisk testdata ./testdata

USER pyrorisk

ENV API_BIND=0.0.0.0:8080 \
    RUST_LOG=info,pyrorisk=info

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:8080/health >/dev/null || exit 1

ENTRYPOINT ["pyrorisk"]
CMD ["run"]
