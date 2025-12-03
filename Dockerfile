# Use a single fat image that has everything
FROM node:20-bullseye

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Setup App
WORKDIR /app

# Copy EVERYTHING at once
COPY . .

# Build Rust Release
# We build it right here, where we will run it
RUN cargo build --release

# Move binary to global path
RUN cp target/release/mesh-optimizer /usr/local/bin/mesh-optimizer

# Setup Server
WORKDIR /app/server
RUN npm install

# Run
EXPOSE 3000
CMD ["node", "index.js"]
