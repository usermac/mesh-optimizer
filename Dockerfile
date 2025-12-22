FROM rust:1.85-bullseye as builder

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build workspace (binaries: mesh-worker, mesh-api)
RUN cargo build --release

# --- Runtime Stage ---
FROM debian:bullseye-slim

# Install dependencies
# - ca-certificates: for HTTPS (Stripe)
# - caddy: web server
# - libssl/openssl: for reqwest/stripe networking
RUN apt-get update && apt-get install -y -q --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    libssl-dev \
    debian-keyring \
    debian-archive-keyring \
    apt-transport-https \
    wget \
    xz-utils \
    libxi6 \
    libxrender1 \
    libgl1 \
    libxkbcommon0 \
    libsm6 \
    libx11-6 \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list \
    && apt-get update \
    && apt-get install -y caddy \
    && rm -rf /var/lib/apt/lists/*

# Install Blender 4.1
RUN mkdir -p /opt/blender && \
    wget -qO /tmp/blender.tar.xz https://download.blender.org/release/Blender4.1/blender-4.1.0-linux-x64.tar.xz && \
    tar -xf /tmp/blender.tar.xz -C /opt/blender --strip-components=1 && \
    rm /tmp/blender.tar.xz

ENV PATH="/opt/blender:$PATH"
ENV BLENDER_PATH="/opt/blender/blender"

WORKDIR /app

# Copy compiled binaries
# Worker (renamed to what the API expects: "mesh-optimizer")
COPY --from=builder /usr/src/app/target/release/mesh-worker /usr/local/bin/mesh-optimizer
# API Server
COPY --from=builder /usr/src/app/target/release/mesh-api /usr/local/bin/mesh-api

# Copy static assets (Code expects them at ./server/public)
COPY server/public ./server/public
COPY server/pricing.json ./server/pricing.json
COPY server/processing_messages.json ./server/processing_messages.json
COPY scripts ./scripts

# Ensure directory for DB mount exists
RUN mkdir -p server

# Caddy Environment for persistence
ENV XDG_DATA_HOME=/data
ENV XDG_CONFIG_HOME=/config

# Copy Caddy configuration
COPY Caddyfile /etc/caddy/Caddyfile

EXPOSE 80 443

# Copy startup script
COPY scripts/start.sh /app/start.sh
RUN chmod +x /app/start.sh

# Start Caddy in background, then run API
CMD ["/app/start.sh"]
