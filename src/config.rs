use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

fn default_transport() -> String {
    "stdio".to_string()
}

/// Tool configuration within a server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub enabled: bool,
}

/// Configuration for a single MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    /// Transport type: "stdio" (default) or "http"
    #[serde(default = "default_transport")]
    pub transport: String,
    /// For stdio: command to execute
    #[serde(default)]
    pub command: String,
    /// For stdio: command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// For http: URL to connect to
    pub url: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(rename = "alwaysActive", default)]
    pub always_active: bool,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(rename = "autoStart", default)]
    pub auto_start: bool,
    /// Working directory for the server process (optional, defaults to project directory)
    /// Supports: "project_root", absolute paths like "/usr/local/bin", or relative paths
    #[serde(rename = "workingDirectory", default)]
    pub working_directory: Option<String>,
    /// Individual tool configurations with enabled flags
    #[serde(default)]
    pub tools: HashMap<String, ToolConfig>,
}

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServersConfig {
    pub servers: HashMap<String, ServerConfig>,
}

/// Configuration manager for loading and managing server configs
#[derive(Debug, Clone)]
pub struct SystemConfigManager {
    config_path: PathBuf,
    config: ServersConfig,
}

impl SystemConfigManager {
    pub fn new(project_dir: Option<PathBuf>) -> Result<Self> {
        let config_path = if let Some(dir) = project_dir {
            dir.join("servers-config.json")
        } else {
            PathBuf::from("servers-config.json")
        };

        let config = if config_path.exists() {
            let config_content = std::fs::read_to_string(&config_path)?;
            serde_json::from_str(&config_content)?
        } else {
            ServersConfig {
                servers: HashMap::new(),
            }
        };

        Ok(Self {
            config_path,
            config,
        })
    }

    pub fn get_servers(&self) -> &HashMap<String, ServerConfig> {
        &self.config.servers
    }

    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.config.servers.get(name)
    }

    /// Save the configuration to file
    pub fn save(&self) -> Result<()> {
        let config_json = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(&self.config_path, config_json)?;
        Ok(())
    }

    /// Update tool enabled status
    pub fn update_tool_enabled(&mut self, server_name: &str, tool_name: &str, enabled: bool) {
        if let Some(server) = self.config.servers.get_mut(server_name) {
            server
                .tools
                .insert(tool_name.to_string(), ToolConfig { enabled });
        }
    }

    /// Set all tools to disabled for a server
    pub fn disable_all_tools_for_server(&mut self, server_name: &str) {
        if let Some(server) = self.config.servers.get_mut(server_name) {
            for tool_config in server.tools.values_mut() {
                tool_config.enabled = false;
            }
        }
    }

    /// Get a mutable reference to the config for bulk updates
    pub fn get_config_mut(&mut self) -> &mut ServersConfig {
        &mut self.config
    }

    /// Get the path to the configuration file (for debugging)
    pub fn get_config_path(&self) -> &std::path::Path {
        &self.config_path
    }

    /// Enhanced atomic save with backup and recovery
    pub fn save_atomic(&self) -> Result<()> {
        let backup_path = self.create_backup()?;

        match self.save_atomic_impl() {
            Ok(()) => {
                self.cleanup_old_backups()?;
                Ok(())
            }
            Err(e) => {
                // Attempt to restore from backup on failure
                if let Err(restore_err) = self.restore_from_backup(&backup_path) {
                    eprintln!("Failed to restore from backup after save failure: {restore_err}");
                }
                Err(e)
            }
        }
    }

    /// Core atomic write implementation using temp file + rename
    fn save_atomic_impl(&self) -> Result<()> {
        use std::process;

        // 1. Generate unique temporary filename
        let timestamp = chrono::Utc::now().timestamp();
        let pid = process::id();
        let temp_filename = format!(
            "{}.tmp.{}.{}",
            self.config_path.file_name().unwrap().to_string_lossy(),
            pid,
            timestamp
        );
        let temp_path = self.config_path.parent().unwrap().join(temp_filename);

        // 2. Write configuration to temporary file
        let config_json = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(&temp_path, &config_json)?;

        // 3. Validate temporary file
        self.validate_config_file(&temp_path)?;

        // 4. Sync to disk for durability
        self.sync_file(&temp_path)?;

        // 5. Atomic rename
        std::fs::rename(&temp_path, &self.config_path)?;

        Ok(())
    }

    /// Create timestamped backup of current config
    fn create_backup(&self) -> Result<PathBuf> {
        if !self.config_path.exists() {
            // No existing config to backup
            return Ok(PathBuf::new());
        }

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let backup_filename = format!(
            "{}.backup.{}",
            self.config_path.file_name().unwrap().to_string_lossy(),
            timestamp
        );
        let backup_path = self.config_path.parent().unwrap().join(backup_filename);

        std::fs::copy(&self.config_path, &backup_path)?;
        Ok(backup_path)
    }

    /// Restore configuration from backup file
    fn restore_from_backup(&self, backup_path: &PathBuf) -> Result<()> {
        if backup_path.exists() && !backup_path.as_os_str().is_empty() {
            std::fs::copy(backup_path, &self.config_path)?;
        }
        Ok(())
    }

    /// Validate configuration file for JSON syntax and basic structure
    fn validate_config_file(&self, file_path: &PathBuf) -> Result<()> {
        let content = std::fs::read_to_string(file_path)?;

        // Parse JSON to validate syntax
        let parsed_config: ServersConfig = serde_json::from_str(&content)?;

        // Basic structure validation
        if parsed_config.servers.is_empty() {
            return Err(anyhow::anyhow!(
                "Configuration validation failed: no servers defined"
            ));
        }

        // Validate each server configuration
        for (server_name, server_config) in &parsed_config.servers {
            if server_config.command.is_empty() {
                return Err(anyhow::anyhow!(
                    "Configuration validation failed: server '{}' has empty command",
                    server_name
                ));
            }
        }

        Ok(())
    }

    /// Sync file to disk for durability
    fn sync_file(&self, file_path: &PathBuf) -> Result<()> {
        use std::fs::File;

        let file = File::open(file_path)?;

        // Use fsync on Unix-like systems for durability
        #[cfg(unix)]
        {
            unsafe {
                if libc::fsync(file.as_raw_fd()) != 0 {
                    return Err(anyhow::anyhow!("Failed to sync file to disk"));
                }
            }
        }

        // On Windows, file is automatically synced on close
        #[cfg(windows)]
        {
            // File will be synced when dropped
        }

        Ok(())
    }

    /// Cleanup old backup files, keeping only the 5 most recent
    fn cleanup_old_backups(&self) -> Result<()> {
        let parent_dir = match self.config_path.parent() {
            Some(dir) => dir,
            None => return Ok(()), // Can't clean up if no parent directory
        };

        let config_filename = self.config_path.file_name().unwrap().to_string_lossy();
        let backup_pattern = format!("{config_filename}.backup.");

        // Find all backup files
        let mut backup_files = Vec::new();
        for entry in std::fs::read_dir(parent_dir)? {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();

            if filename.starts_with(&backup_pattern) {
                if let Ok(metadata) = entry.metadata() {
                    backup_files.push((
                        entry.path(),
                        metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    ));
                }
            }
        }

        // Sort by modification time (newest first)
        backup_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove files beyond the 5 most recent
        for (path, _) in backup_files.iter().skip(5) {
            if let Err(e) = std::fs::remove_file(path) {
                eprintln!("Warning: Failed to remove old backup file {path:?}: {e}");
            }
        }

        Ok(())
    }

    /// Cleanup orphaned temporary files on startup
    pub fn cleanup_temp_files(&self) -> Result<()> {
        let parent_dir = match self.config_path.parent() {
            Some(dir) => dir,
            None => return Ok(()),
        };

        let config_filename = self.config_path.file_name().unwrap().to_string_lossy();
        let temp_pattern = format!("{config_filename}.tmp.");

        for entry in std::fs::read_dir(parent_dir)? {
            let entry = entry?;
            let filename = entry.file_name().to_string_lossy().to_string();

            if filename.starts_with(&temp_pattern) {
                // Remove temp files older than 1 hour
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let age = std::time::SystemTime::now()
                            .duration_since(modified)
                            .unwrap_or_default();
                        if age > std::time::Duration::from_secs(3600) {
                            // 1 hour
                            if let Err(e) = std::fs::remove_file(entry.path()) {
                                eprintln!(
                                    "Warning: Failed to remove temp file {:?}: {}",
                                    entry.path(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Helper function to resolve working directory patterns
pub fn resolve_working_directory(working_dir: &str, project_dir: &std::path::Path) -> PathBuf {
    match working_dir {
        "project_root" | "project" => project_dir.to_path_buf(),
        path if path.starts_with('/') => PathBuf::from(path), // Absolute path
        path => project_dir.join(path),                       // Relative to project directory
    }
}
