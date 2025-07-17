#!/usr/bin/env bash

# Toolman MCP Secrets Management Script
# Creates Kubernetes secrets following the naming convention: toolman-{server-name}-secrets

set -e

SCRIPT_NAME=$(basename "$0")
NAMESPACE="default"
DRY_RUN="false"

usage() {
    cat << EOF
Usage: $SCRIPT_NAME [OPTIONS] <server-name> <key1>=<value1> [<key2>=<value2> ...]

Creates a Kubernetes secret for an MCP server following the naming convention:
  Secret Name: toolman-{server-name}-secrets
  
OPTIONS:
  -n, --namespace NAMESPACE    Kubernetes namespace (default: default)
  -d, --dry-run               Show what would be created without applying
  -h, --help                  Show this help message
  -f, --from-file FILE        Load key-value pairs from a file (format: KEY=VALUE per line)
  --delete                    Delete the secret instead of creating it

EXAMPLES:
  # Create secret for Brave Search API
  $SCRIPT_NAME brave-search BRAVE_API_KEY=sk-1234567890abcdef

  # Create secret for Ansible with multiple keys
  $SCRIPT_NAME ansible AAP_TOKEN=token123 AAP_URL=https://aap.example.com/api/controller/v2

  # Create secret in specific namespace
  $SCRIPT_NAME -n production brave-search BRAVE_API_KEY=prod-key-12345

  # Dry run to see what would be created
  $SCRIPT_NAME --dry-run brave-search BRAVE_API_KEY=test-key

  # Load from file
  $SCRIPT_NAME --from-file secrets.env brave-search

  # Delete a secret
  $SCRIPT_NAME --delete brave-search

SUPPORTED SERVERS:
  - brave-search      (requires: BRAVE_API_KEY)
  - ansible          (requires: AAP_TOKEN, AAP_URL)
  - kubernetes       (requires: KUBECONFIG - kubeconfig file contents)
  - terraform        (no secrets needed)
  - solana           (no secrets needed)

NOTE: Only servers with published packages/images are currently supported.
Custom/unpublished MCP servers requiring source builds are not yet supported.

EOF
}

# Parse command line arguments
DELETE_MODE="false"
FROM_FILE=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        -d|--dry-run)
            DRY_RUN="true"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -f|--from-file)
            FROM_FILE="$2"
            shift 2
            ;;
        --delete)
            DELETE_MODE="true"
            shift
            ;;
        -*)
            echo "Error: Unknown option $1" >&2
            usage >&2
            exit 1
            ;;
        *)
            break
            ;;
    esac
done

# Validate arguments
if [[ $# -lt 1 ]]; then
    echo "Error: Missing server name" >&2
    usage >&2
    exit 1
fi

SERVER_NAME="$1"
SECRET_NAME="toolman-${SERVER_NAME}-secrets"
shift

# Validate server name (alphanumeric and hyphens only)
if [[ ! "$SERVER_NAME" =~ ^[a-zA-Z0-9-]+$ ]]; then
    echo "Error: Server name must contain only alphanumeric characters and hyphens" >&2
    exit 1
fi

# Handle delete mode
if [[ "$DELETE_MODE" == "true" ]]; then
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "Would delete secret: $SECRET_NAME in namespace: $NAMESPACE"
    else
        echo "Deleting secret: $SECRET_NAME in namespace: $NAMESPACE"
        kubectl delete secret "$SECRET_NAME" -n "$NAMESPACE" --ignore-not-found=true
        echo "✅ Secret deleted successfully (or didn't exist)"
    fi
    exit 0
fi

# Collect key-value pairs using simple arrays
SECRET_KEYS=()
SECRET_VALUES=()

# Load from file if specified
if [[ -n "$FROM_FILE" ]]; then
    if [[ ! -f "$FROM_FILE" ]]; then
        echo "Error: File not found: $FROM_FILE" >&2
        exit 1
    fi
    
    while IFS='=' read -r key value; do
        # Skip empty lines and comments
        [[ -z "$key" || "$key" =~ ^[[:space:]]*# ]] && continue
        # Remove leading/trailing whitespace
        key=$(echo "$key" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        value=$(echo "$value" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
        SECRET_KEYS+=("$key")
        SECRET_VALUES+=("$value")
    done < "$FROM_FILE"
fi

# Load from command line arguments
for arg in "$@"; do
    if [[ "$arg" =~ ^([A-Z_][A-Z0-9_]*)=(.*)$ ]]; then
        key="${BASH_REMATCH[1]}"
        value="${BASH_REMATCH[2]}"
        SECRET_KEYS+=("$key")
        SECRET_VALUES+=("$value")
    else
        echo "Error: Invalid format '$arg'. Use KEY=VALUE format" >&2
        exit 1
    fi
done

# Validate we have at least one key-value pair
if [[ ${#SECRET_KEYS[@]} -eq 0 ]]; then
    echo "Error: No key-value pairs provided" >&2
    usage >&2
    exit 1
fi

# Create kubectl command
KUBECTL_ARGS=(
    "create" "secret" "generic" "$SECRET_NAME"
    "-n" "$NAMESPACE"
)

# Add all key-value pairs
for i in "${!SECRET_KEYS[@]}"; do
    KUBECTL_ARGS+=("--from-literal=${SECRET_KEYS[$i]}=${SECRET_VALUES[$i]}")
done

# Add dry-run flag if needed
if [[ "$DRY_RUN" == "true" ]]; then
    KUBECTL_ARGS+=("--dry-run=client" "-o" "yaml")
fi

# Execute command
if [[ "$DRY_RUN" == "true" ]]; then
    echo "# Dry run - would create the following secret:"
    echo "# Command: kubectl ${KUBECTL_ARGS[*]}"
    echo "---"
    kubectl "${KUBECTL_ARGS[@]}"
else
    echo "Creating secret: $SECRET_NAME in namespace: $NAMESPACE"
    echo "Keys: ${SECRET_KEYS[*]}"
    
    # Delete existing secret if it exists
    kubectl delete secret "$SECRET_NAME" -n "$NAMESPACE" --ignore-not-found=true 2>/dev/null
    
    # Create new secret
    kubectl "${KUBECTL_ARGS[@]}"
    echo "✅ Secret created successfully"
    
    # Show the secret (without values)
    echo ""
    echo "Created secret details:"
    kubectl get secret "$SECRET_NAME" -n "$NAMESPACE" -o yaml | grep -E '^(apiVersion|kind|metadata|type|data:)' | head -20
fi

echo ""
echo "💡 To use this secret in your Helm values:"
echo "config:"
echo "  servers:"
echo "    $SERVER_NAME:"
echo "      enabled: true"
echo "      secretRef:"
echo "        name: \"$SECRET_NAME\""
echo "        keys:"
for key in "${SECRET_KEYS[@]}"; do
    echo "          - \"$key\""
done