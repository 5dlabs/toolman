# MCP Proxy Development Notes

## Pre-Push Checklist

**ALWAYS run Clippy before pushing code:**
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

This command uses the same syntax as our CI action and will catch all warnings that would fail the build. Fix all warnings before pushing.

## Development Context

This is an MCP (Model Context Protocol) proxy that provides session-based configuration handshake functionality to replace header-based tool filtering with a more robust approach.

## Key Features
- Session-based configuration handshake
- HTTP server with session initialization support  
- Stdio wrapper with configuration loading
- Backward compatibility with standard MCP protocol
- ExecutionContext enum for local vs remote server classification
- Dynamic server management support

## Testing
- Run integration tests: `cargo test --test integration`
- Run session config tests: `cargo test session_config_tests`
- Test with Docker: `./test-session-config.sh`

## Architecture
- `src/config.rs`: Core configuration structures
- `src/bin/http_server.rs`: HTTP server with session support
- `src/stdio_wrapper.rs`: Stdio wrapper with config loading
- `tests/integration/`: Comprehensive integration tests