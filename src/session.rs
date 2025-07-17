use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: String,
    pub client_info: ClientInfo,
    pub working_directory: PathBuf,
    pub local_servers: Vec<LocalServerConfig>,
    pub requested_tools: Vec<ToolRequest>,
    pub spawned_servers: HashMap<String, SpawnedServerInfo>,
    pub created_at: SystemTime,
    pub last_accessed: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ToolSource {
    #[serde(rename = "local")]
    Local(String), // server name
    #[serde(rename = "global")]
    Global(String), // server name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnedServerInfo {
    pub name: String,
    pub status: ServerStatus,
    pub working_directory: Option<PathBuf>,
    pub tools: Vec<String>,
    pub process_info: Option<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerStatus {
    Starting,
    Running,
    Failed(String),
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub started_at: SystemTime,
}

// Session initialization request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInitRequest {
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
    #[serde(rename = "workingDirectory")]
    pub working_directory: String,
    #[serde(rename = "localServers")]
    pub local_servers: Vec<LocalServerConfig>,
    #[serde(rename = "requestedTools")]
    pub requested_tools: Vec<ToolRequest>,
}

// Session initialization response to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInitResponse {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: SessionCapabilities,
    #[serde(rename = "globalServers")]
    pub global_servers: HashMap<String, GlobalServerInfo>,
    #[serde(rename = "localServers")]
    pub local_servers: HashMap<String, SpawnedServerInfo>,
    #[serde(rename = "availableTools")]
    pub available_tools: Vec<AvailableToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCapabilities {
    pub tools: serde_json::Value,
    pub session: SessionFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFeatures {
    pub local_execution: bool,
    pub remote_execution: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalServerInfo {
    pub tools: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableToolInfo {
    pub name: String,
    pub source: String, // "local:filesystem" or "global:web-search"
    pub status: String,
}

impl SessionContext {
    pub fn new(request: SessionInitRequest) -> Self {
        let session_id = Uuid::new_v4().to_string();
        let now = SystemTime::now();
        
        Self {
            session_id,
            client_info: request.client_info,
            working_directory: PathBuf::from(request.working_directory),
            local_servers: request.local_servers,
            requested_tools: request.requested_tools,
            spawned_servers: HashMap::new(),
            created_at: now,
            last_accessed: now,
        }
    }
    
    pub fn update_last_accessed(&mut self) {
        self.last_accessed = SystemTime::now();
    }
    
    pub fn add_spawned_server(&mut self, name: String, info: SpawnedServerInfo) {
        self.spawned_servers.insert(name, info);
    }
    
    pub fn get_tool_source(&self, tool_name: &str) -> Option<&ToolSource> {
        self.requested_tools
            .iter()
            .find(|t| t.name == tool_name)
            .map(|t| &t.source)
    }
}

impl ToolSource {
    pub fn parse(source_str: &str) -> Result<Self, String> {
        if let Some((context, server)) = source_str.split_once(':') {
            match context {
                "local" => Ok(ToolSource::Local(server.to_string())),
                "global" => Ok(ToolSource::Global(server.to_string())),
                _ => Err(format!("Unknown tool source context: {context}")),
            }
        } else {
            Err("Tool source must be in format 'context:server'".to_string())
        }
    }
    
    #[allow(clippy::inherent_to_string)] // TODO: Implement Display trait instead
    pub fn to_string(&self) -> String {
        match self {
            ToolSource::Local(server) => format!("local:{server}"),
            ToolSource::Global(server) => format!("global:{server}"),
        }
    }
}