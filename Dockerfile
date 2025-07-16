# Runtime stage - Use Alpine with Docker CLI for running Docker-based MCP servers
FROM alpine:latest

# Install system dependencies and all runtimes using Alpine packages
RUN apk add --no-cache \
    ca-certificates \
    curl \
    git \
    bash \
    # Node.js runtime
    nodejs \
    npm \
    # Python runtime
    python3 \
    python3-dev \
    py3-pip \
    # Go runtime
    go \
    # Java runtime
    openjdk17-jre \
    # .NET runtime (if available)
    dotnet8-runtime \
    # Docker CLI for Docker-based MCP servers
    docker-cli \
    # Core utilities
    coreutils \
    findutils \
    grep \
    sed \
    tar \
    gzip \
    unzip \
    # Additional libraries for compatibility
    libstdc++ \
    gcompat \
    || true

# Create symbolic links for Python
RUN ln -sf python3 /usr/bin/python || true

# Install UV for Python package management
RUN curl -LsSf https://astral.sh/uv/install.sh | bash && \
    mv /root/.local/bin/uv /usr/local/bin/ 2>/dev/null || \
    mv /root/.cargo/bin/uv /usr/local/bin/ 2>/dev/null || \
    echo "UV installed successfully"

# Install minimal Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal

# Copy the prebuilt binaries from CI/CD artifacts
COPY toolman-http-linux /usr/local/bin/toolman-http
COPY toolman-linux /usr/local/bin/toolman
RUN chmod +x /usr/local/bin/toolman-http /usr/local/bin/toolman

# Create non-root user
RUN adduser -D -u 1000 -s /bin/bash mcp

# Create directories for configs
RUN mkdir -p /config && \
    chown mcp:mcp /config

# Set up environment for all runtimes
ENV PORT=3000 \
    PROJECT_DIR=/config \
    PATH="/usr/local/go/bin:/root/.cargo/bin:/usr/lib/jvm/java-17-openjdk/bin:$PATH" \
    PYTHONUNBUFFERED=1 \
    NODE_ENV=production \
    JAVA_HOME=/usr/lib/jvm/java-17-openjdk \
    DOTNET_ROOT=/usr/lib/dotnet \
    DOCKER_HOST=unix:///var/run/docker.sock

# Create entrypoint script to handle environment setup
RUN cat > /entrypoint.sh << 'EOF'
#!/bin/bash
set -e

# Set up runtime paths
export PATH="/usr/local/bin:/usr/local/go/bin:/root/.cargo/bin:/usr/lib/jvm/java-17-openjdk/bin:$PATH"

# Check if Docker socket is available (for Docker-based MCP servers)
if [ -S "/var/run/docker.sock" ]; then
    echo "Docker socket available - Docker-based MCP servers can run containers"
else
    echo "Docker socket not available - Docker-based MCP servers may not work"
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