#[cfg(test)]
mod tests {

    #[test]
    fn test_sse_url_detection() {
        // URLs that should trigger SSE detection
        assert!(is_sse_url("http://example.com/sse"));
        assert!(is_sse_url("https://rustdocs-server.com/sse"));
        assert!(is_sse_url("http://localhost:3000/sse"));
        assert!(is_sse_url("https://api.example.com/v1/sse"));
        
        // URLs that should NOT trigger SSE detection
        assert!(!is_sse_url("http://example.com/api"));
        assert!(!is_sse_url("https://mcp.solana.com/mcp"));
        assert!(!is_sse_url("http://localhost:3000/mcp"));
        assert!(!is_sse_url("https://example.com/sse/sub"));
        assert!(!is_sse_url("https://example.com/sse?param=value"));
        assert!(!is_sse_url("https://example.com/api/sse/endpoint"));
    }

    #[test]
    fn test_known_server_urls() {
        // Test known server URLs
        
        // Solana - direct HTTP
        assert!(!is_sse_url("https://mcp.solana.com/mcp"));
        
        // Rust Docs - SSE
        assert!(is_sse_url("http://rustdocs-mcp-rust-docs-mcp-server.mcp.svc.cluster.local:3000/sse"));
        
        // Our own server - direct HTTP
        assert!(!is_sse_url("http://localhost:3000/mcp"));
        assert!(!is_sse_url("http://toolman.mcp.svc.cluster.local:3000/mcp"));
    }

    fn is_sse_url(url: &str) -> bool {
        url.ends_with("/sse")
    }
}