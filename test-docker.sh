#!/bin/bash

# Docker Integration Test Runner
# This script runs the integration tests in a Docker environment

set -e

echo "🐳 Running MCP Proxy Integration Tests in Docker"
echo "================================================="

# Create test output directory
mkdir -p test_output

# Build the test Docker image
echo "📦 Building test Docker image..."
docker build -f Dockerfile.test -t mcp-proxy-test .

# Run the integration tests
echo "🧪 Running integration tests..."
docker compose -f docker-compose.test.yml run --rm integration-tests

echo "✅ Docker integration tests completed"
echo "📋 Test output saved to ./test_output/"

# Optional: Run specific test suites
if [ "$1" = "filesystem" ]; then
    echo "🗂️  Running filesystem tests..."
    docker compose -f docker-compose.test.yml run --rm test-filesystem
elif [ "$1" = "fetch" ]; then
    echo "🌐 Running fetch tests..."
    docker compose -f docker-compose.test.yml run --rm test-fetch
elif [ "$1" = "docker" ]; then
    echo "🐋 Running Docker tests..."
    docker compose -f docker-compose.test.yml run --rm test-docker
elif [ "$1" = "http" ]; then
    echo "🌐 Running HTTP/SSE tests..."
    docker compose -f docker-compose.test.yml run --rm test-http
fi

echo "🎉 All tests completed!"