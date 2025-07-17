# Session-Based MCP Proxy Architecture

## Overview

The MCP Proxy implements a session-based architecture to enable multi-tenant operation with intelligent local vs remote execution context routing. This design allows multiple agents/clients to safely share the same proxy server while maintaining isolation and optimal execution placement.

## Problem Statement

### Multi-Agent Conflicts
Without session isolation, multiple agents connecting to the same proxy would experience:

- **Configuration Conflicts**: Agent A configures filesystem for `/project-a`, Agent B overwrites with `/project-b`
- **Security Issues**: Agent A could access Agent B's files or API keys
- **Resource Contention**: MCP servers receive mixed requests from different clients
- **State Corruption**: Shared server state leads to unpredictable behavior

### Local vs Remote Execution
Different MCP servers have different optimal execution contexts:

- **Filesystem operations**: Must run locally to access client's actual files
- **Web APIs**: Better performance and network access from server
- **Databases**: Already running in cluster, better to keep server-side
- **Processing**: More compute resources available on server

## Session-Based Solution

### Session Lifecycle

```mermaid
sequenceDiagram
    participant Client
    participant Proxy
    participant LocalServer
    participant RemoteServer

    Client->>Proxy: POST /session/init
    Note over Client,Proxy: Handshake with workingDir, capabilities
    Proxy->>Proxy: Create session context
    Proxy->>LocalServer: Spawn local servers
    Proxy->>RemoteServer: Initialize remote servers
    Proxy->>Client: Session config response

    Client->>Proxy: POST /mcp (with sessionId)
    Proxy->>Proxy: Route based on execution context
    alt Local execution
        Proxy->>LocalServer: Forward request
        LocalServer->>Proxy: Response
    else Remote execution
        Proxy->>RemoteServer: Forward request
        RemoteServer->>Proxy: Response
    end
    Proxy->>Client: Unified response

    Client->>Proxy: DELETE /session/{sessionId}
    Proxy->>LocalServer: Cleanup local servers
    Proxy->>Proxy: Clean session state
```

### Session Configuration Handshake

#### 1. Session Initialization Request
```http
POST /session/init
Content-Type: application/json

{
  "clientInfo": {
    "name": "claude-desktop",
    "version": "1.0.0"
  },
  "workingDirectory": "/Users/alice/project-a",
  "capabilities": {
    "filesystem": true,
    "experimental": ["local-execution"]
  },
  "preferences": {
    "executionContext": {
      "filesystem": "local",
      "web-search": "remote",
      "databases": "remote"
    }
  }
}
```

#### 2. Session Configuration Response
```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "sessionId": "session-abc123",
  "protocolVersion": "2024-11-05",
  "capabilities": {
    "tools": {},
    "session": {
      "local_execution": true,
      "remote_execution": true
    }
  },
  "servers": {
    "filesystem": {
      "executionContext": "local",
      "workingDirectory": "/Users/alice/project-a",
      "allowedDirectories": ["/Users/alice/project-a"]
    },
    "web-search": {
      "executionContext": "remote",
      "endpoint": "internal"
    },
    "brave-search": {
      "executionContext": "remote", 
      "endpoint": "internal",
      "enabled": false,
      "reason": "Missing BRAVE_API_KEY"
    }
  },
  "tools": [
    {
      "name": "filesystem_read_file",
      "executionContext": "local",
      "server": "filesystem"
    },
    {
      "name": "web_search",
      "executionContext": "remote", 
      "server": "web-search"
    }
  ]
}
```

### Execution Context Routing

#### Local Execution Context
- **When**: Filesystem operations, document access, IDE integration
- **Where**: Client's machine, agent's working directory
- **Servers**: `filesystem`, `git`, `editor-integration`
- **Benefits**: 
  - Access to actual user files
  - Respect user's file permissions
  - Work with local development environment

#### Remote Execution Context  
- **When**: Web APIs, databases, processing, memory operations
- **Where**: Kubernetes cluster, server infrastructure
- **Servers**: `web-search`, `databases`, `memory`, `brave-search`
- **Benefits**:
  - Better network connectivity
  - More compute resources
  - Persistent storage (PVC)
  - Shared infrastructure

### Session State Management

```rust
#[derive(Debug, Clone)]
pub struct SessionContext {
    pub session_id: String,
    pub client_info: ClientInfo,
    pub working_directory: PathBuf,
    pub server_configs: HashMap<String, ServerConfig>,
    pub local_servers: HashMap<String, LocalServerHandle>,
    pub created_at: SystemTime,
    pub last_accessed: SystemTime,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub execution_context: ExecutionContext,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_directory: Option<PathBuf>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionContext {
    Local,   // Run on client's machine
    Remote,  // Run on server infrastructure
}
```

## Security Considerations

### Session Isolation
- Each session has isolated server instances
- File access restricted to session's working directory
- API keys scoped to session configuration
- No cross-session data leakage

### Local Execution Security
- Local servers run in client's security context
- File access limited to explicitly allowed directories
- No privilege escalation
- Client controls local server lifecycle

### Remote Execution Security
- Server-side resource limits and quotas
- Network policies for outbound access
- Secrets management for API keys
- Audit logging for all operations

## Implementation Plan

### Phase 1: Session Management Core
1. **Session store** - In-memory session state management
2. **Session endpoints** - `/session/init`, `/session/{id}`, `/session/{id}/destroy`
3. **Session middleware** - Extract and validate session from requests
4. **Configuration handshake** - Parse client preferences and return server config

### Phase 2: Execution Context Routing
1. **Local server spawning** - Spawn MCP servers in client context
2. **Remote server management** - Session-scoped remote server instances
3. **Request routing** - Route tools based on execution context
4. **Tool aggregation** - Combine tools from local and remote servers

### Phase 3: Local Server Integration
1. **Local server protocols** - Communication with client-side servers
2. **Working directory handling** - Proper path resolution
3. **Server lifecycle** - Cleanup on session destruction
4. **Error handling** - Graceful degradation when local servers fail

### Phase 4: Production Features
1. **Session persistence** - Survive proxy restarts
2. **Session timeouts** - Automatic cleanup of stale sessions
3. **Monitoring** - Session metrics and health checks
4. **Load balancing** - Distribute sessions across proxy instances

## Configuration Examples

### Default Server Classifications
```yaml
servers:
  # Local execution (filesystem access)
  filesystem:
    executionContext: local
    allowedDirectories: ["{workingDirectory}"]
  
  git:
    executionContext: local
    workingDirectory: "{workingDirectory}"
  
  # Remote execution (APIs, services)
  web-search:
    executionContext: remote
    
  brave-search:
    executionContext: remote
    env:
      BRAVE_API_KEY: "${BRAVE_API_KEY}"
      
  database:
    executionContext: remote
    
  memory:
    executionContext: remote
    storage: pvc
```

### Client Preferences Override
```json
{
  "preferences": {
    "executionContext": {
      "memory": "local",  // Override: use local memory instead of server PVC
      "git": "remote"     // Override: use server-side git operations
    }
  }
}
```

## Benefits

### For Developers
- **Seamless experience**: Files work locally, APIs work remotely
- **Performance**: Optimal execution placement
- **Security**: Isolated sessions, appropriate permissions
- **Flexibility**: Override execution context based on needs

### For Operations  
- **Scalability**: Multiple agents per proxy instance
- **Resource efficiency**: Share remote infrastructure
- **Monitoring**: Session-based metrics and logging
- **Security**: Audit trail and access controls

### For Architecture
- **Clean separation**: Local vs remote concerns
- **Extensibility**: Easy to add new execution contexts
- **Maintainability**: Clear session boundaries
- **Testability**: Isolated session state

## Conclusion

The session-based architecture with execution context routing provides an elegant solution to multi-agent MCP proxy operation. By defaulting to server-side execution with selective local execution for filesystem operations, we achieve optimal performance while maintaining security isolation and operational simplicity.