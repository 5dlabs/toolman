use anyhow::Result;
use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use toolman::tool_suggester::ToolSuggester;
use toolman::{resolve_working_directory, ConfigManager, ContextManager, ServerConfig};
use tower_http::cors::CorsLayer;

/// Simple HTTP MCP Bridge Server
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// HTTP server port
    #[arg(short = 'P', long = "port", default_value = "3000")]
    port: u16,

    /// Project directory containing servers-config.json
    #[arg(short = 'p', long = "project-dir")]
    project_dir: Option<std::path::PathBuf>,

    /// Export all discovered tools to a JSON file and exit
    #[arg(long = "export-tools")]
    export_tools: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
    server_name: String,
}

// Tool name parsing structures and functions
#[derive(Debug, Clone, PartialEq)]
struct ParsedTool {
    server_name: String,
    tool_name: String,
}

#[derive(Debug, thiserror::Error)]
enum ToolParseError {
    #[error("Invalid tool name format: '{0}' (expected 'server_tool' format)")]
    InvalidFormat(String),
    #[error("Missing tool name: '{0}' (server name only)")]
    MissingToolName(String),
    #[error("Missing server name: '{0}' (tool name only)")]
    MissingServerName(String),
    #[error("Empty tool name")]
    EmptyToolName,
}

/// Parse a prefixed tool name into server and tool components
///
/// Examples:
/// - "memory_delete_entities" → ParsedTool { server_name: "memory", tool_name: "delete_entities" }
/// - "filesystem_read_file" → ParsedTool { server_name: "filesystem", tool_name: "read_file" }
/// - "complex_server_complex_tool_name" → ParsedTool { server_name: "complex_server", tool_name: "complex_tool_name" }
fn parse_tool_name_with_servers(
    tool_name: &str,
    available_servers: &[String],
) -> Result<ParsedTool, ToolParseError> {
    if tool_name.is_empty() {
        return Err(ToolParseError::EmptyToolName);
    }

    // Convert server names to underscore format for matching
    // e.g., "task-master-ai" -> "task_master_ai"
    let underscore_servers: Vec<String> = available_servers
        .iter()
        .map(|s| s.replace('-', "_"))
        .collect();

    // Find underscore positions
    let underscore_positions: Vec<usize> = tool_name
        .char_indices()
        .filter(|(_, c)| *c == '_')
        .map(|(i, _)| i)
        .collect();

    if underscore_positions.is_empty() {
        return Err(ToolParseError::InvalidFormat(tool_name.to_string()));
    }

    // Try each underscore position to find a match with known servers
    for &underscore_pos in &underscore_positions {
        let potential_server_underscore = &tool_name[..underscore_pos];
        let potential_tool = &tool_name[underscore_pos + 1..];

        if !potential_server_underscore.is_empty() && !potential_tool.is_empty() {
            // Check if this matches any of our known servers (in underscore format)
            if underscore_servers
                .iter()
                .any(|s| s == potential_server_underscore)
            {
                // Find the original server name (with hyphens)
                let original_server = available_servers
                    .iter()
                    .find(|s| s.replace('-', "_") == potential_server_underscore)
                    .unwrap(); // Safe because we just found it above

                return Ok(ParsedTool {
                    server_name: original_server.clone(),
                    tool_name: potential_tool.to_string(),
                });
            }
        }
    }

    Err(ToolParseError::InvalidFormat(format!(
        "{} (no matching server found in: {})",
        tool_name,
        available_servers.join(", ")
    )))
}

// Legacy function for backwards compatibility with tests
fn parse_tool_name(tool_name: &str) -> Result<ParsedTool, ToolParseError> {
    // Fallback to simple pattern matching when server list not available
    if tool_name.is_empty() {
        return Err(ToolParseError::EmptyToolName);
    }

    let underscore_positions: Vec<usize> = tool_name
        .char_indices()
        .filter(|(_, c)| *c == '_')
        .map(|(i, _)| i)
        .collect();

    if underscore_positions.is_empty() {
        return Err(ToolParseError::InvalidFormat(tool_name.to_string()));
    }

    // Use first underscore as fallback
    let underscore_pos = underscore_positions[0];
    let server_name = &tool_name[..underscore_pos];
    let tool_name_part = &tool_name[underscore_pos + 1..];

    if server_name.is_empty() || tool_name_part.is_empty() {
        return Err(ToolParseError::InvalidFormat(tool_name.to_string()));
    }

    Ok(ParsedTool {
        server_name: server_name.to_string(),
        tool_name: tool_name_part.to_string(),
    })
}

// Server connection pool structures and implementations
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug)]
struct McpServerConnection {
    process: Child,
    stdin: tokio::process::ChildStdin,
    stdout_reader: BufReader<tokio::process::ChildStdout>,
    server_name: String,
    next_request_id: Arc<Mutex<u64>>,
}

#[derive(Debug)]
struct ServerConnectionPool {
    connections: Arc<RwLock<HashMap<String, Arc<Mutex<McpServerConnection>>>>>,
    config_manager: Arc<RwLock<ConfigManager>>,
}

impl ServerConnectionPool {
    fn new(config_manager: Arc<RwLock<ConfigManager>>) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            config_manager,
        }
    }

    /// Start an MCP server and establish a connection
    async fn start_server(&self, server_name: &str) -> anyhow::Result<()> {
        self.start_server_with_context(server_name, None).await
    }

    /// Start an MCP server with optional user working directory context
    async fn start_server_with_context(
        &self,
        server_name: &str,
        user_working_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        // For filesystem server, always restart with new context to ensure correct allowed directories
        if server_name == "filesystem" {
            println!(
                "🔄 Restarting filesystem server with user context: {:?}",
                user_working_dir
            );
            let _ = self.stop_server(server_name).await; // Stop existing server if any
        } else {
            // Check if server is already connected for non-filesystem servers
            let connections = self.connections.read().await;
            if connections.contains_key(server_name) {
                println!("🔗 Server '{}' is already connected", server_name);
                return Ok(());
            }
        }

        let servers = self.config_manager.read().await;
        let config = servers.get_servers().get(server_name).ok_or_else(|| {
            anyhow::anyhow!("Server '{}' not found in configuration", server_name)
        })?;

        println!("🚀 Starting MCP server: {}", server_name);

        // Spawn the server process
        let mut cmd = Command::new(&config.command);

        // For filesystem server, override args to use user's working directory
        if server_name == "filesystem" && user_working_dir.is_some() {
            let user_dir = user_working_dir.unwrap();
            println!(
                "📁 [{}] Using user working directory for filesystem server: {}",
                server_name,
                user_dir.display()
            );

            // Build filesystem-specific args: npx -y @modelcontextprotocol/server-filesystem <user_working_dir>
            let mut fs_args = Vec::new();
            fs_args.push("-y".to_string());
            fs_args.push("@modelcontextprotocol/server-filesystem".to_string());
            fs_args.push(user_dir.to_string_lossy().to_string());

            cmd.args(&fs_args);
        } else {
            // Use original config args for all other servers
            cmd.args(&config.args);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set working directory (default to project directory if not specified)
        let project_dir = servers
            .get_config_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let working_dir = config
            .working_directory
            .as_ref()
            .map(|wd| resolve_working_directory(wd, project_dir))
            .unwrap_or_else(|| project_dir.to_path_buf());
        cmd.current_dir(&working_dir);
        println!(
            "🔍 [{}] Setting working directory: {}",
            server_name,
            working_dir.display()
        );

        // Inherit all environment variables from parent process
        cmd.envs(std::env::vars());

        // Add/override with server-specific environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // 🎯 EDGE CASE HANDLING: Inject working directory for specific servers
        // Hard-coded for internal use - will make configurable later
        match server_name {
            "memory" => {
                // Memory server needs MEMORY_FILE_PATH set to project directory
                let memory_file = working_dir.join("memory.json");
                cmd.env(
                    "MEMORY_FILE_PATH",
                    memory_file.to_string_lossy().to_string(),
                );
                println!(
                    "🧠 [{}] Setting MEMORY_FILE_PATH: {}",
                    server_name,
                    memory_file.display()
                );
            }
            "docs-manager" | "mcp-docs-service" => {
                // MCP Docs service - set default docs directory
                let docs_dir = working_dir.join("docs");
                cmd.env("MCP_DOCS_ROOT", docs_dir.to_string_lossy().to_string());
                println!(
                    "📚 [{}] Setting MCP_DOCS_ROOT: {}",
                    server_name,
                    docs_dir.display()
                );
                // TODO: Modify command args to include docs path as well
            }
            "filesystem" => {
                // Filesystem server uses command-line args, not environment variables
                // Args are handled above in the filesystem-specific section
                println!(
                    "📁 [{}] Filesystem server configured via command-line args",
                    server_name
                );
            }
            name if name.contains("file") || name.contains("docs") || name.contains("storage") => {
                // Generic file-related servers - set working directory env var
                cmd.env(
                    "WORKING_DIRECTORY",
                    working_dir.to_string_lossy().to_string(),
                );
                cmd.env("PROJECT_DIR", working_dir.to_string_lossy().to_string());
                println!(
                    "📂 [{}] Setting PROJECT_DIR and WORKING_DIRECTORY: {}",
                    server_name,
                    working_dir.display()
                );
            }
            _ => {
                // No special handling needed for this server
            }
        }

        let mut process = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn server '{}': {}", server_name, e))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin for server '{}'", server_name))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout for server '{}'", server_name))?;

        let stdout_reader = BufReader::new(stdout);

        // Create connection object
        let connection = McpServerConnection {
            process,
            stdin,
            stdout_reader,
            server_name: server_name.to_string(),
            next_request_id: Arc::new(Mutex::new(1)),
        };

        let connection_arc = Arc::new(Mutex::new(connection));

        // Initialize the MCP server
        self.initialize_server(connection_arc.clone()).await?;

        // Store the connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(server_name.to_string(), connection_arc);
        }

        println!(
            "✅ Successfully started and initialized server: {}",
            server_name
        );
        Ok(())
    }

    /// Initialize an MCP server with the required handshake
    async fn initialize_server(
        &self,
        connection: Arc<Mutex<McpServerConnection>>,
    ) -> anyhow::Result<()> {
        let server_name = {
            let conn = connection.lock().await;
            conn.server_name.clone()
        };

        println!("🔄 Initializing MCP server: {}", server_name);

        // Send initialize request
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": true
                    }
                },
                "clientInfo": {
                    "name": "toolman-http",
                    "version": "1.0.0"
                }
            }
        });

        self.send_request(connection.clone(), init_request).await?;

        // Read initialization response
        let _init_response = self.read_response(connection.clone()).await?;

        // Send initialized notification
        let initialized_notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        self.send_notification(connection.clone(), initialized_notification)
            .await?;

        // Give server time to initialize
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        println!("✅ Server '{}' initialized successfully", server_name);
        Ok(())
    }

    /// Send a JSON-RPC request to a server
    async fn send_request(
        &self,
        connection: Arc<Mutex<McpServerConnection>>,
        request: Value,
    ) -> anyhow::Result<()> {
        let request_msg = format!("{}\n", serde_json::to_string(&request)?);

        let mut conn = connection.lock().await;
        conn.stdin
            .write_all(request_msg.as_bytes())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send request: {}", e))?;

        Ok(())
    }

    /// Send a JSON-RPC notification to a server
    async fn send_notification(
        &self,
        connection: Arc<Mutex<McpServerConnection>>,
        notification: Value,
    ) -> anyhow::Result<()> {
        let notification_msg = format!("{}\n", serde_json::to_string(&notification)?);

        let mut conn = connection.lock().await;
        conn.stdin
            .write_all(notification_msg.as_bytes())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send notification: {}", e))?;

        Ok(())
    }

    /// Read a response from a server
    async fn read_response(
        &self,
        connection: Arc<Mutex<McpServerConnection>>,
    ) -> anyhow::Result<Value> {
        let mut conn = connection.lock().await;

        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = tokio::time::timeout(
                tokio::time::Duration::from_secs(30), // Increased timeout for large responses
                conn.stdout_reader.read_line(&mut line),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Timeout reading response"))?
            .map_err(|e| anyhow::anyhow!("Failed to read response: {}", e))?;

            if bytes_read == 0 {
                return Err(anyhow::anyhow!("Server connection closed"));
            }

            // Try to parse as JSON
            if let Ok(response) = serde_json::from_str::<Value>(&line) {
                // Check if this is a notification (has "method" but no "id")
                if response.get("method").is_some() && response.get("id").is_none() {
                    // This is a notification message, continue reading for the actual response
                    continue;
                }

                // Check if this is an actual response (has "id" and either "result" or "error")
                if response.get("id").is_some()
                    && (response.get("result").is_some() || response.get("error").is_some())
                {
                    // This is the actual response we want
                    return Ok(response);
                }

                // If it's JSON but doesn't match our criteria, continue reading
                continue;
            }

            // If not JSON, it might be a status message, continue reading
        }
    }

    /// Forward a tool call to the appropriate server with user context
    async fn forward_tool_call_with_context(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
        user_working_dir: Option<&std::path::Path>,
    ) -> anyhow::Result<Value> {
        // Start server if not already started, with user context for filesystem server
        if server_name == "filesystem" && user_working_dir.is_some() {
            self.start_server_with_context(server_name, user_working_dir)
                .await?;
        } else {
            self.start_server(server_name).await?;
        }

        let connection = {
            let connections = self.connections.read().await;
            connections
                .get(server_name)
                .ok_or_else(|| anyhow::anyhow!("Server '{}' connection not found", server_name))?
                .clone()
        };

        // Get next request ID
        let request_id = {
            let conn = connection.lock().await;
            let mut id = conn.next_request_id.lock().await;
            let current_id = *id;
            *id += 1;
            current_id
        };

        // Create tools/call request
        let tool_request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        println!(
            "🔧 Forwarding tool call: {} to server {}",
            tool_name, server_name
        );

        // Send request and read response
        self.send_request(connection.clone(), tool_request).await?;
        let response = self.read_response(connection).await?;

        println!("📨 Received response from server {}", server_name);

        Ok(response)
    }

    /// Forward a tool call to the appropriate server
    async fn forward_tool_call(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<Value> {
        self.forward_tool_call_with_context(server_name, tool_name, arguments, None)
            .await
    }

    /// Stop a server connection
    async fn stop_server(&self, server_name: &str) -> anyhow::Result<()> {
        let connection = {
            let mut connections = self.connections.write().await;
            connections.remove(server_name)
        };

        if let Some(connection) = connection {
            let mut conn = connection.lock().await;
            let _ = conn.process.kill().await;
            println!("🛑 Stopped server: {}", server_name);
        }

        Ok(())
    }

    /// Get all active connections
    async fn list_active_connections(&self) -> Vec<String> {
        let connections = self.connections.read().await;
        connections.keys().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct BridgeState {
    // System-level configuration manager (for server startup and discovery)
    system_config_manager: Arc<RwLock<ConfigManager>>,
    // Available tools from all configured servers (discovered once from system config)
    available_tools: Arc<RwLock<HashMap<String, Tool>>>,
    // Connection pool for active MCP servers
    connection_pool: Arc<ServerConnectionPool>,
    // Context manager for user-specific tool configurations
    context_manager: Arc<RwLock<ContextManager>>,
    // Current project directory for context resolution
    project_dir: Option<std::path::PathBuf>,
    // Current working directory for user context (per-request)
    current_working_dir: Arc<RwLock<Option<std::path::PathBuf>>>,
    // Current user configuration (per-request)
    user_config: Arc<RwLock<UserConfig>>,
    // Agent header name for per-session isolation
    agent_header_name: String,
}

// JSON-RPC 2.0 message types
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

/// Simple user configuration stored in project directory
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UserConfig {
    /// Tools that the user has explicitly enabled
    enabled_tools: HashMap<String, bool>,
    /// Last updated timestamp
    #[serde(default)]
    last_updated: String,
}

impl UserConfig {
    /// Create a new empty user config
    fn new() -> Self {
        Self {
            enabled_tools: HashMap::new(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Load user config from project directory
    fn load_from_project(project_dir: &std::path::Path) -> Result<Self> {
        let config_file = project_dir.join(".mcp-bridge-proxy-config.json");

        if config_file.exists() {
            let content = std::fs::read_to_string(&config_file)?;
            let mut config: UserConfig = serde_json::from_str(&content)?;

            // Ensure last_updated is set
            if config.last_updated.is_empty() {
                config.last_updated = chrono::Utc::now().to_rfc3339();
            }

            println!("📂 Loaded user config from: {}", config_file.display());
            Ok(config)
        } else {
            println!(
                "📂 No user config found, creating new one at: {}",
                config_file.display()
            );
            Ok(Self::new())
        }
    }

    /// Save user config to project directory
    fn save_to_project(&mut self, project_dir: &std::path::Path) -> Result<()> {
        let config_file = project_dir.join(".mcp-bridge-proxy-config.json");

        // Update timestamp
        self.last_updated = chrono::Utc::now().to_rfc3339();

        // Write config file
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_file, content)?;

        println!("💾 Saved user config to: {}", config_file.display());
        Ok(())
    }

    /// Check if a tool is enabled by the user
    fn is_tool_enabled(&self, tool_name: &str) -> Option<bool> {
        self.enabled_tools.get(tool_name).copied()
    }



    fn load_from_file(config_file: &std::path::Path) -> Result<Self> {
        if config_file.exists() {
            let content = std::fs::read_to_string(config_file)?;
            let mut config: UserConfig = serde_json::from_str(&content)?;
            if config.last_updated.is_empty() {
                config.last_updated = chrono::Utc::now().to_rfc3339();
            }
            println!("📂 Loaded user config from: {}", config_file.display());
            Ok(config)
        } else {
            println!("📂 No user config found, creating new one at: {}", config_file.display());
            Ok(Self::new())
        }
    }
}

impl BridgeState {
    pub fn new(project_dir: Option<std::path::PathBuf>) -> Result<Self> {
        // Determine system config path from environment variable or fallback to project
        let system_config_path = std::env::var("SYSTEM_CONFIG_PATH")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| project_dir.clone());

        println!("🔧 System config path: {:?}", system_config_path);
        println!("🔧 Project directory: {:?}", project_dir);

        // Create system-level config manager (for server discovery and startup)
        let system_config_manager_instance = ConfigManager::new(system_config_path)?;

        // Cleanup orphaned temporary files from previous runs
        if let Err(e) = system_config_manager_instance.cleanup_temp_files() {
            eprintln!("Warning: Failed to cleanup temporary files: {}", e);
        }

        let system_config_manager = Arc::new(RwLock::new(system_config_manager_instance));
        let connection_pool = Arc::new(ServerConnectionPool::new(system_config_manager.clone()));

        // Initialize context manager
        let context_manager_instance = ContextManager::new()
            .map_err(|e| anyhow::anyhow!("Failed to initialize context manager: {}", e))?;
        let context_manager = Arc::new(RwLock::new(context_manager_instance));

        // Agent header name for per-session isolation
        let agent_header_name = std::env::var("AGENT_HEADER_NAME").unwrap_or("X-Agent-ID".to_string());

        Ok(Self {
            system_config_manager,
            available_tools: Arc::new(RwLock::new(HashMap::new())),
            connection_pool,
            context_manager,
            project_dir,
            current_working_dir: Arc::new(RwLock::new(None)),
            user_config: Arc::new(RwLock::new(UserConfig::new())),
            agent_header_name,
        })
    }

    // Discover all tools from all configured servers and enable those marked as enabled in config
    /// Extract user context from request headers and working directory
    async fn load_user_context(
        &self,
        request_headers: Option<&axum::http::HeaderMap>,
    ) -> anyhow::Result<()> {
        // Extract working directory from X-Working-Directory header
        let working_dir = if let Some(headers) = request_headers {
            if let Some(working_dir_header) = headers.get("X-Working-Directory") {
                let working_dir_str = working_dir_header
                    .to_str()
                    .map_err(|e| anyhow::anyhow!("Invalid working directory header: {}", e))?;
                std::path::PathBuf::from(working_dir_str)
            } else {
                // Fallback to project directory if no header
                self.project_dir
                    .clone()
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            }
        } else {
            // No headers provided, use project directory
            self.project_dir
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        };

        println!(
            "🔍 DEBUG: Loading user context for working directory: {}",
            working_dir.display()
        );

        // Canonicalize the path for consistency
        let working_dir = working_dir.canonicalize().unwrap_or(working_dir);

        // Store the current working directory
        {
            let mut current_wd = self.current_working_dir.write().await;
            *current_wd = Some(working_dir.clone());
        }

        // Extract agent ID from configurable header
        let agent_id = request_headers.and_then(|headers| {
            headers.get(&self.agent_header_name)
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
        });

        let config_file = match agent_id {
            Some(id) => working_dir.join(format!(".mcp-bridge-proxy-config-{}.json", id)),
            None => working_dir.join(".mcp-bridge-proxy-config.json"),
        };

        println!("🔍 Loading user config from: {}", config_file.display());

        let user_config = UserConfig::load_from_file(&config_file)?;

        // Store the loaded user config
        {
            let mut config = self.user_config.write().await;
            *config = user_config;
        }

        println!(
            "✅ DEBUG: User context loaded successfully for: {}",
            working_dir.display()
        );
        Ok(())
    }

    /// Get the context identifier for debugging/logging
    async fn get_context_id(&self) -> String {
        let context_manager = self.context_manager.read().await;
        if let Some(context) = context_manager.get_context() {
            format!(
                "{}:{}",
                context.project_path,
                context.user_id.as_deref().unwrap_or("default")
            )
        } else {
            "unknown".to_string()
        }
    }


    /// Determine if a tool should be enabled based on context preferences
    /// SYSTEM vs USER CONFIG ARCHITECTURE:
    /// - System config (servers-config.json) is used ONLY for server discovery and startup
    /// - User contexts control tool visibility (default: all tools DISABLED)
    /// - Only tools explicitly enabled via enable_tool appear in Cursor
    async fn should_tool_be_enabled(&self, server_name: &str, tool_name: &str) -> bool {
        // Check context preference first (this is the primary mechanism now)
        let context_manager = self.context_manager.read().await;
        if let Some(context_preference) =
            context_manager.should_tool_be_enabled(server_name, tool_name)
        {
            return context_preference;
        }
        drop(context_manager);

        // NEW ARCHITECTURE: Default to FALSE (all tools disabled by default)
        // System config is no longer used for tool visibility - only for server startup
        // This ensures clean separation: system discovers everything, user enables specific tools
        false
    }

    // Discover tools from a single server (without "starting" it permanently)
    async fn discover_server_tools(
        &self,
        server_name: &str,
        config: &ServerConfig,
    ) -> anyhow::Result<Vec<Tool>> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        let start_time = std::time::Instant::now();
        println!(
            "🔍 [{}] Starting tool discovery at {:?}",
            server_name,
            chrono::Utc::now().format("%H:%M:%S")
        );
        println!(
            "🔍 [{}] Command: {} {}",
            server_name,
            config.command,
            config.args.join(" ")
        );
        println!(
            "🔍 [{}] Environment variables: {:?}",
            server_name, config.env
        );

        // Debug: Check if command exists
        let command_check = std::process::Command::new("which")
            .arg(&config.command)
            .output();

        match command_check {
            Ok(output) => {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    println!("🔍 [{}] Command found at: {}", server_name, path.trim());
                } else {
                    println!(
                        "⚠️ [{}] Command '{}' not found in PATH",
                        server_name, config.command
                    );
                    println!(
                        "⚠️ [{}] Current PATH: {}",
                        server_name,
                        std::env::var("PATH").unwrap_or_default()
                    );
                }
            }
            Err(e) => {
                println!(
                    "⚠️ [{}] Failed to check command existence: {}",
                    server_name, e
                );
            }
        }

        // Spawn the server process for tool discovery only
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set working directory (default to project directory if not specified)
        let config_manager = self.system_config_manager.read().await;
        let project_dir = config_manager
            .get_config_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let working_dir = config
            .working_directory
            .as_ref()
            .map(|wd| resolve_working_directory(wd, project_dir))
            .unwrap_or_else(|| project_dir.to_path_buf());
        cmd.current_dir(&working_dir);
        println!(
            "🔍 [{}] Setting working directory: {}",
            server_name,
            working_dir.display()
        );

        // Inherit all environment variables from parent process
        cmd.envs(std::env::vars());

        // Add/override with server-specific environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
            println!("🔍 [{}] Setting env: {}={}", server_name, key, value);
        }

        // 🎯 EDGE CASE HANDLING: Inject working directory for specific servers
        // Hard-coded for internal use - will make configurable later
        match server_name {
            "memory" => {
                // Memory server needs MEMORY_FILE_PATH set to project directory
                let memory_file = working_dir.join("memory.json");
                cmd.env(
                    "MEMORY_FILE_PATH",
                    memory_file.to_string_lossy().to_string(),
                );
                println!(
                    "🧠 [{}] Setting MEMORY_FILE_PATH: {}",
                    server_name,
                    memory_file.display()
                );
            }
            "docs-manager" | "mcp-docs-service" => {
                // MCP Docs service - set default docs directory
                let docs_dir = working_dir.join("docs");
                cmd.env("MCP_DOCS_ROOT", docs_dir.to_string_lossy().to_string());
                println!(
                    "📚 [{}] Setting MCP_DOCS_ROOT: {}",
                    server_name,
                    docs_dir.display()
                );
                // TODO: Modify command args to include docs path as well
            }
            "filesystem" => {
                // Filesystem server - set allowed directory
                cmd.env(
                    "ALLOWED_DIRECTORY",
                    working_dir.to_string_lossy().to_string(),
                );
                println!(
                    "📁 [{}] Setting ALLOWED_DIRECTORY: {}",
                    server_name,
                    working_dir.display()
                );
            }
            name if name.contains("file") || name.contains("docs") || name.contains("storage") => {
                // Generic file-related servers - set working directory env var
                cmd.env(
                    "WORKING_DIRECTORY",
                    working_dir.to_string_lossy().to_string(),
                );
                cmd.env("PROJECT_DIR", working_dir.to_string_lossy().to_string());
                println!(
                    "📂 [{}] Setting PROJECT_DIR and WORKING_DIRECTORY: {}",
                    server_name,
                    working_dir.display()
                );
            }
            _ => {
                // No special handling needed for this server
            }
        }

        println!(
            "🔍 [{}] Spawning process... (elapsed: {:?})",
            server_name,
            start_time.elapsed()
        );
        let mut process = match cmd.spawn() {
            Ok(p) => {
                println!(
                    "✅ [{}] Process spawned successfully (elapsed: {:?})",
                    server_name,
                    start_time.elapsed()
                );
                p
            }
            Err(e) => {
                println!(
                    "❌ [{}] Failed to spawn process: {} (elapsed: {:?})",
                    server_name,
                    e,
                    start_time.elapsed()
                );
                return Err(e.into());
            }
        };

        let mut stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();

        // Spawn a task to consume stderr to prevent blocking
        let server_name_clone = server_name.to_string();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut stderr_reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match stderr_reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if !line.trim().is_empty() {
                            println!("🔍 [{}] stderr: {}", server_name_clone, line.trim());
                        }
                    }
                    Err(e) => {
                        println!("❌ [{}] Error reading stderr: {}", server_name_clone, e);
                        break;
                    }
                }
            }
        });

        // Initialize the MCP server
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                                            "name": "toolman",
                    "version": "1.0.0"
                }
            }
        });

        let init_msg = format!("{}\n", serde_json::to_string(&init_request)?);
        println!(
            "🔍 [{}] Sending init request (elapsed: {:?}): {}",
            server_name,
            start_time.elapsed(),
            init_msg.trim()
        );

        if let Err(e) = stdin.write_all(init_msg.as_bytes()).await {
            println!(
                "❌ [{}] Failed to write init request: {} (elapsed: {:?})",
                server_name,
                e,
                start_time.elapsed()
            );
            return Err(e.into());
        }

        // Read initialization response
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        println!(
            "🔍 [{}] Reading init response... (elapsed: {:?})",
            server_name,
            start_time.elapsed()
        );

        // Increase timeout for servers that may take longer to initialize
        let timeout_secs = match server_name {
            "git" | "github" => 30, // Git and GitHub servers may take longer
            "docs-service" | "task-master-ai" => 20, // Document scanning servers need more time
            _ => 15,                // Default timeout
        };
        println!(
            "🔍 [{}] Using timeout of {} seconds for initialization",
            server_name, timeout_secs
        );

        // Some servers print status messages before JSON responses
        // Keep reading lines until we get valid JSON or EOF
        let mut init_attempts = 0;
        let max_init_attempts = 10; // Increased from 5

        loop {
            line.clear();
            println!(
                "🔍 [{}] Waiting for init response line {} (elapsed: {:?})",
                server_name,
                init_attempts + 1,
                start_time.elapsed()
            );

            match tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                reader.read_line(&mut line),
            )
            .await
            {
                Ok(read_result) => match read_result {
                    Ok(bytes_read) => {
                        if bytes_read == 0 {
                            println!("❌ [{}] No more lines to read for init response (EOF) (elapsed: {:?})", server_name, start_time.elapsed());
                            return Ok(Vec::new());
                        }

                        println!(
                            "🔍 [{}] Read init line {} ({} bytes, elapsed: {:?}): {}",
                            server_name,
                            init_attempts + 1,
                            bytes_read,
                            start_time.elapsed(),
                            line.trim()
                        );

                        // Try to parse as JSON
                        if let Ok(_) = serde_json::from_str::<Value>(&line) {
                            println!(
                                "✅ [{}] Found valid JSON init response (elapsed: {:?})",
                                server_name,
                                start_time.elapsed()
                            );
                            break;
                        } else {
                            println!(
                                "🔍 [{}] Init line is not JSON, continuing... (elapsed: {:?})",
                                server_name,
                                start_time.elapsed()
                            );
                            init_attempts += 1;
                            if init_attempts >= max_init_attempts {
                                println!("⚠️ [{}] No JSON init response found after {} attempts, but continuing anyway... (elapsed: {:?})", server_name, max_init_attempts, start_time.elapsed());
                                break; // Continue without valid init response
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "❌ [{}] Failed to read init line: {} (elapsed: {:?})",
                            server_name,
                            e,
                            start_time.elapsed()
                        );
                        return Err(e.into());
                    }
                },
                Err(_) => {
                    println!(
                        "❌ [{}] Timeout reading init line after {} seconds (elapsed: {:?})",
                        server_name,
                        timeout_secs,
                        start_time.elapsed()
                    );
                    return Ok(Vec::new());
                }
            }
        }

        // Init response parsing is now handled in the loop above

        // Send initialized notification (required by MCP protocol)
        let initialized_notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        let initialized_msg = format!("{}\n", serde_json::to_string(&initialized_notification)?);
        println!(
            "🔍 [{}] Sending initialized notification (elapsed: {:?}): {}",
            server_name,
            start_time.elapsed(),
            initialized_msg.trim()
        );

        if let Err(e) = stdin.write_all(initialized_msg.as_bytes()).await {
            println!(
                "❌ [{}] Failed to write initialized notification: {} (elapsed: {:?})",
                server_name,
                e,
                start_time.elapsed()
            );
            return Err(e.into());
        }

        // Give server time to initialize (especially important for document-scanning servers)
        println!(
            "🔍 [{}] Waiting for server initialization (3 seconds)... (elapsed: {:?})",
            server_name,
            start_time.elapsed()
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        println!(
            "🔍 [{}] Server initialization wait complete (elapsed: {:?})",
            server_name,
            start_time.elapsed()
        );

        // Get tools list
        let tools_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        let tools_msg = format!("{}\n", serde_json::to_string(&tools_request)?);
        println!(
            "🔍 [{}] Sending tools/list request (elapsed: {:?}): {}",
            server_name,
            start_time.elapsed(),
            tools_msg.trim()
        );

        if let Err(e) = stdin.write_all(tools_msg.as_bytes()).await {
            println!(
                "❌ [{}] Failed to write tools request: {} (elapsed: {:?})",
                server_name,
                e,
                start_time.elapsed()
            );
            return Err(e.into());
        }

        // Read tools response (may also have status messages before JSON)
        line.clear();
        println!(
            "🔍 [{}] Reading tools response... (elapsed: {:?})",
            server_name,
            start_time.elapsed()
        );

        // Keep reading lines until we get valid JSON or EOF
        let mut tools_attempts = 0;
        let max_tools_attempts = 10; // Increased from 5

        loop {
            line.clear();
            println!(
                "🔍 [{}] Waiting for tools response line {} (elapsed: {:?})",
                server_name,
                tools_attempts + 1,
                start_time.elapsed()
            );

            match tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                reader.read_line(&mut line),
            )
            .await
            {
                Ok(read_result) => match read_result {
                    Ok(bytes_read) => {
                        if bytes_read == 0 {
                            println!("❌ [{}] No more lines to read for tools response (EOF) (elapsed: {:?})", server_name, start_time.elapsed());
                            return Ok(Vec::new());
                        }

                        println!(
                            "🔍 [{}] Read tools line {} ({} bytes, elapsed: {:?}): {}",
                            server_name,
                            tools_attempts + 1,
                            bytes_read,
                            start_time.elapsed(),
                            line.trim()
                        );

                        // Try to parse as JSON
                        if let Ok(_) = serde_json::from_str::<Value>(&line) {
                            println!(
                                "✅ [{}] Found valid JSON tools response (elapsed: {:?})",
                                server_name,
                                start_time.elapsed()
                            );
                            break;
                        } else {
                            println!(
                                "🔍 [{}] Tools line is not JSON, continuing... (elapsed: {:?})",
                                server_name,
                                start_time.elapsed()
                            );
                            tools_attempts += 1;
                            if tools_attempts >= max_tools_attempts {
                                println!("❌ [{}] Too many non-JSON tools lines after {} attempts, giving up (elapsed: {:?})", server_name, max_tools_attempts, start_time.elapsed());
                                return Ok(Vec::new());
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "❌ [{}] Failed to read tools line: {} (elapsed: {:?})",
                            server_name,
                            e,
                            start_time.elapsed()
                        );
                        return Err(e.into());
                    }
                },
                Err(_) => {
                    println!(
                        "❌ [{}] Timeout reading tools line after {} seconds (elapsed: {:?})",
                        server_name,
                        timeout_secs,
                        start_time.elapsed()
                    );
                    return Ok(Vec::new());
                }
            }
        }

        let tools = if let Ok(response) = serde_json::from_str::<Value>(&line) {
            println!(
                "🔍 [{}] Parsed tools response JSON successfully",
                server_name
            );
            if let Some(result) = response.get("result") {
                println!("🔍 [{}] Found 'result' field in response", server_name);
                if let Some(tools_array) = result.get("tools").and_then(|t| t.as_array()) {
                    println!(
                        "🔍 [{}] Found 'tools' array with {} items",
                        server_name,
                        tools_array.len()
                    );
                    let parsed_tools: Vec<Tool> = tools_array
                        .iter()
                        .filter_map(|tool| {
                            if let (Some(name), Some(description)) = (
                                tool.get("name").and_then(|n| n.as_str()),
                                tool.get("description").and_then(|d| d.as_str()),
                            ) {
                                println!(
                                    "🔍 [{}] Found tool: {} - {}",
                                    server_name, name, description
                                );
                                Some(Tool {
                                    name: name.to_string(),
                                    description: description.to_string(),
                                    input_schema: tool
                                        .get("inputSchema")
                                        .cloned()
                                        .unwrap_or(json!({})),
                                    server_name: server_name.to_string(),
                                })
                            } else {
                                println!(
                                    "❌ [{}] Skipping malformed tool: {:?}",
                                    server_name, tool
                                );
                                None
                            }
                        })
                        .collect();
                    println!(
                        "🔍 [{}] Successfully parsed {} tools",
                        server_name,
                        parsed_tools.len()
                    );
                    parsed_tools
                } else {
                    println!("❌ [{}] No 'tools' array found in result", server_name);
                    println!("🔍 [{}] Result content: {:?}", server_name, result);
                    Vec::new()
                }
            } else {
                println!("❌ [{}] No 'result' field found in response", server_name);
                println!("🔍 [{}] Response content: {:?}", server_name, response);
                Vec::new()
            }
        } else {
            println!(
                "❌ [{}] Failed to parse tools response as JSON",
                server_name
            );
            println!("🔍 [{}] Raw response: {}", server_name, line);
            Vec::new()
        };

        // Kill the discovery process - we only needed it for tool discovery
        println!("🔍 [{}] Killing discovery process", server_name);
        let _ = process.kill().await;

        println!(
            "🔍 [{}] Tool discovery complete. Found {} tools",
            server_name,
            tools.len()
        );
        Ok(tools)
    }

    async fn handle_jsonrpc_request(
        &self,
        request: JsonRpcRequest,
        headers: Option<&axum::http::HeaderMap>,
    ) -> JsonRpcResponse {
        println!(
            "🔍 DEBUG: handle_jsonrpc_request called with method: {}",
            request.method
        );
        match request.method.as_str() {
            "initialize" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": true
                        }
                    },
                    "serverInfo": {
                        "name": "toolman",
                        "version": "1.0.0"
                    }
                })),
                error: None,
            },
            "tools/list" => {
                println!("🔍 DEBUG: tools/list handler called");
                // Load context based on request headers (for multi-project support)
                if let Err(e) = self.load_user_context(headers).await {
                    return JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Failed to load user context: {}", e),
                        }),
                    };
                }

                println!("🔍 DEBUG: Context loaded successfully, building tool list");

                let mut all_tools = vec![
                    json!({
                        "name": "suggest_tools_for_tasks",
                        "description": "🤖 Analyze TaskMaster tasks and suggest appropriate MCP tools based on task descriptions and requirements",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "tasks": {
                                    "type": "array",
                                    "description": "Array of tasks from TaskMaster to analyze",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "id": { "type": "string" },
                                            "title": { "type": "string" },
                                            "description": { "type": "string" },
                                            "details": { "type": "string" },
                                            "subtasks": { "type": "array" }
                                        }
                                    }
                                }
                            },
                            "required": ["tasks"]
                        }
                    }),
                ];

                // Add tools based on static configuration
                let available_tools = self.available_tools.read().await;
                let config_manager = self.system_config_manager.read().await;
                let servers = config_manager.get_servers();

                println!(
                    "🔍 DEBUG: Starting tool filtering with {} available tools",
                    available_tools.len()
                );

                for (_, tool) in available_tools.iter() {
                    // Create the prefixed tool name as it appears to users
                    let prefixed_tool_name = format!("{}_{}", tool.server_name, tool.name);

                    // Check if this tool should be enabled based on static config
                    let should_enable = if let Some(server_config) = servers.get(&tool.server_name) {
                        // First check if the server itself is enabled
                        if !server_config.enabled {
                            println!(
                                "🔍 DEBUG: Tool {} - Server {} is disabled",
                                prefixed_tool_name, tool.server_name
                            );
                            false
                        } else if let Some(tool_config) = server_config.tools.get(&tool.name) {
                            // If there's a specific tool config, use it
                            println!(
                                "🔍 DEBUG: Tool {} - Static config enabled: {}",
                                prefixed_tool_name, tool_config.enabled
                            );
                            tool_config.enabled
                        } else {
                            // If no specific tool config, default to true (all tools enabled for enabled servers)
                            println!(
                                "🔍 DEBUG: Tool {} - No specific config, defaulting to enabled",
                                prefixed_tool_name
                            );
                            true
                        }
                    } else {
                        // Server not found in config - shouldn't happen
                        println!(
                            "🔍 DEBUG: Tool {} - Server {} not found in config",
                            prefixed_tool_name, tool.server_name
                        );
                        false
                    };

                    println!(
                        "🔍 DEBUG: Final decision for {}: {}",
                        prefixed_tool_name, should_enable
                    );

                    if should_enable {
                        all_tools.push(json!({
                            "name": prefixed_tool_name,
                            "description": tool.description,
                            "inputSchema": tool.input_schema
                        }));
                    }
                }

                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(json!({ "tools": all_tools })),
                    error: None,
                }
            }
            "tools/call" => {
                if let Some(params) = request.params {
                    if let Some(tool_name) = params.get("name").and_then(|v| v.as_str()) {
                        let result = match tool_name {
                            "suggest_tools_for_tasks" => {
                                if let Some(args) = params.get("arguments") {
                                    let tasks = args.get("tasks").ok_or_else(|| {
                                        anyhow::anyhow!("Missing required parameter: tasks")
                                    });

                                    match tasks {
                                        Ok(tasks) => {
                                            // Create tool suggester
                                            let suggester = ToolSuggester::new();

                                            // Analyze tasks
                                            match suggester.analyze_tasks(tasks) {
                                                Ok(analyses) => {
                                                    // Return analysis results
                                                    json!({
                                                        "content": [{
                                                            "type": "text",
                                                            "text": format!(
                                                                "📊 Task Analysis Complete!\n\n{}\n\n{}",
                                                                format!("Analyzed {} tasks with {} total tool suggestions",
                                                                    analyses.len(),
                                                                    analyses.iter().map(|a| a.suggested_tools.len()).sum::<usize>()
                                                                ),
                                                                format!("📋 Detailed Analysis:\n{}",
                                                                    serde_json::to_string_pretty(&analyses).unwrap_or_else(|_| "Error formatting analysis".to_string())
                                                                )
                                                            )
                                                        }]
                                                    })
                                                }
                                                Err(e) => {
                                                    json!({
                                                        "content": [{
                                                            "type": "text",
                                                            "text": format!("❌ Error analyzing tasks: {}", e)
                                                        }]
                                                    })
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            json!({
                                                "content": [{
                                                    "type": "text",
                                                    "text": format!("❌ {}", e)
                                                }]
                                            })
                                        }
                                    }
                                } else {
                                    json!({
                                        "content": [{
                                            "type": "text",
                                            "text": "❌ No arguments provided"
                                        }]
                                    })
                                }
                            }
                            _ => {
                                // Parse prefixed tool name and forward to server
                                let config_manager = self.system_config_manager.read().await;
                                let available_servers: Vec<String> =
                                    config_manager.get_servers().keys().cloned().collect();
                                drop(config_manager);

                                match parse_tool_name_with_servers(tool_name, &available_servers) {
                                    Ok(parsed_tool) => {
                                        // Get arguments for the tool call
                                        let mut arguments =
                                            params.get("arguments").cloned().unwrap_or(json!({}));

                                        // ✨ AUTO-INJECT parameters based on working directory
                                        if let Some(working_dir) =
                                            self.current_working_dir.read().await.as_ref()
                                        {
                                            if let Some(args_obj) = arguments.as_object_mut() {
                                                // 🎯 Universal projectRoot injection (for TaskMaster, etc.)
                                                args_obj.insert(
                                                    "projectRoot".to_string(),
                                                    json!(working_dir.to_string_lossy()),
                                                );
                                                println!(
                                                    "🔧 Auto-injected projectRoot: {}",
                                                    working_dir.display()
                                                );

                                                // 🎯 Server-specific parameter injection
                                                // Note: Memory server uses environment variables, not parameters
                                                // Other servers may need specific parameter injection here
                                                match parsed_tool.server_name.as_str() {
                                                    _ => {
                                                        // No additional parameter injection needed yet
                                                        // Future servers that need working directory as parameters can be added here
                                                    }
                                                }
                                            }
                                        }

                                        // Get user working directory for context-aware server startup
                                        let user_working_dir = {
                                            let wd = self.current_working_dir.read().await;
                                            wd.clone()
                                        };

                                        // Forward to the appropriate server with user context
                                        match self
                                            .connection_pool
                                            .forward_tool_call_with_context(
                                                &parsed_tool.server_name,
                                                &parsed_tool.tool_name,
                                                arguments,
                                                user_working_dir.as_deref(),
                                            )
                                            .await
                                        {
                                            Ok(response) => {
                                                // Extract result from response or return the response directly
                                                if let Some(result) = response.get("result") {
                                                    result.clone()
                                                } else {
                                                    response
                                                }
                                            }
                                            Err(e) => {
                                                json!({
                                                    "content": [{
                                                        "type": "text",
                                                        "text": format!("❌ Error calling tool '{}'\n\n🔍 **Debug Info:**\n- Original tool name: '{}'\n- Parsed as: server='{}', tool='{}'\n- Available servers: [{}]\n- Error: {}\n\n💡 Expected format: {{server_name}}_{{tool_name}}",
                                                                       tool_name,
                                                                       tool_name,
                                                                       parsed_tool.server_name,
                                                                       parsed_tool.tool_name,
                                                                       available_servers.join(", "),
                                                                       e)
                                                    }]
                                                })
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        json!({
                                            "content": [{
                                                "type": "text",
                                                "text": format!("❌ Invalid tool name format\n\n🔍 **Debug Info:**\n- Attempted tool name: '{}'\n- Parse error: {}\n- Available servers: [{}]\n- Expected format: {{server_name}}_{{tool_name}}\n\n📝 **Examples:**\n- memory_read_graph\n- git_git_status\n- task_master_ai_get_tasks",
                                                               tool_name,
                                                               e,
                                                               available_servers.join(", "))
                                            }]
                                        })
                                    }
                                }
                            }
                        };

                        JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: Some(result),
                            error: None,
                        }
                    } else {
                        JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32602,
                                message: "Missing tool name".to_string(),
                            }),
                        }
                    }
                } else {
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params".to_string(),
                        }),
                    }
                }
            }
            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "Method not found".to_string(),
                }),
            },
        }
    }



    async fn discover_available_tools_tool(&self, include_descriptions: bool) -> Value {
        let config_manager = self.system_config_manager.read().await;
        let servers = config_manager.get_servers();
        let available_tools = self.available_tools.read().await;

        let mut server_info = Vec::new();

        for (server_name, config) in servers.iter() {
            let server_tools: Vec<_> = available_tools
                .values()
                .filter(|tool| tool.server_name == *server_name)
                .map(|tool| {
                    let mut tool_info = json!({
                        "name": tool.name,
                        "enabled": true
                    });

                    if include_descriptions {
                        tool_info["description"] = json!(tool.description);
                    }

                    tool_info
                })
                .collect();

            if !server_tools.is_empty() {
                server_info.push(json!({
                    "server": server_name,
                    "description": config.description.as_deref().unwrap_or("No description"),
                    "tools_count": server_tools.len(),
                    "tools": server_tools
                }));
            }
        }

        let summary = json!({
            "summary": {
                "total_servers": server_info.len(),
                "total_tools_available": available_tools.len(),
                "total_tools_enabled": 0
            },
            "servers": server_info
        });

        json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&summary).unwrap_or_default()
            }]
        })
    }


    async fn add_server(&self, repo_url: &str) -> Value {
        // TODO: Implement intelligent server addition
        // This should:
        // 1. Download README via GitHub CLI
        // 2. Analyze for server type (npm, python, docker, rust)
        // 3. Test server setup in isolation
        // 4. Add to ephemeral config if successful
        json!({
            "content": [{
                "type": "text",
                "text": format!("🚀 add_server called with repo_url: {} - Implementation needed", repo_url)
            }]
        })
    }
}

async fn mcp_endpoint(
    State(state): State<BridgeState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<JsonRpcError>)> {
    // Log all incoming headers for debugging
    eprintln!("📨 DEBUG: Incoming HTTP request headers:");
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            eprintln!("  {}: {}", name, v);
        }
    }
    
    if let Ok(request) = serde_json::from_value::<JsonRpcRequest>(body) {
        eprintln!("📨 DEBUG: Request method: {}", request.method);
        let response = state.handle_jsonrpc_request(request, Some(&headers)).await;
        Ok(Json(response))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(JsonRpcError {
                code: -32700,
                message: "Parse error".to_string(),
            }),
        ))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle export-tools flag - discover tools and export to file, then exit
    if let Some(export_path) = args.export_tools {
        println!("🔍 Export mode: Discovering all tools from configured servers...");

        let state = BridgeState::new(args.project_dir)?;

        // Discover all tools without enabling them
        let config_manager = state.system_config_manager.read().await;
        let servers = config_manager.get_servers();
        let mut all_discovered_tools = std::collections::HashMap::new();

        for (server_name, config) in servers.iter() {
            println!("🔍 Discovering tools from server: {}", server_name);
            match state.discover_server_tools(server_name, config).await {
                Ok(tools) => {
                    println!(
                        "✅ Discovered {} tools from server '{}'",
                        tools.len(),
                        server_name
                    );
                    all_discovered_tools.insert(server_name.clone(), tools);
                }
                Err(e) => {
                    println!(
                        "❌ Failed to discover tools from server '{}': {}",
                        server_name, e
                    );
                    all_discovered_tools.insert(server_name.clone(), Vec::new());
                }
            }
        }

        // Create export structure
        let export_data = serde_json::json!({
            "export_timestamp": chrono::Utc::now().to_rfc3339(),
            "total_servers": servers.len(),
            "total_tools_discovered": all_discovered_tools.values().map(|tools| tools.len()).sum::<usize>(),
            "servers": all_discovered_tools.iter().map(|(server_name, tools)| {
                let config = servers.get(server_name).unwrap();
                serde_json::json!({
                    "name": server_name,
                    "description": config.description.as_deref().unwrap_or("No description"),
                    "command": config.command,
                    "args": config.args,
                    "enabled": config.enabled,
                    "always_active": config.always_active,
                    "tools_count": tools.len(),
                    "tools": tools.iter().map(|tool| serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema
                    })).collect::<Vec<_>>()
                })
            }).collect::<Vec<_>>()
        });

        // Write to file
        std::fs::write(&export_path, serde_json::to_string_pretty(&export_data)?)?;

        println!(
            "✅ Exported {} tools from {} servers to: {}",
            export_data["total_tools_discovered"],
            export_data["total_servers"],
            export_path.display()
        );

        return Ok(());
    }

    println!("🚀 Starting MCP Tools HTTP Server on port {}", args.port);

    // Print ALL environment variables for debugging
    println!("🔍 ALL Environment Variables Available:");
    let mut env_vars: Vec<_> = std::env::vars().collect();
    env_vars.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, value) in env_vars {
        // Mask potentially sensitive values but show everything
        let masked_value = if key.contains("API_KEY")
            || key.contains("TOKEN")
            || key.contains("PASSWORD")
            || key.contains("SECRET")
        {
            if value.len() > 8 {
                format!("{}...{}", &value[..4], &value[value.len() - 4..])
            } else {
                "***".to_string()
            }
        } else {
            value
        };
        println!("  {}: {}", key, masked_value);
    }
    println!("🔍 End Environment Variables\n");

    let state = BridgeState::new(args.project_dir)?;

    let app = Router::new()
        .route("/mcp", post(mcp_endpoint))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("✅ HTTP server listening on http://{}", addr);
    println!("🔗 MCP endpoint: http://{}/mcp", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_name_valid() {
        // Simple case
        let result = parse_tool_name("memory_delete_entities").unwrap();
        assert_eq!(
            result,
            ParsedTool {
                server_name: "memory".to_string(),
                tool_name: "delete_entities".to_string(),
            }
        );

        // Single underscore in tool name
        let result = parse_tool_name("filesystem_read_file").unwrap();
        assert_eq!(
            result,
            ParsedTool {
                server_name: "filesystem".to_string(),
                tool_name: "read_file".to_string(),
            }
        );

        // Multiple underscores in tool name - should split at first underscore only
        let result = parse_tool_name("memory_delete_all_entities").unwrap();
        assert_eq!(
            result,
            ParsedTool {
                server_name: "memory".to_string(),
                tool_name: "delete_all_entities".to_string(),
            }
        );

        // Numbers and hyphens
        let result = parse_tool_name("server123_tool-name").unwrap();
        assert_eq!(
            result,
            ParsedTool {
                server_name: "server123".to_string(),
                tool_name: "tool-name".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_tool_name_invalid() {
        // Empty string
        assert!(matches!(
            parse_tool_name(""),
            Err(ToolParseError::EmptyToolName)
        ));

        // No underscore
        assert!(matches!(
            parse_tool_name("memorydel"),
            Err(ToolParseError::InvalidFormat(_))
        ));

        // Missing server name (starts with underscore)
        assert!(matches!(
            parse_tool_name("_delete_entities"),
            Err(ToolParseError::InvalidFormat(_))
        ));

        // Missing tool name (ends with underscore)
        assert!(matches!(
            parse_tool_name("memory_"),
            Err(ToolParseError::InvalidFormat(_))
        ));

        // Only underscore
        assert!(matches!(
            parse_tool_name("_"),
            Err(ToolParseError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_parse_tool_name_edge_cases() {
        // Multiple consecutive underscores
        let result = parse_tool_name("server__tool").unwrap();
        assert_eq!(
            result,
            ParsedTool {
                server_name: "server".to_string(),
                tool_name: "_tool".to_string(),
            }
        );

        // Very long names
        let long_server = "a".repeat(100);
        let long_tool = "b".repeat(100);
        let long_name = format!("{}_{}", long_server, long_tool);
        let result = parse_tool_name(&long_name).unwrap();
        assert_eq!(
            result,
            ParsedTool {
                server_name: long_server,
                tool_name: long_tool,
            }
        );
    }
}
