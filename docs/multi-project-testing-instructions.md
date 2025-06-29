# 🧪 MULTI-PROJECT MCP BRIDGE PROXY ISOLATION TEST

**Objective**: Test that the MCP Bridge Proxy correctly isolates tool preferences between different projects.

## 📋 CONTEXT YOU NEED TO KNOW

The MCP Bridge Proxy implements a **context-based tool management system** where:
- Each project gets its own isolated tool context based on project directory path
- Tool enable/disable preferences are stored per-project in `~/.mcp-bridge-proxy/contexts/{hash}.json`
- Multiple projects can have completely different sets of enabled tools
- Changes in one project don't affect other projects

## 🎯 YOUR TESTING MISSION

You need to verify that **your project gets its own isolated context** separate from the main MCP Bridge Proxy project.

## ⚙️ SETUP STEPS

### 1. Verify Different Project Directory
Ensure you're in a DIFFERENT project directory (not `/Users/jonathonfritz/mcp-proxy`):
```bash
pwd  # Should show your project path, NOT /Users/jonathonfritz/mcp-proxy
```

### 2. Create Project-Specific `.cursor/mcp.json`
Create this file in your project root:
```json
{
  "mcpServers": {
    "mcp-bridge-proxy": {
      "command": "/Users/jonathonfritz/mcp-proxy/target/release/toolman",
      "args": ["--url", "http://localhost:3002/mcp"],
      "cwd": "/Users/jonathonfritz/mcp-proxy"
    }
  },
  "env": {
    "ANTHROPIC_API_KEY": "your-key-here"
  }
}
```

### 3. Verify MCP Bridge Proxy Is Running
```bash
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}' | jq '.result.tools | length'
```
**Expected**: Should return a number (like 16)

## 🧪 CRITICAL TEST STEPS

### Test 1: Cursor UI Core Functionality Test (RECOMMENDED)

**The most important test is using the actual Cursor UI tools, just as we verified in the main project:**

#### 1A. Enable Tool via Cursor UI
```
Use the enable_tool directly in Cursor (not curl commands):
- Try: enable_tool with server_name="github" and tool_name="create_issue"
```

**Expected Response**:
```
✅ I have enabled the tool 'create_issue' from server 'github'! The tool is ready and configured.

🎯 Context: '/your/project/path:default'
💾 Note: This preference is saved to your user context and will persist across sessions.
```

**🎯 CRITICAL CHECK**: The context should show **YOUR project path**, NOT `/Users/jonathonfritz/mcp-proxy:default`

#### 1B. Disable Tool via Cursor UI
```
Use the disable_tool directly in Cursor:
- Try: disable_tool with server_name="memory" and tool_name="read_graph"
```

**Expected Response**:
```
✅ Disabled tool 'read_graph' in context '/your/project/path:default'. Preference saved to user context.
```

#### 1C. Run Tools via Cursor UI
```
Test actual tool execution:
- Try: github_search_repositories with query="test"
```

**Expected**: Should return real GitHub search results (JSON data with repositories), not errors

#### 1D. Save Config via Cursor UI
```
Use the save_config tool:
- Try: save_config (no parameters needed)
```

**Expected Response**:
```
✅ Configuration saved atomically! X tools are now persistent.
🔒 Enhanced features: automatic backup, validation, and recovery
```

### Test 2: Verify Context File Creation

```bash
# Check if your project got its own context file
ls -la ~/.mcp-bridge-proxy/contexts/
```

**Expected**: You should see **two different** context files:
- One for the main project (`67a397f4f83b09d7.json` or similar)
- One for YOUR project (different hash)

### Test 3: Verify Context Isolation

```bash
# Check your context file content
find ~/.mcp-bridge-proxy/contexts/ -name "*.json" -exec jq '.project_path' {} \;
```

**Expected**: Should show **two different project paths**:
- `/Users/jonathonfritz/mcp-proxy`
- `/your/project/path`

### Test 4: Verify Tool State Persistence

```bash
# Check all context files show different enabled tools
find ~/.mcp-bridge-proxy/contexts/ -name "*.json" -exec sh -c 'echo "=== $1 ===" && jq "{project_path, enabled_tools, disabled_tools}" "$1"' _ {} \;
```

**Expected**: Each project should have **different enabled/disabled tools**

### Test 5: Alternative CLI Testing (If Cursor UI Unavailable)

If you cannot test via Cursor UI, use these curl commands:

```bash
# Enable a tool and check the context path
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "enable_tool", "arguments": {"server_name": "memory", "tool_name": "read_graph"}}}' | jq '.result.content[0].text'
```

```bash
# Test tool execution
curl -X POST http://localhost:3002/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "github_search_repositories", "arguments": {"query": "test"}}}' | jq '.result | keys'
```

## 📊 SUCCESS CRITERIA

You've successfully verified multi-project isolation if:

### **✅ Cursor UI Functionality Working**
- **enable_tool**: Successfully enables tools and shows YOUR project context path
- **disable_tool**: Successfully disables tools and saves preferences to YOUR context
- **Tool execution**: Real tools work and return actual data (e.g., GitHub search results)
- **save_config**: Persists configuration with atomic backup creation

### **✅ Context Isolation Verified**
- **Unique Context Path**: Your enable_tool responses show your project path, not `/Users/jonathonfritz/mcp-proxy:default`
- **Separate Context Files**: Two different `.json` files exist in `~/.mcp-bridge-proxy/contexts/`
- **Different Project Paths**: Context files show different `project_path` values
- **Tool Isolation**: Each project can have different enabled/disabled tools
- **Independent Changes**: Enabling tools in your project doesn't affect the other project

### **✅ Expected Final Context State**
Your context file should look similar to this (but with YOUR project path):
```json
{
  "project_path": "/your/project/path",
  "enabled_tools": {
    "github": ["create_issue"],
    "memory": ["read_graph"]
  },
  "disabled_tools": {
    "memory": ["search_nodes"]
  }
}
```

### **✅ Main Project Context Should Remain Unchanged**
The main project context (`/Users/jonathonfritz/mcp-proxy:default`) should have its own separate state:
```json
{
  "project_path": "/Users/jonathonfritz/mcp-proxy",
  "enabled_tools": {
    "github": ["create_issue"],
    "filesystem": ["write_file"]
  },
  "disabled_tools": {
    "memory": ["search_nodes", "read_graph"],
    "github": ["create_repository"]
  }
}
```

## 🚨 FAILURE INDICATORS

If you see any of these, **multi-project isolation is broken**:

- ❌ Context path shows `/Users/jonathonfritz/mcp-proxy` instead of your project
- ❌ Only one context file exists
- ❌ Your tool changes appear in the other project's context file
- ❌ Both projects show identical enabled tools

## 📝 REPORT BACK

Please provide:

1. **Your project directory path**: `pwd` output
2. **Context response**: The exact context path from enable_tool response
3. **Context files**: Output from `ls -la ~/.mcp-bridge-proxy/contexts/`
4. **Context content**: The project_path and enabled_tools from your context file
5. **Success/Failure**: Whether multi-project isolation is working correctly

## 🎯 EXPECTED FINAL STATE

After successful testing:
- **Project A** (mcp-proxy): Has its own context with some tools enabled/disabled
- **Project B** (your project): Has separate context with different tools enabled/disabled
- **Complete isolation**: Changes in one project don't affect the other
- **Persistent preferences**: Each project remembers its own tool preferences

## 📞 TROUBLESHOOTING

### Common Issues

1. **No response from server**: Check if MCP Bridge Proxy is running on port 3002
2. **Permission denied**: Ensure the binary path is correct in your config
3. **Context shows wrong path**: Verify you're running the test from your project directory
4. **No context files created**: Check if `~/.mcp-bridge-proxy/contexts/` directory exists

### Debug Commands

```bash
# Check if server is running
ps aux | grep toolman-http | grep -v grep

# Check server logs
tail -f /tmp/mcp-bridge-proxy.log

# Test basic connectivity
curl -v http://localhost:3002/mcp -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

**This test validates that the MCP Bridge Proxy correctly implements project-based tool isolation, allowing multiple projects to maintain independent tool preferences without interference.**