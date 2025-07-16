# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build for release
RUN cargo build --release --bin toolman-http

# Runtime stage - Use Debian base for comprehensive runtime support
FROM debian:bookworm-slim

# Install system dependencies and package managers
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    bash \
    gnupg \
    lsb-release \
    apt-transport-https \
    software-properties-common \
    # Core utilities
    coreutils \
    findutils \
    grep \
    sed \
    tar \
    gzip \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install Docker CLI (not daemon) for Docker-based MCP servers
RUN curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null && \
    apt-get update && \
    apt-get install -y docker-ce-cli && \
    rm -rf /var/lib/apt/lists/*

# Install Node.js and npm for Node.js-based MCP servers
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g npm@latest && \
    rm -rf /var/lib/apt/lists/*

# Install Python and Python package managers for Python-based MCP servers
RUN apt-get update && apt-get install -y \
    python3 \
    python3-dev \
    python3-pip \
    python3-venv \
    && ln -sf python3 /usr/bin/python \
    && rm -rf /var/lib/apt/lists/*

# Install uv for faster Python package management
RUN curl -LsSf https://astral.sh/uv/install.sh | bash && \
    mv /root/.local/bin/uv /usr/local/bin/ 2>/dev/null || \
    mv /root/.cargo/bin/uv /usr/local/bin/ 2>/dev/null || \
    echo "UV installed successfully"

# Install Go for Go-based MCP servers
RUN curl -fsSL https://golang.org/dl/go1.21.5.linux-$(dpkg --print-architecture).tar.gz | tar -xzC /usr/local && \
    ln -s /usr/local/go/bin/go /usr/local/bin/go && \
    ln -s /usr/local/go/bin/gofmt /usr/local/bin/gofmt

# Install Rust for Rust-based MCP servers (minimal runtime)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal && \
    echo 'export PATH="/root/.cargo/bin:$PATH"' >> /etc/profile && \
    ln -s /root/.cargo/bin/cargo /usr/local/bin/cargo && \
    ln -s /root/.cargo/bin/rustc /usr/local/bin/rustc

# Install Java for Java-based MCP servers
RUN apt-get update && apt-get install -y openjdk-17-jre-headless && \
    rm -rf /var/lib/apt/lists/*

# Install .NET for .NET-based MCP servers (if available for architecture)
RUN curl -fsSL https://packages.microsoft.com/config/debian/12/packages-microsoft-prod.deb -o packages-microsoft-prod.deb && \
    dpkg -i packages-microsoft-prod.deb && \
    rm packages-microsoft-prod.deb && \
    apt-get update && \
    (apt-get install -y dotnet-runtime-8.0 || apt-get install -y dotnet-runtime-6.0 || echo "No .NET runtime available for this architecture") && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/toolman-http /usr/local/bin/toolman-http

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash mcp

# Create directories for configs
RUN mkdir -p /config && \
    chown mcp:mcp /config

# Set up environment for all runtimes
ENV PORT=3000 \
    PROJECT_DIR=/config \
    PATH="/root/.cargo/bin:/usr/local/go/bin:$PATH" \
    PYTHONUNBUFFERED=1 \
    NODE_ENV=production \
    DOCKER_HOST=unix:///var/run/docker.sock

# Create entrypoint script to handle environment setup
RUN cat > /entrypoint.sh << 'EOF'
#!/bin/bash
set -e

# Set up runtime paths
export PATH="/usr/local/bin:/usr/local/go/bin:/root/.cargo/bin:$PATH"

# Check if Docker socket is available (for Docker-based MCP servers)
if [ -S "/var/run/docker.sock" ]; then
    # Add mcp user to docker group if socket exists
    if ! groups mcp | grep -q docker; then
        usermod -aG docker mcp 2>/dev/null || true
    fi
fi

# Run the application
cd /config
exec "$@"
EOF

RUN chmod +x /entrypoint.sh

# Switch to non-root user
USER mcp
WORKDIR /config

# Expose default port
EXPOSE 3000

# Use custom entrypoint
ENTRYPOINT ["/entrypoint.sh"]
CMD ["toolman-http", "--port", "3000", "--project-dir", "/config"]