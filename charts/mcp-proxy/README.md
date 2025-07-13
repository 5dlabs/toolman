# MCP Proxy Helm Chart

A Helm chart for deploying the MCP (Model Context Protocol) Bridge Proxy on Kubernetes.

## Overview

This Helm chart deploys the MCP Bridge Proxy, which provides dynamic tool management for MCP servers. It supports 25+ MCP servers with 278+ tools, offering centralized configuration and management.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- Node.js runtime (for MCP servers)

## Installation

### Quick Start

```bash
# Add the chart repository (if published)
helm repo add mcp-proxy https://your-username.github.io/mcp-proxy

# Install with default values
helm install mcp-proxy mcp-proxy/mcp-proxy

# Or install from local chart
helm install mcp-proxy ./charts/mcp-proxy
```

### Custom Installation

```bash
# Install with custom values
helm install mcp-proxy ./charts/mcp-proxy \
  --set image.tag=v1.0.0 \
  --set ingress.enabled=true \
  --set ingress.hosts[0].host=mcp-proxy.example.com

# Install with values file
helm install mcp-proxy ./charts/mcp-proxy -f values-production.yaml
```

## Configuration

### Basic Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of replicas | `1` |
| `image.repository` | Container image repository | `ghcr.io/your-username/mcp-proxy` |
| `image.tag` | Container image tag | `""` (uses appVersion) |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |

### Service Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `service.type` | Service type | `ClusterIP` |
| `service.port` | Service port | `3000` |
| `service.targetPort` | Target port | `3000` |

### Ingress Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `ingress.enabled` | Enable ingress | `false` |
| `ingress.className` | Ingress class name | `""` |
| `ingress.hosts` | Ingress hosts | `[{host: mcp-proxy.local, paths: [{path: /, pathType: Prefix}]}]` |
| `ingress.tls` | TLS configuration | `[]` |

### Resource Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `resources.limits.cpu` | CPU limit | `500m` |
| `resources.limits.memory` | Memory limit | `512Mi` |
| `resources.requests.cpu` | CPU request | `100m` |
| `resources.requests.memory` | Memory request | `128Mi` |

### Autoscaling Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `autoscaling.enabled` | Enable HPA | `false` |
| `autoscaling.minReplicas` | Minimum replicas | `1` |
| `autoscaling.maxReplicas` | Maximum replicas | `100` |
| `autoscaling.targetCPUUtilizationPercentage` | Target CPU utilization | `80` |

### MCP Server Configuration

Configure MCP servers in the `config.servers` section:

```yaml
config:
  servers:
    mcpServers:
      filesystem:
        command: "npx"
        args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
        env: {}
        disabled: false
      brave-search:
        command: "npx"
        args: ["-y", "@modelcontextprotocol/server-brave-search"]
        env:
          BRAVE_API_KEY: "your-api-key"
        disabled: false
```

## Deployment Examples

### Development Environment

```bash
helm install mcp-proxy ./charts/mcp-proxy -f values-development.yaml
```

### Production Environment

```bash
helm install mcp-proxy ./charts/mcp-proxy -f values-production.yaml \
  --set config.servers.mcpServers.brave-search.env.BRAVE_API_KEY=your-api-key
```

### With Ingress and TLS

```bash
helm install mcp-proxy ./charts/mcp-proxy \
  --set ingress.enabled=true \
  --set ingress.className=nginx \
  --set ingress.hosts[0].host=mcp-proxy.yourdomain.com \
  --set ingress.tls[0].secretName=mcp-proxy-tls \
  --set ingress.tls[0].hosts[0]=mcp-proxy.yourdomain.com
```

## Monitoring

### Health Checks

The chart includes built-in health checks:

- **Liveness probe**: `GET /health` (port 3000)
- **Readiness probe**: `GET /ready` (port 3000)

### Prometheus Monitoring

Enable Prometheus monitoring:

```yaml
monitoring:
  enabled: true
  serviceMonitor:
    enabled: true
    interval: 30s
```

## Security

### Pod Security Context

The chart runs with a non-root user and restricted security context:

```yaml
securityContext:
  allowPrivilegeEscalation: false
  capabilities:
    drop:
    - ALL
  readOnlyRootFilesystem: true
  runAsNonRoot: true
  runAsUser: 1000
```

### Network Policies

Enable network policies for enhanced security:

```yaml
networkPolicy:
  enabled: true
  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              name: ingress-nginx
      ports:
        - protocol: TCP
          port: 3000
```

## Persistence

Enable persistent storage for caching and temporary files:

```yaml
persistence:
  enabled: true
  storageClass: "fast-ssd"
  size: 5Gi
```

## Upgrading

### Upgrade the Release

```bash
helm upgrade mcp-proxy ./charts/mcp-proxy -f your-values.yaml
```

### Rollback

```bash
helm rollback mcp-proxy 1
```

## Troubleshooting

### Common Issues

1. **Pod not starting**: Check resource limits and node capacity
2. **Configuration errors**: Verify MCP server configuration in ConfigMap
3. **Network issues**: Check service and ingress configuration

### Debug Commands

```bash
# Check pod status
kubectl get pods -l app.kubernetes.io/name=mcp-proxy

# View logs
kubectl logs -f deployment/mcp-proxy

# Check configuration
kubectl get configmap mcp-proxy-config -o yaml

# Test connectivity
kubectl port-forward svc/mcp-proxy 8080:3000
```

## Uninstalling

```bash
helm uninstall mcp-proxy
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test with different values files
5. Submit a pull request

## License

This chart is licensed under the MIT License.