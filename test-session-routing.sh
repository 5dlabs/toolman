#!/bin/bash

# Test session-based execution context routing
echo "🧪 Testing session-based execution context routing..."

# Start the HTTP server in background
echo "🚀 Starting HTTP server..."
cargo run --bin toolman-http -- --port 3000 &
SERVER_PID=$!

# Wait for server to start
echo "⏳ Waiting for server to start..."
sleep 5

# Test session creation
echo "🔄 Creating test session..."
SESSION_RESPONSE=$(curl -s -X POST http://localhost:3000/session/init \
  -H "Content-Type: application/json" \
  -d '{
    "clientInfo": {
      "name": "test-client",
      "version": "1.0.0"
    },
    "workingDirectory": "/tmp/test",
    "localServers": [
      {
        "name": "filesystem",
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem"],
        "env": {},
        "tools": ["filesystem_read_file", "filesystem_write_file"]
      }
    ],
    "requestedTools": [
      {"name": "filesystem_read_file", "source": {"type": "local", "value": "filesystem"}},
      {"name": "web_search", "source": {"type": "global", "value": "web-search"}}
    ]
  }')

echo "📋 Session creation response:"
echo "$SESSION_RESPONSE" | jq .

# Extract session ID
SESSION_ID=$(echo "$SESSION_RESPONSE" | jq -r '.sessionId')
echo "📋 Session ID: $SESSION_ID"

if [ "$SESSION_ID" = "null" ] || [ -z "$SESSION_ID" ]; then
  echo "❌ Failed to create session"
  kill $SERVER_PID
  exit 1
fi

# Test local tool routing
echo "📡 Testing local tool routing (filesystem)..."
LOCAL_RESPONSE=$(curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "x-session-id: $SESSION_ID" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "filesystem_read_file",
      "arguments": {
        "path": "/tmp/test.txt"
      }
    }
  }')

echo "📋 Local tool response:"
echo "$LOCAL_RESPONSE" | jq .

# Test global tool routing
echo "🌐 Testing global tool routing (web-search)..."
GLOBAL_RESPONSE=$(curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "x-session-id: $SESSION_ID" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
      "name": "web_search",
      "arguments": {
        "query": "test query"
      }
    }
  }')

echo "📋 Global tool response:"
echo "$GLOBAL_RESPONSE" | jq .

# Test tool not in session
echo "❌ Testing tool not available in session..."
INVALID_RESPONSE=$(curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "x-session-id: $SESSION_ID" \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "unknown_tool",
      "arguments": {}
    }
  }')

echo "📋 Invalid tool response:"
echo "$INVALID_RESPONSE" | jq .

# Test session-filtered tools/list
echo "📋 Testing session-filtered tools/list..."
TOOLS_RESPONSE=$(curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "x-session-id: $SESSION_ID" \
  -d '{
    "jsonrpc": "2.0",
    "id": 4,
    "method": "tools/list",
    "params": {}
  }')

echo "📋 Session tools list:"
echo "$TOOLS_RESPONSE" | jq .

# Clean up session
echo "🗑️  Destroying session..."
curl -s -X DELETE "http://localhost:3000/session/$SESSION_ID"

# Stop server
echo "🛑 Stopping server..."
kill $SERVER_PID

echo "✅ Session routing test complete!"