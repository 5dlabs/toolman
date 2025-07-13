# MCP Proxy Deployment Guide

This guide covers various deployment options for the MCP Bridge Proxy.

## Table of Contents

- [Quick Start](#quick-start)
- [Docker Deployment](#docker-deployment)
- [Kubernetes Deployment](#kubernetes-deployment)
- [Local Development](#local-development)
- [Production Deployment](#production-deployment)
- [Configuration](#configuration)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)

## Quick Start

### Using Docker

```bash
# Run with default configuration
docker run -p 3000:3000 ghcr.io/your-username/mcp-proxy:latest

# Run with custom configuration
docker run -p 3000:3000 \
  -v ./servers-config.json:/config/servers-config.json:ro \
  ghcr.io/your-username/mcp-proxy:latest
```

### Using Helm

```bash
# Install with default values
helm install mcp-proxy ./charts/mcp-proxy

# Install with custom values
helm install mcp-proxy ./charts/mcp-proxy -f my-values.yaml
```

## Docker Deployment

### Docker Compose

Create a `docker-compose.yml` file:

```yaml
version: '3.8'
services:
  mcp-proxy:
    image: ghcr.io/your-username/mcp-proxy:latest
    ports:
      - "3000:3000"
    volumes:
      - ./servers-config.json:/config/servers-config.json:ro
      - ./data:/data
    environment:
      - PORT=3000
      - PROJECT_DIR=/config
      - RUST_LOG=info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
```

Deploy:

```bash
docker-compose up -d
```

### Docker Swarm

```bash
# Deploy to Docker Swarm
docker stack deploy -c docker-compose.yml mcp-proxy
```

## Kubernetes Deployment

### Prerequisites

- Kubernetes cluster (1.19+)
- Helm 3.0+
- kubectl configured

### Basic Deployment

```bash
# Clone the repository
git clone https://github.com/your-username/mcp-proxy.git
cd mcp-proxy

# Install with default values
helm install mcp-proxy ./charts/mcp-proxy
```

### Development Deployment

```bash
# Deploy to development environment
helm install mcp-proxy-dev ./charts/mcp-proxy \
  -f charts/mcp-proxy/values-development.yaml \
  --namespace mcp-proxy-dev \
  --create-namespace
```

### Production Deployment

```bash
# Deploy to production environment
helm install mcp-proxy-prod ./charts/mcp-proxy \
  -f charts/mcp-proxy/values-production.yaml \
  --namespace mcp-proxy-prod \
  --create-namespace \
  --set ingress.hosts[0].host=mcp-proxy.yourdomain.com
```

### Custom Configuration

Create a custom values file (`my-values.yaml`):

```yaml
replicaCount: 3

image:
  tag: "v1.0.0"

ingress:
  enabled: true
  className: "nginx"
  hosts:
    - host: mcp-proxy.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: mcp-proxy-tls
      hosts:
        - mcp-proxy.example.com

resources:
  limits:
    cpu: 1000m
    memory: 1Gi
  requests:
    cpu: 200m
    memory: 256Mi

autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 10

config:
  servers:
    mcpServers:
      filesystem:
        command: "npx"
        args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
        env: {}
        disabled: false
      github:
        command: "npx"
        args: ["-y", "@modelcontextprotocol/server-github"]
        env:
          GITHUB_PERSONAL_ACCESS_TOKEN: "your-token"
        disabled: false
```

Deploy with custom configuration:

```bash
helm install mcp-proxy ./charts/mcp-proxy -f my-values.yaml
```

## Local Development

### Prerequisites

- Rust 1.70+
- Node.js 18+
- Git

### Setup

```bash
# Clone the repository
git clone https://github.com/your-username/mcp-proxy.git
cd mcp-proxy

# Build the project
cargo build --release

# Run the HTTP server
cargo run --bin toolman-http -- --port 3000 --project-dir ./
```

### Development with Docker

```bash
# Build development image
docker build -t mcp-proxy:dev .

# Run development container
docker run -p 3000:3000 \
  -v $(pwd)/servers-config.json:/config/servers-config.json:ro \
  -e RUST_LOG=debug \
  mcp-proxy:dev
```

## Production Deployment

### High Availability Setup

For production deployments, consider:

1. **Multiple Replicas**: Use at least 3 replicas
2. **Resource Limits**: Set appropriate CPU and memory limits
3. **Health Checks**: Configure liveness and readiness probes
4. **Persistent Storage**: Enable persistence for caching
5. **Monitoring**: Set up metrics and logging
6. **Security**: Use network policies and security contexts

### Example Production Values

```yaml
replicaCount: 3

resources:
  limits:
    cpu: 1000m
    memory: 1Gi
  requests:
    cpu: 200m
    memory: 256Mi

autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70

persistence:
  enabled: true
  storageClass: "fast-ssd"
  size: 5Gi

podDisruptionBudget:
  enabled: true
  minAvailable: 2

networkPolicy:
  enabled: true

monitoring:
  enabled: true
  serviceMonitor:
    enabled: true
```

### Load Balancing

Configure your load balancer to distribute traffic across multiple instances:

```yaml
ingress:
  enabled: true
  className: "nginx"
  annotations:
    nginx.ingress.kubernetes.io/load-balance: "round_robin"
    nginx.ingress.kubernetes.io/upstream-hash-by: "$request_uri"
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | HTTP server port | `3000` |
| `PROJECT_DIR` | Configuration directory | `/config` |
| `RUST_LOG` | Log level | `info` |
| `RUST_BACKTRACE` | Enable backtraces | `0` |

### MCP Server Configuration

Configure MCP servers in `servers-config.json`:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "env": {},
      "disabled": false
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "your-token"
      },
      "disabled": false
    }
  }
}
```

### Helm Configuration

Key configuration options in `values.yaml`:

```yaml
# Application configuration
config:
  servers:
    mcpServers: {} # MCP server definitions

# Networking
service:
  type: ClusterIP
  port: 3000

ingress:
  enabled: false
  className: ""
  hosts: []

# Resources
resources:
  limits:
    cpu: 500m
    memory: 512Mi
  requests:
    cpu: 100m
    memory: 128Mi

# Scaling
autoscaling:
  enabled: false
  minReplicas: 1
  maxReplicas: 100
```

## Monitoring

### Health Checks

The application provides health check endpoints:

- **Liveness**: `GET /health`
- **Readiness**: `GET /ready`

### Metrics

Enable Prometheus metrics:

```yaml
monitoring:
  enabled: true
  serviceMonitor:
    enabled: true
    interval: 30s
    scrapeTimeout: 10s
```

### Logging

Configure logging levels:

```yaml
env:
  - name: RUST_LOG
    value: "info"
  - name: RUST_BACKTRACE
    value: "1"
```

View logs:

```bash
# Docker
docker logs mcp-proxy

# Kubernetes
kubectl logs -f deployment/mcp-proxy
```

## Troubleshooting

### Common Issues

1. **Port Already in Use**
   ```bash
   # Check what's using the port
   lsof -i :3000

   # Use a different port
   docker run -p 3001:3000 mcp-proxy
   ```

2. **Configuration Not Loading**
   ```bash
   # Check volume mount
   docker run -v $(pwd)/servers-config.json:/config/servers-config.json:ro mcp-proxy

   # Verify file permissions
   ls -la servers-config.json
   ```

3. **MCP Server Not Starting**
   ```bash
   # Check Node.js availability
   docker exec -it mcp-proxy node --version

   # Check MCP server installation
   docker exec -it mcp-proxy npx @modelcontextprotocol/server-filesystem --version
   ```

### Debug Commands

```bash
# Check pod status
kubectl get pods -l app.kubernetes.io/name=mcp-proxy

# View detailed pod information
kubectl describe pod <pod-name>

# Check service endpoints
kubectl get endpoints mcp-proxy

# Port forward for testing
kubectl port-forward svc/mcp-proxy 8080:3000

# Check configuration
kubectl get configmap mcp-proxy-config -o yaml
```

### Performance Tuning

1. **Resource Allocation**
   ```yaml
   resources:
     limits:
       cpu: 2000m
       memory: 2Gi
     requests:
       cpu: 500m
       memory: 512Mi
   ```

2. **Horizontal Pod Autoscaler**
   ```yaml
   autoscaling:
     enabled: true
     minReplicas: 3
     maxReplicas: 20
     targetCPUUtilizationPercentage: 60
   ```

3. **Persistent Storage**
   ```yaml
   persistence:
     enabled: true
     storageClass: "fast-ssd"
     size: 10Gi
   ```

## Security Considerations

### Network Security

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

### Pod Security

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

### Secrets Management

```yaml
# Use Kubernetes secrets for sensitive data
env:
  - name: GITHUB_TOKEN
    valueFrom:
      secretKeyRef:
        name: mcp-proxy-secrets
        key: github-token
```

## Backup and Recovery

### Configuration Backup

```bash
# Backup Helm values
helm get values mcp-proxy > backup-values.yaml

# Backup ConfigMap
kubectl get configmap mcp-proxy-config -o yaml > backup-config.yaml
```

### Data Backup

```bash
# Backup persistent data (if enabled)
kubectl exec -it <pod-name> -- tar czf /tmp/backup.tar.gz /data
kubectl cp <pod-name>:/tmp/backup.tar.gz ./backup.tar.gz
```

## Scaling

### Manual Scaling

```bash
# Scale deployment
kubectl scale deployment mcp-proxy --replicas=5

# Scale using Helm
helm upgrade mcp-proxy ./charts/mcp-proxy --set replicaCount=5
```

### Auto Scaling

```yaml
autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 20
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80
```

## Support

For additional support:

1. Check the [GitHub Issues](https://github.com/your-username/mcp-proxy/issues)
2. Review the [Documentation](https://github.com/your-username/mcp-proxy/docs)
3. Join the community discussions

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines on contributing to the project.