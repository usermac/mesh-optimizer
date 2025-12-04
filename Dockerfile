FROM node:20-bullseye

# 1. Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# 2. Build Rust
COPY Cargo.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src
COPY src ./src
RUN cargo build --release
RUN cp target/release/mesh-optimizer /usr/local/bin/mesh-optimizer

# 3. Setup Node & Dependencies
COPY server/package.json ./server/
WORKDIR /app/server
RUN npm install
COPY server ./

# 4. Install Caddy (HTTPS)
RUN apt-get update && apt-get install -y -q --no-install-recommends \
    debian-keyring debian-archive-keyring apt-transport-https curl gnupg ca-certificates \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg \
    && curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list \
    && apt-get update \
    && apt-get install -y caddy

# 5. Configure Caddy (Domain: webdeliveryengine.com)
# IMPORTANT: This block tells Caddy to handle SSL and proxy to Node
RUN echo "webdeliveryengine.com {" > /etc/caddy/Caddyfile && \
    echo "    reverse_proxy localhost:3000" >> /etc/caddy/Caddyfile && \
    echo "}" >> /etc/caddy/Caddyfile

# 6. Expose Both Ports
EXPOSE 80 443

# 7. Start Caddy AND Node
CMD caddy start --config /etc/caddy/Caddyfile && node index.js
