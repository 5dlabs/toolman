# Per-Session Tool Filtering via HTTP Headers

This document describes how to use the `X-Enabled-Tools` header to dynamically filter which MCP tools are exposed on a per-session basis, without requiring configuration files on the server.

## Overview

The HTTP server now supports dynamic tool filtering through the `X-Enabled-Tools` header. This allows each client to specify exactly which tools they want to use, providing fine-grained control over tool exposure.

## Using the X-Enabled-Tools Header

### Header Format

The `X-Enabled-Tools` header accepts several formats:

1. **Wildcard (all tools)**
   ```
   X-Enabled-Tools: *
   ```

2. **Comma-separated list**
   ```
   X-Enabled-Tools: memory_read_graph,filesystem_read_file,git_git_status
   ```

3. **JSON array**
   ```
   X-Enabled-Tools: ["memory_read_graph", "filesystem_read_file", "git_git_status"]
   ```

4. **Pattern matching with wildcards**
   ```
   X-Enabled-Tools: memory_*,git_*
   ```
   This enables all tools from the memory and git servers.

### Examples

#### Example 1: Enable specific tools only

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "X-Enabled-Tools: memory_read_graph,filesystem_read_file" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list"
  }'
```

This will return only the `memory_read_graph` and `filesystem_read_file` tools (plus built-in tools like `suggest_tools_for_tasks`).

#### Example 2: Enable all tools from specific servers

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "X-Enabled-Tools: memory_*,filesystem_*" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list"
  }'
```

This enables all tools from the memory and filesystem servers.

#### Example 3: Enable all available tools

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "X-Enabled-Tools: *" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list"
  }'
```

## Implementation Details

### Tool Discovery

When the HTTP server starts, it automatically discovers all available tools from configured servers. This happens asynchronously in the background to avoid blocking server startup.

### Filtering Logic

1. If the `X-Enabled-Tools` header is present, it takes precedence over all other configuration
2. The header value is parsed to extract tool patterns
3. Each available tool is checked against the patterns
4. Only matching tools are returned in the `tools/list` response

### Pattern Matching

- `*` matches all tools
- `server_*` matches all tools from a specific server
- `tool_name` matches an exact tool name

### Fallback Behavior

If the `X-Enabled-Tools` header is not provided, the server falls back to the existing configuration-based filtering using:
- Static server configuration (`servers-config.json`)
- User context configuration
- Working directory context

## Use Cases

1. **Development/Testing**: Enable only the tools needed for a specific task
2. **Security**: Limit tool exposure based on client requirements
3. **Multi-tenant**: Different clients can have different tool sets without server-side configuration
4. **Dynamic Tool Management**: Change available tools without restarting the server or modifying config files

## Error Handling

If the `X-Enabled-Tools` header contains invalid JSON or malformed patterns, the server will return an error:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": null,
  "error": {
    "code": -32602,
    "message": "Invalid X-Enabled-Tools header format: ..."
  }
}
```

## Testing

Use the provided test script to verify the functionality:

```bash
./examples/test_enabled_tools_header.sh
```

This script demonstrates various ways to use the `X-Enabled-Tools` header and shows the resulting tool lists.