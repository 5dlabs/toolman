# MCP Transport Implementation Plan

## Overview

This document outlines the plan to implement full MCP transport support (stdio, HTTP, SSE) in toolman.

## Current State

- ✅ **Stdio Transport**: Fully implemented for local processes
- ❌ **HTTP Transport**: Not implemented
- ❌ **SSE Transport**: Not implemented
- ❌ **Remote Servers**: Not supported

## Architecture Design

### 1. Transport Abstraction Layer

Create a unified interface for all transport types:

```rust
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request
    async fn send_request(&mut self, request: Value) -> Result<()>;
    
    /// Receive a JSON-RPC response
    async fn receive_response(&mut self) -> Result<Value>;
    
    /// Send a notification (no response expected)
    async fn send_notification(&mut self, notification: Value) -> Result<()>;
    
    /// Check if transport is still connected
    async fn is_connected(&self) -> bool;
    
    /// Close the transport
    async fn close(&mut self) -> Result<()>;
}
```

### 2. Transport Implementations

#### Stdio Transport (existing, needs refactoring)
```rust
pub struct StdioTransport {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}
```

#### HTTP Transport (new)
```rust
pub struct HttpTransport {
    client: reqwest::Client,
    base_url: String,
    auth: Option<AuthConfig>,
}
```

#### SSE Transport (new)
```rust
pub struct SseTransport {
    client: reqwest::Client,
    event_source: EventSource,
    base_url: String,
    auth: Option<AuthConfig>,
}
```

### 3. Updated Configuration

```json
{
  "servers": {
    "local-filesystem": {
      "name": "Filesystem Server",
      "transport": {
        "type": "stdio",
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem"]
      },
      "enabled": true
    },
    "remote-github": {
      "name": "GitHub Server",
      "transport": {
        "type": "http",
        "url": "https://api.example.com/mcp",
        "auth": {
          "type": "bearer",
          "token": "${GITHUB_TOKEN}"
        }
      },
      "enabled": true
    },
    "streaming-assistant": {
      "name": "AI Assistant",
      "transport": {
        "type": "sse",
        "url": "https://assistant.example.com/mcp",
        "auth": {
          "type": "api_key",
          "header": "X-API-Key",
          "key": "${ASSISTANT_API_KEY}"
        }
      },
      "enabled": true
    }
  }
}
```

## Implementation Steps

### Phase 1: Transport Abstraction (High Priority)
1. Create `Transport` trait in `src/transport/mod.rs`
2. Move existing stdio logic to `src/transport/stdio.rs`
3. Update `ServerConnectionPool` to use transport abstraction
4. Update configuration to support transport specifications

### Phase 2: HTTP Transport (High Priority)
1. Implement `HttpTransport` in `src/transport/http.rs`
2. Add connection pooling for HTTP clients
3. Implement retry logic with exponential backoff
4. Add authentication support (Bearer, API Key, Basic)
5. Handle HTTP-specific errors and status codes

### Phase 3: SSE Transport (High Priority)
1. Add `eventsource-client` or similar crate for SSE support
2. Implement `SseTransport` in `src/transport/sse.rs`
3. Handle reconnection logic for SSE streams
4. Implement message buffering for SSE events
5. Add SSE-specific error handling

### Phase 4: Enhanced Features (Medium Priority)
1. TLS/SSL configuration for secure connections
2. Proxy support for corporate environments
3. Connection health monitoring per transport type
4. Metrics and observability per transport
5. Load balancing for multiple remote servers

### Phase 5: Testing & Documentation (Medium Priority)
1. Unit tests for each transport implementation
2. Integration tests with real MCP servers
3. Performance benchmarks
4. Update documentation with examples
5. Migration guide for existing users

## Dependencies to Add

```toml
# Cargo.toml additions
[dependencies]
# For HTTP transport
reqwest = { version = "0.11", features = ["json", "stream"] }

# For SSE transport
eventsource-client = "0.12"
# or
async-sse = "5.1"

# For async trait definitions
async-trait = "0.1"

# For authentication
jsonwebtoken = "9.2"
base64 = "0.21"
```

## Docker Runtime Requirements

### Current Dockerfile Updates Needed:
1. ✅ Node.js runtime (npm, npx)
2. ✅ Python runtime (python3, pip, uv)
3. ✅ Docker CLI (for Docker-based servers)
4. ❓ Rust runtime (if any MCP servers are written in Rust)
5. ❓ Go runtime (if any MCP servers are written in Go)

### Security Considerations:
- Docker socket mounting requires careful permission management
- Remote server authentication tokens need secure storage
- TLS certificate validation for HTTPS connections
- Network isolation between different MCP servers

## Testing Strategy

### Local Testing:
```bash
# Test stdio transport (existing)
cargo test transport::stdio

# Test HTTP transport
cargo test transport::http

# Test SSE transport  
cargo test transport::sse
```

### Integration Testing:
1. Set up test MCP servers for each transport type
2. Create docker-compose.test.yml with all transport types
3. Run integration test suite against real servers
4. Performance testing with concurrent connections

## Backwards Compatibility

- Existing `servers-config.json` will continue to work
- Stdio-only configurations will be auto-migrated to new format
- Deprecation warnings for old configuration format
- Grace period of 2 versions before removing old format

## Timeline Estimate

- Phase 1: 2-3 days (Transport abstraction)
- Phase 2: 3-4 days (HTTP transport)
- Phase 3: 3-4 days (SSE transport)  
- Phase 4: 2-3 days (Enhanced features)
- Phase 5: 2-3 days (Testing & docs)

**Total: ~2-3 weeks for full implementation**

## Success Criteria

1. Can connect to MCP servers via all three transport types
2. Maintains connection stability over extended periods
3. Handles network failures gracefully with auto-reconnect
4. Performance on par with direct connections
5. Clear documentation and examples for each transport type