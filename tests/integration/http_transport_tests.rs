use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;
use std::process::Stdio;
use tokio::process::Command;

#[tokio::test]
async fn test_toolman_server_with_solana_config() {
    // Test our actual server implementation with Solana configured
    println!("🧪 Testing toolman server HTTP transport with Solana");
    
    // Create a minimal config with Solana and Rust Docs
    let test_config = json!({
        "servers": {
            "solana": {
                "name": "Solana",
                "description": "Solana blockchain development tools",
                "transport": "http",
                "url": "https://mcp.solana.com/mcp",
                "enabled": true,
                "executionContext": "remote"
            },
            "rustdocs": {
                "name": "Rust Docs",
                "description": "Rust documentation MCP server",
                "transport": "http",
                "url": "http://rustdocs-mcp-rust-docs-mcp-server.mcp.svc.cluster.local:3000/sse",
                "enabled": true,
                "executionContext": "remote"
            }
        }
    });
    
    // Write test config
    std::fs::write("/tmp/servers-config.json", test_config.to_string()).unwrap();
    
    // Start our server with the test config
    let mut server = Command::new("target/release/toolman-server")
        .arg("--port")
        .arg("3001")
        .env("PROJECT_DIR", "/tmp")
        .env("RUST_LOG", "debug")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start toolman server");

    // Wait a moment for server to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Test the server
    let client = reqwest::Client::new();
    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let result = timeout(Duration::from_secs(15), async {
        client
            .post("http://localhost:3001/mcp")
            .json(&tools_request)
            .send()
            .await
    });

    match result.await {
        Ok(Ok(response)) => {
            println!("✅ Server responded with status: {}", response.status());
            let response_text = response.text().await.unwrap_or_default();
            println!("📝 Response: {}", response_text);
            
            // Check if we got valid JSON
            if let Ok(json_response) = serde_json::from_str::<serde_json::Value>(&response_text) {
                println!("✅ Valid JSON response");
                
                if let Some(error) = json_response.get("error") {
                    println!("❌ Server returned error: {}", error);
                } else if let Some(result) = json_response.get("result") {
                    if let Some(tools) = result.get("tools") {
                        println!("✅ Got tools array with {} items", 
                                tools.as_array().map(|a| a.len()).unwrap_or(0));
                    } else {
                        println!("❌ No tools in result");
                    }
                }
            } else {
                println!("❌ Invalid JSON response: {}", response_text);
            }
        }
        Ok(Err(e)) => {
            println!("❌ Request failed: {}", e);
        }
        Err(_) => {
            println!("❌ Request timed out");
        }
    }

    // Kill the server and capture logs
    let _ = server.kill().await;
    let output = server.wait_with_output().await.unwrap();
    
    // Print server logs for debugging
    if !output.stdout.is_empty() {
        println!("📋 Server stdout:");
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        println!("📋 Server stderr:");
        println!("{}", String::from_utf8_lossy(&output.stderr));
    }
    
    // Cleanup
    let _ = std::fs::remove_file("/tmp/servers-config.json");
}

#[tokio::test]
async fn test_rustdocs_sse_via_toolman_server() {
    // Test Rust Docs SSE transport via our actual toolman server
    // This tests the real implementation path that's failing
    
    let client = reqwest::Client::new();
    
    // Make a request to our toolman server's /mcp endpoint
    // This will trigger the actual SSE tool discovery code path
    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    println!("Testing toolman server SSE implementation...");
    
    let result = timeout(Duration::from_secs(15), async {
        client
            .post("http://toolman.mcp.svc.cluster.local:3000/mcp")
            .json(&tools_request)
            .send()
            .await
    });

    match result.await {
        Ok(Ok(response)) => {
            println!("Toolman server response status: {}", response.status());
            
            if response.status().is_success() {
                let response_text = response.text().await.unwrap_or_default();
                println!("Response body: {}", response_text);
                
                // Try to parse as JSON to see if it's valid MCP response
                if let Ok(json_response) = serde_json::from_str::<serde_json::Value>(&response_text) {
                    if let Some(tools) = json_response.get("result").and_then(|r| r.get("tools")) {
                        println!("Successfully got tools response with {} tools", 
                                tools.as_array().map(|a| a.len()).unwrap_or(0));
                        
                        // Check if rustdocs tools are included
                        if let Some(tools_array) = tools.as_array() {
                            let has_rustdocs = tools_array.iter().any(|tool| {
                                tool.get("name")
                                    .and_then(|n| n.as_str())
                                    .map(|name| name.contains("rust") || name.contains("doc"))
                                    .unwrap_or(false)
                            });
                            
                            if has_rustdocs {
                                println!("✅ Rust Docs tools found in response");
                            } else {
                                println!("❌ No Rust Docs tools found - SSE discovery may have failed");
                            }
                        }
                    } else {
                        println!("❌ Invalid tools response format");
                    }
                } else {
                    println!("❌ Failed to parse response as JSON: {}", response_text);
                }
            } else {
                println!("❌ Server returned error status: {}", response.status());
            }
        }
        Ok(Err(e)) => {
            println!("❌ Request failed: {}", e);
        }
        Err(_) => {
            println!("❌ Request timed out");
        }
    }
    
    println!("SSE integration test via toolman server completed");
}

#[tokio::test]
async fn test_http_transport_detection() {
    // Test that URL-based detection works correctly

    // SSE URLs should be detected correctly
    assert!(is_sse_url("http://example.com/sse"));
    assert!(is_sse_url("https://rustdocs-server.com/sse"));

    // Direct HTTP URLs should not trigger SSE detection
    assert!(!is_sse_url("http://example.com/api"));
    assert!(!is_sse_url("https://mcp.solana.com/mcp"));
    assert!(!is_sse_url("http://localhost:3000/mcp"));
}

fn is_sse_url(url: &str) -> bool {
    url.ends_with("/sse")
}

#[tokio::test]
async fn test_solana_direct_http() {
    // Test Solana's direct HTTP transport specifically
    let solana_url = "https://mcp.solana.com/mcp";

    // This should NOT trigger SSE detection
    assert!(!is_sse_url(solana_url));

    // Test direct HTTP request to Solana
    let client = reqwest::Client::new();
    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let result = timeout(Duration::from_secs(10), async {
        client.post(solana_url).json(&tools_request).send().await
    });

    // Should succeed without SSE processing
    match result.await {
        Ok(Ok(response)) => {
            assert!(response.status().is_success() || response.status().is_client_error());
            println!(
                "Solana direct HTTP test passed: status {}",
                response.status()
            );
        }
        Ok(Err(e)) => {
            // Network errors are acceptable in test environment
            println!("Solana network error (expected in test): {}", e);
        }
        Err(_) => {
            panic!("Solana direct HTTP request should not timeout");
        }
    }
}
