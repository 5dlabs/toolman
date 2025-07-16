use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use tokio::runtime::Runtime;
use uuid::Uuid;
use crate::config::{SessionConfig, ClientInfo, SessionSettings};

/// Configuration for tool filtering in the stdio wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolFilterConfig {
    /// List of enabled tool patterns (supports wildcards like "filesystem_*")
    enabled_tools: Vec<String>,
}

impl Default for ToolFilterConfig {
    fn default() -> Self {
        Self {
            // By default, enable all tools
            enabled_tools: vec!["*".to_string()],
        }
    }
}

pub struct StdioWrapper {
    http_base_url: String,
    client: reqwest::Client,
    rt: Runtime,
    working_dir: Option<String>,
    filter_config: ToolFilterConfig,
}

impl StdioWrapper {
    pub fn new(http_base_url: String, working_dir: Option<String>) -> Result<Self> {
        let client = reqwest::Client::new();
        let rt = Runtime::new()?;

        // Load filter configuration from working directory
        let filter_config = Self::load_filter_config(&working_dir)?;

        Ok(Self {
            http_base_url,
            client,
            rt,
            working_dir,
            filter_config,
        })
    }

    /// Load filter configuration from a file in the working directory
    fn load_filter_config(working_dir: &Option<String>) -> Result<ToolFilterConfig> {
        let config_path = if let Some(dir) = working_dir {
            PathBuf::from(dir).join(".toolman-filter.json")
        } else {
            PathBuf::from(".toolman-filter.json")
        };

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config = serde_json::from_str(&content)?;
            eprintln!(
                "[Bridge] Loaded filter config from: {}",
                config_path.display()
            );
            Ok(config)
        } else {
            eprintln!(
                "[Bridge] No filter config found at {}, using defaults",
                config_path.display()
            );
            Ok(ToolFilterConfig::default())
        }
    }

    /// Load session configuration from servers-config.json
    fn load_session_config(working_dir: &Option<String>) -> Result<Option<SessionConfig>> {
        let config_path = if let Some(dir) = working_dir {
            PathBuf::from(dir).join("servers-config.json")
        } else {
            PathBuf::from("servers-config.json")
        };

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let servers_config: crate::config::ServersConfig = serde_json::from_str(&content)?;
            
            // Create session config from servers config
            let session_config = SessionConfig {
                client_info: ClientInfo {
                    name: "mcp-stdio-wrapper".to_string(),
                    version: "1.0.0".to_string(),
                    working_directory: working_dir.clone(),
                    session_id: Some(Uuid::new_v4().to_string()),
                },
                servers: servers_config.servers,
                session_settings: SessionSettings {
                    timeout_ms: 30000,
                    max_concurrent: 10,
                    auto_start: true,
                },
            };
            
            eprintln!(
                "[Bridge] Loaded session config from: {}",
                config_path.display()
            );
            Ok(Some(session_config))
        } else {
            eprintln!(
                "[Bridge] No session config found at {}, using header-based approach",
                config_path.display()
            );
            Ok(None)
        }
    }

    /// Check if a tool matches any of the enabled patterns
    fn is_tool_enabled(&self, tool_name: &str) -> bool {
        for pattern in &self.filter_config.enabled_tools {
            if pattern == "*" {
                return true;
            }

            if pattern.ends_with("*") {
                let prefix = &pattern[..pattern.len() - 1];
                if tool_name.starts_with(prefix) {
                    return true;
                }
            }

            if tool_name == pattern {
                return true;
            }
        }

        false
    }

    pub fn run(&self) -> Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        // Send initial capabilities
        self.send_capabilities(&mut stdout)?;

        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<Value>(&line) {
                Ok(request) => {
                    if let Some(response) = self.handle_request(&request)? {
                        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                        stdout.flush()?;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse JSON: {e}");
                    let error_response = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {
                            "code": -32700,
                            "message": "Parse error"
                        }
                    });
                    writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                    stdout.flush()?;
                }
            }
        }

        Ok(())
    }

    fn send_capabilities(&self, stdout: &mut io::Stdout) -> Result<()> {
        // Get current tools from HTTP server
        let tools = self
            .rt
            .block_on(async { self.get_tools_from_http().await })?;

        eprintln!("[Bridge] Notifying clients about initial tools...");
        eprintln!("[Bridge] Initial tool count: {}", tools.len());

        // Send notification with empty params like working Node.js version
        let capabilities = json!({
            "jsonrpc": "2.0",
            "method": "notifications/tools/list_changed",
            "params": {}
        });

        writeln!(stdout, "{}", serde_json::to_string(&capabilities)?)?;
        stdout.flush()?;
        eprintln!("[Bridge] Initial tools list changed notification sent successfully");
        Ok(())
    }

    async fn get_tools_from_http(&self) -> Result<Vec<Value>> {
        // Check if HTTP server is running and get tools
        eprintln!("[Bridge] Making HTTP request to: {}", self.http_base_url);

        // Get working directory to pass as context (canonicalized for consistency)
        let current_dir = if let Some(ref dir) = self.working_dir {
            // Use provided working directory, canonicalized
            std::path::Path::new(dir)
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| dir.clone())
        } else {
            // Fall back to current working directory
            std::env::current_dir()
                .map(|d| d.canonicalize().unwrap_or(d).to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        };

        eprintln!("[Bridge] Sending working directory: {current_dir}");

        let response = self
            .client
            .post(&self.http_base_url)
            .header("X-Working-Directory", &current_dir)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            }))
            .send()
            .await?;

        eprintln!("[Bridge] HTTP response status: {}", response.status());

        let response_text = response.text().await?;
        let json_response: Value = serde_json::from_str(&response_text)?;

        if let Some(result) = json_response.get("result") {
            if let Some(tools) = result.get("tools") {
                if let Ok(tools_vec) = serde_json::from_value::<Vec<Value>>(tools.clone()) {
                    // Apply client-side filtering based on configuration
                    let filtered_tools = self.filter_tools(tools_vec);

                    // Apply Cursor-specific compatibility fixes
                    let cursor_compatible_tools = self.apply_cursor_compatibility(filtered_tools);
                    return Ok(cursor_compatible_tools);
                }
            }
        }

        Ok(vec![])
    }

    /// Filter tools based on the local configuration
    fn filter_tools(&self, tools: Vec<Value>) -> Vec<Value> {
        tools
            .into_iter()
            .filter(|tool| {
                if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                    let enabled = self.is_tool_enabled(name);
                    if !enabled {
                        eprintln!("[Bridge] Filtering out tool: {name}");
                    }
                    enabled
                } else {
                    false
                }
            })
            .collect()
    }

    fn apply_cursor_compatibility(&self, tools: Vec<Value>) -> Vec<Value> {
        tools
            .into_iter()
            .map(|mut tool| {
                // Fix 1: Sanitize tool names (replace dashes with underscores)
                let original_name = tool
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string());
                if let Some(name) = &original_name {
                    if name.contains('-') {
                        let sanitized_name = name.replace('-', "_");
                        tool["name"] = json!(sanitized_name);
                        eprintln!(
                            "[Bridge] Cursor compatibility: Sanitized tool name {name} -> {sanitized_name}"
                        );
                    }
                }

                // Fix 2: Ensure description is always a string (Cursor requirement)
                if tool.get("description").is_none() {
                    tool["description"] = json!("Tool description");
                }

                // Fix 3: Ensure inputSchema is always present (Cursor requirement)
                if tool.get("inputSchema").is_none() {
                    tool["inputSchema"] = json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    });
                }

                tool
            })
            .collect()
    }

    fn handle_request(&self, request: &Value) -> Result<Option<Value>> {
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let id = request.get("id").cloned();

        match method {
            "initialize" => {
                // Try to load and send session configuration
                match Self::load_session_config(&self.working_dir) {
                    Ok(Some(session_config)) => {
                        eprintln!("[Bridge] Sending session-based initialization");
                        
                        // Send session-based initialization to HTTP server
                        let init_with_session = json!({
                            "jsonrpc": "2.0",
                            "id": 1,
                            "method": "initialize",
                            "params": {
                                "sessionConfig": session_config
                            }
                        });
                        
                        // Forward to HTTP server
                        let _response = self.rt.block_on(async {
                            self.forward_initialization_with_session(&init_with_session).await
                        })?;
                        
                        // Return standard MCP response to client
                        Ok(Some(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "protocolVersion": "2024-11-05",
                                "capabilities": {
                                    "tools": {
                                        "listChanged": true
                                    }
                                },
                                "serverInfo": {
                                    "name": "mcp-bridge-stdio-wrapper",
                                    "version": "1.0.0"
                                }
                            }
                        })))
                    }
                    Ok(None) => {
                        // No session config, use standard initialization
                        eprintln!("[Bridge] Using standard initialization (no session config)");
                        Ok(Some(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "protocolVersion": "2024-11-05",
                                "capabilities": {
                                    "tools": {
                                        "listChanged": true
                                    }
                                },
                                "serverInfo": {
                                    "name": "mcp-bridge-stdio-wrapper",
                                    "version": "1.0.0"
                                }
                            }
                        })))
                    }
                    Err(e) => {
                        eprintln!("[Bridge] Error loading session config: {}", e);
                        // Fall back to standard initialization
                        Ok(Some(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "protocolVersion": "2024-11-05",
                                "capabilities": {
                                    "tools": {
                                        "listChanged": true
                                    }
                                },
                                "serverInfo": {
                                    "name": "mcp-bridge-stdio-wrapper",
                                    "version": "1.0.0"
                                }
                            }
                        })))
                    }
                }
            },
            "notifications/initialized" => {
                // This is a notification, no response needed
                Ok(None)
            }
            "tools/list" => {
                let tools = self
                    .rt
                    .block_on(async { self.get_tools_from_http().await.unwrap_or_default() });

                Ok(Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": tools
                    }
                })))
            }
            "tools/call" => {
                // Forward all tool calls to the HTTP server
                let result = self
                    .rt
                    .block_on(async { self.forward_tool_call(request).await });

                match result {
                    Ok(response) => Ok(Some(response)),
                    Err(e) => Ok(Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32603,
                            "message": format!("Internal error: {e}")
                        }
                    }))),
                }
            }
            _ => {
                // Unknown method
                Ok(Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": "Method not found"
                    }
                })))
            }
        }
    }

    async fn forward_tool_call(&self, request: &Value) -> Result<Value> {
        // Get working directory to pass as context
        let current_dir = if let Some(ref dir) = self.working_dir {
            std::path::Path::new(dir)
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| dir.clone())
        } else {
            std::env::current_dir()
                .map(|d| d.canonicalize().unwrap_or(d).to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        };

        let response = self
            .client
            .post(&self.http_base_url)
            .header("X-Working-Directory", &current_dir)
            .json(request)
            .send()
            .await?;

        let response_text = response.text().await?;
        let json_response: Value = serde_json::from_str(&response_text)?;

        Ok(json_response)
    }

    async fn forward_initialization_with_session(&self, request: &Value) -> Result<Value> {
        eprintln!("[Bridge] Forwarding session-based initialization to HTTP server");

        let response = self
            .client
            .post(&self.http_base_url)
            .json(request)
            .send()
            .await?;

        let response_text = response.text().await?;
        let json_response: Value = serde_json::from_str(&response_text)?;

        eprintln!("[Bridge] Received session initialization response: {}", json_response);
        Ok(json_response)
    }
}
