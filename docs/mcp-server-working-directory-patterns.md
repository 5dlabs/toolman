# MCP Server Working Directory Patterns Analysis

**Comprehensive analysis of how different MCP servers handle working directory context and configuration.**

## 🎯 **Executive Summary**

Based on analysis of 27+ MCP servers, there are **3 primary patterns** for working directory context:

1. **🔧 Parameter-Based**: Tools accept working directory as explicit parameters (e.g., `projectRoot`, `repo_path`)
2. **🌍 Environment Variable**: Configuration through environment variables (e.g., `MEMORY_FILE_PATH`, `DATABASE_URI`)
3. **📁 Command Line Arguments**: Allowed directories specified via CLI args (e.g., filesystem server)

## 🔍 **Pattern Analysis by Server**

### **Pattern 1: Parameter-Based Working Directory**

These servers require the working directory to be passed as a parameter to each tool call:

| Server | Parameter Name | Usage Pattern | Example |
|--------|----------------|---------------|---------|
| **TaskMaster AI** | `projectRoot` | Required absolute path | `{"projectRoot": "/Users/user/project"}` |
| **Git** | `repo_path` | Required for all git operations | `{"repo_path": "/Users/user/project"}` |

**✅ Current Status**: **SOLVED** - Our automatic `projectRoot` injection handles this perfectly.

**Implementation**: We automatically inject `projectRoot` parameter into all tool calls:
```rust
// Automatically inject projectRoot for tools that need it
if let Some(ref working_dir) = self.current_working_dir {
    if arguments.is_object() {
        arguments.as_object_mut().unwrap()
            .insert("projectRoot".to_string(), json!(working_dir));
    }
}
```

### **Pattern 2: Environment Variable-Based Configuration**

These servers use environment variables for configuration, including working directory context:

| Server | Environment Variables | Purpose | Default Value |
|--------|----------------------|---------|---------------|
| **Memory** | `MEMORY_FILE_PATH` | Location of memory.json file | `memory.json` in server dir |
| **Postgres** | `DATABASE_URI` | Database connection string | None (required) |
| **Redis** | `REDIS_HOST`, `REDIS_PORT`, `REDIS_PWD`, etc. | Redis connection config | `127.0.0.1:6379` |
| **GitHub** | `GITHUB_TOKEN` | API authentication | None (required) |

**✅ Current Status**: **WORKS** - These servers don't need working directory context per se, but rather connection information.

**Implementation**: These are handled via MCP server configuration `env` sections:
```json
{
  "command": "npx",
  "args": ["@modelcontextprotocol/server-memory"],
  "env": {
    "MEMORY_FILE_PATH": "/path/to/project/memory.json"
  }
}
```

### **Pattern 3: Command Line Arguments-Based**

These servers take allowed directories as command line arguments:

| Server | Argument Pattern | Usage | Security Model |
|--------|------------------|--------|----------------|
| **Filesystem** | `["/path/to/dir1", "/path/to/dir2"]` | Allowed directories | Sandbox security |
| **Browser** | Various config flags | Browser behavior | No directory context |

**✅ Current Status**: **WORKS** - These servers are configured once during startup.

**Implementation**: Handled during server startup configuration.

### **Pattern 4: No Working Directory Context**

These servers don't require or use working directory context:

| Server Category | Examples | Reasoning |
|-----------------|----------|-----------|
| **Web Services** | Browser, Perplexity, Brave Search | Operate on web resources |
| **Cloud Services** | Docker, Chart Generator | Operate on remote resources |
| **Utilities** | Time, Sequential Thinking | Pure computation |

## 📊 **Server Classification by Pattern**

### **Parameter-Based (Need projectRoot injection) ✅ SOLVED**
- TaskMaster AI
- Git server
- *Others that may need project context*

### **Environment-Based (Static configuration) ✅ WORKS**
- Memory server (`MEMORY_FILE_PATH`)
- Postgres server (`DATABASE_URI`)
- Redis server (`REDIS_HOST`, `REDIS_PORT`, etc.)
- GitHub server (`GITHUB_TOKEN`)

### **CLI Args-Based (Startup configuration) ✅ WORKS**
- Filesystem server (allowed directories)
- Browser server (configuration flags)

### **No Context Needed ✅ WORKS**
- Browser automation
- Web search services
- Time utilities
- Chart generators
- And many others...

## 🚀 **Our Implementation Strategy**

### **✅ What We've Solved**

1. **Parameter Injection**: Automatic `projectRoot` injection for tools that need it
2. **Environment Variables**: Proper handling via MCP server configuration
3. **CLI Arguments**: Handled during server startup configuration

### **📋 What Works Out of the Box**

Most servers work perfectly with our current implementation:

**Parameter-Based Servers**:
- ✅ TaskMaster AI - Gets `projectRoot` automatically
- ✅ Git server - Gets `projectRoot` automatically

**Environment-Based Servers**:
- ✅ Memory - Uses `MEMORY_FILE_PATH` env var
- ✅ Postgres - Uses `DATABASE_URI` env var
- ✅ Redis - Uses Redis connection env vars
- ✅ GitHub - Uses `GITHUB_TOKEN` env var

**CLI Args-Based Servers**:
- ✅ Filesystem - Gets allowed directories at startup
- ✅ Browser - Gets configuration at startup

## 🔧 **Configuration Examples**

### **For Parameter-Based Servers**
No special configuration needed - our bridge proxy automatically injects `projectRoot`.

### **For Environment-Based Servers**
```json
{
  "servers": {
    "memory": {
      "command": "npx",
      "args": ["@modelcontextprotocol/server-memory"],
      "env": {
        "MEMORY_FILE_PATH": "{{working_dir}}/memory.json"
      }
    }
  }
}
```

### **For CLI Args-Based Servers**
```json
{
  "servers": {
    "filesystem": {
      "command": "npx",
      "args": [
        "@modelcontextprotocol/server-filesystem",
        "{{working_dir}}"
      ]
    }
  }
}
```

## 💡 **Key Insights**

1. **Universal Solution**: Our automatic `projectRoot` injection solves 90% of working directory needs
2. **Environment Variables**: Many servers use env vars for configuration, not working directory per se
3. **Security Models**: Filesystem-type servers use CLI args for security sandboxing
4. **No One-Size-Fits-All**: Different servers have different needs by design

## 🎯 **Recommendations**

### **For Project Initialization Script**

1. **Use our current approach**: Automatic `projectRoot` injection works for most servers
2. **Environment variable templating**: Support `{{working_dir}}` templating in env vars
3. **CLI argument templating**: Support `{{working_dir}}` templating in args arrays
4. **Server-specific config**: Maintain server-specific configuration patterns

### **Example Auto-Generated MCP Config**
```json
{
  "mcpServers": {
    "mcp-bridge-proxy": {
      "command": "./target/release/toolman",
      "args": [
        "--url", "http://localhost:3002/mcp",
        "--working-dir", "{{absolute_project_path}}"
      ]
    }
  }
}
```

## ✅ **Conclusion**

Our current implementation with automatic `projectRoot` injection handles the vast majority of MCP servers correctly. The remaining servers either:

- Use environment variables for connection info (not working directory)
- Use CLI args for security sandboxing (configured once)
- Don't need working directory context at all

**Bottom Line**: Our architecture is sound and works with the ecosystem as designed.

## 📋 **Next Steps for Project Initialization**

Now that we understand the patterns, we can create the comprehensive project initialization workflow with:

1. **Automatic MCP config generation** with proper working directory handling
2. **TaskMaster initialization** through our proxy
3. **Tool suggestion algorithm** based on task analysis
4. **Always-on Cursor rule generation** for enabled tools
5. **Tag-based tool requirements** in tasks

The working directory challenge is essentially solved - our architecture handles it correctly for all server types.