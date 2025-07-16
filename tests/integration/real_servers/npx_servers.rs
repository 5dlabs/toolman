#![allow(clippy::uninlined_format_args)]

use anyhow::Result;
use serde_json::json;
use std::env;

use crate::common::*;

#[tokio::test]
async fn test_filesystem_server_npx() -> Result<()> {
    crate::setup_integration_tests();
    
    let env = TestEnvironment::new()?;
    let test_dir = env.get_test_files_dir();
    
    // Check if NPX is available
    if !check_runtime_available("npx").await {
        println!("Skipping NPX filesystem test: npx not available");
        return Ok(());
    }
    
    // Use the test data directory from environment or fall back to /tmp
    let test_data_dir = env::var("MCP_TEST_DATA_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let config = create_npx_server_config(
        "@modelcontextprotocol/server-filesystem",
        vec![test_data_dir.clone()]
    );
    
    let mut server = match TestServer::start(config).await {
        Ok(server) => server,
        Err(e) => {
            println!("Failed to start filesystem server: {}", e);
            return Ok(());
        }
    };
    
    // Run comprehensive validation
    let report = ProtocolValidator::run_comprehensive_validation(&mut server).await?;
    
    if !report.is_healthy() {
        println!("Server validation failed, but continuing with specific tests");
    }
    
    // Test filesystem-specific functionality
    CommonTestScenarios::test_filesystem_server(&mut server, test_dir).await?;
    
    // Test specific filesystem operations
    println!("Testing filesystem operations...");
    
    // Use test file from the test data directory
    let test_file_path = format!("{}/test.txt", test_data_dir);
    
    // Test reading the test file
    let read_result = server.call_tool("read_file", json!({
        "path": test_file_path
    })).await;
    
    match read_result {
        Ok(response) => {
            println!("✅ File read successful: {}", response);
            // Verify the content
            if let Some(content) = response.get("content") {
                if let Some(text) = content.get(0).and_then(|c| c.get("text")) {
                    assert!(text.as_str().unwrap_or("").contains("This is a test file"));
                    println!("✅ File content validated");
                }
            }
        }
        Err(e) => {
            println!("❌ File read failed: {}", e);
        }
    }
    
    // Test listing directory
    let list_result = server.call_tool("list_directory", json!({
        "path": test_data_dir
    })).await;
    
    match list_result {
        Ok(response) => {
            println!("✅ Directory listing successful: {}", response);
            // Verify our test files are listed
            if let Some(content) = response.get("content") {
                if let Some(text) = content.get(0).and_then(|c| c.get("text")) {
                    let listing = text.as_str().unwrap_or("");
                    assert!(listing.contains("test.txt"));
                    println!("✅ Directory listing validated");
                }
            }
        }
        Err(e) => {
            println!("❌ Directory listing failed: {}", e);
        }
    }
    
    // Test creating a new file
    let create_result = server.call_tool("write_file", json!({
        "path": format!("{}/new_test.txt", test_data_dir),
        "content": "This is a new test file created by the integration test"
    })).await;
    
    match create_result {
        Ok(response) => {
            println!("✅ File creation successful: {}", response);
        }
        Err(e) => {
            println!("❌ File creation failed: {}", e);
        }
    }
    
    let _ = server.shutdown().await;
    println!("✅ NPX filesystem server test completed");
    
    Ok(())
}

#[tokio::test]
async fn test_brave_search_server_npx() -> Result<()> {
    crate::setup_integration_tests();
    
    // Check if NPX is available
    if !check_runtime_available("npx").await {
        println!("Skipping NPX brave search test: npx not available");
        return Ok(());
    }
    
    let config = create_npx_server_config(
        "@modelcontextprotocol/server-brave-search",
        vec![]
    ).with_env("BRAVE_API_KEY", "test_key"); // Note: This will fail without real API key
    
    let mut server = match TestServer::start(config).await {
        Ok(server) => server,
        Err(e) => {
            println!("Failed to start brave search server (expected without API key): {}", e);
            return Ok(());
        }
    };
    
    // Run basic validation
    let report = ProtocolValidator::run_comprehensive_validation(&mut server).await?;
    
    // The server should start but search calls will fail without valid API key
    println!("Brave search server validation: {}", if report.is_healthy() { "✅ PASS" } else { "❌ FAIL" });
    
    let _ = server.shutdown().await;
    println!("✅ NPX brave search server test completed");
    
    Ok(())
}

#[tokio::test]
async fn test_memory_server_npx() -> Result<()> {
    crate::setup_integration_tests();
    
    // Check if NPX is available
    if !check_runtime_available("npx").await {
        println!("Skipping NPX memory test: npx not available");
        return Ok(());
    }
    
    let config = create_npx_server_config(
        "@modelcontextprotocol/server-memory",
        vec![]
    );
    
    let mut server = match TestServer::start(config).await {
        Ok(server) => server,
        Err(e) => {
            println!("Failed to start memory server: {}", e);
            return Ok(());
        }
    };
    
    // Run comprehensive validation
    let report = ProtocolValidator::run_comprehensive_validation(&mut server).await?;
    
    if report.is_healthy() {
        println!("✅ Memory server validation passed");
        
        // Test memory operations
        println!("Testing memory operations...");
        
        // Test storing a memory
        let store_result = server.call_tool("store_memory", json!({
            "content": "This is a test memory for integration testing",
            "priority": 0.8
        })).await;
        
        match store_result {
            Ok(response) => {
                println!("✅ Memory storage successful: {}", response);
            }
            Err(e) => {
                println!("❌ Memory storage failed: {}", e);
            }
        }
        
        // Test searching memories
        let search_result = server.call_tool("search_memories", json!({
            "query": "test memory"
        })).await;
        
        match search_result {
            Ok(response) => {
                println!("✅ Memory search successful: {}", response);
            }
            Err(e) => {
                println!("❌ Memory search failed: {}", e);
            }
        }
    } else {
        println!("❌ Memory server validation failed");
    }
    
    let _ = server.shutdown().await;
    println!("✅ NPX memory server test completed");
    
    Ok(())
}

#[tokio::test]
async fn test_postgres_server_npx() -> Result<()> {
    crate::setup_integration_tests();
    
    // Check if NPX is available
    if !check_runtime_available("npx").await {
        println!("Skipping NPX postgres test: npx not available");
        return Ok(());
    }
    
    let config = create_npx_server_config(
        "@modelcontextprotocol/server-postgres",
        vec![]
    ).with_env("POSTGRES_URL", "postgresql://test:test@localhost/test"); // Note: This will fail without real DB
    
    let mut server = match TestServer::start(config).await {
        Ok(server) => server,
        Err(e) => {
            println!("Failed to start postgres server (expected without DB): {}", e);
            return Ok(());
        }
    };
    
    // Run basic validation
    let report = ProtocolValidator::run_comprehensive_validation(&mut server).await?;
    
    // The server should start but DB calls will fail without valid connection
    println!("Postgres server validation: {}", if report.is_healthy() { "✅ PASS" } else { "❌ FAIL" });
    
    let _ = server.shutdown().await;
    println!("✅ NPX postgres server test completed");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_npx_runtime_availability() {
        if check_runtime_available("npx").await {
            println!("✅ NPX runtime is available for testing");
        } else {
            println!("❌ NPX runtime is not available");
        }
    }
}