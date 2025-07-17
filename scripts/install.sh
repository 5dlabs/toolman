#!/bin/bash
set -euo pipefail

# MCP Bridge Proxy Installation Script
# This script downloads and installs the latest MCP proxy binary from GitHub releases

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Default values
REPO="5dlabs/toolman"
INSTALL_DIR="$HOME/.local/bin"
CLIENT_BINARY_NAME="toolman-client"
SERVER_BINARY_NAME="toolman-server"
VERSION="latest"
FORCE=false
UPDATE_CONFIG=true
UPDATE_PATH=true
MCP_CONFIG_FILE="$HOME/.cursor/mcp.json"
# Always install both client and server binaries

# Function to print colored output
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_header() {
    echo -e "${BLUE}${BOLD}🚀 MCP Bridge Proxy Installer${NC}"
    echo -e "${BLUE}Dynamic tool management for MCP servers${NC}"
    echo ""
}

print_success() {
    echo -e "${GREEN}${BOLD}✅ $1${NC}"
}

# Show usage
usage() {
    print_header
    cat << EOF
Download and install the MCP Bridge Proxy from GitHub releases.

USAGE:
    $0 [OPTIONS]

OPTIONS:
    -h, --help              Show this help message
    -v, --version VERSION   Install specific version (default: latest)
    -d, --dir DIR          Installation directory (default: ~/.local/bin)
    -f, --force            Force reinstall even if already installed
    --no-config            Skip MCP configuration update
    --no-path              Skip updating PATH in shell profile
    --no-http              Skip installing HTTP server binary
    --config FILE          Custom MCP config file path
    --repo REPO            GitHub repository (default: your-username/mcp-proxy)

EXAMPLES:
    $0                                    # Install latest version
    $0 --version v1.2.3                  # Install specific version
    $0 --dir /usr/local/bin --force       # System install with force
    $0 --no-config                        # Skip config update
    $0 --no-path                          # Skip PATH update
    $0 --no-http                          # Only install CLI binary

The installer will:
  ✅ Auto-detect your platform (Linux, macOS, Windows)
  ✅ Download the appropriate binaries
  ✅ Verify checksums for security
  ✅ Install to specified directory
  ✅ Update your .cursor/mcp.json configuration
  ✅ Add install directory to PATH (bash/zsh)
  ✅ Provide next steps for usage

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -v|--version)
            VERSION="$2"
            shift 2
            ;;
        -d|--dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        -f|--force)
            FORCE=true
            shift
            ;;
        --no-config)
            UPDATE_CONFIG=false
            shift
            ;;
        --no-http)
            # Both binaries are always installed
            shift
            ;;
        --config)
            MCP_CONFIG_FILE="$2"
            shift 2
            ;;
        --repo)
            REPO="5dlabs/toolman"
            shift 2
            ;;
        --no-path)
            UPDATE_PATH=false
            shift
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Detect platform and architecture
detect_platform() {
    local os arch

    case "$OSTYPE" in
        linux-gnu*)
            os="linux"
            ;;
        darwin*)
            os="macos"
            ;;
        msys*|mingw*|cygwin*)
            os="windows"
            ;;
        *)
            print_error "Unsupported operating system: $OSTYPE"
            print_info "Supported platforms: Linux, macOS, Windows"
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            if [[ "$os" == "macos" ]]; then
                arch="aarch64"
            else
                arch="x86_64"  # Fallback to x86_64 for ARM Linux
                print_warning "ARM64 Linux detected, using x86_64 binary"
            fi
            ;;
        *)
            print_error "Unsupported architecture: $(uname -m)"
            print_info "Supported architectures: x86_64, aarch64 (macOS only)"
            exit 1
            ;;
    esac

    local target
    if [[ "$os" == "macos" ]]; then
        target="${arch}-apple-darwin"
    elif [[ "$os" == "linux" ]]; then
        target="${arch}-unknown-linux-gnu"
    elif [[ "$os" == "windows" ]]; then
        target="${arch}-pc-windows-msvc"
    fi

    echo "$target"
}

# Get latest release version from GitHub API
get_latest_version() {
    print_info "Fetching latest release information..."

    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | \
                  grep -o '"tag_name": *"v[^"]*"' | \
                  head -1 | \
                  sed 's/"tag_name": *"v\([^"]*\)"/\1/')
    elif command -v wget >/dev/null 2>&1; then
        VERSION=$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" | \
                  grep -o '"tag_name": *"v[^"]*"' | \
                  head -1 | \
                  sed 's/"tag_name": *"v\([^"]*\)"/\1/')
    else
        print_error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi

    if [[ -z "$VERSION" ]]; then
        print_error "Could not determine latest version"
        print_info "You can specify a version manually with --version"
        exit 1
    fi

    print_info "Latest version: $VERSION"
}

# Download file with curl or wget
download_file() {
    local url="$1"
    local output="$2"

    print_info "Downloading: $(basename "$url")"

    if command -v curl >/dev/null 2>&1; then
        if ! curl -fsSL -o "$output" "$url"; then
            print_error "Failed to download $url"
            return 1
        fi
    elif command -v wget >/dev/null 2>&1; then
        if ! wget -q -O "$output" "$url"; then
            print_error "Failed to download $url"
            return 1
        fi
    else
        print_error "Neither curl nor wget found"
        exit 1
    fi
}

# Verify checksum
verify_checksum() {
    local binary_file="$1"
    local checksum_file="$2"
    local expected_filename="$3"

    if [[ ! -f "$checksum_file" ]]; then
        print_warning "Checksum file not found, skipping verification"
        return 0
    fi

    print_info "Verifying checksum for $(basename "$binary_file")..."

    # Create a temporary directory for verification
    local verify_dir
    verify_dir=$(mktemp -d)

    # Copy binary to verification directory with expected filename
    cp "$binary_file" "$verify_dir/$expected_filename"
    cp "$checksum_file" "$verify_dir/"

    # Change to verification directory and verify
    local verification_result=0
    if command -v shasum >/dev/null 2>&1; then
        if (cd "$verify_dir" && shasum -a 256 -c "$(basename "$checksum_file")" >/dev/null 2>&1); then
            print_success "Checksum verification passed"
        else
            print_error "Checksum verification failed"
            verification_result=1
        fi
    elif command -v sha256sum >/dev/null 2>&1; then
        if (cd "$verify_dir" && sha256sum -c "$(basename "$checksum_file")" >/dev/null 2>&1); then
            print_success "Checksum verification passed"
        else
            print_error "Checksum verification failed"
            verification_result=1
        fi
    else
        print_warning "No checksum utility found, skipping verification"
    fi

    # Cleanup verification directory
    rm -rf "$verify_dir"

    if [[ $verification_result -ne 0 ]]; then
        exit 1
    fi
}

# Create example configuration file
create_example_config() {
    local config_dir="$1"
    local example_config="$config_dir/servers-config.example.json"

    cat > "$example_config" << 'EOF'
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "env": {},
      "disabled": false
    },
    "brave-search": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-brave-search"],
      "env": {
        "BRAVE_API_KEY": ""
      },
      "disabled": true
    },
    "fetch": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-fetch"],
      "env": {},
      "disabled": false
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": ""
      },
      "disabled": true
    },
    "postgres": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-postgres"],
      "env": {
        "POSTGRES_CONNECTION_STRING": ""
      },
      "disabled": true
    }
  }
}
EOF

    print_info "Created example configuration: $example_config"
}

# Update MCP configuration for Cursor
update_mcp_config() {
    local stdio_binary_path="$1"

    if [[ "$UPDATE_CONFIG" != true ]]; then
        return 0
    fi

    print_info "Updating MCP configuration for Cursor..."

    # Create config directory if it doesn't exist
    mkdir -p "$(dirname "$MCP_CONFIG_FILE")"

    # Create backup if file exists
    if [[ -f "$MCP_CONFIG_FILE" ]]; then
        local backup_file="${MCP_CONFIG_FILE}.backup.$(date +%Y%m%d-%H%M%S)"
        cp "$MCP_CONFIG_FILE" "$backup_file"
        print_info "Backup created: $backup_file"
    fi

    # Update configuration with jq if available
    if command -v jq >/dev/null 2>&1; then
        if [[ ! -f "$MCP_CONFIG_FILE" ]]; then
            echo '{"mcpServers": {}}' > "$MCP_CONFIG_FILE"
        fi

        local config="{\"command\": \"$stdio_binary_path\", \"args\": []}"

        echo "$config" | jq '.' > /tmp/mcp-proxy-config.json
        jq --slurpfile new /tmp/mcp-proxy-config.json '.mcpServers."mcp-proxy" = $new[0]' "$MCP_CONFIG_FILE" > "${MCP_CONFIG_FILE}.tmp"
        mv "${MCP_CONFIG_FILE}.tmp" "$MCP_CONFIG_FILE"
        rm -f /tmp/mcp-proxy-config.json

        print_success "MCP configuration updated"
    else
        print_warning "jq not found - you'll need to manually update your MCP configuration"
        echo ""
        print_info "Add this to your $MCP_CONFIG_FILE:"
        echo '{'
        echo '  "mcpServers": {'
        echo '    "mcp-proxy": {'
        echo "      \"command\": \"$stdio_binary_path\","
        echo '      "args": []'
        echo '    }'
        echo '  }'
        echo '}'
    fi
}

# Update shell profile to include install directory in PATH
update_shell_path() {
    local install_dir="$1"

    if [[ "$UPDATE_PATH" != true ]]; then
        return 0
    fi

    # Skip if directory is already in PATH or is a system directory
    if [[ ":$PATH:" == *":$install_dir:"* ]] || [[ "$install_dir" == "/usr/local/bin" ]] || [[ "$install_dir" == "/usr/bin" ]]; then
        return 0
    fi

    # Detect shell and appropriate profile file
    local shell_name profile_file
    shell_name=$(basename "$SHELL")

    case "$shell_name" in
        bash)
            # Check for different bash profile files in order of preference
            if [[ -f "$HOME/.bash_profile" ]]; then
                profile_file="$HOME/.bash_profile"
            elif [[ -f "$HOME/.bashrc" ]]; then
                profile_file="$HOME/.bashrc"
            else
                # Create .bashrc if neither exists
                profile_file="$HOME/.bashrc"
            fi
            ;;
        zsh)
            profile_file="$HOME/.zshrc"
            ;;
        *)
            print_warning "Unsupported shell: $shell_name"
            print_info "Supported shells: bash, zsh"
            print_info "Please manually add $install_dir to your PATH:"
            print_info "  export PATH=\"\$PATH:$install_dir\""
            return 0
            ;;
    esac

    # Check if PATH export already exists in profile
    if [[ -f "$profile_file" ]] && grep -q "export PATH.*$install_dir" "$profile_file" 2>/dev/null; then
        print_info "PATH already configured in $profile_file"
        return 0
    fi

    print_warning "⚠️  $install_dir is not in your PATH"
    echo ""
    print_info "To use the binaries directly from anywhere, we can add it to your PATH."

    # Ask user if they want to update PATH
    if [[ -t 0 ]]; then  # Only prompt if running interactively
        echo -n "Add $install_dir to PATH in $profile_file? (y/N): "
        read -r response

        if [[ "$response" =~ ^[Yy]$ ]]; then
            # Add PATH export to profile file
            echo "" >> "$profile_file"
            echo "# Added by MCP Bridge Proxy installer" >> "$profile_file"
            echo "export PATH=\"\$PATH:$install_dir\"" >> "$profile_file"

            print_success "✅ PATH updated in $profile_file"
            echo ""
            print_info "To use the new PATH in this session, run:"
            print_info "  source $profile_file"
            print_info "Or restart your terminal."

            return 0
        else
            print_info "Skipped PATH update."
        fi
    else
        print_info "Running non-interactively, skipping PATH update."
    fi

    echo ""
    print_info "To manually add to PATH, add this line to $profile_file:"
    print_info "  export PATH=\"\$PATH:$install_dir\""
}

# Install binary from archive
install_binary_from_archive() {
    local archive_path="$1"
    local target_dir="$2"
    local binary_name="$3"

    print_info "Extracting $binary_name from archive..."

    # Create a temporary directory for extraction
    local extract_dir
    extract_dir=$(mktemp -d)

    # Extract the archive
    if tar -xzf "$archive_path" -C "$extract_dir"; then
        # Find the binary in the extracted files
        local binary_path
        binary_path=$(find "$extract_dir" -name "$binary_name" -type f | head -1)

        if [[ -n "$binary_path" ]]; then
            # Move binary to target directory
            mv "$binary_path" "$target_dir/"
            chmod +x "$target_dir/$binary_name"
            print_success "Installed $binary_name to $target_dir"
        else
            print_error "Could not find $binary_name in archive"
            return 1
        fi
    else
        print_error "Failed to extract archive"
        return 1
    fi

    # Cleanup
    rm -rf "$extract_dir"
}

# Main installation function
main() {
    print_header

    # Check prerequisites
    if [[ ! "$INSTALL_DIR" =~ ^/ ]] && [[ ! "$INSTALL_DIR" =~ ^\$HOME ]] && [[ ! "$INSTALL_DIR" =~ ^~ ]]; then
        # Convert relative path to absolute
        INSTALL_DIR="$(pwd)/$INSTALL_DIR"
    fi

    # Expand ~ to home directory
    INSTALL_DIR="${INSTALL_DIR/#\~/$HOME}"
    INSTALL_DIR="${INSTALL_DIR/#\$HOME/$HOME}"

    print_info "Installation directory: $INSTALL_DIR"

    # Detect platform
    local target
    target=$(detect_platform)
    print_info "Detected platform: $target"

    # Get version
    if [[ "$VERSION" == "latest" ]]; then
        get_latest_version
    fi

    # Prepare URLs and paths
    local tag="v$VERSION"
    local base_url="https://github.com/$REPO/releases/download/$tag"
    local archive_name="toolman-${target}.tar.gz"
    local archive_url="$base_url/$archive_name"

    local dest_cli_path="$INSTALL_DIR/$CLIENT_BINARY_NAME"
    local dest_http_path="$INSTALL_DIR/$SERVER_BINARY_NAME"
    local temp_dir
    temp_dir=$(mktemp -d)
    local temp_archive="$temp_dir/$archive_name"

    # Check if already installed
    if [[ -f "$dest_cli_path" ]] && [[ "$FORCE" != true ]]; then
        print_warning "MCP proxy already installed at $dest_cli_path"
        echo "Use --force to reinstall or --help for options"
        exit 0
    fi

    # Create installation directory
    mkdir -p "$INSTALL_DIR"

    # Download archive
    if ! download_file "$archive_url" "$temp_archive"; then
        print_error "Failed to download release archive"
        print_info "Please check that version $VERSION exists at:"
        print_info "  https://github.com/$REPO/releases/tag/$tag"
        exit 1
    fi

    # Install client binary
    if ! install_binary_from_archive "$temp_archive" "$INSTALL_DIR" "$CLIENT_BINARY_NAME"; then
        print_error "Failed to install client binary"
        exit 1
    fi

    # Install server binary
    if ! install_binary_from_archive "$temp_archive" "$INSTALL_DIR" "$SERVER_BINARY_NAME"; then
        print_error "Failed to install server binary"
        exit 1
    fi

    # Create example configuration
    create_example_config "$INSTALL_DIR"

    # Update MCP configuration (using CLI binary for stdio)
    update_mcp_config "$dest_cli_path"

    # Update shell PATH
    update_shell_path "$INSTALL_DIR"

    # Cleanup
    rm -rf "$temp_dir"

    # Success message
    print_success "Installation completed successfully!"
    echo ""
    print_info "📋 Installation Summary:"
    print_info "  CLI Binary: $dest_cli_path"
    if [[ -f "$dest_http_path" ]]; then
        print_info "  HTTP Server: $dest_http_path"
    fi
    print_info "  Version: $VERSION"
    if [[ "$UPDATE_CONFIG" == true ]]; then
        print_info "  Cursor Config: $MCP_CONFIG_FILE"
    fi
    print_info "  Example Config: $INSTALL_DIR/servers-config.example.json"

    echo ""
    print_info "🔄 Next Steps:"
    print_info "  1. Copy and customize the example configuration:"
    print_info "     cp $INSTALL_DIR/servers-config.example.json $INSTALL_DIR/servers-config.json"
    print_info "  2. Edit servers-config.json to enable/configure MCP servers"
    if [[ -f "$dest_http_path" ]]; then
        print_info "  3. Start the HTTP server: $SERVER_BINARY_NAME --port 3000"
    fi
    print_info "  4. Restart Cursor to load the new MCP server"

    echo ""
    print_info "📖 Usage Examples:"
    print_info "  # Use as MCP stdio server (automatically configured in Cursor)"
    print_info "  $CLIENT_BINARY_NAME"
    echo ""
    if [[ -f "$dest_http_path" ]]; then
        print_info "  # Run HTTP server for centralized management"
        print_info "  $SERVER_BINARY_NAME --port 3000 --project-dir ."
        echo ""
    fi
    print_info "  # View available MCP servers"
    print_info "  $CLIENT_BINARY_NAME --list-servers"
    print_info "  # Enable/disable servers"
    print_info "  $CLIENT_BINARY_NAME --enable filesystem --disable brave-search"

    echo ""
    print_info "🔗 Documentation:"
    print_info "  GitHub: https://github.com/$REPO"
    print_info "  Issues: https://github.com/$REPO/issues"

    echo ""
    print_success "🎉 Ready to use the MCP Bridge Proxy!"
}

# Run main function
main "$@"