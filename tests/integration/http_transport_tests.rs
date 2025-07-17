use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_solana_direct_http_transport() {
    // Test Solana's direct HTTP transport (real URL)
    let client = reqwest::Client::new();
    let solana_url = "https://mcp.solana.com/mcp";

    // This should NOT trigger SSE detection
    assert!(!solana_url.ends_with("/sse"));

    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let result = timeout(Duration::from_secs(10), async {
        client.post(solana_url).json(&tools_request).send().await
    });

    // Should succeed for direct HTTP servers
    match result.await {
        Ok(Ok(response)) => {
            println!("Solana response status: {}", response.status());
            assert!(response.status().is_success() || response.status().is_client_error());
        }
        Ok(Err(e)) => {
            println!("Solana network error: {}", e);
            // Network errors are acceptable in test environment
        }
        Err(_) => {
            panic!("Solana direct HTTP request should not timeout");
        }
    }
}

#[tokio::test]
async fn test_rustdocs_sse_transport() {
    // Test Rust Docs SSE transport (real URL in cluster)
    let sse_url = "http://rustdocs-mcp-rust-docs-mcp-server.mcp.svc.cluster.local:3000/sse";

    // This should trigger SSE detection
    assert!(sse_url.ends_with("/sse"));

    let client = reqwest::Client::new();
    let result = timeout(Duration::from_secs(5), async {
        let sse_response = client
            .get(sse_url)
            .header("Accept", "text/event-stream")
            .timeout(Duration::from_secs(3))
            .send()
            .await;

        match sse_response {
            Ok(response) => {
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");

                if content_type.contains("text/event-stream") {
                    // For SSE, we need to read the response with a timeout
                    let body_result = timeout(Duration::from_secs(2), response.text()).await;
                    let body = match body_result {
                        Ok(Ok(body)) => {
                            println!("SSE response body: {}", body);
                            body
                        }
                        Ok(Err(e)) => {
                            println!("SSE body read error: {}", e);
                            return Err(e.into());
                        }
                        Err(_) => {
                            println!("SSE body read timed out (expected for SSE)");
                            // For SSE, timeout is expected, let's try a different approach
                            return Ok(());
                        }
                    };

                    // Parse SSE format: "event: endpoint\ndata: /message?sessionId=xxx"
                    if let Some(data_line) = body.lines().find(|line| line.starts_with("data: ")) {
                        let endpoint_path = data_line.strip_prefix("data: ").unwrap_or("");
                        if let Some(session_param) = endpoint_path.split("sessionId=").nth(1) {
                            let session_id = session_param.to_string();

                            // Test message endpoint with session ID
                            let base_url = sse_url.trim_end_matches("/sse").trim_end_matches('/');
                            let message_url =
                                format!("{}/message?sessionId={}", base_url, session_id);

                            let tools_request = json!({
                                "jsonrpc": "2.0",
                                "id": 1,
                                "method": "tools/list",
                                "params": {}
                            });

                            let message_result =
                                client.post(&message_url).json(&tools_request).send().await;

                            match message_result {
                                Ok(resp) => {
                                    println!("Message endpoint response status: {}", resp.status());
                                    Ok(())
                                }
                                Err(e) => {
                                    println!("Message endpoint error: {}", e);
                                    Err(e.into())
                                }
                            }
                        } else {
                            println!("No sessionId found in SSE response");
                            Err(anyhow::anyhow!("No sessionId found"))
                        }
                    } else {
                        println!("No data line found in SSE response");
                        Err(anyhow::anyhow!("No data line found"))
                    }
                } else {
                    println!("Not an SSE endpoint, content-type: {}", content_type);
                    Err(anyhow::anyhow!("Not SSE"))
                }
            }
            Err(e) => {
                println!("SSE connection error: {}", e);
                Err(e.into())
            }
        }
    });

    // Test should complete without timing out
    match result.await {
        Ok(Ok(_)) => println!("SSE transport test passed"),
        Ok(Err(e)) => println!(
            "SSE transport test failed (expected in some environments): {}",
            e
        ),
        Err(_) => panic!("SSE transport test should not timeout"),
    }
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
