#!/bin/bash
set -euo pipefail

# Setup KubeConfig Secret for GitHub Actions
# This script extracts the current kubeconfig and adds it to GitHub repository secrets

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

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
    echo -e "${BLUE}${BOLD}🔧 KubeConfig Secret Setup${NC}"
    echo ""
}

print_success() {
    echo -e "${GREEN}${BOLD}✅ $1${NC}"
}

# Check prerequisites
check_prerequisites() {
    print_info "Checking prerequisites..."

    # Check if kubectl is available
    if ! command -v kubectl >/dev/null 2>&1; then
        print_error "kubectl is not installed or not in PATH"
        exit 1
    fi

    # Check if gh (GitHub CLI) is available
    if ! command -v gh >/dev/null 2>&1; then
        print_error "GitHub CLI (gh) is not installed or not in PATH"
        print_info "Install it from: https://cli.github.com/"
        exit 1
    fi

    # Check if we're in a git repository
    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        print_error "Not in a git repository"
        exit 1
    fi

    # Check if GitHub CLI is authenticated
    if ! gh auth status >/dev/null 2>&1; then
        print_error "GitHub CLI is not authenticated"
        print_info "Run: gh auth login"
        exit 1
    fi

    print_success "Prerequisites check passed"
}

# Get current kubeconfig context
get_current_context() {
    print_info "Getting current kubectl context..."

    CURRENT_CONTEXT=$(kubectl config current-context 2>/dev/null || echo "")

    if [[ -z "$CURRENT_CONTEXT" ]]; then
        print_error "No current kubectl context found"
        print_info "Set a context with: kubectl config use-context <context-name>"
        exit 1
    fi

    print_info "Current context: $CURRENT_CONTEXT"

    # Verify we can connect to the cluster
    if ! kubectl cluster-info >/dev/null 2>&1; then
        print_error "Cannot connect to Kubernetes cluster"
        print_info "Please ensure your kubeconfig is valid and you have network access"
        exit 1
    fi

    print_success "Successfully connected to cluster"
}

# Extract minimal kubeconfig for the current context
extract_kubeconfig() {
    print_info "Extracting kubeconfig for context: $CURRENT_CONTEXT"

    # Create a temporary file for the kubeconfig
    TEMP_KUBECONFIG=$(mktemp /tmp/kubeconfig-XXXXXX)

    # Extract only the current context and its dependencies
    if ! kubectl config view --minify --flatten --context="$CURRENT_CONTEXT" > "$TEMP_KUBECONFIG"; then
        print_error "Failed to extract kubeconfig"
        rm -f "$TEMP_KUBECONFIG"
        exit 1
    fi

    # Verify the extracted config works (with retries)
    local retries=3
    local count=0
    while [[ $count -lt $retries ]]; do
        if KUBECONFIG="$TEMP_KUBECONFIG" kubectl cluster-info --request-timeout=10s >/dev/null 2>&1; then
            print_success "Successfully extracted kubeconfig"
            echo "$TEMP_KUBECONFIG"
            return 0
        fi
        count=$((count + 1))
        print_warning "Connection attempt $count/$retries failed, retrying..."
        sleep 2
    done

    print_error "Extracted kubeconfig is not valid after $retries attempts"
    rm -f "$TEMP_KUBECONFIG"
    exit 1
}

# Add kubeconfig to GitHub secrets
add_to_github_secrets() {
    local kubeconfig_file="$1"
    local secret_name="${2:-KUBECONFIG}"

    print_info "Adding kubeconfig to GitHub repository secrets..."

    # Get repository information
    REPO_INFO=$(gh repo view --json owner,name)
    REPO_OWNER=$(echo "$REPO_INFO" | jq -r '.owner.login')
    REPO_NAME=$(echo "$REPO_INFO" | jq -r '.name')

    print_info "Repository: $REPO_OWNER/$REPO_NAME"
    print_info "Secret name: $secret_name"

    # Base64 encode the kubeconfig
    KUBECONFIG_B64=$(base64 < "$kubeconfig_file" | tr -d '\n')

    # Add to GitHub secrets
    if echo "$KUBECONFIG_B64" | gh secret set "$secret_name" --repo "$REPO_OWNER/$REPO_NAME"; then
        print_success "Successfully added kubeconfig to GitHub secrets"
    else
        print_error "Failed to add kubeconfig to GitHub secrets"
        return 1
    fi
}

# Display usage instructions
show_usage_instructions() {
    print_info "📋 Usage Instructions for GitHub Actions:"
    echo ""
    echo "The CI/CD pipeline will automatically use this secret when deploying."
    echo ""
    echo "To enable development deployment, set the DEPLOY_DEV_ENABLED variable:"
    echo "  gh variable set DEPLOY_DEV_ENABLED --body \"true\""
    echo ""
    echo "Manual usage in workflows:"
    echo ""
    echo "- name: Configure kubectl"
    echo "  run: |"
    echo "    mkdir -p ~/.kube"
    echo "    echo \"\${{ secrets.KUBECONFIG }}\" | base64 -d > ~/.kube/config"
    echo "    chmod 600 ~/.kube/config"
    echo "    kubectl cluster-info"
    echo ""
}

# Main function
main() {
    print_header

    # Parse command line arguments
    SECRET_NAME="KUBECONFIG"
    FORCE=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            --secret-name)
                SECRET_NAME="$2"
                shift 2
                ;;
            --force)
                FORCE=true
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --secret-name NAME    Name of the GitHub secret (default: KUBECONFIG)"
                echo "  --force              Force overwrite existing secret"
                echo "  -h, --help           Show this help message"
                echo ""
                echo "This script will:"
                echo "  1. Extract your current kubeconfig context"
                echo "  2. Create a minimal kubeconfig file"
                echo "  3. Add it to GitHub repository secrets"
                echo ""
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    # Check if secret already exists
    if ! $FORCE; then
        if gh secret list --repo "$(gh repo view --json owner,name | jq -r '.owner.login + "/" + .name')" | grep -q "^$SECRET_NAME"; then
            print_warning "Secret '$SECRET_NAME' already exists"
            echo -n "Do you want to overwrite it? (y/N): "
            read -r response
            if [[ ! "$response" =~ ^[Yy]$ ]]; then
                print_info "Aborted"
                exit 0
            fi
        fi
    fi

    # Run the setup process
    check_prerequisites
    get_current_context

    KUBECONFIG_FILE=$(extract_kubeconfig)

    # Show cluster info
    print_info "Cluster information:"
    KUBECONFIG="$KUBECONFIG_FILE" kubectl cluster-info | head -5

    # Confirm before proceeding
    echo ""
    print_warning "⚠️  This will add your kubeconfig to GitHub repository secrets"
    print_warning "⚠️  Make sure you trust this repository and its maintainers"
    echo ""
    echo -n "Continue? (y/N): "
    read -r response

    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        print_info "Aborted"
        rm -f "$KUBECONFIG_FILE"
        exit 0
    fi

    # Add to GitHub secrets
    if add_to_github_secrets "$KUBECONFIG_FILE" "$SECRET_NAME"; then
        print_success "Setup completed successfully!"
        echo ""
        show_usage_instructions
    else
        print_error "Setup failed"
        exit 1
    fi

    # Cleanup
    rm -f "$KUBECONFIG_FILE"
}

# Run main function
main "$@"
