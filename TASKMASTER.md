# Toolman (MCP Bridge Proxy) - TaskMaster

## Project Status

**Last Updated**: 2025-06-29

### Current State
- ✅ Core multi-project functionality is working
- ✅ Tool filtering per project via `.mcp-bridge-proxy-config.json`
- ✅ HTTP server running and responding correctly
- ✅ Project builds successfully with minor warnings
- ✅ Git and GitHub servers now enabled and discovering tools successfully
- ✅ Different projects show different tool counts (mcp-proxy: 13, agent-team: 10)

### Known Issues
1. **Server Discovery**: Git and GitHub servers were disabled in config (fixed by enabling them)
2. **Tool Naming**: Tool names use server prefix (e.g., `git_git_status` not `git_status`)
3. **Tool Visibility**: Enabled tools not immediately visible in tool list after enabling
4. **Build Warnings**: Unused imports and variables need cleanup
5. **Cursor UI Refresh**: Tools don't appear until sending another message

## Architecture Overview

### Key Components
- **Stdio Wrapper** (`src/main.rs`): Handles MCP protocol communication with Cursor
- **HTTP Server** (`src/bin/http_server.rs`): Central hub for tool management
- **Dual Config System**: 
  - System config (`servers-config.json`): Defines available servers
  - User config (`.mcp-bridge-proxy-config.json`): Per-project tool enabling

### Multi-Project Support
- Working directory passed via `--working-dir` argument
- HTTP server uses `X-Working-Directory` header
- Each project maintains its own tool configuration

## Tasks for MVP

### High Priority (Blockers)
- [x] Fix Git server initialization timeout (was disabled - now enabled)
- [x] Fix GitHub server initialization timeout (was disabled - now enabled)
- [ ] Fix tool visibility after enabling (tools don't appear immediately)
- [ ] Clean up build warnings
- [ ] Test with Claude Code (not just Cursor)

### Medium Priority (Polish)
- [ ] Update documentation with current architecture
- [ ] Add comprehensive error handling for server timeouts
- [ ] Implement retry logic for server initialization
- [ ] Add health check endpoints

### Low Priority (Nice to Have)
- [ ] Implement `add_server` functionality
- [ ] Add server status monitoring dashboard
- [ ] Improve Cursor UI refresh mechanism
- [ ] Add configuration validation

## Testing Checklist

### Regression Tests
- [x] Build project: `cargo build --release`
- [x] HTTP server responds to POST /mcp
- [x] Tool filtering works per project
- [x] Different projects show different tools
- [ ] All enabled tools are accessible
- [ ] Tool execution works correctly
- [ ] Cursor UI shows correct tools

### Test Commands
```bash
# Check tool count for a project
curl -s -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -H "X-Working-Directory: /path/to/project" \
  -d '{"jsonrpc": "2.0", "method": "tools/list", "params": {}, "id": 1}' | jq '.result.tools | length'

# List tools for a project
curl -s -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -H "X-Working-Directory: /path/to/project" \
  -d '{"jsonrpc": "2.0", "method": "tools/list", "params": {}, "id": 1}' | jq -r '.result.tools[] | .name'
```

## Next Steps

1. Investigate and fix server initialization timeouts
2. Ensure tool discovery completes for all servers
3. Clean up code warnings
4. Prepare for PR with comprehensive testing