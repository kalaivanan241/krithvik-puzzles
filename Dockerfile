# syntax=docker/dockerfile:1

FROM node:22-bookworm-slim AS frontend
WORKDIR /build
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm \
    npm ci --no-audit --no-fund
COPY index.html tsconfig.json vite.config.ts ./
COPY public ./public
COPY src ./src
RUN npm run build

FROM rust:1.99-bookworm AS backend
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build/backend
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL=1 \
    CARGO_PROFILE_RELEASE_LTO=off \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_STRIP=symbols \
    CARGO_TERM_COLOR=never
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/build/backend/target,sharing=locked \
    cargo build --release --locked --no-default-features \
    && cp target/release/peekaboo-pals-api /peekaboo-pals-api

FROM debian:bookworm-slim AS runtime
LABEL org.opencontainers.image.title="Peekaboo Pals" \
      org.opencontainers.image.description="Peekaboo Pals frontend, API, uploads, and MCP in one Rust process"
# Only the SQLite runtime is needed: ElevenLabs HTTPS calls use bundled
# rustls root certificates, so no CA package or HTTP client is installed.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 app \
    && useradd --uid 10001 --gid app --no-create-home app \
    && mkdir -p /app/dist /data \
    && chown app:app /data
WORKDIR /app
COPY --from=backend --chmod=0555 /peekaboo-pals-api /app/peekaboo-pals-api
COPY --from=frontend /build/dist /app/dist
ENV BIND_ADDR=0.0.0.0:3001 \
    FRONTEND_DIST=/app/dist \
    DATABASE_PATH=/data/peekaboo.sqlite \
    UPLOAD_DIR=/data/uploads \
    RUST_LOG=info
USER app
EXPOSE 3001
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/app/peekaboo-pals-api", "--healthcheck"]
ENTRYPOINT ["/app/peekaboo-pals-api"]
