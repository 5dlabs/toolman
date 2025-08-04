# Platform Repo Changes for Toolman Tool Discovery Fix

## Problem Summary

Toolman is not discovering tools after the migration to Argo CD because of a ConfigMap naming mismatch and missing local tools configuration.

## Root Cause

1. **ConfigMap Naming**: Toolman code expects a ConfigMap named `toolman-local-tools` but the Helm chart only creates `{release-name}-toolman-config`
2. **Missing Local Tools Config**: The Argo CD application likely doesn't have `localTools` values configured
3. **Namespace Issues**: Potential namespace detection problems after the migration

## Changes Required

### 1. Platform Repo - Argo CD Configuration

**File**: `infra/gitops/applications/toolman.yaml`

Ensure the Argo CD application includes the `localTools` configuration in the Helm values:

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: toolman
  namespace: argocd  # or wherever ArgoCD is installed
spec:
  project: default
  source:
    repoURL: https://github.com/5dlabs/toolman  # or your fork
    path: charts/toolman
    targetRevision: main  # or specific tag/branch
    helm:
      values: |
        # Your existing values...
        
        # Add this localTools section if it's missing:
        localTools:
          servers:
            filesystem:
              name: "Filesystem"
              description: "File system operations for reading, writing, and managing files"
              transport: "stdio"
              command: "npx"
              args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
              workingDirectory: "/tmp"
            
            # Add any other local tools you want available in agent containers
            # Example:
            # git:
            #   name: "Git"
            #   description: "Git version control operations"
            #   transport: "stdio"
            #   command: "git-mcp-server"
            #   args: []
            #   workingDirectory: "/tmp"
        
        # Ensure your main config.servers section exists:
        config:
          servers:
            kubernetes:
              # ... your existing server configs
            reddit:
              # ... your existing server configs
            # ... other servers
  
  destination:
    server: https://kubernetes.default.svc
    namespace: toolman  # Make sure this is the correct namespace
  
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
    - CreateNamespace=true
```

### 2. Toolman Repo Changes

**Status**: ✅ **Already Fixed**

The following changes have been made to the toolman repo:

1. **Created**: `charts/toolman/templates/local-tools-configmap.yaml`
   - Creates a separate ConfigMap named `toolman-local-tools` 
   - Only created when `localTools` values are provided
   - Contains the `local-tools-config.json` data

2. **Modified**: `charts/toolman/templates/configmap.yaml`
   - Removed the local tools section since it now has its own ConfigMap
   - Main servers config remains in the primary ConfigMap

## Verification Steps

After applying the changes:

1. **Check ConfigMaps are created**:
   ```bash
   kubectl get configmaps -n toolman
   # Should show both:
   # - {release-name}-toolman-config (main servers config)
   # - toolman-local-tools (local tools config)
   ```

2. **Check ConfigMap content**:
   ```bash
   kubectl get configmap toolman-local-tools -n toolman -o yaml
   # Should contain local-tools-config.json with your localTools configuration
   ```

3. **Check Toolman logs**:
   ```bash
   kubectl logs -n toolman deployment/toolman -f
   # Look for:
   # ✅ Loaded local tools config from namespace: toolman
   # ✅ Loaded X local tool servers from ConfigMap
   # 🔍 Starting tool discovery for all configured servers...
   # ✅ Tool discovery complete. Total tools available: X
   ```

## Troubleshooting

### If tools still aren't discovered:

1. **Check namespace detection**:
   ```bash
   kubectl exec -n toolman deployment/toolman -- cat /var/run/secrets/kubernetes.io/serviceaccount/namespace
   # Should output: toolman (or your actual namespace)
   ```

2. **Check filesystem config mount**:
   ```bash
   kubectl exec -n toolman deployment/toolman -- ls -la /config/
   # Should show: servers-config.json
   kubectl exec -n toolman deployment/toolman -- cat /config/servers-config.json
   # Should show your main servers configuration
   ```

3. **Check RBAC permissions**:
   ```bash
   kubectl auth can-i get configmaps --as=system:serviceaccount:toolman:toolman -n toolman
   # Should return: yes
   ```

### If ConfigMap isn't created:

- Verify the `localTools` section is properly indented in the Argo CD YAML
- Check Argo CD sync status: `argocd app get toolman`
- Force sync: `argocd app sync toolman`

## Expected Behavior After Fix

1. Toolman will read main servers config from `/config/servers-config.json` (mounted from main ConfigMap)
2. Toolman will read local tools config from `toolman-local-tools` ConfigMap via Kubernetes API
3. Both configurations will be merged for tool discovery
4. All configured tools should be discovered and available

## Migration Notes

- This fix maintains backward compatibility
- No breaking changes to existing server configurations
- Local tools are optional - if not configured, only main servers will be used
- The namespace detection should work automatically with proper RBAC