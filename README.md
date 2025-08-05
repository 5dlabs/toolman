# 🛠️ Toolman

**Smart MCP tool management for AI development**

Toolman is a high-performance Rust proxy that manages multiple MCP (Model Context Protocol) servers, giving you precise control over which tools are available to your AI assistants.

[![Rust](https://img.shields.io/badge/rust-1.79+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue.svg)](https://github.com/5dlabs/toolman/pkgs/container/toolman)

## 🎯 What is Toolman?

When working with AI assistants like Claude or Cursor, you often need different tools for different projects. Toolman acts as a smart gateway that:

- **Consolidates multiple MCP servers** into a single endpoint
- **Exposes all available tools** from configured servers via HTTP
- **Supports client-side filtering** through the stdio wrapper for per-agent tool selection
- **Runs anywhere** - locally, in Docker, or on Kubernetes

## ✨ Key Features

- 🎛️ **HTTP Server** - Aggregates all tools from configured MCP servers
- 🏗️ **Multi-Server Support** - Connect to 25+ MCP servers through one endpoint
- 🔧 **Client-Side Filtering** - Each agent can specify which tools to use via `.toolman-filter.json`
- 🐳 **Container Ready** - Multi-platform Docker images (amd64/arm64)
- ⚡ **High Performance** - Built in Rust for speed and reliability
- 🔒 **Flexible Architecture** - HTTP server exposes everything, clients filter as needed

## 🚀 Quick Start

### One-Line Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/5dlabs/toolman/main/scripts/install.sh | bash
```

This will:
- ✅ Auto-detect your platform (Linux, macOS, Windows)
- ✅ Download and install the latest binaries
- ✅ Configure Cursor IDE integration
- ✅ Set up example configuration files
- ✅ Add binaries to your PATH

### Manual Installation

Download the appropriate binary for your platform from the [releases page](https://github.com/5dlabs/toolman/releases).

### Using Docker

```bash
# Pull the latest image
docker pull ghcr.io/5dlabs/toolman:latest

# Run with your config
docker run -d \
  -p 3000:3000 \
  -v $(pwd)/servers-config.json:/config/servers-config.json:ro \
  ghcr.io/5dlabs/toolman:latest
```

### Using Docker Compose

```yaml
version: '3.8'
services:
  mcp-proxy:
    image: ghcr.io/5dlabs/toolman:latest
    ports:
      - "3000:3000"
    volumes:
      - ./servers-config.json:/config/servers-config.json:ro
    restart: unless-stopped
```

### Building from Source

```bash
# Clone the repository
git clone https://github.com/5dlabs/toolman.git
cd toolman

# Build the project
cargo build --release

# Run the HTTP server
./target/release/toolman-http --project-dir $(pwd) --port 3000
```

### Configure Your AI Assistant

```bash
# For Claude Desktop or Cursor
claude mcp add --transport http toolman http://localhost:3000/mcp
```

## 📝 Configuration

Configure which tools are available by editing `servers-config.json`:

```json
{
  "servers": {
    "filesystem": {
      "name": "Filesystem MCP Server",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/project"],
      "enabled": true,
      "tools": {
        "read_file": { "enabled": true },
        "write_file": { "enabled": true },
        "list_directory": { "enabled": true },
        "move_file": { "enabled": false }  // Disabled for safety
      }
    },
    "github": {
      "name": "GitHub MCP Server",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "enabled": true
      // No tools specified = all tools enabled
    }
  }
}
```

## 🎯 Client-Side Tool Filtering

The stdio wrapper (`toolman` binary) supports client-side filtering of tools. Create a `.toolman-filter.json` file in your working directory:

```json
{
  "enabled_tools": [
    "filesystem_*",           // Enable all filesystem tools
    "memory_store",          // Enable specific tool
    "github_create_issue",   // Enable another specific tool
    "github_list_*"         // Enable all GitHub list tools
  ]
}
```

Special patterns:
- `"*"` - Enable all tools (default if no config file exists)
- `"prefix_*"` - Enable all tools starting with prefix
- `"exact_name"` - Enable only the specific tool

## 🔧 Supported MCP Servers

Toolman works with any MCP server, including:

- **Development**: Git, GitHub, Filesystem
- **Databases**: PostgreSQL, Redis
- **Automation**: Browser/Playwright, Docker
- **AI Tools**: Memory (knowledge graphs), Sequential Thinking
- **Utilities**: Time zones, Fetch (HTTP requests)
- And many more...

## 🐳 Kubernetes Deployment

Deploy Toolman on Kubernetes using a ConfigMap:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mcp-proxy-config
data:
  servers-config.json: |
    {
      "servers": {
        "filesystem": {
          "name": "Filesystem MCP Server",
          "command": "npx",
          "args": ["-y", "@modelcontextprotocol/server-filesystem", "/workspace"],
          "enabled": true
        }
      }
    }
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mcp-proxy
spec:
  replicas: 1
  selector:
    matchLabels:
      app: mcp-proxy
  template:
    metadata:
      labels:
        app: mcp-proxy
    spec:
      containers:
      - name: mcp-proxy
        image: ghcr.io/5dlabs/toolman:latest
        ports:
        - containerPort: 3000
        volumeMounts:
        - name: config
          mountPath: /config
          readOnly: true
      volumes:
      - name: config
        configMap:
          name: mcp-proxy-config
---
apiVersion: v1
kind: Service
metadata:
  name: mcp-proxy
spec:
  selector:
    app: mcp-proxy
  ports:
  - port: 3000
    targetPort: 3000
```

## 🎮 Usage Examples

### Web Development Project
Enable only web-related tools:
```json
{
  "servers": {
    "filesystem": { "enabled": true },
    "git": { "enabled": true },
    "github": { "enabled": true },
    "browser-mcp": { "enabled": true }
  }
}
```

### Data Science Project
Enable data and computation tools:
```json
{
  "servers": {
    "filesystem": { "enabled": true },
    "postgres": { "enabled": true },
    "memory": { "enabled": true }
  }
}
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
# Release Testing
Testing cargo-dist integration fixes.
