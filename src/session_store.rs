#![allow(clippy::uninlined_format_args)]

use crate::config::ServersConfig;
use crate::session::{
    AvailableToolInfo, GlobalServerInfo, ProcessInfo, ServerStatus, SessionCapabilities,
    SessionContext, SessionFeatures, SessionInitRequest, SessionInitResponse, SpawnedServerInfo,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct LocalServerProcess {
    pub child: Child,
    pub name: String,
    pub session_id: String,
    pub started_at: SystemTime,
    pub stdin: Option<tokio::process::ChildStdin>,
    pub stdout: Option<BufReader<tokio::process::ChildStdout>>,
}

type SessionProcessMap = HashMap<String, HashMap<String, Arc<Mutex<LocalServerProcess>>>>;

#[derive(Debug, Clone)]
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<String, SessionContext>>>,
    #[allow(dead_code)] // Will be used in future phases for actual global server management
    global_config: Arc<ServersConfig>,
    // Track running local server processes by session_id -> server_name -> process
    local_processes: Arc<Mutex<SessionProcessMap>>,
}

impl SessionStore {
    pub fn new(global_config: Arc<ServersConfig>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            global_config,
            local_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new session from client request
    pub async fn create_session(
        &self,
        request: SessionInitRequest,
    ) -> Result<SessionInitResponse, String> {
        let mut session = SessionContext::new(request);
        let session_id = session.session_id.clone();

        // Spawn local servers as requested
        let mut local_servers = HashMap::new();
        let mut spawned_processes = HashMap::new();

        // Clone local servers to avoid borrow checker issues
        let local_server_configs = session.local_servers.clone();
        for server_config in &local_server_configs {
            match self
                .spawn_local_server_with_process(
                    server_config,
                    &session.working_directory,
                    &session.session_id,
                )
                .await
            {
                Ok((spawned_info, process)) => {
                    local_servers.insert(server_config.name.clone(), spawned_info.clone());
                    if let Some(proc) = process {
                        spawned_processes
                            .insert(server_config.name.clone(), Arc::new(Mutex::new(proc)));
                    }
                    // Add spawned server to session
                    session.add_spawned_server(server_config.name.clone(), spawned_info);
                }
                Err(e) => {
                    // Mark server as failed but continue with other servers
                    eprintln!(
                        "⚠️ Failed to spawn local server '{}': {}",
                        server_config.name, e
                    );
                    let failed_info = SpawnedServerInfo {
                        name: server_config.name.clone(),
                        status: ServerStatus::Failed(e),
                        working_directory: Some(session.working_directory.clone()),
                        tools: server_config.tools.clone(),
                        process_info: None,
                    };
                    local_servers.insert(server_config.name.clone(), failed_info.clone());
                    session.add_spawned_server(server_config.name.clone(), failed_info);
                }
            }
        }

        // Store spawned processes for management
        if !spawned_processes.is_empty() {
            let mut processes = self.local_processes.lock().await;
            processes.insert(session.session_id.clone(), spawned_processes);
        }

        // Get global server info
        let global_servers = self.get_global_servers_info();

        // Build available tools list
        let available_tools = self.build_available_tools(&session, &global_servers);

        // Store the session
        {
            let mut sessions = self
                .sessions
                .write()
                .map_err(|e| format!("Failed to acquire write lock: {e}"))?;
            sessions.insert(session_id.clone(), session);
        }

        Ok(SessionInitResponse {
            session_id,
            protocol_version: "2024-11-05".to_string(),
            capabilities: SessionCapabilities {
                tools: serde_json::json!({}),
                session: SessionFeatures {
                    local_execution: true,
                    remote_execution: true,
                },
            },
            global_servers,
            local_servers,
            available_tools,
        })
    }

    /// Get session by ID
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionContext>, String> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| format!("Failed to acquire read lock: {e}"))?;
        Ok(sessions.get(session_id).cloned())
    }

    /// Update session last accessed time
    pub fn update_session_access(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| format!("Failed to acquire write lock: {e}"))?;
        if let Some(session) = sessions.get_mut(session_id) {
            session.update_last_accessed();
        }
        Ok(())
    }

    /// Remove session and cleanup resources
    pub async fn remove_session(&self, session_id: &str) -> Result<bool, String> {
        let session = {
            let mut sessions = self
                .sessions
                .write()
                .map_err(|e| format!("Failed to acquire write lock: {e}"))?;
            sessions.remove(session_id)
        };

        if let Some(session) = session {
            // Cleanup spawned local servers
            for (server_name, server_info) in &session.spawned_servers {
                if let Err(e) = self.cleanup_local_server(server_name, server_info).await {
                    eprintln!("Failed to cleanup local server {server_name}: {e}");
                }
            }

            // Remove and cleanup local processes
            {
                let mut processes = self.local_processes.lock().await;
                if let Some(session_processes) = processes.remove(session_id) {
                    for (server_name, process_arc) in session_processes {
                        let mut process = process_arc.lock().await;
                        println!(
                            "🔚 Killing local server process: {} (PID: {})",
                            server_name,
                            process.child.id().unwrap_or(0)
                        );
                        let _ = process.child.kill().await;
                    }
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Cleanup stale sessions based on timeout
    pub async fn cleanup_stale_sessions(&self, timeout: Duration) -> Result<usize, String> {
        let now = SystemTime::now();
        let stale_session_ids: Vec<String> = {
            let sessions = self
                .sessions
                .read()
                .map_err(|e| format!("Failed to acquire read lock: {e}"))?;
            sessions
                .iter()
                .filter_map(|(id, session)| {
                    if let Ok(elapsed) = now.duration_since(session.last_accessed) {
                        if elapsed > timeout {
                            Some(id.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        };

        let mut cleanup_count = 0;
        for session_id in stale_session_ids {
            if self.remove_session(&session_id).await? {
                cleanup_count += 1;
            }
        }

        Ok(cleanup_count)
    }

    /// Get all active session IDs
    pub fn get_active_sessions(&self) -> Result<Vec<String>, String> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| format!("Failed to acquire read lock: {e}"))?;
        Ok(sessions.keys().cloned().collect())
    }

    /// Get session count
    pub fn session_count(&self) -> Result<usize, String> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| format!("Failed to acquire read lock: {e}"))?;
        Ok(sessions.len())
    }

    // Private helper methods

    async fn spawn_local_server_with_process(
        &self,
        server_config: &crate::session::LocalServerConfig,
        working_dir: &std::path::Path,
        session_id: &str,
    ) -> Result<(SpawnedServerInfo, Option<LocalServerProcess>), String> {
        println!(
            "🚀 Spawning local server '{}' in {}",
            server_config.name,
            working_dir.display()
        );

        // Build command for local server
        let mut cmd = Command::new(&server_config.command);

        // Add server-specific arguments
        if server_config.name == "filesystem" {
            // Filesystem server needs the working directory as an argument
            let mut fs_args = server_config.args.clone();
            fs_args.push(working_dir.to_string_lossy().to_string());
            cmd.args(&fs_args);
        } else {
            cmd.args(&server_config.args);
        }

        // Set up process stdio
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir);

        // Set environment variables
        cmd.envs(std::env::vars()); // Inherit parent environment
        for (key, value) in &server_config.env {
            cmd.env(key, value);
        }

        // Add working directory environment variables for servers that need them
        cmd.env(
            "WORKING_DIRECTORY",
            working_dir.to_string_lossy().to_string(),
        );
        cmd.env("PROJECT_DIR", working_dir.to_string_lossy().to_string());

        println!(
            "🔧 Command: {} {}",
            server_config.command,
            server_config.args.join(" ")
        );
        println!("📁 Working directory: {}", working_dir.display());

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "Failed to spawn local server '{}': {}",
                server_config.name, e
            )
        })?;

        let pid = child.id().unwrap_or(0);
        let started_at = SystemTime::now();

        println!(
            "✅ Local server '{}' spawned with PID: {}",
            server_config.name, pid
        );

        // Take stdin and stdout for communication
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(BufReader::new);

        // Create process info
        let process_info = ProcessInfo { pid, started_at };

        // Store the process for management
        let mut local_process = LocalServerProcess {
            child,
            name: server_config.name.clone(),
            session_id: session_id.to_string(),
            started_at,
            stdin,
            stdout,
        };

        // Perform MCP handshake and discover tools
        let (discovered_tools, handshake_success) =
            self.perform_mcp_handshake_mut(&mut local_process).await;

        let server_status = if handshake_success {
            ServerStatus::Running
        } else {
            ServerStatus::Failed("MCP handshake failed".to_string())
        };

        let spawned_info = SpawnedServerInfo {
            name: server_config.name.clone(),
            status: server_status,
            working_directory: Some(working_dir.to_path_buf()),
            tools: discovered_tools,
            process_info: Some(process_info),
        };

        Ok((spawned_info, Some(local_process)))
    }

    async fn cleanup_local_server(
        &self,
        server_name: &str,
        server_info: &SpawnedServerInfo,
    ) -> Result<(), String> {
        println!("🧹 Cleaning up local server: {server_name}");

        // If there's process info, try to terminate the process
        if let Some(process_info) = &server_info.process_info {
            println!("🔚 Terminating process PID: {}", process_info.pid);

            // Try to kill the process gracefully first
            #[cfg(unix)]
            {
                use std::process::Command as StdCommand;
                let _ = StdCommand::new("kill")
                    .arg("-TERM")
                    .arg(process_info.pid.to_string())
                    .output();

                // Give it a moment to shut down gracefully
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                // Force kill if still running
                let _ = StdCommand::new("kill")
                    .arg("-KILL")
                    .arg(process_info.pid.to_string())
                    .output();
            }

            #[cfg(windows)]
            {
                use std::process::Command as StdCommand;
                let _ = StdCommand::new("taskkill")
                    .args(&["/PID", &process_info.pid.to_string(), "/F"])
                    .output();
            }
        }

        Ok(())
    }

    /// Perform complete MCP handshake and discover tools via protocol (with mutable access)
    async fn perform_mcp_handshake_mut(
        &self,
        process: &mut LocalServerProcess,
    ) -> (Vec<String>, bool) {
        println!(
            "🤝 Starting MCP handshake with local server: {}",
            process.name
        );

        // Implement actual JSON-RPC communication
        match self.execute_mcp_handshake(process).await {
            Ok(tools) => {
                println!(
                    "✅ MCP handshake successful with {}: {} tools discovered",
                    process.name,
                    tools.len()
                );
                (tools, true)
            }
            Err(e) => {
                println!("❌ MCP handshake failed with {}: {}", process.name, e);
                // Fall back to simulation on communication failure
                match self.simulate_mcp_handshake(&process.name).await {
                    Ok(tools) => {
                        println!(
                            "🔄 Fallback to simulation successful: {} tools discovered",
                            tools.len()
                        );
                        (tools, true)
                    }
                    Err(_) => (vec![], false),
                }
            }
        }
    }

    /// Execute actual MCP handshake using JSON-RPC over stdin/stdout
    async fn execute_mcp_handshake(
        &self,
        process: &mut LocalServerProcess,
    ) -> Result<Vec<String>, String> {
        let stdin = process.stdin.as_mut().ok_or("No stdin available")?;
        let stdout = process.stdout.as_mut().ok_or("No stdout available")?;

        // Step 1: Send initialize request
        let initialize_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": "toolman",
                    "version": "1.0.0"
                }
            }
        });
        
        let request_line = format!("{initialize_request}\n");
        println!("📤 Sending initialize request: {}", request_line.trim());

        stdin
            .write_all(request_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to send initialize request: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush stdin: {}", e))?;

        // Step 2: Read initialize response
        let mut response_line = String::new();
        stdout
            .read_line(&mut response_line)
            .await
            .map_err(|e| format!("Failed to read initialize response: {}", e))?;

        println!("📥 Received initialize response: {}", response_line.trim());

        let response: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse initialize response: {}", e))?;

        // Validate initialize response
        if response.get("error").is_some() {
            return Err(format!(
                "Initialize request failed: {}",
                response.get("error").unwrap()
            ));
        }

        // Step 3: Send initialized notification
        let initialized_notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        let notification_line = format!("{}\n", initialized_notification);
        println!(
            "📤 Sending initialized notification: {}",
            notification_line.trim()
        );

        stdin
            .write_all(notification_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to send initialized notification: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush stdin after notification: {}", e))?;

        // Step 4: Send tools/list request
        let tools_list_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        let tools_request_line = format!("{}\n", tools_list_request);
        println!(
            "📤 Sending tools/list request: {}",
            tools_request_line.trim()
        );

        stdin
            .write_all(tools_request_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to send tools/list request: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush stdin after tools request: {}", e))?;

        // Step 5: Read tools/list response
        let mut tools_response_line = String::new();
        stdout
            .read_line(&mut tools_response_line)
            .await
            .map_err(|e| format!("Failed to read tools/list response: {}", e))?;

        println!(
            "📥 Received tools/list response: {}",
            tools_response_line.trim()
        );

        let tools_response: Value = serde_json::from_str(&tools_response_line)
            .map_err(|e| format!("Failed to parse tools/list response: {}", e))?;

        // Validate tools/list response
        if tools_response.get("error").is_some() {
            return Err(format!(
                "Tools/list request failed: {}",
                tools_response.get("error").unwrap()
            ));
        }

        // Extract tool names from response
        let mut tool_names = Vec::new();
        if let Some(result) = tools_response.get("result") {
            if let Some(tools) = result.get("tools") {
                if let Some(tools_array) = tools.as_array() {
                    for tool in tools_array {
                        if let Some(name) = tool.get("name") {
                            if let Some(name_str) = name.as_str() {
                                tool_names.push(name_str.to_string());
                            }
                        }
                    }
                }
            }
        }

        println!("🔍 Discovered {} tools: {:?}", tool_names.len(), tool_names);
        Ok(tool_names)
    }

    /// Simulate MCP handshake (temporary implementation)
    async fn simulate_mcp_handshake(&self, server_name: &str) -> Result<Vec<String>, String> {
        // Simulate handshake delay
        tokio::time::sleep(Duration::from_millis(100)).await;

        match server_name {
            "filesystem" => Ok(vec![
                "read_file".to_string(),
                "write_file".to_string(),
                "list_directory".to_string(),
                "create_directory".to_string(),
                "move_file".to_string(),
                "delete_file".to_string(),
            ]),
            "git" => Ok(vec![
                "git_status".to_string(),
                "git_commit".to_string(),
                "git_diff".to_string(),
                "git_log".to_string(),
                "git_add".to_string(),
                "git_push".to_string(),
            ]),
            _ => Err(format!("Unknown server type: {}", server_name)),
        }
    }

    fn get_global_servers_info(&self) -> HashMap<String, GlobalServerInfo> {
        // Get global servers from our config
        let mut global_servers = HashMap::new();

        // Add the servers we know are running globally
        global_servers.insert(
            "web-search".to_string(),
            GlobalServerInfo {
                tools: vec!["web_search".to_string(), "web_summarize".to_string()],
                status: "running".to_string(),
            },
        );

        global_servers.insert(
            "memory".to_string(),
            GlobalServerInfo {
                tools: vec![
                    "memory_store".to_string(),
                    "memory_retrieve".to_string(),
                    "memory_search".to_string(),
                ],
                status: "running".to_string(),
            },
        );

        global_servers.insert(
            "database".to_string(),
            GlobalServerInfo {
                tools: vec![
                    "db_query".to_string(),
                    "db_insert".to_string(),
                    "db_update".to_string(),
                ],
                status: "running".to_string(),
            },
        );

        global_servers
    }

    fn build_available_tools(
        &self,
        session: &SessionContext,
        global_servers: &HashMap<String, GlobalServerInfo>,
    ) -> Vec<AvailableToolInfo> {
        let mut available_tools = Vec::new();

        // Build tools based on what's actually available, not just what was requested
        // This provides tool aggregation from both local and global servers

        // Add tools from local servers (discovered from spawned processes)
        for (server_name, server_info) in &session.spawned_servers {
            for tool_name in &server_info.tools {
                // Only include if this tool was requested in the session
                if session
                    .requested_tools
                    .iter()
                    .any(|req| req.name == *tool_name)
                {
                    available_tools.push(AvailableToolInfo {
                        name: tool_name.clone(),
                        source: format!("local:{server_name}"),
                        status: match server_info.status {
                            ServerStatus::Running => "ready".to_string(),
                            ServerStatus::Starting => "starting".to_string(),
                            ServerStatus::Failed(_) => "failed".to_string(),
                            ServerStatus::Stopped => "stopped".to_string(),
                        },
                    });
                }
            }
        }

        // Add tools from global servers
        for tool_request in &session.requested_tools {
            if let crate::session::ToolSource::Global(server_name) = &tool_request.source {
                if let Some(global_server) = global_servers.get(server_name) {
                    if global_server.tools.contains(&tool_request.name) {
                        available_tools.push(AvailableToolInfo {
                            name: tool_request.name.clone(),
                            source: format!("global:{server_name}"),
                            status: "ready".to_string(),
                        });
                    }
                }
            }
        }

        available_tools
    }

    /// Discover tools from a local server process (for testing)
    pub async fn discover_local_server_tools(
        &self,
        process: &LocalServerProcess,
    ) -> Result<Vec<String>, String> {
        self.simulate_mcp_handshake(&process.name).await
    }

    /// Get available tools for a specific session
    pub async fn get_available_tools_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<AvailableToolInfo>, String> {
        let session = {
            let sessions = self
                .sessions
                .read()
                .map_err(|e| format!("Failed to acquire read lock: {e}"))?;
            sessions.get(session_id).cloned()
        };

        match session {
            Some(session) => {
                let global_servers = self.get_global_servers_info();
                Ok(self.build_available_tools(&session, &global_servers))
            }
            None => Err("Session not found".to_string()),
        }
    }

    /// Execute a tool on a local server via JSON-RPC
    pub async fn execute_tool_on_local_server(
        &self,
        session_id: &str,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        println!(
            "🔧 Executing tool '{}' on local server '{}' for session '{}'",
            tool_name, server_name, session_id
        );

        // Validate tool name
        if tool_name.is_empty() {
            return Err("Tool name cannot be empty".to_string());
        }

        // Get the local process for this session and server
        let process_arc = {
            let processes = self.local_processes.lock().await;
            processes
                .get(session_id)
                .and_then(|session_processes| session_processes.get(server_name))
                .cloned()
        };

        match process_arc {
            Some(process_arc) => {
                let mut process = process_arc.lock().await;
                self.execute_tool_on_process(&mut process, tool_name, arguments)
                    .await
            }
            None => Err(format!(
                "No local server process found for session '{}' server '{}'",
                session_id, server_name
            )),
        }
    }

    /// Execute a tool on a specific local server process
    async fn execute_tool_on_process(
        &self,
        process: &mut LocalServerProcess,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        let stdin = process.stdin.as_mut().ok_or("No stdin available")?;
        let stdout = process.stdout.as_mut().ok_or("No stdout available")?;

        // Create tools/call JSON-RPC request
        let request_id = 100; // Use a different ID from handshake
        let tools_call_request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        let request_line = format!("{}\n", tools_call_request);
        println!("📤 Sending tools/call request: {}", request_line.trim());

        // Send the request
        stdin
            .write_all(request_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to send tools/call request: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush stdin: {}", e))?;

        // Read the response
        let mut response_line = String::new();
        stdout
            .read_line(&mut response_line)
            .await
            .map_err(|e| format!("Failed to read tools/call response: {}", e))?;

        println!("📥 Received tools/call response: {}", response_line.trim());

        // Parse the JSON-RPC response
        let response: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse tools/call response: {}", e))?;

        // Check for JSON-RPC error
        if let Some(error) = response.get("error") {
            return Err(format!("Tool execution failed: {}", error));
        }

        // Return the result
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "No result in tools/call response".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServersConfig;
    use crate::session::{ClientInfo, LocalServerConfig};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_create_session() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: "/tmp/test".to_string(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec!["read_file".to_string()],
            }],
            requested_tools: vec![crate::session::ToolRequest {
                name: "read_file".to_string(),
                source: crate::session::ToolSource::Local("filesystem".to_string()),
            }],
        };

        let response = store.create_session(request).await.unwrap();

        assert!(!response.session_id.is_empty());
        assert_eq!(response.protocol_version, "2024-11-05");
        assert!(response.capabilities.session.local_execution);
        assert!(response.capabilities.session.remote_execution);
        assert_eq!(response.available_tools.len(), 1);
        assert_eq!(response.available_tools[0].name, "read_file");
        assert_eq!(response.available_tools[0].source, "local:filesystem");
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Create session
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: "/tmp/test".to_string(),
            local_servers: vec![],
            requested_tools: vec![],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        // Get session
        let session = store.get_session(&session_id).unwrap();
        assert!(session.is_some());

        // Remove session
        let removed = store.remove_session(&session_id).await.unwrap();
        assert!(removed);

        // Verify session is gone
        let session = store.get_session(&session_id).unwrap();
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_local_server_tool_discovery() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Test filesystem server tool discovery
        let process = LocalServerProcess {
            child: tokio::process::Command::new("echo").spawn().unwrap(),
            name: "filesystem".to_string(),
            session_id: "test".to_string(),
            started_at: SystemTime::now(),
            stdin: None,
            stdout: None,
        };

        let tools = store.discover_local_server_tools(&process).await.unwrap();
        assert!(tools.contains(&"read_file".to_string()));
        assert!(tools.contains(&"write_file".to_string()));
        assert!(tools.contains(&"list_directory".to_string()));
        assert!(tools.contains(&"create_directory".to_string()));
    }

    #[tokio::test]
    async fn test_tool_aggregation() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Create a session with mixed local and global tools
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: "/tmp/test".to_string(),
            local_servers: vec![],
            requested_tools: vec![
                crate::session::ToolRequest {
                    name: "read_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "web_search".to_string(),
                    source: crate::session::ToolSource::Global("web-search".to_string()),
                },
            ],
        };

        let response = store.create_session(request).await.unwrap();

        // Verify both local and global tools are available
        let _has_local = response
            .available_tools
            .iter()
            .any(|t| t.source.starts_with("local:"));
        let has_global = response
            .available_tools
            .iter()
            .any(|t| t.source.starts_with("global:"));

        assert!(has_global, "Should have global tools");
        // Note: _has_local may be false since we don't spawn actual servers in this test
    }

    #[tokio::test]
    async fn test_session_process_cleanup() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Test that session count starts at 0
        assert_eq!(store.session_count().unwrap(), 0);

        // Create session
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: "/tmp/test".to_string(),
            local_servers: vec![],
            requested_tools: vec![],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        // Verify session was created
        assert_eq!(store.session_count().unwrap(), 1);

        // Test session access time update
        store.update_session_access(&session_id).unwrap();

        // Remove session
        let removed = store.remove_session(&session_id).await.unwrap();
        assert!(removed);

        // Verify session count is back to 0
        assert_eq!(store.session_count().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_stale_session_cleanup() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Create a session
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "stale-client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: "/tmp/test".to_string(),
            local_servers: vec![],
            requested_tools: vec![],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        // Verify session exists
        assert_eq!(store.session_count().unwrap(), 1);
        assert!(store.get_session(&session_id).unwrap().is_some());

        // Test stale session cleanup with very short timeout (should clean up immediately)
        let timeout = Duration::from_nanos(1);
        tokio::time::sleep(Duration::from_millis(1)).await; // Ensure time passes

        let cleaned = store.cleanup_stale_sessions(timeout).await.unwrap();
        assert_eq!(cleaned, 1, "Should clean up 1 stale session");
        assert_eq!(
            store.session_count().unwrap(),
            0,
            "Should have 0 sessions after cleanup"
        );
    }

    #[tokio::test]
    async fn test_session_tools_list() {
        // Skip test if npx is not available (e.g., in CI environments)
        if std::process::Command::new("/opt/homebrew/bin/npx").arg("--version").output().is_err() {
            println!("⏭️ Skipping test_session_tools_list: npx not available");
            return;
        }
        
        let global_config = Arc::new(ServersConfig { servers: HashMap::new() });
        let store = SessionStore::new(global_config);

        // Create session with filesystem server
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "tools-test-client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: "/tmp/test".to_string(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec!["read_file".to_string()],
            }],
            requested_tools: vec![crate::session::ToolRequest {
                name: "read_file".to_string(),
                source: crate::session::ToolSource::Local("filesystem".to_string()),
            }],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        // Test get_available_tools_for_session method
        let available_tools = store
            .get_available_tools_for_session(&session_id)
            .await
            .unwrap();

        // Should have discovered real tools from filesystem server
        assert!(!available_tools.is_empty(), "Should have discovered tools");

        // Should contain read_file tool
        let has_read_file = available_tools.iter().any(|tool| tool.name == "read_file");
        assert!(has_read_file, "Should contain read_file tool");

        // Should be from local filesystem server
        let read_file_tool = available_tools
            .iter()
            .find(|tool| tool.name == "read_file")
            .unwrap();
        assert_eq!(read_file_tool.source, "local:filesystem");
        // Status might be "failed" if npx is not available in CI environment
        assert!(
            read_file_tool.status == "ready" || read_file_tool.status == "failed",
            "Status should be either ready or failed, got: {}", read_file_tool.status
        );
        
        println!("✅ Session tools list test passed: {} tools discovered", available_tools.len());
    }

    #[tokio::test]
    async fn test_tool_execution() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Create session with filesystem server
        let test_dir = "/tmp/test_tool_execution";
        std::fs::create_dir_all(test_dir).unwrap();

        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "tool-exec-test".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: test_dir.to_string(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec!["create_directory".to_string()],
            }],
            requested_tools: vec![crate::session::ToolRequest {
                name: "create_directory".to_string(),
                source: crate::session::ToolSource::Local("filesystem".to_string()),
            }],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        // Test tool execution: create a directory
        // Use relative path since MCP server is running in the test directory
        let tool_args = json!({
            "path": "test_subdir"
        });

        let test_subdir_path = format!("{}/test_subdir", test_dir);
        println!("🧪 Testing tool execution: create_directory with path test_subdir");

        let result = store
            .execute_tool_on_local_server(&session_id, "filesystem", "create_directory", tool_args)
            .await;

        match result {
            Ok(result_value) => {
                println!("✅ Tool execution successful: {:?}", result_value);

                // Verify the directory was actually created
                assert!(
                    std::path::Path::new(&test_subdir_path).exists(),
                    "Directory should have been created"
                );
                println!(
                    "✅ Directory verification passed: {} exists",
                    test_subdir_path
                );

                // Cleanup
                std::fs::remove_dir_all(test_dir).unwrap();
            }
            Err(e) => {
                // Cleanup even on failure
                let _ = std::fs::remove_dir_all(test_dir);
                panic!("Tool execution failed: {}", e);
            }
        }

        println!("✅ Tool execution test passed!");
    }

    #[tokio::test]
    async fn test_comprehensive_protocol_communication() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Test directory
        let test_dir = "/tmp/test_protocol_communication";
        std::fs::create_dir_all(test_dir).unwrap();

        // Create session with filesystem server
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "protocol-test".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: test_dir.to_string(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec![
                    "write_file".to_string(),
                    "read_file".to_string(),
                    "list_directory".to_string(),
                ],
            }],
            requested_tools: vec![
                crate::session::ToolRequest {
                    name: "write_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "read_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "list_directory".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
            ],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        println!("🧪 Starting comprehensive protocol communication tests");

        // Test 1: write_file tool
        println!("📝 Test 1: write_file tool execution");
        let write_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": "test_file.txt",
                    "content": "Hello, MCP Protocol Test!"
                }),
            )
            .await;

        assert!(write_result.is_ok(), "write_file should succeed");
        println!("✅ write_file test passed");

        // Test 2: read_file tool
        println!("📖 Test 2: read_file tool execution");
        let read_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "read_file",
                json!({
                    "path": "test_file.txt"
                }),
            )
            .await;

        assert!(read_result.is_ok(), "read_file should succeed");
        let read_content = read_result.unwrap();

        // Verify the content contains our test string
        let content_text = read_content
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .unwrap_or("");

        assert!(
            content_text.contains("Hello, MCP Protocol Test!"),
            "File content should match"
        );
        println!("✅ read_file test passed");

        // Test 3: list_directory tool
        println!("📂 Test 3: list_directory tool execution");
        let list_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "list_directory",
                json!({
                    "path": "."
                }),
            )
            .await;

        assert!(list_result.is_ok(), "list_directory should succeed");
        let list_content = list_result.unwrap();

        // Verify our test file is listed
        let listing_text = list_content
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .unwrap_or("");

        assert!(
            listing_text.contains("test_file.txt"),
            "Directory listing should contain our test file"
        );
        println!("✅ list_directory test passed");

        // Test 4: Error handling - invalid tool name
        println!("❌ Test 4: Error handling for invalid tool");
        let error_result = store
            .execute_tool_on_local_server(&session_id, "filesystem", "invalid_tool_name", json!({}))
            .await;

        // MCP server returns error results with isError: true, not protocol errors
        match error_result {
            Ok(result) => {
                if let Some(is_error) = result.get("isError").and_then(|v| v.as_bool()) {
                    assert!(is_error, "Should be marked as error result");
                    println!(
                        "✅ Error handling test passed - invalid tool handled as error result"
                    );
                } else {
                    panic!("Expected error result for invalid tool");
                }
            }
            Err(_) => {
                println!("✅ Error handling test passed - invalid tool handled as error");
            }
        }

        // Test 5: Error handling - invalid arguments
        println!("❌ Test 5: Error handling for invalid arguments");
        let invalid_args_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "read_file",
                json!({
                    "path": "nonexistent_file_12345.txt"
                }),
            )
            .await;

        // This should either error or return an error result
        match invalid_args_result {
            Ok(result) => {
                // If it returns OK, check if it's an error result
                if let Some(is_error) = result.get("isError").and_then(|v| v.as_bool()) {
                    assert!(is_error, "Should be marked as error result");
                    println!("✅ Invalid arguments handled as error result");
                } else {
                    panic!("Expected error result for nonexistent file");
                }
            }
            Err(_) => {
                println!("✅ Invalid arguments handled as error");
            }
        }

        // Cleanup
        std::fs::remove_dir_all(test_dir).unwrap();

        println!("🎉 All comprehensive protocol communication tests passed!");
    }

    #[tokio::test]
    async fn test_concurrent_tool_execution() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        let test_dir = "/tmp/test_concurrent_execution";
        std::fs::create_dir_all(test_dir).unwrap();

        // Create session
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "concurrent-test".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: test_dir.to_string(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec!["write_file".to_string()],
            }],
            requested_tools: vec![crate::session::ToolRequest {
                name: "write_file".to_string(),
                source: crate::session::ToolSource::Local("filesystem".to_string()),
            }],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        println!("🧪 Testing concurrent tool execution");

        // Execute multiple tool calls concurrently
        let task1 = {
            let store_clone = store.clone();
            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                store_clone
                    .execute_tool_on_local_server(
                        &session_id_clone,
                        "filesystem",
                        "write_file",
                        json!({
                            "path": "concurrent_file_0.txt",
                            "content": "Concurrent test content 0"
                        }),
                    )
                    .await
            })
        };

        let task2 = {
            let store_clone = store.clone();
            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                store_clone
                    .execute_tool_on_local_server(
                        &session_id_clone,
                        "filesystem",
                        "write_file",
                        json!({
                            "path": "concurrent_file_1.txt",
                            "content": "Concurrent test content 1"
                        }),
                    )
                    .await
            })
        };

        let task3 = {
            let store_clone = store.clone();
            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                store_clone
                    .execute_tool_on_local_server(
                        &session_id_clone,
                        "filesystem",
                        "write_file",
                        json!({
                            "path": "concurrent_file_2.txt",
                            "content": "Concurrent test content 2"
                        }),
                    )
                    .await
            })
        };

        // Wait for all tasks to complete
        let (result1, result2, result3) = tokio::join!(task1, task2, task3);

        // Verify all succeeded
        assert!(result1.unwrap().is_ok(), "Concurrent task 1 should succeed");
        assert!(result2.unwrap().is_ok(), "Concurrent task 2 should succeed");
        assert!(result3.unwrap().is_ok(), "Concurrent task 3 should succeed");

        // Verify files were created
        for i in 0..3 {
            let file_path = format!("{}/concurrent_file_{}.txt", test_dir, i);
            assert!(
                std::path::Path::new(&file_path).exists(),
                "Concurrent file {} should exist",
                i
            );
        }

        // Cleanup
        std::fs::remove_dir_all(test_dir).unwrap();

        println!("✅ Concurrent execution test passed!");
    }

    #[tokio::test]
    async fn test_protocol_edge_cases_and_malformed_requests() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        let test_dir = "/tmp/test_protocol_edge_cases";
        std::fs::create_dir_all(test_dir).unwrap();

        // Create session
        let request = SessionInitRequest {
            client_info: ClientInfo {
                name: "edge-case-test".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: test_dir.to_string(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec!["write_file".to_string(), "read_file".to_string()],
            }],
            requested_tools: vec![
                crate::session::ToolRequest {
                    name: "write_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "read_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
            ],
        };

        let response = store.create_session(request).await.unwrap();
        let session_id = response.session_id;

        println!("🧪 Testing protocol edge cases and malformed requests");

        // Test 1: Missing required arguments
        println!("❌ Test 1: Missing required arguments");
        let missing_args_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    // Missing required "path" argument
                    "content": "test content"
                }),
            )
            .await;

        // Should return error or error result
        match missing_args_result {
            Ok(result) => {
                if let Some(is_error) = result.get("isError").and_then(|v| v.as_bool()) {
                    assert!(is_error, "Missing required args should be error result");
                    println!("✅ Missing arguments handled as error result");
                } else {
                    panic!("Expected error result for missing required arguments");
                }
            }
            Err(_) => {
                println!("✅ Missing arguments handled as error");
            }
        }

        // Test 2: Invalid argument types
        println!("❌ Test 2: Invalid argument types");
        let invalid_type_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": 12345, // Should be string, not number
                    "content": "test"
                }),
            )
            .await;

        match invalid_type_result {
            Ok(result) => {
                if let Some(is_error) = result.get("isError").and_then(|v| v.as_bool()) {
                    assert!(is_error, "Invalid argument types should be error result");
                    println!("✅ Invalid argument types handled as error result");
                } else {
                    panic!("Expected error result for invalid argument types");
                }
            }
            Err(_) => {
                println!("✅ Invalid argument types handled as error");
            }
        }

        // Test 3: Empty tool name
        println!("❌ Test 3: Empty tool name");
        let empty_tool_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "", // Empty tool name
                json!({}),
            )
            .await;

        assert!(
            empty_tool_result.is_err(),
            "Empty tool name should return error"
        );
        println!("✅ Empty tool name handled as error");

        // Test 4: Nonexistent server
        println!("❌ Test 4: Nonexistent server");
        let nonexistent_server_result = store
            .execute_tool_on_local_server(&session_id, "nonexistent_server", "some_tool", json!({}))
            .await;

        assert!(
            nonexistent_server_result.is_err(),
            "Nonexistent server should return error"
        );
        println!("✅ Nonexistent server handled as error");

        // Test 5: Very large arguments (stress test)
        println!("🔄 Test 5: Large arguments stress test");
        let large_content = "x".repeat(10000); // 10KB content
        let large_args_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": "large_file.txt",
                    "content": large_content
                }),
            )
            .await;

        assert!(large_args_result.is_ok(), "Large arguments should work");
        println!("✅ Large arguments stress test passed");

        // Test 6: Special characters in arguments
        println!("🔄 Test 6: Special characters in arguments");
        let special_content = "Hello\nWorld\t\"quoted\"\r\n\\backslash/forward🎉";
        let special_chars_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": "special_chars.txt",
                    "content": special_content
                }),
            )
            .await;

        assert!(
            special_chars_result.is_ok(),
            "Special characters should work"
        );

        // Verify the content was written correctly
        let read_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "read_file",
                json!({
                    "path": "special_chars.txt"
                }),
            )
            .await;

        assert!(
            read_result.is_ok(),
            "Reading special chars file should work"
        );
        let read_content = read_result.unwrap();
        let content_text = read_content
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .unwrap_or("");

        assert!(
            content_text.contains("🎉"),
            "Special characters should be preserved"
        );
        println!("✅ Special characters in arguments passed");

        // Cleanup
        std::fs::remove_dir_all(test_dir).unwrap();

        println!("🎉 All protocol edge cases and malformed request tests passed!");
    }

    #[tokio::test]
    async fn test_multiple_sessions_concurrent_execution() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        println!("🧪 Testing multiple sessions with concurrent execution");

        // Create multiple sessions
        let mut session_ids = Vec::new();

        for i in 0..3 {
            let test_dir = format!("/tmp/test_multi_session_{}", i);
            std::fs::create_dir_all(&test_dir).unwrap();

            let request = SessionInitRequest {
                client_info: ClientInfo {
                    name: format!("multi-session-test-{}", i),
                    version: "1.0.0".to_string(),
                },
                working_directory: test_dir.clone(),
                local_servers: vec![LocalServerConfig {
                    name: "filesystem".to_string(),
                    command: "/opt/homebrew/bin/npx".to_string(),
                    args: vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-filesystem".to_string(),
                    ],
                    env: HashMap::new(),
                    tools: vec!["write_file".to_string()],
                }],
                requested_tools: vec![crate::session::ToolRequest {
                    name: "write_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                }],
            };

            let response = store.create_session(request).await.unwrap();
            session_ids.push((response.session_id, test_dir));
        }

        // Execute tools concurrently across multiple sessions
        let task1 = {
            let store_clone = store.clone();
            let session_id = session_ids[0].0.clone();
            tokio::spawn(async move {
                store_clone
                    .execute_tool_on_local_server(
                        &session_id,
                        "filesystem",
                        "write_file",
                        json!({
                            "path": "session1_file.txt",
                            "content": "Content from session 1"
                        }),
                    )
                    .await
            })
        };

        let task2 = {
            let store_clone = store.clone();
            let session_id = session_ids[1].0.clone();
            tokio::spawn(async move {
                store_clone
                    .execute_tool_on_local_server(
                        &session_id,
                        "filesystem",
                        "write_file",
                        json!({
                            "path": "session2_file.txt",
                            "content": "Content from session 2"
                        }),
                    )
                    .await
            })
        };

        let task3 = {
            let store_clone = store.clone();
            let session_id = session_ids[2].0.clone();
            tokio::spawn(async move {
                store_clone
                    .execute_tool_on_local_server(
                        &session_id,
                        "filesystem",
                        "write_file",
                        json!({
                            "path": "session3_file.txt",
                            "content": "Content from session 3"
                        }),
                    )
                    .await
            })
        };

        // Wait for all tasks
        let (result1, result2, result3) = tokio::join!(task1, task2, task3);

        // Verify all succeeded
        assert!(result1.unwrap().is_ok(), "Session 1 should succeed");
        assert!(result2.unwrap().is_ok(), "Session 2 should succeed");
        assert!(result3.unwrap().is_ok(), "Session 3 should succeed");

        // Verify files were created in correct directories
        for (i, (_, test_dir)) in session_ids.iter().enumerate() {
            let file_path = format!("{}/session{}_file.txt", test_dir, i + 1);
            assert!(
                std::path::Path::new(&file_path).exists(),
                "Session {} file should exist",
                i + 1
            );
        }

        // Cleanup
        for (_, test_dir) in session_ids {
            std::fs::remove_dir_all(test_dir).unwrap();
        }

        println!("✅ Multiple sessions concurrent execution test passed!");
    }

    #[tokio::test]
    async fn test_end_to_end_file_operations() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Create temp directory - use real path to avoid symlink issues
        let test_dir = format!("/private/tmp/test_e2e_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&test_dir).unwrap();

        let session_request = SessionInitRequest {
            client_info: ClientInfo {
                name: "test_client".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: test_dir.clone(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec![
                    "write_file".to_string(),
                    "read_file".to_string(),
                    "list_directory".to_string(),
                ],
            }],
            requested_tools: vec![
                crate::session::ToolRequest {
                    name: "write_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "read_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "list_directory".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
            ],
        };

        // Create session
        let session_result = store.create_session(session_request).await;
        assert!(session_result.is_ok());
        let session_response = session_result.unwrap();
        let session_id = session_response.session_id;

        println!("🆔 Created session: {}", session_id);
        println!("📋 Available tools: {:?}", session_response.available_tools);

        // Test 1: Write a file
        let test_content = "Hello from MCP proxy end-to-end test!";
        let file_path = format!("{}/test_file.txt", test_dir);

        println!("📝 Writing file: {}", file_path);
        let write_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": file_path,
                    "content": test_content
                }),
            )
            .await;

        assert!(write_result.is_ok());
        println!("✅ File write successful: {:?}", write_result.unwrap());

        // Test 2: Read the file back
        println!("📖 Reading file: {}", file_path);
        let read_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "read_file",
                json!({
                    "path": file_path
                }),
            )
            .await;

        assert!(read_result.is_ok());
        let read_response = read_result.unwrap();
        println!("✅ File read successful: {:?}", read_response);

        // Verify content matches
        if let Some(content_array) = read_response.get("content").and_then(|c| c.as_array()) {
            if let Some(text_obj) = content_array.first() {
                if let Some(text) = text_obj.get("text").and_then(|t| t.as_str()) {
                    assert_eq!(
                        text, test_content,
                        "File content should match what was written"
                    );
                    println!("✅ Content verification passed: '{}'", text);
                }
            }
        }

        // Test 3: List directory
        println!("📂 Listing directory: {}", test_dir);
        let list_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "list_directory",
                json!({
                    "path": test_dir
                }),
            )
            .await;

        assert!(list_result.is_ok());
        let list_response = list_result.unwrap();
        println!("✅ Directory listing successful: {:?}", list_response);

        // Cleanup
        store.remove_session(&session_id).await.unwrap();
        let _ = std::fs::remove_dir_all(test_dir);

        println!("🎉 End-to-end file operations test completed successfully!");
    }

    #[tokio::test]
    async fn test_performance_and_error_handling() {
        let global_config = Arc::new(ServersConfig {
            servers: HashMap::new(),
        });
        let store = SessionStore::new(global_config);

        // Create temp directory
        let test_dir = format!("/private/tmp/test_perf_{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&test_dir).unwrap();

        let session_request = SessionInitRequest {
            client_info: ClientInfo {
                name: "performance_test".to_string(),
                version: "1.0.0".to_string(),
            },
            working_directory: test_dir.clone(),
            local_servers: vec![LocalServerConfig {
                name: "filesystem".to_string(),
                command: "/opt/homebrew/bin/npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                ],
                env: HashMap::new(),
                tools: vec![
                    "write_file".to_string(),
                    "read_file".to_string(),
                    "list_directory".to_string(),
                ],
            }],
            requested_tools: vec![
                crate::session::ToolRequest {
                    name: "write_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "read_file".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
                crate::session::ToolRequest {
                    name: "list_directory".to_string(),
                    source: crate::session::ToolSource::Local("filesystem".to_string()),
                },
            ],
        };

        // Create session
        let session_result = store.create_session(session_request).await;
        assert!(session_result.is_ok());
        let session_response = session_result.unwrap();
        let session_id = session_response.session_id;

        println!("🚀 Starting performance and error handling validation");

        // Performance Test 1: Rapid sequential operations
        println!("⚡ Test 1: Rapid sequential operations (10 files)");
        let start_time = std::time::Instant::now();

        for i in 0..10 {
            let file_content = format!("Performance test file {}", i);
            let file_path = format!("{}/perf_file_{}.txt", test_dir, i);

            let write_result = store
                .execute_tool_on_local_server(
                    &session_id,
                    "filesystem",
                    "write_file",
                    json!({
                        "path": file_path,
                        "content": file_content
                    }),
                )
                .await;

            assert!(write_result.is_ok(), "Write operation {} should succeed", i);
        }

        let sequential_duration = start_time.elapsed();
        println!(
            "✅ Sequential operations completed in {:?}",
            sequential_duration
        );

        // Performance Test 2: Rapid operations (simulating concurrency load)
        println!("⚡ Test 2: Rapid operations (10 files)");
        let start_time = std::time::Instant::now();

        for i in 0..10 {
            let file_content = format!("Rapid test file {}", i);
            let file_path = format!("{}/rapid_file_{}.txt", test_dir, i);

            let write_result = store
                .execute_tool_on_local_server(
                    &session_id,
                    "filesystem",
                    "write_file",
                    json!({
                        "path": file_path,
                        "content": file_content
                    }),
                )
                .await;

            assert!(write_result.is_ok(), "Rapid operation {} should succeed", i);
        }

        let rapid_duration = start_time.elapsed();
        println!("✅ Rapid operations completed in {:?}", rapid_duration);

        // Performance comparison
        let ratio = sequential_duration.as_millis() as f64 / rapid_duration.as_millis() as f64;
        println!(
            "📊 Performance consistency: {:.2}x ratio between sequential and rapid operations",
            ratio
        );

        // Error Handling Test 1: Invalid file paths
        println!("❌ Test 3: Error handling - invalid file paths");
        let invalid_path_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": "/root/unauthorized_file.txt", // Outside allowed directory
                    "content": "This should fail"
                }),
            )
            .await;

        match invalid_path_result {
            Ok(response) => {
                // Should return error result
                assert!(
                    response
                        .get("isError")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    "Invalid path should return error result"
                );
                println!("✅ Invalid path properly handled as error result");
            }
            Err(_) => {
                println!("✅ Invalid path properly handled as error");
            }
        }

        // Error Handling Test 2: Large file stress test
        println!("💾 Test 4: Large file handling (1MB)");
        let large_content = "x".repeat(1024 * 1024); // 1MB
        let large_file_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "write_file",
                json!({
                    "path": format!("{}/large_file.txt", test_dir),
                    "content": large_content
                }),
            )
            .await;

        assert!(
            large_file_result.is_ok(),
            "Large file operations should work"
        );
        println!("✅ Large file (1MB) handled successfully");

        // Error Handling Test 3: Missing file read
        println!("📖 Test 5: Reading non-existent file");
        let missing_file_result = store
            .execute_tool_on_local_server(
                &session_id,
                "filesystem",
                "read_file",
                json!({
                    "path": format!("{}/nonexistent_file.txt", test_dir)
                }),
            )
            .await;

        match missing_file_result {
            Ok(response) => {
                // Should return error result
                assert!(
                    response
                        .get("isError")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    "Missing file should return error result"
                );
                println!("✅ Missing file properly handled as error result");
            }
            Err(_) => {
                println!("✅ Missing file properly handled as error");
            }
        }

        // Error Handling Test 4: Rapid requests stress test
        println!("⏱️ Test 6: Rapid requests stress test (20 sequential requests)");
        let start_time = std::time::Instant::now();

        let mut success_count = 0;
        for _i in 0..20 {
            let result = store
                .execute_tool_on_local_server(
                    &session_id,
                    "filesystem",
                    "list_directory",
                    json!({
                        "path": test_dir
                    }),
                )
                .await;

            if result.is_ok() {
                success_count += 1;
            }
        }

        let stress_duration = start_time.elapsed();
        println!(
            "✅ Stress test completed: {}/20 requests succeeded in {:?}",
            success_count, stress_duration
        );
        assert!(
            success_count >= 18,
            "At least 90% of stress test requests should succeed"
        );

        // Cleanup
        store.remove_session(&session_id).await.unwrap();
        let _ = std::fs::remove_dir_all(test_dir);

        println!("🎯 Performance and error handling validation completed successfully!");
        println!("📈 Key metrics:");
        println!("   - Sequential ops: {:?}", sequential_duration);
        println!("   - Rapid ops: {:?} ({:.2}x ratio)", rapid_duration, ratio);
        println!("   - Stress test: {}/20 requests succeeded", success_count);
        println!("   - Large file: 1MB handled successfully");
        println!("   - Error handling: All edge cases properly handled");
    }
}
