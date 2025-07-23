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

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  -h, --help     Show this help"
            echo "  -v, --version  Install specific version"
            echo "  -d, --dir      Installation directory"
            echo "  -f, --force    Force reinstall"
            echo "  --no-config    Skip MCP config update"
            echo "  --no-path      Skip PATH update"
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
        --no-path)
            UPDATE_PATH=false
            shift
            ;;
        *)
            print_error "Unknown option: $1"
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
        *)
            print_error "Unsupported operating system: $OSTYPE"
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        *)
            print_error "Unsupported architecture: $(uname -m)"
            exit 1
            ;;
    esac

    if [[ "$os" == "macos" ]]; then
        echo "${arch}-apple-darwin"
    else
        echo "${arch}-unknown-linux-gnu"
    fi
}

# Try to download with fallback to x86_64 if arm64 fails
download_with_fallback() {
    local base_url="$1"
    local target="$2"
    local temp_archive="$3"
    local version="$4"

    # Try the native architecture first
    local archive_name="toolman-${target}.tar.gz"
    local archive_url="$base_url/$archive_name"

    print_info "Attempting to download for architecture: $target"
    if download_file "$archive_url" "$temp_archive"; then
        return 0
    fi

    # If arm64 fails and we're on arm64, try x86_64 as fallback
    if [[ "$target" == "aarch64-unknown-linux-gnu" ]]; then
        print_warning "ARM64 binary not available, trying x86_64 fallback..."
        local fallback_target="x86_64-unknown-linux-gnu"
        local fallback_archive="toolman-${fallback_target}.tar.gz"
        local fallback_url="$base_url/$fallback_archive"

        if download_file "$fallback_url" "$temp_archive"; then
            print_warning "Using x86_64 binary on ARM64 system (may require emulation)"
            return 0
        fi
    fi

    # Try a generic linux binary as last resort
    print_warning "Architecture-specific binary not found, trying generic linux binary..."
    local generic_archive="toolman-linux.tar.gz"
    local generic_url="$base_url/$generic_archive"

    if download_file "$generic_url" "$temp_archive"; then
        print_warning "Using generic linux binary"
        return 0
    fi

    return 1
}

# Get latest release version
get_latest_version() {
    print_info "Fetching latest release information..."

    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | \
                  grep -o '"tag_name": *"v[^"]*"' | \
                  head -1 | \
                  sed 's/"tag_name": *"v\([^"]*\)"/\1/')
    else
        print_error "curl not found. Please install curl."
        exit 1
    fi

    if [[ -z "$VERSION" ]]; then
        print_error "Could not determine latest version"
        exit 1
    fi

    print_info "Latest version: $VERSION"
}

# Download file
download_file() {
    local url="$1"
    local output="$2"

    print_info "Downloading: $(basename "$url")"

    if command -v curl >/dev/null 2>&1; then
        if ! curl -fsSL -o "$output" "$url"; then
            print_error "Failed to download $url"
            return 1
        fi
    else
        print_error "curl not found"
        exit 1
    fi
}

# Install binary from archive
install_binary_from_archive() {
    local archive_path="$1"
    local target_dir="$2"
    local binary_name="$3"

    print_info "Extracting $binary_name from archive..."

    local extract_dir
    extract_dir=$(mktemp -d)

    if tar -xzf "$archive_path" -C "$extract_dir"; then
        local binary_path
        binary_path=$(find "$extract_dir" -name "$binary_name" -type f | head -1)

        if [[ -n "$binary_path" ]]; then
            mv "$binary_path" "$target_dir/"
            chmod +x "$target_dir/$binary_name"
            print_success "Installed $binary_name"
        else
            print_error "Could not find $binary_name in archive"
            rm -rf "$extract_dir"
            return 1
        fi
    else
        print_error "Failed to extract archive"
        rm -rf "$extract_dir"
        return 1
    fi

    rm -rf "$extract_dir"
}

# Main installation function
main() {
    print_header

    # Expand install directory
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

    local dest_cli_path="$INSTALL_DIR/$CLIENT_BINARY_NAME"
    local dest_server_path="$INSTALL_DIR/$SERVER_BINARY_NAME"

    local temp_dir
    temp_dir=$(mktemp -d)
    local temp_archive="$temp_dir/$archive_name"

    # Check if already installed
    if [[ -f "$dest_cli_path" ]] && [[ "$FORCE" != true ]]; then
        print_info "Toolman is already installed at $dest_cli_path"
        print_info "Use --force to reinstall"
        exit 0
    fi

    # Create installation directory
    mkdir -p "$INSTALL_DIR"

    # Download archive
    if ! download_with_fallback "$base_url" "$target" "$temp_archive" "$VERSION"; then
        print_error "Failed to download release archive for architecture $target"
        print_info "Check that version $VERSION exists at:"
        print_info "  https://github.com/$REPO/releases/tag/$tag"
        rm -rf "$temp_dir"
        exit 1
    fi

    # Install binaries
    if ! install_binary_from_archive "$temp_archive" "$INSTALL_DIR" "$CLIENT_BINARY_NAME"; then
        rm -rf "$temp_dir"
        exit 1
    fi

    if ! install_binary_from_archive "$temp_archive" "$INSTALL_DIR" "$SERVER_BINARY_NAME"; then
        rm -rf "$temp_dir"
        exit 1
    fi

    # Cleanup
    rm -rf "$temp_dir"

    # Success message
    print_success "Installation completed successfully!"
    print_info "CLI Binary: $dest_cli_path"
    print_info "Server Binary: $dest_server_path"
    print_info "Version: $VERSION"
}

# Run main function
main "$@"
