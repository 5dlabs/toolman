# 🛠️ Toolman

**Your ultimate tool companion for MCP (Model Context Protocol) servers**

Toolman is a production-grade Rust HTTP server that provides intelligent MCP server and tool management for Cursor IDE and Claude Code. It acts as a centralized proxy that manages multiple MCP servers with static configuration-based tool exposure.

[![Rust](https://img.shields.io/badge/rust-1.79+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Docker](https://img.shields.io/badge/docker-ghcr.io-blue.svg)](https://github.com/5dlabs/toolman/pkgs/container/toolman)

## 🎯 What is Toolman?

Toolman solves the **tool overwhelm problem** in AI development environments. Instead of being flooded with hundreds of tools from dozens of MCP servers, Toolman lets you:

- **Selectively expose** only the tools you need through static configuration
- **Configure tool availability** per server and per tool in `servers-config.json`
- **Manage multiple projects** with different tool configurations
- **Run in containers** with Docker and Kubernetes support

## ✨ Key Features

### 🎛️ Static Configuration
- Tools are enabled/disabled through `servers-config.json`
- Fine-grained control: enable servers and specific tools within them
- No runtime configuration changes - restart to apply new settings
- Secure by default: only explicitly enabled tools are exposed

### 🏗️ Architecture
- Single HTTP endpoint that proxies to multiple MCP servers
- Stateless design - configuration is loaded from file
- Multi-platform Docker images (amd64/arm64)
- Kubernetes-ready with ConfigMap support

### 🔧 Integrated MCP Servers
Supporting 25+ MCP servers with hundreds of tools:
- **Filesystem** - Local file operations
- **GitHub** - Repository management, issues, PRs
- **Git** - Version control operations
- **Memory** - Knowledge graph and persistent storage
- **PostgreSQL** - Database operations
- **Docker** - Container management
- **Time** - Time and timezone operations
- And many more...

## 🚀 Quick Start

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

### Configure Your IDE

For Cursor IDE, add to your `.cursorrules` or configure the MCP client:
```bash
# Using Claude CLI
claude mcp add --transport http toolman http://localhost:3000/mcp
```

## 📝 Configuration

### Server Configuration (servers-config.json)

Enable servers and specific tools:

```json
{
  "servers": {
    "filesystem": {
      "name": "Filesystem MCP Server",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed/dir"],
      "enabled": true,
      "tools": {
        "read_file": { "enabled": true },
        "write_file": { "enabled": true },
        "list_directory": { "enabled": true },
        "move_file": { "enabled": false }
      }
    },
    "git": {
      "name": "Git MCP Server",
      "command": "uvx",
      "args": ["mcp-server-git"],
      "enabled": true,
      "tools": {
        "git_status": { "enabled": true },
        "git_diff": { "enabled": true },
        "git_commit": { "enabled": false }
      }
    }
  }
}
```

### Tool Behavior

- **Server disabled** (`enabled: false`): No tools from this server are available
- **Server enabled, no tool config**: All tools from the server are available
- **Server enabled with tool config**: Only explicitly enabled tools are available

## 🐳 Kubernetes Deployment

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mcp-proxy-config
data:
  servers-config.json: |
    {
      "servers": {
        # Your configuration here
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
```

## 🔒 Security

- Static configuration prevents runtime manipulation
- Only explicitly enabled tools are exposed
- No dynamic tool enabling/disabling
- Runs as non-root user in containers
- Read-only configuration mounting recommended

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.