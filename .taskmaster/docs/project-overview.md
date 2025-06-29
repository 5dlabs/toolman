# MCP Bridge Proxy - Project Overview (Updated)

## 🎯 Project Purpose

The MCP Bridge Proxy is a Rust-based HTTP server that provides intelligent MCP server and tool management for Cursor IDE and Claude Code, solving several key problems:

1. **Tool Overwhelm**: Users don't want to see all tools from all servers - only the specific tools they need for their project
2. **Dynamic Tool Management**: Need ability to enable/disable specific tools without restarting or editing config files manually
3. **Configuration Persistence**: Ability to experiment with tools/servers and then save successful configurations
4. **Tool Clarity**: Prefix tools with server names for clear identification

## 🏗️ **CORRECT ARCHITECTURE: System-Wide Startup + Per-Request User Filtering**

### **Core Concept**
- **System-Wide Server Startup**: Start ALL servers from system config, discover ALL tools
- **Per-Request Tool Filtering**: When user calls `tools/list`, filter based on their user config
- **Multi-User Support**: One server instance supports unlimited users with isolated preferences
- **No Server Restarts**: Tool visibility changes are immediate, no server restarts needed

### **Startup Flow**
```
Server Startup → Load System Config → Start ALL Servers → Discover ALL Tools (default: enabled=false)
↓
User Request → Extract Working Directory → Load User Config → Filter Tools → Return User's Tools
```

### **Configuration Architecture**

#### **System Config** (`servers-config.json`)
- **Purpose**: Golden registry of all available servers and tools
- **Location**: Project directory or `SYSTEM_CONFIG_PATH` environment variable
- **Content**: All servers with all tools defaulting to `enabled: false`
- **Modification**: Only modified by system administrators, never by user actions

#### **User Config** (`.mcp-bridge-proxy-config.json`)
- **Purpose**: Per-project tool visibility preferences
- **Location**: `{project_directory}/.mcp-bridge-proxy-config.json`
- **Content**: User's enabled tools override system defaults
- **Modification**: Modified by `enable_tool`, `disable_tool`, `save_config`

### **Tool Visibility Logic**
```rust
fn is_tool_enabled(tool_name: &str, user_config: &UserConfig, system_config: &SystemConfig) -> bool {
    // 1. Check user config override first
    if let Some(user_enabled) = user_config.enabled_tools.get(tool_name) {
        return *user_enabled;
    }

    // 2. Fall back to system config (defaults to false)
    system_config.get_tool_enabled(tool_name).unwrap_or(false)
}
```

### **Per-Request Processing**
```rust
async fn handle_tools_list(request: &JsonRpcRequest, headers: &HeaderMap) -> JsonRpcResponse {
    // 1. Extract user's working directory from request headers
    let working_dir = extract_working_directory(headers)?;

    // 2. Load user's config from their project directory
    let user_config = load_user_config(&working_dir)?;

    // 3. Filter all discovered tools based on user preferences
    let all_tools = self.available_tools.read().await;
    let visible_tools: Vec<Tool> = all_tools
        .iter()
        .filter(|tool| is_tool_enabled(&tool.name, &user_config, &self.system_config))
        .cloned()
        .collect();

    // 4. Return filtered tool list
    JsonRpcResponse::success(ToolsListResult { tools: visible_tools })
}
```

## 🔧 **Implementation Requirements**

### **1. System Config Loading (Startup)**
- Load `servers-config.json` from system location
- Start ALL servers defined in system config
- Discover ALL tools from all servers
- Default all tools to `enabled: false`
- Store in `available_tools` for filtering

### **2. User Config Management (Per-Request)**
- Extract working directory from `X-Working-Directory` header
- Load `.mcp-bridge-proxy-config.json` from user's project directory
- Cache user configs with TTL for performance
- Handle missing user config gracefully (all tools disabled)

### **3. Tool Filtering (Per-Request)**
- Apply user preferences over system defaults
- Return only enabled tools in `tools/list` responses
- Maintain tool prefixing for clarity
- Preserve all tool metadata and descriptions

### **4. Dynamic Tool Management**
```rust
// enable_tool: Update user config, immediate effect
async fn enable_tool(&self, server_name: &str, tool_name: &str, working_dir: &Path) -> Result<()> {
    let mut user_config = load_user_config(working_dir)?;
    user_config.enabled_tools.insert(format!("{}_{}", server_name, tool_name), true);
    save_user_config(&user_config, working_dir)?;
    // Tool immediately visible on next tools/list call
    Ok(())
}

// disable_tool: Update user config, immediate effect
async fn disable_tool(&self, server_name: &str, tool_name: &str, working_dir: &Path) -> Result<()> {
    let mut user_config = load_user_config(working_dir)?;
    user_config.enabled_tools.insert(format!("{}_{}", server_name, tool_name), false);
    save_user_config(&user_config, working_dir)?;
    // Tool immediately hidden on next tools/list call
    Ok(())
}

// save_config: Persist current ephemeral state to user config
async fn save_config(&self, working_dir: &Path) -> Result<()> {
    // Save current user preferences to .mcp-bridge-proxy-config.json
    // NO modification of system config
    save_user_config(&self.ephemeral_config, working_dir)?;
    Ok(())
}
```

## 📁 **File Structure**

### **System Level**
```
/Users/jonathonfritz/mcp-proxy/servers-config.json  # System config (golden registry)
```

### **User Level**
```
/Users/jonathonfritz/mcp-proxy/.mcp-bridge-proxy-config.json        # mcp-proxy user config
/Users/jonathonfritz/agent-team/.mcp-bridge-proxy-config.json       # agent-team user config
/Users/jonathonfritz/other-project/.mcp-bridge-proxy-config.json    # other-project user config
```

## 🎯 **Implementation Priority**

### **Phase 1: Core Architecture (IMMEDIATE)**
1. ✅ **System startup works** - All servers start, all tools discovered
2. 🔄 **Per-request user config loading** - Load from project directory
3. 🔄 **Tool filtering logic** - Apply user preferences over system defaults
4. 🔄 **Working directory extraction** - From request headers

### **Phase 2: Tool Management (NEXT)**
1. 🔄 **`enable_tool`** - Update user config, immediate visibility
2. 🔄 **`disable_tool`** - Update user config, immediate hiding
3. 🔄 **`save_config`** - Persist to user config (NOT system config)
4. 🔄 **User config format** - Define JSON structure

### **Phase 3: Optimization (FUTURE)**
1. ❌ **User config caching** - TTL-based caching for performance
2. ❌ **Config validation** - Ensure user configs are valid
3. ❌ **Migration tools** - Help users migrate existing configs

## 🚨 **Critical Design Principles**

### **✅ DO**
- Start all servers at system startup
- Filter tools per-request based on user config
- Store user preferences in project directories
- Default all tools to disabled for security
- Support unlimited concurrent users

### **❌ DON'T**
- Restart servers when users enable/disable tools
- Modify system config based on user actions
- Tie server startup to user preferences
- Store user preferences in system config
- Assume single-user operation

## 📊 **Current Implementation Status**

Based on debug logs analysis:

### **✅ Working Correctly**
- System-wide server startup
- All 241 tools discovered and prefixed
- Default all tools to `enabled: false`
- Per-request user config loading (shows `Context preference: Some(false)`)

### **🔄 Needs Implementation**
- User config file format and location
- Tool filtering logic in `tools/list` handler
- `enable_tool` and `disable_tool` implementations
- `save_config` implementation (to user config, not system config)

### **❌ Current Issues**
- All tools currently showing as disabled (need user config with enabled tools)
- User config format undefined
- Tool management commands may not exist or work correctly

---

**Next Steps**: Implement user config loading, tool filtering, and the 4 core tool management commands based on this architecture.