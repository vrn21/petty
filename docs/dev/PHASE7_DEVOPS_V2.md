# Phase 7: AWS Deployment (Refined v2)

> Deploying Petty on c5.metal Spot Instance with Docker container.

---

## Overview

This document outlines a streamlined deployment strategy:

- **Docker multi-stage build**: petty-mcp compiled from source INSIDE Docker build
- **Download from source (in Docker)**: Firecracker, jailer, and kernel downloaded during Docker build
- **S3 for rootfs only**: Only the ext4 image needs to be in your S3 (fetched at runtime)
- **Manual control**: Start/stop instance manually (no spot termination handling)
- **Environment variables**: All paths configurable at runtime

---

## Cost Summary

| Mode      | Hourly     | 40hrs/month |
| --------- | ---------- | ----------- |
| On-Demand | $4.08      | $163        |
| **Spot**  | **~$1.26** | **~$50**    |

---

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                        AWS Account                            │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │                  c5.metal (Spot)                      │    │
│  │                                                       │    │
│  │  ┌─────────────────────────────────────────────────┐ │    │
│  │  │           Docker Container (petty-mcp)          │ │    │
│  │  │                                                  │ │    │
│  │  │  /usr/local/bin/                                 │ │    │
│  │  │  ├── petty-mcp    (built from source)            │ │    │
│  │  │  ├── firecracker  (from GitHub)                  │ │    │
│  │  │  └── jailer       (from GitHub)                  │ │    │
│  │  │                                                  │ │    │
│  │  │  /var/lib/petty/                                 │ │    │
│  │  │  ├── vmlinux         (from AWS S3, baked in)     │ │    │
│  │  │  └── debian-devbox   (from YOUR S3, at runtime)  │ │    │
│  │  │                                                  │ │    │
│  │  │  Endpoints:                                      │ │    │
│  │  │  - GET  /health  → Health check                  │ │    │
│  │  │  - POST /mcp     → JSON-RPC requests             │ │    │
│  │  │  - GET  /mcp     → SSE stream                    │ │    │
│  │  └─────────────────────────────────────────────────┘ │    │
│  │                                                       │    │
│  │  Security Group: SSH (22) + HTTP (8080)               │    │
│  └──────────────────────────────────────────────────────┘    │
│                               │                               │
│                               ▼                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │                   S3 Bucket (yours)                    │    │
│  │                   debian-devbox.ext4                   │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

---

## Docker Multi-Stage Build

**File**: `Dockerfile.server`

The Dockerfile uses 5 stages for optimal caching and minimal image size:

| Stage      | Base Image  | Purpose                                   |
| ---------- | ----------- | ----------------------------------------- |
| 1. chef    | rust:1.83   | Install cargo-chef for dependency caching |
| 2. planner | chef        | Generate dependency recipe                |
| 3. builder | chef        | Compile dependencies + petty-mcp          |
| 4. fetcher | debian:slim | Download Firecracker, jailer, kernel      |
| 5. runtime | debian:slim | Minimal production image (~150MB)         |

### Why Docker Instead of Building on EC2?

1. **Reproducible builds** - Same image on dev/staging/prod
2. **Faster deployment** - No 5-10 minute Rust compilation on boot
3. **Portable** - Image can run anywhere (EC2, ECS, local)
4. **Cached layers** - Dependency layer cached, only app code rebuilds
5. **Industry standard** - Follows Rust Docker best practices

> **Note**: All binaries (Firecracker, jailer, kernel) are baked INTO the Docker image during build. Only the rootfs is fetched from S3 at container startup.

---

## MCP Server Transport Modes

**petty-mcp now supports dual transport** via `rmcp` SDK:

| Mode               | Env Value | stdio | HTTP | Use Case               |
| ------------------ | --------- | ----- | ---- | ---------------------- |
| **Both (default)** | `both`    | ✅    | ✅   | Maximum compatibility  |
| Stdio only         | `stdio`   | ✅    | ❌   | Claude Desktop, Cursor |
| HTTP only          | `http`    | ❌    | ✅   | Remote AI agents only  |

### HTTP/SSE Endpoints

| Endpoint  | Method | Description                    |
| --------- | ------ | ------------------------------ |
| `/health` | GET    | Health check (JSON)            |
| `/mcp`    | POST   | JSON-RPC requests              |
| `/mcp`    | GET    | SSE stream for server messages |
| `/`       | GET    | Server info page (HTML)        |

### Example Request

```bash
# List available tools
curl -X POST http://localhost:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Health check
curl http://localhost:8080/health
```

### Systemd with HTTP Transport

Systemd is still the right choice because:

1. **Process management**: Auto-restart on crash
2. **Logging**: Journald captures stdout/stderr
3. **Boot startup**: Service starts on instance boot
4. **Environment**: Clean env var handling

The HTTP server runs as part of the petty-mcp process - no separate web server needed.

---

## Environment Variables

**Already implemented in `petty-mcp`** (see `crates/petty-mcp/src/config.rs`):

| Variable               | Default                      | Description                      |
| ---------------------- | ---------------------------- | -------------------------------- |
| `PETTY_KERNEL`         | `/var/lib/petty/vmlinux`     | Path to kernel image             |
| `PETTY_ROOTFS`         | `/var/lib/petty/debian.ext4` | Path to rootfs image             |
| `PETTY_FIRECRACKER`    | `/usr/bin/firecracker`       | Path to Firecracker binary       |
| `PETTY_CHROOT`         | `/tmp/petty`                 | Working directory for VMs        |
| `PETTY_POOL_ENABLED`   | `true`                       | Enable warm sandbox pool         |
| `PETTY_POOL_MIN_SIZE`  | `3`                          | Minimum warm sandboxes           |
| `PETTY_POOL_MAX_BOOTS` | `2`                          | Max concurrent pool boots        |
| `PETTY_TRANSPORT`      | `both`                       | Transport mode (stdio/http/both) |
| `PETTY_HTTP_HOST`      | `0.0.0.0`                    | HTTP bind address                |
| `PETTY_HTTP_PORT`      | `8080`                       | HTTP port                        |
| `RUST_LOG`             | -                            | Logging level (e.g., `info`)     |

---

## Implementation Tasks

### Task 1: Terraform - VPC & Security

- VPC with public subnet
- Security group:
  - SSH (22) - management access
  - HTTP (8080) - MCP server for remote AI agents
- Internet gateway for outbound

### Task 2: Terraform - S3 Bucket (Separate/Optional)

> **Note**: You've already created your S3 bucket manually. This Terraform is optional, kept for reference.

- Bucket for rootfs image
- IAM policy for EC2 read access

### Task 3: Terraform - Spot Instance

- c5.metal spot request
- IAM instance profile (S3 read access)
- User-data script for bootstrap
- Root volume: 50GB gp3

### Task 4: Bootstrap Script (user-data.sh)

See [Bootstrap Script](#bootstrap-script-user-datash) section below.

### Task 5: Systemd Service

See [Systemd Service](#systemd-service) section below.

---

## File Structure

```

terraform/
├── main.tf # Provider config
├── variables.tf # Inputs (region, key, bucket)
├── outputs.tf # Instance IP
├── vpc.tf # Network
├── ec2.tf # Spot instance
├── iam.tf # Roles (S3 read access)
└── scripts/
├── user-data.sh # Bootstrap
└── petty-mcp.service # Systemd unit

# Optional (separate apply)

terraform/s3/
├── main.tf # S3 bucket
└── outputs.tf # Bucket name

```

---

## Bootstrap Script (user-data.sh)

```bash
#!/bin/bash
set -euo pipefail

# ============================================================================
# CONFIGURATION
# ============================================================================
S3_BUCKET="${S3_BUCKET:-petty-artifacts}"
DOCKER_IMAGE="${DOCKER_IMAGE:-ghcr.io/vrn21/petty-mcp:latest}"

# ============================================================================
# SYSTEM SETUP
# ============================================================================
echo ">>> Updating system..."
apt-get update
apt-get install -y --no-install-recommends \
    awscli \
    docker.io \
    curl

# ============================================================================
# FIX KVM PERMISSIONS
# ============================================================================
echo ">>> Setting up /dev/kvm..."
if [ -e /dev/kvm ]; then
    chmod 666 /dev/kvm
    echo "KVM permissions set"
else
    echo "WARNING: /dev/kvm not found - not a bare metal instance?"
fi

# ============================================================================
# ENABLE DOCKER
# ============================================================================
echo ">>> Starting Docker..."
systemctl enable docker
systemctl start docker

# ============================================================================
# PULL DOCKER IMAGE
# ============================================================================
echo ">>> Pulling Docker image: $DOCKER_IMAGE"
docker pull "$DOCKER_IMAGE"

# ============================================================================
# CREATE SYSTEMD SERVICE FOR DOCKER CONTAINER
# ============================================================================
echo ">>> Installing systemd service..."
cat > /etc/systemd/system/petty-mcp.service << EOF
[Unit]
Description=Petty MCP Server (Docker)
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
Restart=on-failure
RestartSec=5

# Pull latest image before starting (optional, remove for faster restarts)
ExecStartPre=/usr/bin/docker pull $DOCKER_IMAGE

# Run the container
ExecStart=/usr/bin/docker run --rm --name petty-mcp \\
    --privileged \\
    --device=/dev/kvm \\
    -p 8080:8080 \\
    -e PETTY_ROOTFS_S3_URL=s3://${S3_BUCKET}/debian-devbox.ext4 \\
    -e PETTY_TRANSPORT=both \\
    -e PETTY_HTTP_HOST=0.0.0.0 \\
    -e PETTY_HTTP_PORT=8080 \\
    -e RUST_LOG=info \\
    $DOCKER_IMAGE

# Stop the container gracefully
ExecStop=/usr/bin/docker stop petty-mcp

[Install]
WantedBy=multi-user.target
EOF

# ============================================================================
# ENABLE AND START SERVICE
# ============================================================================
echo ">>> Starting petty-mcp service..."
systemctl daemon-reload
systemctl enable petty-mcp.service
systemctl start petty-mcp.service

echo ">>> Bootstrap complete!"
echo "Service status: $(systemctl is-active petty-mcp.service)"
echo ""
echo "Container running - no Rust compilation needed!"
echo "First start fetches rootfs from S3, subsequent starts are instant."
```

---

## Systemd Service (Docker)

The bootstrap script creates a systemd service that runs the Docker container. Key features:

- Pulls latest image on restart (optional)
- Runs container with `--privileged` for KVM access
- Exposes port 8080
- Fetches rootfs from S3 on first run

### Why Systemd + Docker?

1. **Process supervision**: Auto-restart container on crash
2. **Clean separation**: App in container, management on host
3. **Easy updates**: Just `docker pull` and restart
4. **Boot startup**: Service starts on instance boot

---

## MCP Server Access Patterns

### Pattern 1: SSH + Manual Run (Dev/Testing)

```bash
# SSH in and run petty-mcp interactively
ssh -i key.pem ubuntu@<ip>
/usr/local/bin/petty-mcp
```

### Pattern 2: SSH Tunnel (Remote Client)

```bash
# From your local machine, proxy stdio over SSH
ssh -i key.pem ubuntu@<ip> /usr/local/bin/petty-mcp
```

MCP clients can spawn this SSH command as their "server process".

### Pattern 3: Systemd Background (Always Running)

Systemd keeps petty-mcp running. Useful when:

- You want warm pools pre-created
- Multiple SSH sessions need to share state

---

## S3 Bucket Structure

```text
s3://your-bucket/
└── debian-devbox.ext4      # Rootfs image (~1.5-2GB)
```

That's it! Only the rootfs needs to be in S3.

### Manual Upload Steps

```bash
# 1. Build rootfs (works on macOS via Docker)
make rootfs ARCH=x86_64

# 2. Upload to your existing S3 bucket
aws s3 cp images/output/debian-devbox.ext4 s3://your-bucket/
```

---

## Docker Build Commands

```bash
# Build the image locally
docker build -f Dockerfile.server -t petty-mcp:latest .

# Build for x86_64 (from ARM Mac)
docker build --platform linux/amd64 -f Dockerfile.server -t petty-mcp:latest .

# Tag for registry
docker tag petty-mcp:latest ghcr.io/vrn21/petty-mcp:latest

# Push to registry
docker push ghcr.io/vrn21/petty-mcp:latest

# Run locally for testing (requires /dev/kvm on Linux)
docker run --privileged --device=/dev/kvm -p 8080:8080 \
  -e PETTY_ROOTFS=/path/to/rootfs.ext4 \
  petty-mcp:latest
```

---

## Quick Commands

```bash
# Build Docker image
docker build -f Dockerfile.server -t petty-mcp:latest .

# Deploy infrastructure
terraform apply -var="ssh_key_name=your-key" -var="s3_bucket=your-bucket"

# SSH
ssh -i ~/.ssh/key.pem ubuntu@$(terraform output -raw public_ip)

# Check service
sudo systemctl status petty-mcp
sudo journalctl -u petty-mcp -f

# View logs
tail -f /var/log/petty/mcp.log

# Manual stop (saves money)
aws ec2 stop-instances --instance-ids $(terraform output -raw instance_id)

# Manual start
aws ec2 start-instances --instance-ids $(terraform output -raw instance_id)

# Destroy
terraform destroy
```

---

## Variables

| Name           | Required | Default   | Description            |
| -------------- | -------- | --------- | ---------------------- |
| `ssh_key_name` | Yes      | -         | EC2 key pair name      |
| `region`       | No       | us-east-1 | AWS region             |
| `spot_price`   | No       | 1.50      | Max spot bid           |
| `s3_bucket`    | Yes      | -         | Your S3 bucket name    |
| `petty_repo`   | No       | (default) | Git repo URL for petty |
| `petty_branch` | No       | main      | Git branch to build    |

---

## Clarifications

### Q: Is Firecracker downloaded for every microVM?

**No!** Firecracker is downloaded **once** during EC2 bootstrap. Here's the flow:

```text
Bootstrap (once):
  curl → /usr/local/bin/firecracker

Running microVMs (many times):
  petty-mcp → fork() → /usr/local/bin/firecracker (reuses binary)
                    → /usr/local/bin/firecracker (reuses binary)
                    → /usr/local/bin/firecracker (reuses binary)
```

Each `Firecracker` process is a new fork, but uses the same binary on disk. No network downloads during VM creation.

### Q: What about the kernel and rootfs?

Same principle:

- **Kernel**: Downloaded once to `/var/lib/petty/vmlinux`
- **Rootfs**: Downloaded once to `/var/lib/petty/debian-devbox.ext4`

Each VM uses Copy-on-Write (COW) overlay on the rootfs, so changes in one VM don't affect others.

---

## Potential Issues & Mitigations

### 1. Long First Boot Time

**Issue**: First boot takes ~5-10 minutes due to Rust compilation.

**Mitigations**:

- **Option A**: Create a custom AMI after first successful boot (bake the compiled binary)
- **Option B**: Accept first-boot delay, subsequent stop/start is instant

### 2. GitHub Rate Limiting

**Issue**: Firecracker downloads from GitHub may be rate-limited (~60/hour).

**Mitigation**: Only affects first boot. If needed, mirror to your S3.

### 3. Kernel Compatibility

**Issue**: AWS public kernel may not match rootfs expectations.

**Mitigation**: AWS kernels are well-tested with Firecracker. If issues arise, build a custom kernel.

### 4. petty-mcp Environment Variables ✅

**Already implemented!** The `petty-mcp` server reads from environment via `PettyConfig::from_env()`.

No code changes needed - just set the environment variables in systemd unit.

---

## Acceptance Criteria

- [ ] Terraform provisions c5.metal spot
- [ ] Firecracker downloads from GitHub on first boot
- [ ] Kernel downloads from AWS S3 on first boot
- [ ] Rootfs downloads from your S3 on first boot
- [ ] petty-mcp builds from source on first boot
- [ ] petty-mcp service auto-starts
- [ ] SSH access works
- [ ] Manual stop/start works
- [ ] Environment variables configure all paths

---

## Future Enhancements

- [ ] AMI baking (pre-compile petty-mcp for instant boot)
- [ ] Automated image building (EC2 runs `make rootfs`)
- [x] HTTP transport for remote MCP access ✅ (implemented)
- [x] Health endpoint in petty-mcp ✅ (implemented)
- [ ] CloudWatch metrics and alarms
- [ ] Spot interruption handling (graceful shutdown)
