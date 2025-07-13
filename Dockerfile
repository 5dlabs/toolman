# Build stage
FROM rust:1.75 as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY build.rs ./

# Build for release
RUN cargo build --release --bin toolman-http

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/toolman-http /usr/local/bin/mcp-bridge-proxy

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
ENTRYPOINT ["mcp-bridge-proxy"]
CMD ["--port", "3000", "--project-dir", "/config"]