# Docker Integration Testing

This guide explains how to run integration tests in a Docker environment that closely matches production.

## Why Docker Testing?

Our integration tests need to run in the same environment as production to catch real issues:
- **Filesystem permissions** behave differently in Docker vs local macOS
- **Network configuration** matches production setup
- **Runtime environments** (NPX, UVX, Docker) are configured identically to production
- **Dependency conflicts** are caught early

## Quick Start

```bash
# Run all integration tests in Docker
./test-docker.sh

# Run specific test suites
./test-docker.sh filesystem  # Test NPX filesystem server
./test-docker.sh fetch      # Test UVX fetch server
./test-docker.sh docker     # Test Docker-based servers
./test-docker.sh http       # Test HTTP/SSE servers
```

## Manual Docker Commands

```bash
# Build test image
docker build -f Dockerfile.test -t mcp-proxy-test .

# Run all tests
docker-compose -f docker-compose.test.yml run --rm integration-tests

# Run specific tests
docker-compose -f docker-compose.test.yml run --rm test-filesystem
docker-compose -f docker-compose.test.yml run --rm test-fetch
```

## Test Environment

The Docker test environment includes:
- **Test data directory**: `/test_data` with pre-created test files
- **Test output directory**: `/test_output` for test results
- **All MCP runtimes**: NPX, UVX, Docker, Python, Node.js, Go, Rust
- **Docker-in-Docker**: For testing Docker-based MCP servers
- **Environment variables**: Configured for testing

## Test Files

The Docker environment includes these test files in `/test_data`:
- `test.txt` - Sample text file for filesystem tests
- `test.json` - Sample JSON file for filesystem tests

## Environment Variables

- `MCP_TEST_DATA_DIR=/test_data` - Location of test files
- `MCP_TEST_OUTPUT_DIR=/test_output` - Location for test outputs
- `DOCKER_HOST=unix:///var/run/docker.sock` - Docker-in-Docker support
- `RUST_LOG=info` - Logging level
- `RUST_BACKTRACE=1` - Enable backtraces

## Troubleshooting

### Docker Socket Permission Issues
If you get permission errors accessing Docker socket:
```bash
# On Linux
sudo usermod -a -G docker $USER
# Then restart your shell

# On macOS
# Docker Desktop should handle this automatically
```

### Test Data Directory Issues
If tests fail with "directory not found":
```bash
# Check that test data was created
docker run --rm mcp-proxy-test ls -la /test_data

# Check environment variables
docker run --rm mcp-proxy-test env | grep MCP_TEST
```

### Runtime Availability Issues
If tests are skipped due to missing runtimes:
```bash
# Check available runtimes
docker run --rm mcp-proxy-test which npx uvx docker python node

# Check runtime versions
docker run --rm mcp-proxy-test sh -c "npx --version && uvx --version && docker --version"
```

## Expected Results

When running in Docker, the tests should:
- ✅ **NPX Filesystem Server**: Work correctly with `/test_data` directory
- ✅ **UVX Fetch Server**: Handle HTTP requests properly
- ✅ **Docker Servers**: Run containers via Docker-in-Docker
- ✅ **HTTP/SSE Servers**: Connect to remote servers if available

This provides much more realistic testing than local macOS testing.