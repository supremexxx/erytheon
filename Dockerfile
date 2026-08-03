# syntax=docker/dockerfile:1.7

FROM rust:1.94-bookworm AS builder

ARG ERYTHEON_GIT_COMMIT=unknown
ENV ERYTHEON_GIT_COMMIT=${ERYTHEON_GIT_COMMIT}

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p engine && \
    cp /src/target/release/pyrorisk /tmp/pyrorisk

FROM debian:bookworm-slim AS runtime

ARG OCI_REVISION=unknown
ARG OCI_CREATED=unknown
ARG OCI_TITLE=erytheon
ARG ERYTHEON_PHASE=unknown
ARG ERYTHEON_SCIENCE_CONSOLE=false

LABEL org.opencontainers.image.revision="${OCI_REVISION}" \
      org.opencontainers.image.created="${OCI_CREATED}" \
      org.opencontainers.image.title="${OCI_TITLE}" \
      erytheon.phase="${ERYTHEON_PHASE}" \
      erytheon.science_console="${ERYTHEON_SCIENCE_CONSOLE}"

RUN apt-get update && \
    apt-get install --yes --no-install-recommends ca-certificates curl gdal-bin libeccodes-tools && \
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
