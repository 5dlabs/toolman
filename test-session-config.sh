#!/bin/bash

# Test script for session-based configuration handshake
set -e

echo "🐳 Testing session-based configuration with Docker Compose"
echo "============================================================"

# Build the images
echo "📦 Building Docker images..."
docker compose -f docker-compose.dev.yml build

# Start the MCP proxy
echo "🚀 Starting MCP proxy..."
docker compose -f docker-compose.dev.yml up -d mcp-proxy

# Wait for server to start
echo "⏳ Waiting for MCP proxy to start..."
sleep 5

# Check if server is responding
echo "🔍 Checking server health..."
if curl -f http://localhost:3000/health; then
    echo "✅ MCP proxy is running!"
else
    echo "❌ MCP proxy is not responding"
    docker compose -f docker-compose.dev.yml logs mcp-proxy
    exit 1
fi

# Run the integration tests
echo "🧪 Running session configuration tests..."
docker compose -f docker-compose.dev.yml run --rm test-client

# Show server logs
echo "📋 MCP proxy logs:"
docker compose -f docker-compose.dev.yml logs mcp-proxy

# Clean up
echo "🧹 Cleaning up..."
docker compose -f docker-compose.dev.yml down

echo "✅ Session configuration tests completed!"