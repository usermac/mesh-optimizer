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
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list \
    && apt-get update \
    && apt-get install -y caddy \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binaries
# Worker (renamed to what the API expects: "mesh-optimizer")
COPY --from=builder /usr/src/app/target/release/mesh-worker /usr/local/bin/mesh-optimizer
# API Server
COPY --from=builder /usr/src/app/target/release/mesh-api /usr/local/bin/mesh-api

# Copy static assets (Code expects them at ./server/public)
COPY server/public ./server/public

# Ensure directory for DB mount exists
RUN mkdir -p server

# Caddy Environment for persistence
ENV XDG_DATA_HOME=/data
ENV XDG_CONFIG_HOME=/config

# Configure Caddy (Same config as before)
RUN echo "{" > /etc/caddy/Caddyfile && \
    echo "    email Brian@BrianGinn.com" >> /etc/caddy/Caddyfile && \
    echo "}" >> /etc/caddy/Caddyfile && \
    echo "www.webdeliveryengine.com {" >> /etc/caddy/Caddyfile && \
    echo "    reverse_proxy localhost:3000" >> /etc/caddy/Caddyfile && \
    echo "}" >> /etc/caddy/Caddyfile

EXPOSE 80 443

# Start Caddy in background, then run API
# Use exec to ensure mesh-api receives signals and logs go to stdout
CMD caddy start --config /etc/caddy/Caddyfile && exec mesh-api
