use serde_json::json;
use std::fs;
use tempfile::TempDir;
use toolman::ConfigManager;

#[tokio::test]
async fn test_config_manager_save() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary directory for test
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("servers-config.json");

    // Create initial config
    let initial_config = json!({
        "servers": {
            "memory": {
                "name": "Memory Server",
                "description": "Knowledge graph operations",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-memory"],
                "env": {},
                "enabled": true,
                "always_active": false,
                "tools": {
                    "read_graph": { "enabled": false },
                    "create_entities": { "enabled": false }
                }
            }
        }
    });

    // Write initial config to file
    fs::write(&config_path, serde_json::to_string_pretty(&initial_config)?)?;
    println!("📝 Created test config at: {:?}", config_path);

    // Test: Create ConfigManager and verify it loads the config
    let mut config_manager = ConfigManager::new(Some(temp_dir.path().to_path_buf()))?;

    // Verify initial state
    let servers = config_manager.get_servers();
    assert_eq!(servers.len(), 1, "Should have 1 server");

    let memory_server = servers.get("memory").unwrap();
    let read_graph_enabled = memory_server.tools.get("read_graph").unwrap().enabled;
    assert!(
        !read_graph_enabled,
        "read_graph should initially be disabled"
    );

    println!("✅ Initial config loaded correctly");

    // Test: Update tool enabled status
    config_manager.update_tool_enabled("memory", "read_graph", true);

    // Test: Save configuration
    config_manager.save()?;
    println!("✅ Configuration saved");

    // Test: Reload from file and verify changes persisted
    let config_content = fs::read_to_string(&config_path)?;
    let saved_config: serde_json::Value = serde_json::from_str(&config_content)?;

    let read_graph_enabled_after_save = saved_config
        .get("servers")
        .and_then(|s| s.get("memory"))
        .and_then(|m| m.get("tools"))
        .and_then(|t| t.get("read_graph"))
        .and_then(|rg| rg.get("enabled"))
        .and_then(|e| e.as_bool())
        .unwrap_or(false);

    assert!(
        read_graph_enabled_after_save,
        "read_graph should be enabled after save"
    );

    println!("✅ Configuration changes persisted correctly");
    println!("🎉 ConfigManager save functionality works!");

    Ok(())
}
