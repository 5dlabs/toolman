# 🧪 Testing Multi-Project MCP Bridge Proxy Functionality

**Comprehensive guide for testing the MCP Bridge Proxy's multi-project working directory system.**

## 🎯 **What We're Testing**

The MCP Bridge Proxy now supports **per-project tool filtering and context isolation**:

1. **Tool Filtering**: Each project shows only its enabled tools
2. **Working Directory Context**: Tools receive correct project directory
3. **User Config Isolation**: Each project has independent tool preferences
4. **Parameter Injection**: Tools get `projectRoot` automatically

## 🔧 **Prerequisites**

### **1. Server Must Be Running**
```bash
# Check if server is running
ps aux | grep toolman-http

# If not running, start it:
cd /Users/jonathonfritz/mcp-proxy
nohup ./target/release/toolman-http --project-dir $(pwd) --port 3002 > /tmp/mcp-bridge-proxy.log 2>&1 &

# Wait for startup (30-60 seconds)
tail -f /tmp/mcp-bridge-proxy.log
# Wait until you see: "✅ HTTP server listening on http://127.0.0.1:3002"
```

### **2. Verify MCP Configurations**

**mcp-proxy project** (`.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "mcp-bridge-proxy": {
      "command": "./target/release/toolman",
      "args": ["--url", "http://localhost:3002/mcp", "--working-dir", "/Users/jonathonfritz/mcp-proxy"]
    }
  }
}
```

**agent-team project** (`.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "mcp-bridge-proxy": {
      "command": "/Users/jonathonfritz/mcp-proxy/target/release/toolman",
      "args": ["--url", "http://localhost:3002/mcp", "--working-dir", "/Users/jonathonfritz/agent-team"]
    }
  }
}
```

## 🧪 **Test Scenarios**

### **Test 1: Tool Filtering Per Project**

**Expected Results:**
- **mcp-proxy**: Shows ~11 tools (3 core + enabled tools from user config)
- **agent-team**: Shows ~3-10 tools (3 core + any enabled tools)

**How to Test:**
1. **In mcp-proxy project**: Ask "What tools are available?"
2. **In agent-team project**: Ask "What tools are available?"
3. **Compare tool counts** - should be different

### **Test 2: TaskMaster Working Directory**

**Expected Results:**
- TaskMaster tools should find `.taskmaster/` directory in the correct project

**How to Test in mcp-proxy:**
```
Can you use TaskMaster to list the current tasks?
```

**How to Test in agent-team:**
```
Can you use TaskMaster to initialize this project or list tasks?
```

**Expected Behavior:**
- **mcp-proxy**: Should find existing tasks
- **agent-team**: Should either find tasks or offer to initialize

### **Test 3: Memory Server Context**

**Current Status**: ⚠️ **Memory server still uses startup-time working directory**

**How to Test:**
```
Can you create a memory entity to test the memory server?
```

**Expected Current Behavior:**
- Both projects will write to `/Users/jonathonfritz/mcp-proxy/memory.json`
- This is a **known limitation** we're working on

### **Test 4: Tool Management**

**How to Test:**
```
Can you enable the git_git_status tool for this project?
```

**Expected Results:**
- Tool should be enabled only for the current project
- Other project should not see the newly enabled tool
- Preference should be saved to project-specific config file

## 🔍 **Manual Testing Commands**

### **Direct API Testing**

**Test from mcp-proxy directory:**
```bash
cd /Users/jonathonfritz/mcp-proxy
echo '{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}' | ./target/release/toolman --url http://localhost:3002/mcp --working-dir /Users/jonathonfritz/mcp-proxy
```

**Test from agent-team directory:**
```bash
echo '{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}' | /Users/jonathonfritz/mcp-proxy/target/release/toolman --url http://localhost:3002/mcp --working-dir /Users/jonathonfritz/agent-team
```

### **Test Tool Calls**

**Test TaskMaster (should work correctly):**
```bash
echo '{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "task_master_ai_get_tasks", "arguments": {}}}' | ./target/release/toolman --url http://localhost:3002/mcp --working-dir /Users/jonathonfritz/mcp-proxy
```

## 📊 **Expected Results Summary**

| Feature | mcp-proxy Project | agent-team Project | Status |
|---------|------------------|-------------------|---------|
| **Tool Count** | ~11 tools | ~3-10 tools | ✅ Working |
| **TaskMaster Context** | Finds existing tasks | Project-specific behavior | ✅ Working |
| **Memory Context** | Uses mcp-proxy memory.json | Uses mcp-proxy memory.json | ⚠️ Needs Fix |
| **Tool Management** | Project-specific config | Project-specific config | ✅ Working |
| **User Config** | `.mcp-bridge-proxy-config.json` | Independent config | ✅ Working |

## 🚨 **Known Issues & Workarounds**

### **Issue 1: Memory Server Context**
- **Problem**: Memory server uses startup-time `MEMORY_FILE_PATH`
- **Impact**: Both projects write to same memory.json file
- **Workaround**: Use different memory entity names per project
- **Fix**: Planned - implement per-call environment variable injection

### **Issue 2: Cursor UI Tool Refresh**
- **Problem**: New tools don't appear until user sends another message
- **Workaround**: After enabling tools, ask "Should I continue?" to trigger refresh
- **Status**: Documented limitation we've decided to live with

## 🔧 **Troubleshooting**

### **Server Not Responding**
```bash
# Check server status
ps aux | grep toolman-http

# Check logs
tail -20 /tmp/mcp-bridge-proxy.log

# Restart if needed
pkill -f toolman-http
cd /Users/jonathonfritz/mcp-proxy
nohup ./target/release/toolman-http --project-dir $(pwd) --port 3002 > /tmp/mcp-bridge-proxy.log 2>&1 &
```

### **Wrong Tool Count**
- **Restart Cursor** to refresh MCP connections
- **Check user config** files in each project directory
- **Verify working directory** is being passed correctly

### **TaskMaster Not Finding Files**
- **Check projectRoot injection** in server logs
- **Verify .taskmaster directory** exists in target project
- **Check file permissions** and directory structure

## 🎯 **Success Criteria**

### **✅ Multi-Project System is Working When:**
1. **Different tool counts** per project
2. **TaskMaster finds correct files** per project
3. **Tool enabling/disabling** affects only current project
4. **User configs are isolated** per project
5. **No cross-contamination** between projects

### **🔄 Future Improvements:**
1. **Memory server per-call context** (in development)
2. **Other file-based servers** context isolation
3. **Performance optimizations** for large server lists

---

**Remember**: This is a major architectural improvement that enables true multi-project development with isolated tool contexts!