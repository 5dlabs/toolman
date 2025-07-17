#![allow(clippy::uninlined_format_args)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::too_many_arguments)]

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use toolman::config::{ServerConfig, SystemConfigManager as ConfigManager};
use toolman::resolve_working_directory;
use tower_http::cors::CorsLayer;

/// Toolman HTTP MCP Server
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
#[cfg(test)]
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

    /// Check if Docker is available and ready
    async fn is_docker_ready(&self) -> bool {
        use tokio::process::Command;

        // Try to run docker version command
        match Command::new("docker").arg("version").output().await {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Docker is available and ready");
                    true
                } else {
                    println!("❌ Docker command failed with status: {}", output.status);
                    false
                }
            }
            Err(e) => {
                println!("❌ Docker command error: {}", e);
                false
            }
        }
    }

    /// Wait for Docker to be ready with timeout and retry logic
    async fn wait_for_docker(&self, timeout_secs: u64) -> anyhow::Result<()> {
        use tokio::time::{sleep, Duration, Instant};

        let start_time = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let retry_interval = Duration::from_secs(2);

        println!(
            "🐳 Waiting for Docker to be ready (timeout: {}s)...",
            timeout_secs
        );

        loop {
            if self.is_docker_ready().await {
                println!("✅ Docker is ready (elapsed: {:?})", start_time.elapsed());
                return Ok(());
            }

            if start_time.elapsed() >= timeout {
                return Err(anyhow::anyhow!(
                    "Docker not ready after {} seconds. Docker-based MCP servers may not work.",
                    timeout_secs
                ));
            }

            println!(
                "⏳ Docker not ready yet, retrying in {}s...",
                retry_interval.as_secs()
            );
            sleep(retry_interval).await;
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

        // Check if this is a Docker-based server and ensure Docker is ready
        if config.command == "docker" {
            println!(
                "🐳 [{}] Detected Docker-based server, checking Docker readiness...",
                server_name
            );
            if let Err(e) = self.wait_for_docker(30).await {
                println!("⚠️ [{}] Docker readiness check failed: {}", server_name, e);
                // Continue anyway, but warn that it might not work
            }
        }

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
            let fs_args = vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                user_dir.to_string_lossy().to_string(),
            ];

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
        // Check if this is an HTTP transport server
        let config_manager = self.config_manager.read().await;
        let server_config = config_manager
            .get_servers()
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        // Handle HTTP transport
        if server_config.transport == "http" {
            if let Some(url) = &server_config.url {
                println!("🌐 Forwarding HTTP request to: {}", url);

                // Create HTTP client if not exists
                let client = reqwest::Client::new();

                // Check if this is an SSE endpoint by trying to GET the URL
                // Only try SSE detection if the URL ends with /sse
                let message_url = if url.ends_with("/sse") {
                    let sse_response = client
                        .get(url)
                        .header("Accept", "text/event-stream")
                        .send()
                        .await;

                    if let Ok(response) = sse_response {
                        let content_type = response
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");

                        if content_type.contains("text/event-stream") {
                            // This is an SSE endpoint, parse the session info
                            // For SSE, we only need to read the initial chunk with session ID
                            let mut body = response.bytes_stream();
                            use futures::StreamExt;

                            let first_chunk = match body.next().await {
                                Some(Ok(chunk)) => String::from_utf8_lossy(&chunk).to_string(),
                                Some(Err(e)) => {
                                    return Err(anyhow::anyhow!("Failed to read SSE chunk: {}", e))
                                }
                                None => {
                                    return Err(anyhow::anyhow!(
                                        "No data received from SSE endpoint"
                                    ))
                                }
                            };

                            // Parse SSE format: "event: endpoint\ndata: /message?sessionId=xxx"
                            let session_id = if let Some(data_line) =
                                first_chunk.lines().find(|line| line.starts_with("data: "))
                            {
                                let endpoint_path = data_line.strip_prefix("data: ").unwrap_or("");
                                if let Some(session_param) =
                                    endpoint_path.split("sessionId=").nth(1)
                                {
                                    session_param.to_string()
                                } else {
                                    return Err(anyhow::anyhow!(
                                        "No sessionId found in SSE response"
                                    ));
                                }
                            } else {
                                return Err(anyhow::anyhow!("No data line found in SSE response"));
                            };

                            // Construct the message URL
                            let base_url = url.trim_end_matches("/sse").trim_end_matches('/');
                            format!("{}/message?sessionId={}", base_url, session_id)
                        } else {
                            // Not SSE, use original direct HTTP approach
                            url.to_string()
                        }
                    } else {
                        // Failed to GET, try original direct HTTP approach
                        url.to_string()
                    }
                } else {
                    // URL doesn't end with /sse, use direct HTTP approach
                    url.to_string()
                };

                // Create JSON-RPC request
                let request_body = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {
                        "name": tool_name,
                        "arguments": arguments
                    }
                });

                // Send HTTP POST request
                let response = client
                    .post(&message_url)
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

                // Parse response
                let response_json: Value = response
                    .json()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to parse HTTP response: {}", e))?;

                println!("📨 Received HTTP response from server {}", server_name);
                return Ok(response_json);
            } else {
                return Err(anyhow::anyhow!("HTTP transport requires 'url' field"));
            }
        }

        // Original stdio logic
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
}

#[derive(Clone)]
pub struct BridgeState {
    // System-level configuration manager (for server startup and discovery)
    system_config_manager: Arc<RwLock<ConfigManager>>,
    // Available tools from all configured servers (discovered once from system config)
    available_tools: Arc<RwLock<HashMap<String, Tool>>>,
    // Connection pool for active MCP servers
    connection_pool: Arc<ServerConnectionPool>,
    // Current working directory for user context (per-request)
    current_working_dir: Arc<RwLock<Option<std::path::PathBuf>>>,
}

// JSON-RPC 2.0 message types
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
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

        // Create the state
        let state = Self {
            system_config_manager,
            available_tools: Arc::new(RwLock::new(HashMap::new())),
            connection_pool,
            current_working_dir: Arc::new(RwLock::new(None)),
        };

        // Discover all available tools at startup
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Err(e) = state_clone.discover_all_tools().await {
                eprintln!("❌ Failed to discover tools at startup: {}", e);
            }
        });

        Ok(state)
    }

    /// Discover all available tools from all configured servers
    async fn discover_all_tools(&self) -> anyhow::Result<()> {
        println!("🔍 Discovering all available tools from configured servers...");

        let config_manager = self.system_config_manager.read().await;
        let servers = config_manager.get_servers();
        let mut all_tools = HashMap::new();

        for (server_name, config) in servers.iter() {
            if config.enabled {
                println!("🔍 Discovering tools from server: {}", server_name);
                match self.discover_server_tools(server_name, config).await {
                    Ok(tools) => {
                        println!(
                            "✅ Discovered {} tools from server '{}'",
                            tools.len(),
                            server_name
                        );
                        for tool in tools {
                            let prefixed_name = format!("{}_{}", tool.server_name, tool.name);
                            all_tools.insert(prefixed_name, tool);
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "⚠️ Failed to discover tools from server '{}': {}",
                            server_name, e
                        );
                    }
                }
            }
        }

        // Store discovered tools
        let mut available_tools = self.available_tools.write().await;
        *available_tools = all_tools;
        println!(
            "✅ Tool discovery complete. Total tools available: {}",
            available_tools.len()
        );

        Ok(())
    }

    // Discover tools from a single server (without "starting" it permanently)
    async fn discover_server_tools(
        &self,
        server_name: &str,
        config: &ServerConfig,
    ) -> anyhow::Result<Vec<Tool>> {
        let start_time = std::time::Instant::now();
        println!(
            "🔍 [{}] Starting tool discovery at {:?}",
            server_name,
            chrono::Utc::now().format("%H:%M:%S")
        );

        // Handle HTTP transport
        if config.transport == "http" {
            if let Some(url) = &config.url {
                println!(
                    "🌐 [{}] Discovering tools from HTTP server: {}",
                    server_name, url
                );

                let client = reqwest::Client::new();

                // Check if this is an SSE endpoint by trying to GET the URL
                // Only try SSE detection if the URL ends with /sse
                println!(
                    "🔍 [{}] URL: {}, ends_with(/sse): {}",
                    server_name,
                    url,
                    url.ends_with("/sse")
                );
                let (message_url, _session_id) = if url.ends_with("/sse") {
                    println!(
                        "🔄 [{}] Detected SSE endpoint, starting SSE handshake",
                        server_name
                    );
                    let sse_response = client
                        .get(url)
                        .header("Accept", "text/event-stream")
                        .send()
                        .await;

                    if let Ok(response) = sse_response {
                        let content_type = response
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");

                        if content_type.contains("text/event-stream") {
                            // This is an SSE endpoint, parse the session info
                            // For SSE, we only need to read the initial chunk with session ID
                            let mut body = response.bytes_stream();
                            use futures::StreamExt;

                            let first_chunk = match body.next().await {
                                Some(Ok(chunk)) => String::from_utf8_lossy(&chunk).to_string(),
                                Some(Err(e)) => {
                                    return Err(anyhow::anyhow!("Failed to read SSE chunk: {}", e))
                                }
                                None => {
                                    return Err(anyhow::anyhow!(
                                        "No data received from SSE endpoint"
                                    ))
                                }
                            };

                            // Parse SSE format: "event: endpoint\ndata: /message?sessionId=xxx"
                            let session_id = if let Some(data_line) =
                                first_chunk.lines().find(|line| line.starts_with("data: "))
                            {
                                let endpoint_path = data_line.strip_prefix("data: ").unwrap_or("");
                                if let Some(session_param) =
                                    endpoint_path.split("sessionId=").nth(1)
                                {
                                    session_param.to_string()
                                } else {
                                    return Err(anyhow::anyhow!(
                                        "No sessionId found in SSE response"
                                    ));
                                }
                            } else {
                                return Err(anyhow::anyhow!("No data line found in SSE response"));
                            };

                            // Construct the message URL
                            let base_url = url.trim_end_matches("/sse").trim_end_matches('/');
                            let message_url =
                                format!("{}/message?sessionId={}", base_url, session_id);
                            println!("🔗 [{}] SSE session ID: {}", server_name, session_id);
                            println!("🎯 [{}] SSE message URL: {}", server_name, message_url);
                            (message_url, session_id)
                        } else {
                            // Not SSE, use original direct HTTP approach
                            (url.to_string(), String::new())
                        }
                    } else {
                        // Failed to GET, try original direct HTTP approach
                        (url.to_string(), String::new())
                    }
                } else {
                    // URL doesn't end with /sse, use direct HTTP approach
                    println!("🔗 [{}] Using direct HTTP approach", server_name);
                    (url.to_string(), String::new())
                };

                println!("🎯 [{}] Final message_url: {}", server_name, message_url);

                // For SSE endpoints, skip initialize due to short session timeouts
                // Go directly to tools/list
                let tools_request = if url.ends_with("/sse") {
                    println!("🔄 [{}] SSE endpoint detected - skipping initialize due to session timeout issues", server_name);
                    json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/list",
                        "params": {}
                    })
                } else {
                    println!(
                        "🔄 [{}] HTTP endpoint - sending initialize first",
                        server_name
                    );
                    // Initialize the server for non-SSE endpoints
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

                    println!(
                        "📤 [{}] Sending initialize request to: {}",
                        server_name, message_url
                    );
                    let init_response = client
                        .post(&message_url)
                        .header("Accept", "application/json, text/event-stream")
                        .json(&init_request)
                        .send()
                        .await
                        .map_err(|e| anyhow::anyhow!("HTTP init request failed: {}", e))?;

                    println!(
                        "📥 [{}] Initialize response status: {}",
                        server_name,
                        init_response.status()
                    );

                    // Get tools list
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "tools/list",
                        "params": {}
                    })
                };

                println!(
                    "📤 [{}] Sending tools/list request to: {}",
                    server_name, message_url
                );
                let tools_response = client
                    .post(&message_url)
                    .header("Accept", "application/json, text/event-stream")
                    .json(&tools_request)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("HTTP tools request failed: {}", e))?;

                println!(
                    "📥 [{}] Tools response status: {}",
                    server_name,
                    tools_response.status()
                );
                let response_text = tools_response
                    .text()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to get response text: {}", e))?;
                println!("🔍 [{}] Raw tools response: {}", server_name, response_text);

                // Handle SSE format responses (both direct HTTP endpoints like Solana and SSE endpoints like rustdocs)
                let json_content = if response_text.contains("data: ")
                    && (response_text.starts_with("event:") || response_text.starts_with("data:"))
                {
                    println!(
                        "🔄 [{}] Detected SSE format response, extracting JSON data",
                        server_name
                    );
                    // Extract JSON from SSE format - handle multiple possible formats:
                    // 1. "event: message\ndata: {json}\n\n"
                    // 2. "data: {json}\n\n"
                    // 3. Multiple data lines that need to be concatenated
                    let data_lines: Vec<&str> = response_text
                        .lines()
                        .filter(|line| line.starts_with("data: "))
                        .collect();

                    if !data_lines.is_empty() {
                        // Concatenate all data lines and remove "data: " prefix
                        let combined_data = data_lines
                            .iter()
                            .map(|line| line.strip_prefix("data: ").unwrap_or(line))
                            .collect::<Vec<_>>()
                            .join("");
                        println!("🔍 [{}] Extracted SSE data: {}", server_name, combined_data);
                        combined_data
                    } else {
                        println!(
                            "⚠️ [{}] SSE format detected but no data lines found",
                            server_name
                        );
                        response_text
                    }
                } else {
                    response_text
                };

                let response_json: Value = serde_json::from_str(&json_content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse tools response: {}", e))?;

                println!(
                    "🔍 [{}] Tools response JSON: {}",
                    server_name, response_json
                );

                // Parse tools from response
                if let Some(result) = response_json.get("result") {
                    if let Some(tools_array) = result.get("tools").and_then(|t| t.as_array()) {
                        let parsed_tools: Vec<Tool> = tools_array
                            .iter()
                            .filter_map(|tool| {
                                if let (Some(name), Some(description)) = (
                                    tool.get("name").and_then(|n| n.as_str()),
                                    tool.get("description").and_then(|d| d.as_str()),
                                ) {
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
                                    None
                                }
                            })
                            .collect();

                        println!(
                            "✅ [{}] Discovered {} tools via HTTP (elapsed: {:?})",
                            server_name,
                            parsed_tools.len(),
                            start_time.elapsed()
                        );

                        return Ok(parsed_tools);
                    }
                }

                return Ok(Vec::new());
            } else {
                return Err(anyhow::anyhow!("HTTP transport requires 'url' field"));
            }
        }

        // Original stdio discovery logic
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

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

        // Check if this is a Docker-based server and ensure Docker is ready
        if config.command == "docker" {
            println!(
                "🐳 [{}] Detected Docker-based server, checking Docker readiness...",
                server_name
            );
            if let Err(e) = self.connection_pool.wait_for_docker(30).await {
                println!("⚠️ [{}] Docker readiness check failed: {}", server_name, e);
                // Continue anyway, but warn that it might not work
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
        _headers: Option<&axum::http::HeaderMap>,
    ) -> JsonRpcResponse {
        println!(
            "🔍 DEBUG: handle_jsonrpc_request called with method: {}",
            request.method
        );
        match request.method.as_str() {
            "initialize" => {
                // Standard MCP initialization - simplified, no session complexity
                JsonRpcResponse {
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
                }
            }
            "tools/list" => {
                println!(
                    "🔍 DEBUG: tools/list handler called - returning ALL tools without filtering"
                );

                // Get ALL available tools without any filtering
                let available_tools = self.available_tools.read().await;
                let mut all_tools = Vec::new();

                println!(
                    "🔍 Returning {} tools from all servers",
                    available_tools.len()
                );

                // Add ALL tools - no filtering
                for (prefixed_tool_name, tool) in available_tools.iter() {
                    println!("✅ Including tool: {}", prefixed_tool_name);
                    all_tools.push(json!({
                        "name": prefixed_tool_name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema
                    }));
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
                        let result = {
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
}

// Liveness probe - just checks if HTTP server is alive
async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "toolman",
        "timestamp": Utc::now().to_rfc3339()
    }))
}

// Readiness probe - checks if MCP servers are available and ready
async fn readiness_check(State(state): State<BridgeState>) -> Result<Json<Value>, StatusCode> {
    let config_manager = state.system_config_manager.read().await;
    let servers = config_manager.get_servers();

    // Check if we have any servers configured
    if servers.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // For now, just check if servers are configured
    // TODO: In the future, we could ping each server to check actual availability
    Ok(Json(json!({
        "status": "ready",
        "service": "toolman",
        "servers_configured": servers.len(),
        "timestamp": Utc::now().to_rfc3339()
    })))
}

// Session initialization endpoint
async fn mcp_endpoint(
    State(state): State<BridgeState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<JsonRpcResponse>, (StatusCode, Json<JsonRpcError>)> {
    if let Ok(request) = serde_json::from_value::<JsonRpcRequest>(body) {
        eprintln!("📨 Processing request: {}", request.method);
        // Simple tool aggregation - no session complexity
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

    // Give the tool discovery a moment to start
    // In production, you might want to wait for discovery to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let app = Router::new()
        .route("/mcp", post(mcp_endpoint))
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
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
