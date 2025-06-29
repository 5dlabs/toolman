# Toolman - Claude Code Configuration

## Project Overview

Toolman is a production-grade Rust HTTP server that provides intelligent MCP (Model Context Protocol) server and tool management for Cursor IDE and Claude Code. It acts as a centralized proxy that manages multiple MCP servers, providing selective tool exposure with dynamic runtime management capabilities.

### Key Technologies

- **Language**: Rust (latest stable)
- **Async Runtime**: Tokio
- **HTTP Framework**: Axum
- **JSON-RPC**: Custom implementation for MCP protocol
- **Serialization**: Serde (JSON)
- **Error Handling**: Anyhow + custom error types
- **Testing**: Built-in test harness with integration tests
- **Configuration**: File-based with atomic updates
- **Multi-User Support**: Context-based isolation with SHA256 hashing

## Development Context

### Architecture Decisions

- **Selective Tool Exposure**: NOT a meta-tool proxy - exposes only tools marked as `enabled: true`
- **Dynamic Management**: Tools can be enabled/disabled at runtime without config edits
- **Ephemeral Changes**: Runtime changes are temporary until explicitly saved
- **Multi-Project Support**: Context-based isolation using project directory as primary identifier
- **HTTP Server**: Runs on configurable port (default 3002) with stdio wrapper for Cursor

### Current Development Phase

- Core functionality complete (Tasks 1-5)
- Multi-user/multi-project support implemented (Task 15.1)
- AI-driven server addition planned (Task 12)
- 26 MCP servers integrated with 278+ tools

### Important Notes

**GOLDEN COPY**: The file `.taskmaster/docs/project-overview.md` is the authoritative specification and must NEVER be deleted.

**TESTING PHILOSOPHY**: Features are only considered complete when confirmed working in Cursor UI, not just API tests.

**PROJECT NAME**: This project is now called "Toolman" - your ultimate tool companion for MCP servers.

## Code Patterns & Standards

### Rust-Specific Patterns

#### Error Handling

```rust
use anyhow::{Context, Result, bail};

// Custom error types for specific scenarios
#[derive(Debug)]
pub enum BridgeProxyError {
    ServerNotFound(String),
    ToolNotFound { server: String, tool: String },
    ConfigError(String),
    JsonRpcError(String),
}

// Use anyhow for general error propagation
async fn handle_request(request: JsonRpcRequest) -> Result<JsonRpcResponse> {
    match request.method.as_str() {
        "tools/list" => list_tools().await.context("Failed to list tools"),
        "enable_tool" => enable_tool(request.params).await
            .context("Failed to enable tool"),
        _ => bail!("Unknown method: {}", request.method),
    }
}
```

#### Async Patterns

```rust
use tokio::sync::RwLock;
use std::sync::Arc;

// Shared state with async-safe locking
pub struct BridgeState {
    config: Arc<RwLock<BridgeConfig>>,
    ephemeral_config: Arc<RwLock<EphemeralConfig>>,
    context_manager: Arc<RwLock<ContextManager>>,
    tool_cache: Arc<RwLock<HashMap<String, Vec<Tool>>>>,
}

// Concurrent server management
async fn discover_and_enable_tools(state: Arc<BridgeState>) -> Result<()> {
    let config = state.config.read().await;
    let mut handles = vec![];

    for (server_name, server_config) in &config.servers {
        let handle = tokio::spawn(discover_server_tools(
            server_name.clone(),
            server_config.clone()
        ));
        handles.push(handle);
    }

    // Wait for all discoveries to complete
    let results = futures::future::join_all(handles).await;
    // Process results...
}
```

#### Configuration Management

```rust
// Atomic configuration updates
pub async fn save_config(config: &BridgeConfig, path: &Path) -> Result<()> {
    // Create backup first
    let backup_path = create_backup_path(path)?;
    if path.exists() {
        fs::copy(path, &backup_path).await
            .context("Failed to create backup")?;
    }

    // Write to temp file
    let temp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&temp_path, json).await
        .context("Failed to write temp file")?;

    // Atomic rename
    fs::rename(temp_path, path).await
        .context("Failed to rename config file")?;

    Ok(())
}
```

### Testing Strategy

- **Unit Tests**: Core logic, configuration management, tool filtering
- **Integration Tests**: HTTP endpoints, MCP protocol compliance, multi-server scenarios
- **Cursor UI Tests**: Manual validation of tool availability and functionality
- **Regression Tests**: Comprehensive validation of all features after changes

## Key Files & Structure

```
toolman/
├── src/
│   ├── main.rs              # Stdio wrapper entry point
│   ├── lib.rs               # Core Toolman implementation
│   ├── context.rs           # Multi-user context management
│   ├── errors.rs            # Error types
│   ├── stdio_wrapper.rs     # Stdio MCP server wrapper
│   ├── tool_suggester.rs    # AI-driven tool suggestions
│   ├── health_monitor.rs    # Server health monitoring
│   ├── recovery.rs          # Configuration recovery
│   └── bin/
│       └── http_server.rs   # HTTP server implementation
├── servers-config.json       # Main configuration file
├── Cargo.toml               # Dependencies
└── .taskmaster/
    └── docs/
        └── project-overview.md  # GOLDEN COPY - Project specifications
```

### Configuration

#### servers-config.json Structure

```json
{
  "servers": {
    "memory": {
      "name": "Memory Server",
      "description": "Persistent memory and knowledge graph",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-memory"],
      "tools": {
        "create_entities": { "enabled": true },
        "read_graph": { "enabled": true },
        "search_nodes": { "enabled": false }
      }
    }
  }
}
```

#### Context-Based Configuration

- Stored in: `~/.mcp-bridge-proxy/contexts/{hash}.json`
- Hash derived from: project directory + optional user_id
- Contains: per-context tool enable/disable preferences

## Development Workflow

### Testing Philosophy

**CRITICAL**: Always test via Cursor UI first. A feature is only complete when:
1. It works in Cursor UI (not just API tests)
2. All edge cases are handled
3. Performance is acceptable
4. No regressions in existing functionality

### Complete Regression Testing

A full regression test must verify:

#### 1. Core Tool Management
```bash
# Start the HTTP server
nohup ./target/release/toolman-http --project-dir $(pwd) --port 3002 > /tmp/toolman.log 2>&1 &

# Monitor startup (WAIT for completion - typically 30-60 seconds)
tail -f /tmp/toolman.log

# Look for these completion messages:
# "✅ Discovered X tools from server 'Y'"
# "📊 Total tools discovered: X"
# "📊 Total tools enabled: Y"
# "✅ HTTP server listening on http://127.0.0.1:3002"
```

#### 2. Tool Discovery & Listing
- Verify `tools/list` returns only enabled tools
- Confirm tool count matches enabled count in config
- Check tool names are properly prefixed (e.g., `memory_read_graph`)
- Validate tool descriptions and metadata

#### 3. Dynamic Tool Management
- **enable_tool**:
  - Enable a disabled tool
  - Verify it appears immediately in Cursor UI
  - Confirm it's functional when called
- **disable_tool**:
  - Disable an enabled tool
  - Verify it disappears from Cursor UI
  - Confirm calls to it fail appropriately
- **enable_servers** (bulk operation):
  - Enable multiple servers at once
  - Verify all tools from those servers appear
  - Check performance with large tool sets

#### 4. Configuration Persistence
- **save_config**:
  - Make ephemeral changes (enable/disable tools)
  - Call save_config
  - Verify backup is created with timestamp
  - Restart server and confirm changes persist
  - Test atomic update (no corruption on failure)

#### 5. Multi-Project Support
- Test with multiple project directories:
  ```bash
  # Project A
  ./mcp-bridge-proxy-http --project-dir /path/to/projectA --port 3002
  # Enable specific tools for Project A

  # Project B
  ./mcp-bridge-proxy-http --project-dir /path/to/projectB --port 3003
  # Enable different tools for Project B

  # Verify isolation - changes in A don't affect B
  ```

#### 6. Tool Forwarding
- Test actual tool calls through the proxy:
  - `filesystem_read_file` - returns file contents
  - `memory_read_graph` - returns knowledge graph
  - `github_create_issue` - creates real GitHub issue
- Verify error handling for:
  - Invalid parameters
  - Server failures
  - Network timeouts

#### 7. Server Lifecycle
- Test server startup/shutdown
- Verify crashed servers are detected and restarted
- Test environment variable injection per request
- Confirm no server restarts needed for env changes

#### 8. Error Scenarios
- Missing configuration file
- Malformed JSON in config
- Invalid server commands
- Port already in use
- Insufficient permissions
- Server discovery failures

### Testing Commands

```bash
# Build all binaries
cargo build --release

# Run unit tests
cargo test

# Integration tests
cargo test --test '*'

# Manual API testing
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}'

# Test tool forwarding
echo '{"jsonrpc": "2.0", "id": 1, "method": "memory_read_graph", "params": {}}' | \
  ./target/release/toolman --url http://localhost:3002/mcp

# Cursor UI testing
# 1. Update ~/.cursor/mcp.json to point to Toolman stdio wrapper
# 2. Restart Cursor
# 3. Verify tools appear in Claude's tool list
# 4. Test actual tool usage through Claude
```

### Quality Checks

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --all-targets --all-features

# Check for security issues
cargo audit

# Verify no warnings
cargo build --release 2>&1 | grep -i warning
```

## Current Focus Areas

### Completed Features ✅

- [x] HTTP server with MCP protocol support
- [x] Selective tool filtering based on config
- [x] Dynamic tool enable/disable
- [x] Bulk server enabling (enable_servers)
- [x] Configuration persistence with atomic updates
- [x] Multi-project context isolation
- [x] Tool forwarding to actual MCP servers
- [x] Stdio wrapper for Cursor integration

### In Progress 🚧

- [ ] AI-driven server addition (Task 12)
  - GitHub repository analysis
  - Server type detection
  - Automated configuration generation

### Known Issues & Workarounds

1. **Tool Refresh in Cursor**:
   - New tools don't appear until user sends another message
   - Workaround: Ask "Should I continue?" after enable_tool
   - Root cause: MCP notification timing

2. **Server Startup Time**:
   - Initial tool discovery takes 30-60 seconds
   - Must wait for all servers to complete discovery
   - Monitor logs for completion messages

## Performance & Security

### Performance Optimization

```rust
// Connection pooling for MCP servers
lazy_static! {
    static ref HTTP_CLIENT: Client = Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to create HTTP client");
}

// Concurrent tool discovery
async fn discover_all_servers(servers: &HashMap<String, ServerConfig>) -> Result<()> {
    let futures: Vec<_> = servers.iter()
        .map(|(name, config)| discover_server_tools(name, config))
        .collect();

    futures::future::try_join_all(futures).await?;
    Ok(())
}
```

### Security Considerations

- Environment variables injected per-request (not globally)
- API keys never logged or exposed
- Configuration backups preserve permissions
- Input validation on all tool parameters
- Timeout protection on all external calls

## Tool Usage Efficiency

### Critical Performance Notes

1. **Startup Sequence**:
   - Always wait for full server startup before testing
   - Don't proceed until seeing completion logs
   - Typical startup: 30-60 seconds for all servers

2. **Batch Operations**:
   ```bash
   # ✅ EFFICIENT: Test multiple aspects in one command
   cargo build --release && cargo test && cargo clippy

   # ❌ INEFFICIENT: Separate commands
   cargo build --release
   cargo test
   cargo clippy
   ```

3. **Log Monitoring**:
   ```bash
   # ✅ DO: Monitor logs during startup
   tail -f /tmp/mcp-bridge-proxy.log | grep -E "✅|❌|ERROR"
   ```

## Project-Specific Conventions

### Naming Conventions

- MCP tool names: `server_name_tool_name` (e.g., `memory_read_graph`)
- Config fields: `snake_case`
- Rust types: `PascalCase`
- HTTP endpoints: `/mcp` for all JSON-RPC

### Critical Project Rules

1. **Never Delete**: `.taskmaster/docs/project-overview.md` (golden copy)
2. **Never Reduce**: Don't remove servers from config - fix issues instead
3. **Always Test**: Via Cursor UI before marking complete
4. **Always Commit**: After completing each task

### Common Pitfalls & Solutions

#### Server Discovery Issues

```rust
// ✅ DO: Implement retry logic for server discovery
async fn discover_with_retry(server: &str, config: &ServerConfig) -> Result<Vec<Tool>> {
    let mut attempts = 0;
    loop {
        match discover_server_tools(server, config).await {
            Ok(tools) => return Ok(tools),
            Err(e) if attempts < 3 => {
                warn!("Discovery failed for {}: {}, retrying...", server, e);
                tokio::time::sleep(Duration::from_secs(2)).await;
                attempts += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

#### Configuration Updates

```rust
// ✅ DO: Always validate config before saving
fn validate_config(config: &BridgeConfig) -> Result<()> {
    if config.servers.is_empty() {
        bail!("Configuration must have at least one server");
    }

    for (name, server) in &config.servers {
        if server.command.is_empty() {
            bail!("Server {} has empty command", name);
        }
    }

    Ok(())
}
```

## Current Implementation Status

### Task Completion Summary

1. **Task 5 (Configuration Persistence)** ✅
   - Atomic save with temp-file-rename
   - Automatic backups with timestamps
   - Recovery from corrupted configs
   - All subtasks complete

2. **Task 15.1 (Multi-User Support)** ✅
   - Context-based isolation implemented
   - SHA256 hashing for safe filenames
   - Per-project tool preferences
   - No database required

3. **Task 12 (AI-Driven Server Addition)** 📋
   - 8 subtasks planned
   - Integration guide available
   - Not yet started

### Server Inventory

Currently supporting 26 MCP servers with 278+ tools:
- Redis (44 tools)
- GitHub (26 tools)
- Browser-MCP/Playwright (32 tools)
- Task Master AI (33 tools)
- Postgres (19 tools)
- Docker (19 tools)
- Filesystem (11 tools)
- Memory (9 tools)
- And 18 more...

## Monitoring & Debugging

### Log Analysis

```bash
# View startup progress
tail -f /tmp/mcp-bridge-proxy.log | grep -A2 -B2 "Discovered"

# Check for errors
grep ERROR /tmp/mcp-bridge-proxy.log

# Monitor specific server
grep "memory" /tmp/mcp-bridge-proxy.log

# Tool discovery summary
grep "📊" /tmp/mcp-bridge-proxy.log
```

### Common Debug Commands

```bash
# Check if server is running
ps aux | grep toolman

# View current configuration
cat servers-config.json | jq '.servers | keys'

# Test specific tool
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "memory_read_graph",
    "params": {}
  }'

# View context files
ls -la ~/.toolman/contexts/
```

## Development Tips for Claude Code

1. **Always Check Golden Copy**: Reference `.taskmaster/docs/project-overview.md` for specifications
2. **Test in Cursor First**: Don't trust API tests alone
3. **Wait for Startup**: Always monitor logs during server startup
4. **Commit Frequently**: After each completed task or subtask
5. **Preserve All Servers**: Fix issues, don't delete server configs
6. **Use Parallel Testing**: Run multiple tests concurrently when possible
7. **Monitor Performance**: 30-60 second startup is normal
8. **Check Context Isolation**: Verify multi-project support works correctly

## TaskMaster Integration

This project uses TaskMaster for comprehensive task management. TaskMaster provides both MCP tools (preferred) and CLI commands for managing development tasks.

### Getting Started with TaskMaster

```bash
# View current tasks
task-master list

# See what to work on next
task-master next

# View specific task details
task-master show 15.1
```

### Key TaskMaster Concepts

1. **Tagged Task Lists**: Tasks are organized in separate contexts (tags)
   - Default tag is "master"
   - Create feature-specific tags: `task-master add-tag feature-ai-server`
   - Switch contexts: `task-master use-tag feature-ai-server`

2. **Task Structure**:
   - Parent tasks (e.g., "12") can have subtasks (e.g., "12.1", "12.2")
   - Tasks have: title, description, details, status, dependencies
   - Statuses: pending, in-progress, done, blocked, deferred

3. **MCP Tools vs CLI**:
   - Use MCP tools when available (better performance, structured data)
   - CLI is fallback or for user interaction
   - Example: `get_tasks` (MCP) vs `task-master list` (CLI)

### Essential TaskMaster Commands

#### Task Viewing
```bash
# List all tasks in current tag
task-master list

# Show tasks with specific status
task-master list --status=pending

# View next task to work on
task-master next

# Show specific task details
task-master show 12
task-master show 12.1  # Show subtask
```

#### Task Management
```bash
# Update task status
task-master set-status --id=12.1 --status=in-progress
task-master set-status --id=12.1 --status=done

# Add new subtask
task-master add-subtask --parent=12 --title="Research GitHub API patterns" \
  --description="Study how other MCP servers handle GitHub integration"

# Update task with new information
task-master update-task --id=12 --prompt="Added dependency on GitHub CLI authentication"
```

#### Subtask Implementation Workflow

This is the recommended workflow for implementing subtasks:

1. **Start with subtask details**:
   ```bash
   task-master show 12.1
   ```

2. **Mark as in-progress**:
   ```bash
   task-master set-status --id=12.1 --status=in-progress
   ```

3. **Log findings during implementation**:
   ```bash
   # Use update-subtask to append timestamped notes
   task-master update-subtask --id=12.1 --prompt="
   Initial exploration findings:
   - Found that MCP servers use JSON-RPC for GitHub integration
   - Authentication via gh CLI is working correctly
   - Need to handle rate limiting with exponential backoff
   "
   ```

4. **Continue logging progress**:
   ```bash
   task-master update-subtask --id=12.1 --prompt="
   Implementation progress:
   - Created GitHubAnalyzer struct
   - Implemented repository fetching via gh CLI
   - Added error handling for missing repos
   - TODO: Add caching to reduce API calls
   "
   ```

5. **Mark complete when done**:
   ```bash
   task-master set-status --id=12.1 --status=done
   ```

#### Advanced Features

```bash
# Expand task into subtasks
task-master expand --id=12 --num=8 --research

# Move tasks (useful for reorganization)
task-master move --from=12.3 --to=12.8

# Add task dependencies
task-master add-dependency --id=12.4 --depends-on=12.3

# Research with AI assistance
task-master research --query="Latest MCP server development patterns" \
  --id=12.1 --save-to=12.1
```

### Current Project Tasks

Based on the implementation status:

1. **Task 5**: Configuration Persistence ✅ Complete
   - All subtasks done
   - Atomic save implemented
   - Backup system working

2. **Task 12**: AI-Driven Server Addition 📋 Planned
   - 8 subtasks created
   - Focus on GitHub repository analysis
   - Integration with MCP_SERVER_INTEGRATION_GUIDE.md

3. **Task 15.1**: Multi-User Support ✅ Complete
   - Context-based isolation implemented
   - SHA256 hashing for safe filenames

### TaskMaster Best Practices

1. **Always Update Progress**: Use `update-subtask` to log findings
   - Creates timestamped audit trail
   - Helps future developers understand decisions
   - Documents what worked and what didn't

2. **Break Down Complex Tasks**: Use `expand` for tasks scoring 8+ complexity
   ```bash
   task-master analyze-complexity --research
   task-master expand --id=12 --force --research
   ```

3. **Commit After Task Completion**:
   ```bash
   task-master set-status --id=12.1 --status=done
   git add -A
   git commit -m "feat(task-12.1): Implement GitHub repository analysis

   - Added GitHubAnalyzer for repo inspection
   - Integrated with gh CLI for authentication
   - Implemented server type detection
   - Added comprehensive error handling"
   ```

4. **Use Tags for Feature Branches**:
   ```bash
   git checkout -b feature/ai-server
   task-master add-tag feature-ai-server --from-branch
   task-master use-tag feature-ai-server
   ```

### TaskMaster Configuration

- Tasks stored in: `.taskmaster/tasks/tasks.json`
- Complexity reports in: `.taskmaster/reports/`
- Configuration in: `.taskmaster/config.json`
- State tracking in: `.taskmaster/state.json`

### Quick Reference: Task Statuses

- **pending**: Ready to work on (all dependencies met)
- **in-progress**: Currently being implemented
- **done**: Complete and tested
- **blocked**: Waiting on external factors
- **deferred**: Postponed for later
- **review**: Implementation complete, needs review

### Integration with Development

TaskMaster is deeply integrated with this project's workflow:

1. **Before starting work**: Check `task-master next`
2. **During implementation**: Use `update-subtask` to log progress
3. **After completion**: Update status and commit with task reference
4. **For new work**: Use `add-task` or `add-subtask` with clear descriptions

Remember: Toolman emphasizes real-world usability in Cursor IDE. All features must work seamlessly in the actual user interface, not just in tests.