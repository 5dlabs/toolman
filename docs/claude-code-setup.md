# Claude Code Setup Guide for Toolman

This guide explains how to configure Claude Code to use Toolman MCP Bridge Proxy.

## Prerequisites

1. Ensure the Toolman HTTP server is running on port 3002:
   ```bash
   cargo run --bin toolman-http -- --port 3002
   ```

2. Build the Toolman stdio wrapper:
   ```bash
   cargo build --release
   ```

## Configuration

Claude Code uses the `.mcp.json` file in your project root for MCP server configuration.

### Basic Configuration

The `.mcp.json` file has been created with the following configuration:

```json
{
  "mcpServers": {
    "toolman": {
      "command": "/Users/jonathonfritz/mcp-proxy/target/release/toolman",
      "args": [
        "--url",
        "http://localhost:3002/mcp",
        "--working-dir",
        "/Users/jonathonfritz/mcp-proxy"
      ]
    }
  }
}
```

### Configuration Options

- **command**: Path to the toolman binary (stdio wrapper)
- **args**:
  - `--url`: URL of the Toolman HTTP server (default: http://localhost:3002/mcp)
  - `--working-dir`: Working directory for tool context (defaults to current directory)

## Usage

1. Open your project in Claude Code
2. Claude Code will automatically detect the `.mcp.json` file
3. The Toolman server will be started automatically
4. Available tools will be shown in Claude Code's tool list

## Testing

To verify the setup:

1. Open Claude Code in your project directory
2. Check that tools are available in the tool list
3. Try using a tool like `list_servers` to see available MCP servers

## Troubleshooting

### Tools Not Appearing

1. Ensure the HTTP server is running on port 3002
2. Check the Claude Code logs for any connection errors
3. Try restarting Claude Code

### Connection Errors

1. Verify the HTTP server is accessible:
   ```bash
   curl -X POST http://localhost:3002/mcp \
     -H "Content-Type: application/json" \
     -d '{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}'
   ```

2. Check that the toolman binary has execute permissions:
   ```bash
   chmod +x /Users/jonathonfritz/mcp-proxy/target/release/toolman
   ```

### Per-Project Tool Configuration

Toolman supports per-project tool configuration. When Claude Code connects from different project directories, Toolman will provide different tool sets based on the project's configuration.

To configure tools for a specific project:

1. Create a `toolman-config.json` in your project root
2. Enable/disable specific servers and tools for that project
3. Claude Code will automatically get the appropriate tools when working in that directory

## Advanced Features

### Multiple Projects

You can use the same Toolman HTTP server for multiple projects. Each project can have its own `.mcp.json` file pointing to the same server but with different `--working-dir` arguments:

```json
{
  "mcpServers": {
    "toolman": {
      "command": "/Users/jonathonfritz/mcp-proxy/target/release/toolman",
      "args": [
        "--url",
        "http://localhost:3002/mcp",
        "--working-dir",
        "/path/to/your/project"
      ]
    }
  }
}
```

### Custom Server Port

If you need to run the HTTP server on a different port:

1. Start the server with a custom port:
   ```bash
   cargo run --bin toolman-http -- --port 8080
   ```

2. Update the `.mcp.json` file:
   ```json
   {
     "mcpServers": {
       "toolman": {
         "command": "/Users/jonathonfritz/mcp-proxy/target/release/toolman",
         "args": [
           "--url",
           "http://localhost:8080/mcp",
           "--working-dir",
           "/Users/jonathonfritz/mcp-proxy"
         ]
       }
     }
   }
   ```