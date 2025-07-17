use std::path::PathBuf;

// Re-export the stdio wrapper module
pub mod stdio_wrapper;

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

// Session management
pub mod session;
pub mod session_store;

// Re-export key types for convenience
pub use config::{
    ClientInfo, ExecutionContext, ServerConfig, SessionConfig, SessionSettings, SystemConfigManager,
};
pub use context::{ContextConfig, ContextManager};
pub use session::{SessionContext, SessionInitRequest, SessionInitResponse, ToolSource};
pub use session_store::SessionStore;
pub use stdio_wrapper::StdioWrapper;

// Tool suggester module
pub mod tool_suggester;

/// Helper function to resolve working directory patterns
pub fn resolve_working_directory(working_dir: &str, project_dir: &std::path::Path) -> PathBuf {
    match working_dir {
        "project_root" | "project" => project_dir.to_path_buf(),
        path if path.starts_with('/') => PathBuf::from(path), // Absolute path
        path => project_dir.join(path),                       // Relative to project directory
    }
}
