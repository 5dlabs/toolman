# Setting up KubeConfig for GitHub Actions

This guide explains how to set up your Kubernetes cluster configuration as a GitHub repository secret for automated deployments.

## Prerequisites

- `kubectl` installed and configured
- `gh` (GitHub CLI) installed and authenticated
- Access to a Kubernetes cluster
- Repository admin permissions

## Quick Setup

### 1. Ensure you're connected to the right cluster

```bash
# Check current context
kubectl config current-context

# If needed, switch to the correct context
kubectl config use-context admin@simple-cluster-1

# Verify connection
kubectl cluster-info
```

### 2. Run the setup script

```bash
# Make sure you're in the repository root
cd /path/to/mcp-proxy

# Run the setup script
./scripts/setup-kubeconfig-secret.sh
```

The script will:
- ✅ Check prerequisites (kubectl, gh, git)
- ✅ Extract your current kubeconfig context
- ✅ Create a minimal kubeconfig file
- ✅ Add it to GitHub repository secrets
- ✅ Show usage instructions

### 3. Enable development deployment

Set the `DEPLOY_DEV_ENABLED` repository variable to `true`:

```bash
# Using GitHub CLI
gh variable set DEPLOY_DEV_ENABLED --body "true"

# Or through GitHub web interface:
# Settings → Secrets and variables → Actions → Variables → New repository variable
```

## Manual Setup

If you prefer to set up the secret manually:

### 1. Extract kubeconfig

```bash
# Get current context
CONTEXT=$(kubectl config current-context)

# Extract minimal config
kubectl config view --minify --flatten --context="$CONTEXT" > kubeconfig-temp.yaml

# Verify it works
KUBECONFIG=kubeconfig-temp.yaml kubectl cluster-info
```

### 2. Add to GitHub secrets

```bash
# Base64 encode the kubeconfig
base64 < kubeconfig-temp.yaml | tr -d '\n' | gh secret set KUBECONFIG

# Clean up
rm kubeconfig-temp.yaml
```

### 3. Verify the secret

```bash
# List secrets
gh secret list

# Should show KUBECONFIG in the list
```

## Usage in GitHub Actions

The CI/CD pipeline will automatically use the `KUBECONFIG` secret when deploying to development:

```yaml
- name: Configure kubectl
  run: |
    mkdir -p ~/.kube
    echo "${{ secrets.KUBECONFIG }}" | base64 -d > ~/.kube/config
    chmod 600 ~/.kube/config
    kubectl cluster-info
```

## Security Considerations

### What gets stored

The script extracts a **minimal kubeconfig** containing only:
- Current context configuration
- Required cluster information
- Authentication credentials for the current user

### Best practices

1. **Use a service account**: Create a dedicated service account for CI/CD instead of using personal credentials
2. **Limit permissions**: Grant only the minimum required permissions
3. **Rotate credentials**: Regularly rotate the kubeconfig credentials
4. **Monitor access**: Monitor cluster access from CI/CD

### Creating a service account (recommended)

```bash
# Create service account
kubectl create serviceaccount github-actions -n mcp-proxy-dev

# Create role with minimal permissions
kubectl create role github-actions-role \
  --verb=get,list,watch,create,update,patch,delete \
  --resource=deployments,services,configmaps,secrets,pods \
  -n mcp-proxy-dev

# Bind role to service account
kubectl create rolebinding github-actions-binding \
  --role=github-actions-role \
  --serviceaccount=mcp-proxy-dev:github-actions \
  -n mcp-proxy-dev

# Get service account token
kubectl create token github-actions -n mcp-proxy-dev --duration=8760h
```

Then create a kubeconfig using the service account token instead of your personal credentials.

## Troubleshooting

### Common issues

1. **"kubectl: command not found"**
   - Install kubectl: https://kubernetes.io/docs/tasks/tools/

2. **"gh: command not found"**
   - Install GitHub CLI: https://cli.github.com/

3. **"GitHub CLI is not authenticated"**
   ```bash
   gh auth login
   ```

4. **"Cannot connect to Kubernetes cluster"**
   - Check your kubeconfig: `kubectl cluster-info`
   - Verify network connectivity
   - Check if cluster is running

5. **"No current kubectl context found"**
   ```bash
   kubectl config get-contexts
   kubectl config use-context <context-name>
   ```

### Verification

After setup, verify the secret works:

```bash
# Check if secret exists
gh secret list | grep KUBECONFIG

# Test in a workflow (or create a test workflow)
```

### Updating the secret

To update the kubeconfig secret:

```bash
# Run the setup script again
./scripts/setup-kubeconfig-secret.sh --force

# Or manually update
kubectl config view --minify --flatten | base64 | tr -d '\n' | gh secret set KUBECONFIG
```

## Environment Variables

The deployment can be controlled with these repository variables:

- `DEPLOY_DEV_ENABLED`: Set to `"true"` to enable automatic development deployment
- `KUBECONFIG`: The base64-encoded kubeconfig (set as a secret)

## Next Steps

After setting up the kubeconfig secret:

1. Push to the `main` branch to trigger a deployment
2. Check the Actions tab to see the deployment progress
3. Verify the deployment in your cluster:
   ```bash
   kubectl get pods -n mcp-proxy-dev
   kubectl get svc -n mcp-proxy-dev
   ```

For more information, see the [Deployment Guide](DEPLOYMENT.md).