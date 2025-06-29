# MCP Bridge Proxy: save_config Complete Workflow Guide

## 🎯 **Overview**

The `save_config` functionality allows you to persist ephemeral tool changes to the `servers-config.json` file. This document demonstrates the complete workflow from empty config to persistent tool configuration.

## ⚠️ **CRITICAL: UI-Only Tool Management**

**Tool enabling/disabling ONLY works through Cursor's UI, not direct HTTP API calls.**

- ✅ **Use Cursor UI**: Call `mcp_mcp-tools_enable_tool` and `mcp_mcp-tools_disable_tool` through Cursor
- ❌ **Don't use HTTP API**: Direct `curl` calls to enable/disable tools won't update Cursor's tool list
- ✅ **save_config works via both**: HTTP API and Cursor UI

## 🧪 **Complete Workflow Test**

### Step 1: Backup and Create Minimal Config

```bash
# Backup current config
cp servers-config.json servers-config.json.backup-$(date +%Y%m%d-%H%M%S)

# Create minimal config with all tools disabled
cat > servers-config.json << 'EOF'
{
  "servers": {
    "memory": {
      "name": "Memory MCP Server",
      "description": "Knowledge graph management, persistent memory, and information storage",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-memory"],
      "tools": {
        "read_graph": {"enabled": false},
        "delete_entities": {"enabled": false},
        "create_entities": {"enabled": false}
      }
    },
    "filesystem": {
      "name": "Filesystem MCP Server",
      "description": "Local file operations",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/jonathonfritz/code"],
      "tools": {
        "read_file": {"enabled": false},
        "write_file": {"enabled": false}
      }
    }
  }
}
EOF
```

### Step 2: Start HTTP Server

```bash
./target/release/http_server --project-dir $(pwd) --port 3007 &
```

Wait for tool discovery to complete (30-60 seconds for full startup).

### Step 3: Verify Initial State

**Expected Result**: Only management tools available:
- `disable_tool`
- `enable_tool`
- `save_config`

**Verification** (via Cursor or HTTP):
```bash
curl -s http://localhost:3007/mcp -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | \
  jq -r '.result.tools[] | .name' | sort
```

### Step 4: Enable Tools (CURSOR UI ONLY)

**In Cursor, use these tools:**

1. **Enable memory tool**:
   ```
   mcp_mcp-tools_enable_tool
   server_name: memory
   tool_name: read_graph
   ```

2. **Enable filesystem tool**:
   ```
   mcp_mcp-tools_enable_tool
   server_name: filesystem
   tool_name: read_file
   ```

**Expected Result**: Tools appear immediately in Cursor's available tools list:
- `memory_read_graph`
- `filesystem_read_file`
- Plus the 3 management tools

### Step 5: Verify Ephemeral State

Tools are now available in Cursor but **NOT yet saved** to config file:

```bash
# Check current file (should still show "enabled": false)
grep -A 2 -B 2 '"read_graph"' servers-config.json
grep -A 2 -B 2 '"read_file"' servers-config.json
```

### Step 6: Persist Changes with save_config

**In Cursor, call**:
```
mcp_mcp-tools_save_config
restart_proxy: false
```

**Expected Response**:
```
✅ Configuration saved successfully! 2 tools are now persistent.
✨ Changes are saved and will take effect on next startup.
```

### Step 7: Verify File Persistence

```bash
echo "=== FINAL CONFIG STATE ==="
cat servers-config.json | jq .
```

**Expected Result**: Tools now show `"enabled": true`:
```json
{
  "servers": {
    "memory": {
      "tools": {
        "read_graph": {"enabled": true},
        "delete_entities": {"enabled": false},
        "create_entities": {"enabled": false}
      }
    },
    "filesystem": {
      "tools": {
        "read_file": {"enabled": true},
        "write_file": {"enabled": false}
      }
    }
  }
}
```

### Step 8: Test Persistence After Restart

```bash
# Stop current server
pkill -f "http_server.*3007"

# Start fresh server
./target/release/http_server --project-dir $(pwd) --port 3008 &

# Wait for startup, then verify tools are still available in Cursor
```

## 🔧 **Workflow Summary**

1. **Start with minimal config** (all tools disabled)
2. **Start HTTP server** (wait for full startup)
3. **Verify only management tools available**
4. **Enable tools via Cursor UI** (ephemeral changes)
5. **Call save_config via Cursor UI** (persist changes)
6. **Verify file updated** (enabled: true in JSON)
7. **Test restart persistence** (tools remain available)

## 📋 **Key Implementation Details**

### Tool State Management

- **Ephemeral State**: Stored in `BridgeState.enabled_tools` HashMap
- **Persistent State**: Stored in `servers-config.json` file
- **save_config**: Copies ephemeral → persistent

### Configuration File Structure

```json
{
  "servers": {
    "server_name": {
      "name": "Display Name",
      "description": "Server description",
      "command": "command_to_run",
      "args": ["arg1", "arg2"],
      "tools": {
        "tool_name": {"enabled": true|false}
      }
    }
  }
}
```

### save_config Implementation Flow

1. **Collect ephemeral state** from `enabled_tools` HashMap
2. **Group by server** to organize tool changes
3. **Update ConfigManager** with current ephemeral state
4. **Set all existing tools to disabled** (clean slate)
5. **Enable currently active tools** (from ephemeral state)
6. **Save to file** via `ConfigManager.save()`
7. **Return success/failure message**

## ✅ **Verified Working Features**

- ✅ **Tool discovery** from configured servers
- ✅ **Ephemeral tool enabling** via Cursor UI
- ✅ **Tool availability** immediately in Cursor
- ✅ **save_config persistence** to JSON file
- ✅ **Configuration file updates** with correct enabled flags
- ✅ **Tool persistence** across server restarts
- ✅ **Empty/minimal config handling** (creates file structure)

## 🚨 **Common Issues**

### "Tool Not Found" Error
- **Cause**: Tool discovery still in progress
- **Solution**: Wait for full server startup (30-60 seconds)

### "Address Already in Use" Error
- **Cause**: Previous server process still running
- **Solution**: `pkill -f "http_server"` before starting new server

### Tools Don't Appear in Cursor
- **Cause**: Using HTTP API instead of Cursor UI for tool management
- **Solution**: Always use `mcp_mcp-tools_enable_tool` in Cursor UI

### save_config Claims Success But File Unchanged
- **Cause**: No ephemeral changes were made, or server restarted without changes
- **Solution**: Enable tools via Cursor UI first, then save_config

## 🎯 **Best Practices**

1. **Always backup** `servers-config.json` before testing
2. **Use unique ports** to avoid conflicts during testing
3. **Wait for full startup** before testing tool operations
4. **Enable tools via Cursor UI** for proper MCP notification handling
5. **Verify file changes** after save_config operations
6. **Test restart persistence** to confirm workflow completeness

---

**Remember**: The MCP Bridge Proxy provides dynamic tool management through proper MCP protocol compliance, ensuring tools appear immediately in Cursor while maintaining persistent configuration through save_config.