# Phase 7: AWS Deployment (Refined v2)

> Deploying Petty on c5.metal Spot Instance with build-from-source approach.

---

## Overview

This document outlines a streamlined deployment strategy:

- **Download from source**: Firecracker, jailer, and kernel directly from AWS/GitHub
- **S3 for rootfs only**: Only the ext4 image needs to be in your S3
- **Build petty-mcp on EC2**: Compile from source during bootstrap (no binary uploads)
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
│  │  /usr/local/bin/                                      │    │
│  │  ├── firecracker  ← Downloaded from GitHub            │    │
│  │  ├── jailer       ← Downloaded from GitHub            │    │
│  │  └── petty-mcp    ← Built from source (cargo build)   │    │
│  │                                                       │    │
│  │  /var/lib/petty/                                      │    │
│  │  ├── vmlinux              ← Downloaded from AWS S3    │    │
│  │  └── debian-devbox.ext4   ← Downloaded from your S3   │    │
│  │                                                       │    │
│  │  /opt/petty/              ← Git clone of repo         │    │
│  │                                                       │    │
│  │  ┌─────────────────────────────────────────────────┐ │    │
│  │  │                 Petty Stack                      │ │    │
│  │  │  ┌───────────┐  ┌───────────┐  ┌─────────────┐ │ │    │
│  │  │  │ petty-mcp │──│ petty-core│──│ Firecracker │ │ │    │
│  │  │  │  (stdio)  │  └───────────┘  └─────────────┘ │ │    │
│  │  │  └───────────┘                                  │ │    │
│  │  └─────────────────────────────────────────────────┘ │    │
│  │                                                       │    │
│  │  Security Group: SSH (22)                             │    │
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

## Binary Sources

| Binary               | Source          | Download Method                 |
| -------------------- | --------------- | ------------------------------- |
| `firecracker`        | GitHub Releases | Direct curl (once at bootstrap) |
| `jailer`             | GitHub Releases | Direct curl (once at bootstrap) |
| `vmlinux.bin`        | AWS S3 (public) | Direct curl (once at bootstrap) |
| `debian-devbox.ext4` | Your S3 bucket  | aws s3 cp (once at bootstrap)   |
| `petty-mcp`          | Git repo        | cargo build --release           |

> **Note**: Firecracker is downloaded **once** during EC2 bootstrap. All microVMs reuse this single binary. Each VM spawn just forks a new Firecracker process - no re-downloading.

---

## MCP Server Deployment: Research Summary

### How OSS MCP Servers Run

Based on research, there are two primary deployment patterns:

| Pattern              | Transport           | Use Case                                   | Pros                       | Cons                       |
| -------------------- | ------------------- | ------------------------------------------ | -------------------------- | -------------------------- |
| **stdio subprocess** | stdin/stdout        | Local integration (Claude Desktop, Cursor) | Simple, no network, secure | Client must spawn process  |
| **HTTP server**      | Streamable HTTP/SSE | Remote/networked access                    | Scalable, accessible       | Needs security (TLS, auth) |

### Current petty-mcp Design

Your `petty-mcp` uses **stdio transport** (via `rmcp::transport::stdio`). This is the standard for:

- Claude Desktop integration
- Cursor IDE
- Local AI agent tools

### Recommendation: Keep Systemd for stdio

For stdio-based MCP servers, **systemd is appropriate** because:

1. **Process management**: Auto-restart on crash
2. **Logging**: Journald integration
3. **Environment**: Clean environment variable handling
4. **Boot startup**: Service starts on instance boot

However, for **remote access**, you have options:

#### Option A: SSH + stdio (Recommended for Dev)

Clients SSH into the instance and spawn petty-mcp as a subprocess:

```bash
# Client connects via SSH and runs MCP
ssh -i key.pem ubuntu@<ip> /usr/local/bin/petty-mcp
```

This keeps the stdio pattern and adds SSH security.

#### Option B: Add HTTP Transport (Future)

For true remote access, add Streamable HTTP transport to petty-mcp:

```rust
// Future: Add HTTP endpoint alongside stdio
use rmcp::transport::streamable_http;
```

This would allow direct HTTP connections without SSH.

---

## Environment Variables

**Already implemented in `petty-mcp`** (see `crates/petty-mcp/src/config.rs`):

| Variable               | Default                      | Description                  |
| ---------------------- | ---------------------------- | ---------------------------- |
| `PETTY_KERNEL`         | `/var/lib/petty/vmlinux`     | Path to kernel image         |
| `PETTY_ROOTFS`         | `/var/lib/petty/debian.ext4` | Path to rootfs image         |
| `PETTY_FIRECRACKER`    | `/usr/bin/firecracker`       | Path to Firecracker binary   |
| `PETTY_CHROOT`         | `/tmp/petty`                 | Working directory for VMs    |
| `PETTY_POOL_ENABLED`   | `true`                       | Enable warm sandbox pool     |
| `PETTY_POOL_MIN_SIZE`  | `3`                          | Minimum warm sandboxes       |
| `PETTY_POOL_MAX_BOOTS` | `2`                          | Max concurrent pool boots    |
| `RUST_LOG`             | -                            | Logging level (e.g., `info`) |

---

## Implementation Tasks

### Task 1: Terraform - VPC & Security

- VPC with public subnet
- Security group: SSH (22) only
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
├── main.tf           # Provider config
├── variables.tf      # Inputs (region, key, bucket)
├── outputs.tf        # Instance IP
├── vpc.tf            # Network
├── ec2.tf            # Spot instance
├── iam.tf            # Roles (S3 read access)
└── scripts/
    ├── user-data.sh  # Bootstrap
    └── petty-mcp.service # Systemd unit

# Optional (separate apply)
terraform/s3/
├── main.tf           # S3 bucket
└── outputs.tf        # Bucket name
```

---

## Bootstrap Script (user-data.sh)

```bash
#!/bin/bash
set -euo pipefail

# ============================================================================
# CONFIGURATION
# ============================================================================
FC_VERSION="1.5.1"
ARCH=$(uname -m)
S3_BUCKET="${S3_BUCKET:-petty-artifacts}"
PETTY_REPO="https://github.com/YOUR_USERNAME/petty.git"
PETTY_BRANCH="main"

# ============================================================================
# SYSTEM SETUP
# ============================================================================
echo ">>> Updating system..."
apt-get update
apt-get install -y --no-install-recommends \
    awscli \
    curl \
    git \
    build-essential \
    pkg-config \
    libssl-dev

# ============================================================================
# INSTALL RUST
# ============================================================================
echo ">>> Installing Rust..."
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# ============================================================================
# CREATE DIRECTORIES
# ============================================================================
echo ">>> Creating directories..."
mkdir -p /var/lib/petty
mkdir -p /tmp/petty
mkdir -p /var/log/petty
mkdir -p /opt/petty

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
# DOWNLOAD FIRECRACKER FROM GITHUB (ONCE)
# ============================================================================
echo ">>> Downloading Firecracker v${FC_VERSION}..."
curl -sSL -o /usr/local/bin/firecracker \
    "https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/firecracker-v${FC_VERSION}-${ARCH}"
chmod +x /usr/local/bin/firecracker

echo ">>> Downloading Jailer v${FC_VERSION}..."
curl -sSL -o /usr/local/bin/jailer \
    "https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/jailer-v${FC_VERSION}-${ARCH}"
chmod +x /usr/local/bin/jailer

# ============================================================================
# DOWNLOAD KERNEL FROM AWS PUBLIC S3 (ONCE)
# ============================================================================
echo ">>> Downloading kernel..."
if [ "$ARCH" = "x86_64" ]; then
    KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux.bin"
else
    KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/aarch64/kernels/vmlinux.bin"
fi
curl -sSL -o /var/lib/petty/vmlinux "$KERNEL_URL"

# ============================================================================
# DOWNLOAD ROOTFS FROM YOUR S3 (ONCE)
# ============================================================================
echo ">>> Downloading rootfs from S3..."
aws s3 cp "s3://${S3_BUCKET}/debian-devbox.ext4" /var/lib/petty/debian-devbox.ext4

# ============================================================================
# CLONE AND BUILD PETTY-MCP FROM SOURCE
# ============================================================================
echo ">>> Cloning petty repository..."
git clone --branch "$PETTY_BRANCH" "$PETTY_REPO" /opt/petty

echo ">>> Building petty-mcp (this may take a few minutes)..."
cd /opt/petty
cargo build --release -p petty-mcp

echo ">>> Installing petty-mcp..."
cp target/release/petty-mcp /usr/local/bin/petty-mcp
chmod +x /usr/local/bin/petty-mcp

# ============================================================================
# VERIFY DOWNLOADS
# ============================================================================
echo ">>> Verifying binaries..."
/usr/local/bin/firecracker --version
/usr/local/bin/petty-mcp --help 2>&1 || echo "(petty-mcp may not have --help)"

# ============================================================================
# INSTALL SYSTEMD SERVICE
# ============================================================================
echo ">>> Installing systemd service..."
cat > /etc/systemd/system/petty-mcp.service << 'EOF'
[Unit]
Description=Petty MCP Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/petty-mcp
Restart=on-failure
RestartSec=5

# Environment configuration (matches petty-mcp config.rs)
Environment=PETTY_KERNEL=/var/lib/petty/vmlinux
Environment=PETTY_ROOTFS=/var/lib/petty/debian-devbox.ext4
Environment=PETTY_FIRECRACKER=/usr/local/bin/firecracker
Environment=PETTY_CHROOT=/tmp/petty
Environment=PETTY_POOL_ENABLED=true
Environment=PETTY_POOL_MIN_SIZE=3
Environment=RUST_LOG=info

# Logging (minimal)
StandardOutput=append:/var/log/petty/mcp.log
StandardError=append:/var/log/petty/mcp.log

# Security (need root for /dev/kvm access)
NoNewPrivileges=false

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
echo "Build time: Rust compilation took a while, but this is a ONE-TIME cost."
echo "Future instance restarts will be instant (binary already compiled)."
```

---

## Systemd Service

File: `/etc/systemd/system/petty-mcp.service`

```ini
[Unit]
Description=Petty MCP Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/petty-mcp
Restart=on-failure
RestartSec=5

# Environment configuration (matches petty-mcp config.rs)
Environment=PETTY_KERNEL=/var/lib/petty/vmlinux
Environment=PETTY_ROOTFS=/var/lib/petty/debian-devbox.ext4
Environment=PETTY_FIRECRACKER=/usr/local/bin/firecracker
Environment=PETTY_CHROOT=/tmp/petty
Environment=PETTY_POOL_ENABLED=true
Environment=PETTY_POOL_MIN_SIZE=3
Environment=RUST_LOG=info

# Logging (minimal)
StandardOutput=append:/var/log/petty/mcp.log
StandardError=append:/var/log/petty/mcp.log

# Security (need root for /dev/kvm access)
NoNewPrivileges=false

[Install]
WantedBy=multi-user.target
```

### Why Systemd?

Research confirms systemd is the standard approach for MCP servers in production:

1. **Process supervision**: Auto-restart on crash
2. **Environment isolation**: Clean env var handling
3. **Logging integration**: Journald captures stdout/stderr
4. **Boot startup**: Service auto-starts on instance boot
5. **Resource limits**: Can add memory/CPU limits if needed

For stdio-based MCP servers specifically, systemd keeps the process running and clients (via SSH) can connect to it.

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

## Quick Commands

```bash
# Deploy
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
- [ ] HTTP transport for remote MCP access
- [ ] Health endpoint in petty-mcp
- [ ] CloudWatch metrics and alarms
- [ ] Spot interruption handling (graceful shutdown)
