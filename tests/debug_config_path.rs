use std::path::PathBuf;
use toolman::ConfigManager;

#[test]
fn debug_config_path_production_scenario() {
    // Test 1: Unit test scenario (what works)
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_config_path = temp_dir.path().join("servers-config.json");

    // Create a test config file
    let test_config = r#"{
        "servers": {
            "memory": {
                "name": "Memory Server",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-memory"],
                "tools": {
                    "read_graph": {"enabled": false},
                    "delete_entities": {"enabled": false}
                }
            }
        }
    }"#;

    std::fs::write(&temp_config_path, test_config).unwrap();

    println!("=== UNIT TEST SCENARIO ===");
    let mut config_manager = ConfigManager::new(Some(temp_dir.path().to_path_buf())).unwrap();
    println!("Config path: {:?}", config_manager.get_config_path());
    println!(
        "Config path exists: {}",
        config_manager.get_config_path().exists()
    );
    println!(
        "Config path is absolute: {}",
        config_manager.get_config_path().is_absolute()
    );

    // Test save in unit test scenario
    config_manager.update_tool_enabled("memory", "delete_entities", true);
    match config_manager.save() {
        Ok(()) => println!("✅ Unit test save succeeded"),
        Err(e) => println!("❌ Unit test save failed: {}", e),
    }

    // Test 2: Production scenario (what fails)
    println!("\n=== PRODUCTION SCENARIO ===");
    let project_dir = PathBuf::from("/Users/jonathonfritz/code/mcp-proxy");
    let production_config_path = project_dir.join("servers-config.json");

    println!("Production config path: {:?}", production_config_path);
    println!(
        "Production config exists: {}",
        production_config_path.exists()
    );
    println!(
        "Production config is readable: {}",
        production_config_path.metadata().is_ok()
    );

    if let Ok(metadata) = production_config_path.metadata() {
        println!(
            "Production config permissions: {:?}",
            metadata.permissions()
        );
        println!("Production config file size: {}", metadata.len());
    }

    // Test ConfigManager creation in production scenario
    match ConfigManager::new(Some(project_dir.clone())) {
        Ok(mut prod_config_manager) => {
            println!("✅ Production ConfigManager created successfully");
            println!(
                "Production config path: {:?}",
                prod_config_manager.get_config_path()
            );
            println!(
                "Production config path exists: {}",
                prod_config_manager.get_config_path().exists()
            );
            println!(
                "Production config path is absolute: {}",
                prod_config_manager.get_config_path().is_absolute()
            );

            // Test save in production scenario
            prod_config_manager.update_tool_enabled("memory", "delete_entities", true);
            match prod_config_manager.save() {
                Ok(()) => println!("✅ Production save succeeded"),
                Err(e) => {
                    println!("❌ Production save failed: {}", e);

                    // Try to diagnose further
                    let parent_dir = prod_config_manager.get_config_path().parent();
                    if let Some(parent) = parent_dir {
                        println!("Parent directory: {:?}", parent);
                        println!("Parent exists: {}", parent.exists());
                        if let Ok(parent_metadata) = parent.metadata() {
                            println!("Parent permissions: {:?}", parent_metadata.permissions());
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Production ConfigManager creation failed: {}", e);
        }
    }

    // Test 3: Check current working directory
    println!("\n=== CURRENT WORKING DIRECTORY ===");
    if let Ok(cwd) = std::env::current_dir() {
        println!("Current working directory: {:?}", cwd);
        let cwd_config = cwd.join("servers-config.json");
        println!("CWD config path: {:?}", cwd_config);
        println!("CWD config exists: {}", cwd_config.exists());
    }

    // Test 4: Absolute path resolution
    println!("\n=== ABSOLUTE PATH RESOLUTION ===");
    if let Ok(absolute_project_dir) = project_dir.canonicalize() {
        println!("Canonicalized project dir: {:?}", absolute_project_dir);
        let abs_config_path = absolute_project_dir.join("servers-config.json");
        println!("Absolute config path: {:?}", abs_config_path);
        println!("Absolute config exists: {}", abs_config_path.exists());
    } else {
        println!("Failed to canonicalize project directory");
    }
}
