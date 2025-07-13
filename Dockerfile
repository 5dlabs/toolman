# Build stage
FROM rust:1.82 AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build for release
RUN cargo build --release --bin toolman-http

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies and language runtimes
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    git \
    # Node.js for npx/npm MCP servers
    nodejs \
    npm \
    # Python for Python-based MCP servers
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Install uv for faster Python package management
RUN curl -LsSf https://astral.sh/uv/install.sh | sh && \
    mv /root/.cargo/bin/uv /usr/local/bin/

# Update npm to latest
RUN npm install -g npm@latest

# Copy the binary from builder
COPY --from=builder /app/target/release/toolman-http /usr/local/bin/toolman-http

# Create non-root user
RUN useradd -m -u 1000 mcp

# Create directory for configs
RUN mkdir -p /config && chown mcp:mcp /config

USER mcp

# Expose default port
EXPOSE 3000

# Set default environment variables
ENV PORT=3000
ENV PROJECT_DIR=/config

# Run the binary
ENTRYPOINT ["toolman-http"]
CMD ["--port", "3000", "--project-dir", "/config"]