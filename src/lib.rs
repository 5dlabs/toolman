use std::path::PathBuf;

// Re-export the MCP client module
pub mod client;

// Comprehensive error handling system
pub mod errors;

// Server health monitoring and recovery
pub mod health_monitor;

// Comprehensive server recovery system
pub mod recovery;

// Context-based user configuration management
pub mod context;

// Configuration management
pub mod config;

// Re-export key types for convenience
pub use client::McpClient;
pub use config::{ClientInfo, ExecutionContext, ServerConfig, SystemConfigManager};
pub use context::{ContextConfig, ContextManager};

/// Helper function to resolve working directory patterns
pub fn resolve_working_directory(working_dir: &str, project_dir: &std::path::Path) -> PathBuf {
    match working_dir {
        "project_root" | "project" => project_dir.to_path_buf(),
        path if path.starts_with('/') => PathBuf::from(path), // Absolute path
        path => project_dir.join(path),                       // Relative to project directory
    }
}
