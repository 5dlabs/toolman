use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::{ServerConfig, SystemConfigManager};

/// Individual server connection
pub struct ServerConnection {
    pub process: Child,
    pub stdin: tokio::process::ChildStdin,
    pub stdout: tokio::sync::Mutex<tokio::io::Lines<BufReader<tokio::process::ChildStdout>>>,
}

/// Connection manager for health checks
pub struct ConnectionManager {
    startup_timeout: std::time::Duration,
    shutdown_timeout: std::time::Duration,
}

impl ConnectionManager {
    pub fn new(startup_timeout: std::time::Duration, shutdown_timeout: std::time::Duration) -> Self {
        Self {
            startup_timeout,
            shutdown_timeout,
        }
    }
}

/// Pool of server connections
pub struct ServerConnectionPool {
    connections: Arc<RwLock<HashMap<String, Arc<Mutex<ServerConnection>>>>>,
    system_config_manager: Arc<RwLock<SystemConfigManager>>,
}

impl ServerConnectionPool {
    pub async fn new(
        system_config_manager: Arc<RwLock<SystemConfigManager>>,
        _connection_manager: ConnectionManager,
    ) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            system_config_manager,
        }
    }

    /// Start a server if not already running
    pub async fn start_server(&self, server_name: &str) -> Result<()> {
        let connections = self.connections.read().await;
        if connections.contains_key(server_name) {
            return Ok(()); // Already running
        }
        drop(connections);

        let config_manager = self.system_config_manager.read().await;
        let server_config = config_manager
            .get_server(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in configuration", server_name))?
            .clone();
        drop(config_manager);

        // Start the server process
        let mut cmd = Command::new(&server_config.command);
        cmd.args(&server_config.args);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Set environment variables
        for (key, value) in &server_config.env {
            cmd.env(key, value);
        }

        let mut process = cmd.spawn()?;

        let stdin = process.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = process.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;

        let reader = BufReader::new(stdout);
        let lines = reader.lines();

        let connection = ServerConnection {
            process,
            stdin,
            stdout: tokio::sync::Mutex::new(lines),
        };

        let mut connections = self.connections.write().await;
        connections.insert(server_name.to_string(), Arc::new(Mutex::new(connection)));

        Ok(())
    }

    /// Stop a server
    pub async fn stop_server(&self, server_name: &str) -> Result<()> {
        let connection = {
            let mut connections = self.connections.write().await;
            connections.remove(server_name)
        };

        if let Some(connection) = connection {
            let mut conn = connection.lock().await;
            let _ = conn.process.kill().await;
        }

        Ok(())
    }

    /// Forward a JSON-RPC request to a server
    pub async fn forward_request(&self, server_name: &str, request: Value) -> Result<Value> {
        self.start_server(server_name).await?;

        let connection = {
            let connections = self.connections.read().await;
            connections.get(server_name).cloned()
                .ok_or_else(|| anyhow::anyhow!("Server '{}' not connected", server_name))?
        };

        let mut conn = connection.lock().await;
        
        // Send request
        let request_str = serde_json::to_string(&request)?;
        conn.stdin.write_all(request_str.as_bytes()).await?;
        conn.stdin.write_all(b"\n").await?;

        // Read response
        let mut stdout = conn.stdout.lock().await;
        loop {
            if let Some(line) = stdout.next_line().await? {
                if let Ok(response) = serde_json::from_str::<Value>(&line) {
                    if response.get("jsonrpc").is_some() {
                        return Ok(response);
                    }
                }
            } else {
                return Err(anyhow::anyhow!("Server closed connection"));
            }
        }
    }

    /// Forward a tool call to a server
    pub async fn forward_tool_call(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        self.forward_request(server_name, request).await
    }
}