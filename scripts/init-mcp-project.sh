#!/bin/bash

# MCP Bridge Proxy Project Initialization Script
# This script sets up a new project with MCP Bridge Proxy integration,
# TaskMaster, and all necessary configuration files.

set -e  # Exit on error

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
MCP_PROXY_DIR="$(dirname "$SCRIPT_DIR")"

# Parse command line arguments
PROJECT_NAME=""
PROJECT_DIR=""
SKIP_TASKMASTER=false
SKIP_RULES=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --name)
            PROJECT_NAME="$2"
            shift 2
            ;;
        --dir)
            PROJECT_DIR="$2"
            shift 2
            ;;
        --skip-taskmaster)
            SKIP_TASKMASTER=true
            shift
            ;;
        --skip-rules)
            SKIP_RULES=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 --name <project-name> --dir <project-directory> [options]"
            echo ""
            echo "Options:"
            echo "  --name            Project name (required)"
            echo "  --dir             Project directory path (required)"
            echo "  --skip-taskmaster Skip TaskMaster initialization"
            echo "  --skip-rules      Skip Cursor rules generation"
            echo "  -h, --help        Show this help message"
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Validate required arguments
if [[ -z "$PROJECT_NAME" ]] || [[ -z "$PROJECT_DIR" ]]; then
    print_error "Both --name and --dir are required"
    echo "Usage: $0 --name <project-name> --dir <project-directory>"
    exit 1
fi

# Make PROJECT_DIR absolute
PROJECT_DIR=$(cd "$(dirname "$PROJECT_DIR")" && pwd)/$(basename "$PROJECT_DIR")

print_info "Initializing MCP Bridge Proxy project: $PROJECT_NAME"
print_info "Project directory: $PROJECT_DIR"

# Create project directory if it doesn't exist
if [[ ! -d "$PROJECT_DIR" ]]; then
    print_info "Creating project directory..."
    mkdir -p "$PROJECT_DIR"
fi

cd "$PROJECT_DIR"

# Step 1: Create .cursor directory and mcp.json
print_info "Setting up Cursor MCP configuration..."
mkdir -p .cursor

cat > .cursor/mcp.json << EOF
{
  "mcpServers": {
    "mcp-bridge-proxy": {
      "command": "$MCP_PROXY_DIR/target/release/toolman",
      "args": [
        "--url", "http://localhost:3002/mcp",
        "--working-dir", "$PROJECT_DIR"
      ],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
EOF

print_success "Created .cursor/mcp.json"

# Step 2: Create initial .mcp-bridge-proxy-config.json
print_info "Creating initial MCP Bridge Proxy configuration..."
cat > .mcp-bridge-proxy-config.json << EOF
{
  "enabled_tools": {
    "mcp_mcp-bridge-proxy_enable_tool": true,
    "mcp_mcp-bridge-proxy_disable_tool": true,
    "mcp_mcp-bridge-proxy_save_config": true
  },
  "last_updated": "$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")"
}
EOF

print_success "Created .mcp-bridge-proxy-config.json with core tools enabled"

# Step 3: Initialize TaskMaster (if not skipped)
if [[ "$SKIP_TASKMASTER" = false ]]; then
    print_info "Checking for TaskMaster..."

    if command_exists task-master; then
        print_info "Initializing TaskMaster..."

        # First, create a basic .env file for TaskMaster
        if [[ ! -f .env ]]; then
            cat > .env << EOF
# TaskMaster Configuration
# Add your API keys here:
# ANTHROPIC_API_KEY=your-key-here
# PERPLEXITY_API_KEY=your-key-here
# OPENAI_API_KEY=your-key-here
EOF
            print_warning "Created .env file - please add your API keys"
        fi

        # Initialize TaskMaster
        task-master init --yes --name "$PROJECT_NAME" || {
            print_warning "TaskMaster initialization failed - you may need to run it manually"
        }

        # Create PRD template
        if [[ -d .taskmaster ]]; then
            mkdir -p .taskmaster/docs
            cat > .taskmaster/docs/project-prd.txt << 'EOF'
# Project Requirements Document: ${PROJECT_NAME}

## Project Overview
[Describe the project's purpose and goals]

## Core Features
1. [Feature 1]
2. [Feature 2]
3. [Feature 3]

## Technical Requirements
- Programming Language: [e.g., Rust, Python, TypeScript]
- Framework: [if applicable]
- Database: [if applicable]
- External Services: [APIs, etc.]

## MCP Tools Required
- [ ] filesystem (file operations)
- [ ] git (version control)
- [ ] memory (knowledge graph)
- [ ] github (repository management)
- [ ] task-master-ai (project management)
- [ ] [other tools as needed]

## Success Criteria
- [ ] [Criterion 1]
- [ ] [Criterion 2]
- [ ] [Criterion 3]

## Constraints
- [Time constraints]
- [Technical constraints]
- [Resource constraints]
EOF
            # Replace placeholder with actual project name
            sed -i.bak "s/\${PROJECT_NAME}/$PROJECT_NAME/g" .taskmaster/docs/project-prd.txt
            rm -f .taskmaster/docs/project-prd.txt.bak

            print_success "Created PRD template at .taskmaster/docs/project-prd.txt"

            # Generate initial prompt for task parsing
            cat > .taskmaster/docs/parse-prd-prompt.txt << EOF
Please parse the PRD at .taskmaster/docs/project-prd.txt and generate initial tasks.

Focus on:
1. Breaking down each core feature into implementable tasks
2. Setting up proper dependencies between tasks
3. Identifying which MCP tools each task will need
4. Creating clear test strategies for each task

For each task that requires MCP tools, include tags like:
#tool:filesystem #tool:git #tool:memory

This will help with automated tool enablement later.
EOF

            print_success "Created parse-prd prompt at .taskmaster/docs/parse-prd-prompt.txt"
        fi
    else
        print_warning "TaskMaster not found - skipping TaskMaster initialization"
        print_info "Install with: npm install -g task-master-ai"
    fi
fi

# Step 4: Create Cursor rules (if not skipped)
if [[ "$SKIP_RULES" = false ]]; then
    print_info "Creating Cursor rules..."
    mkdir -p .cursor/rules

    cat > .cursor/rules/mcp-bridge-proxy-project.md << 'EOF'
# MCP Bridge Proxy Project Rules

**This project uses MCP Bridge Proxy for tool management. Follow these patterns.**

## 🛠️ Tool Management

### **Enabling Tools**
When you need to use a new MCP tool:
1. Use `mcp_mcp-bridge-proxy_enable_tool` to enable it
2. **IMPORTANT**: After enabling, ask "Should I continue?" to trigger UI refresh
3. Then proceed to use the newly enabled tool

Example:
```
mcp_mcp-bridge-proxy_enable_tool(server_name: "memory", tool_name: "create_entities")
// Tool enabled, but UI needs refresh
"I've enabled the memory_create_entities tool. Should I continue?"
// User responds, UI refreshes, tool is now available
```

### **Disabling Tools**
To clean up unused tools:
```
mcp_mcp-bridge-proxy_disable_tool(server_name: "memory", tool_name: "old_tool")
```

### **Saving Configuration**
To persist tool changes across sessions:
```
mcp_mcp-bridge-proxy_save_config()
```

## 📋 TaskMaster Integration

### **Available Tools** (when enabled)
- `task_master_ai_get_tasks` - List all tasks
- `task_master_ai_add_task` - Add new tasks
- `task_master_ai_expand_task` - Break down complex tasks
- `task_master_ai_analyze_project_complexity` - Analyze task complexity
- `task_master_ai_update_task` - Update task details
- `task_master_ai_set_task_status` - Mark task progress

### **Workflow Pattern**
1. Enable TaskMaster tools as needed
2. Use them through the MCP Bridge Proxy (prefixed names)
3. Remember the "Should I continue?" pattern after enabling

## 🎯 Project-Specific Configuration

- **Working Directory**: ${PROJECT_DIR}
- **Config File**: .mcp-bridge-proxy-config.json
- **PRD Location**: .taskmaster/docs/project-prd.txt

## ⚠️ Known Limitations

- New tools don't appear until context refresh (hence "Should I continue?")
- Tool names are prefixed with server name (e.g., `memory_create_entities`)
- Configuration is project-specific and isolated

## 🔄 Standard Patterns

### **Starting Work Session**
1. Check which tools are enabled (look at available tools)
2. Enable any additional tools needed for the session
3. Save configuration if you want changes to persist

### **Implementing Features**
1. Check TaskMaster for current task
2. Enable required tools mentioned in task
3. Implement feature using enabled tools
4. Update task status when complete

---

**Remember**: This project uses dynamic tool management. Enable only what you need, when you need it.
EOF

    # Replace placeholder with actual project directory
    sed -i.bak "s|\${PROJECT_DIR}|$PROJECT_DIR|g" .cursor/rules/mcp-bridge-proxy-project.md
    rm -f .cursor/rules/mcp-bridge-proxy-project.md.bak

    print_success "Created Cursor rules at .cursor/rules/mcp-bridge-proxy-project.md"
fi

# Step 5: Create project initialization summary
print_info "Creating project summary..."
cat > MCP_PROJECT_SETUP.md << EOF
# MCP Bridge Proxy Project Setup Summary

**Project**: $PROJECT_NAME
**Directory**: $PROJECT_DIR
**Created**: $(date)

## ✅ Setup Completed

### 1. Cursor MCP Configuration
- Created \`.cursor/mcp.json\` with MCP Bridge Proxy configuration
- Configured to use working directory: $PROJECT_DIR

### 2. Tool Configuration
- Created \`.mcp-bridge-proxy-config.json\` with core tools enabled:
  - \`enable_tool\` - For enabling additional tools
  - \`disable_tool\` - For disabling tools
  - \`save_config\` - For persisting configuration

### 3. TaskMaster Setup
$(if [[ "$SKIP_TASKMASTER" = false ]] && [[ -d .taskmaster ]]; then
    echo "- Initialized TaskMaster project structure"
    echo "- Created PRD template at \`.taskmaster/docs/project-prd.txt\`"
    echo "- Created parse prompt at \`.taskmaster/docs/parse-prd-prompt.txt\`"
else
    echo "- TaskMaster initialization skipped or failed"
fi)

### 4. Cursor Rules
$(if [[ "$SKIP_RULES" = false ]]; then
    echo "- Created project-specific rules at \`.cursor/rules/mcp-bridge-proxy-project.md\`"
    echo "- Documented tool management patterns and UI refresh workaround"
else
    echo "- Cursor rules generation skipped"
fi)

## 🚀 Next Steps

1. **Add API Keys** (if using TaskMaster):
   - Edit \`.env\` file and add your API keys

2. **Start MCP Bridge Proxy**:
   \`\`\`bash
   cd $MCP_PROXY_DIR
   nohup ./target/release/toolman-http --project-dir \$(pwd) --port 3002 > /tmp/mcp-bridge-proxy.log 2>&1 &
   \`\`\`

3. **Open in Cursor**:
   - Open this project directory in Cursor
   - The MCP Bridge Proxy should connect automatically

4. **Complete PRD** (if using TaskMaster):
   - Edit \`.taskmaster/docs/project-prd.txt\` with your project requirements
   - Use the parse prompt to generate initial tasks

5. **Enable Required Tools**:
   - Use the MCP Bridge Proxy to enable tools as needed
   - Remember to ask "Should I continue?" after enabling tools

## 📚 Documentation

- [MCP Bridge Proxy Documentation]($MCP_PROXY_DIR/README.md)
- [TaskMaster Documentation](https://github.com/cyanheads/task-master-ai)

---

*This project is configured to use MCP Bridge Proxy for dynamic tool management.*
EOF

print_success "Created project setup summary at MCP_PROJECT_SETUP.md"

# Step 6: Initialize git repository (optional)
if command_exists git && [[ ! -d .git ]]; then
    print_info "Initializing git repository..."
    git init

    # Create .gitignore
    cat > .gitignore << EOF
# Environment variables
.env
.env.local

# Logs
*.log
logs/
/tmp/

# Dependencies
node_modules/
target/

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# TaskMaster
.taskmaster/reports/
.taskmaster/cache/

# MCP Bridge Proxy
.mcp-bridge-proxy-config.json.backup*
EOF

    git add .
    git commit -m "Initial project setup with MCP Bridge Proxy integration"

    print_success "Initialized git repository with initial commit"
fi

# Final summary
echo ""
print_success "Project initialization complete!"
echo ""
echo "Project: $PROJECT_NAME"
echo "Location: $PROJECT_DIR"
echo ""
echo "To start working:"
echo "1. cd $PROJECT_DIR"
echo "2. Start the MCP Bridge Proxy server (see MCP_PROJECT_SETUP.md)"
echo "3. Open the project in Cursor"
echo ""
echo "For detailed next steps, see: MCP_PROJECT_SETUP.md"