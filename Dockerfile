# Stage 1: compile Rust
FROM rust:1.94-bookworm AS rust-builder
RUN apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release -p ab-helpers-cli

# Stage 2: install Node bridge deps
FROM node:20-alpine AS node-setup
WORKDIR /bridge
COPY crates/actual/bridge/package*.json ./
RUN npm ci --omit=dev

# Stage 3: runtime
FROM debian:bookworm-slim AS runner
RUN apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends \
        ca-certificates nodejs \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /build/target/release/abh /usr/local/bin/abh
COPY --from=node-setup /bridge/node_modules /app/bridge/node_modules
COPY crates/actual/bridge/index.js /app/bridge/index.js
COPY crates/ab-helpers-server/configuration /usr/local/bin/configuration

ENV NODE_ENV=production
ENV ABH_ENVIRONMENT=production
ENV ABH_ACTUAL__BRIDGE_SCRIPT=/app/bridge/index.js
ENV ABH_ACTUAL__CACHE_DIR=/data

VOLUME ["/data"]

CMD ["abh", "daemon"]
