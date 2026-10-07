# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# Stage 1 — build the SvelteKit frontend as a static SPA ("web" mode).
# `--mode web` swaps the Tauri packages for the browser shims in src/lib/web.
# ---------------------------------------------------------------------------
FROM node:22-alpine AS frontend
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build:web

# ---------------------------------------------------------------------------
# Stage 2 — build the Rust web server.
# The server crate reuses the Tauri-free modules from src-tauri/src via #[path],
# so both source trees are required in the build context.
# ---------------------------------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /src
COPY server ./server
COPY src-tauri/src ./src-tauri/src
WORKDIR /src/server
RUN cargo build --release

# ---------------------------------------------------------------------------
# Stage 3 — minimal runtime image.
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=server  /src/server/target/release/pomotroid-server /usr/local/bin/pomotroid-server
COPY --from=frontend /app/build ./build
COPY static/themes ./themes

ENV POMOTROID_PORT=6666 \
    POMOTROID_DATA_DIR=/data \
    POMOTROID_STATIC_DIR=/app/build \
    POMOTROID_THEMES_DIR=/app/themes \
    RUST_LOG=info

VOLUME ["/data"]
EXPOSE 6666

CMD ["pomotroid-server"]
