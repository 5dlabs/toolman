#!/bin/bash

# Test script for MCP Bridge Proxy project initialization

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Testing MCP Bridge Proxy Project Initialization${NC}"
echo ""

# Create a test directory
TEST_DIR="/tmp/test-mcp-project-$(date +%s)"
echo -e "${BLUE}Creating test project at: $TEST_DIR${NC}"

# Run the initialization script
./scripts/init-mcp-project.sh --name "Test Project" --dir "$TEST_DIR"

# Show what was created
echo ""
echo -e "${GREEN}Project structure created:${NC}"
tree "$TEST_DIR" -a -I '.git'

echo ""
echo -e "${GREEN}Content of key files:${NC}"
echo ""
echo -e "${BLUE}.cursor/mcp.json:${NC}"
cat "$TEST_DIR/.cursor/mcp.json" | head -20

echo ""
echo -e "${BLUE}.mcp-bridge-proxy-config.json:${NC}"
cat "$TEST_DIR/.mcp-bridge-proxy-config.json"

echo ""
echo -e "${BLUE}Cursor rules (first 30 lines):${NC}"
head -30 "$TEST_DIR/.cursor/rules/mcp-bridge-proxy-project.md"

echo ""
echo -e "${GREEN}Test complete! Project initialized at: $TEST_DIR${NC}"
echo -e "${BLUE}To clean up: rm -rf $TEST_DIR${NC}"