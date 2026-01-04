# Self-Hosting Guide

Deploy Bouvet on your own infrastructure for secure, isolated code execution sandboxes.

---

## Overview

Bouvet requires a bare-metal machine with KVM support to run Firecracker microVMs. This guide covers two deployment options:

| Method                                      | Best For                             | Complexity |
| ------------------------------------------- | ------------------------------------ | ---------- |
| [AWS Terraform](#aws-deployment)            | Production deployments               | Medium     |
| [Manual Installation](#manual-installation) | Custom infrastructure, local servers | Low        |

---

## Prerequisites

### Hardware Requirements

Bouvet uses Firecracker microVMs which require **hardware virtualization (KVM)**:

| Requirement | Specification                                 |
| ----------- | --------------------------------------------- |
| **CPU**     | x86_64 or ARM64 with VT-x/AMD-V or equivalent |
| **RAM**     | 4GB minimum (each sandbox uses ~256MB)        |
| **Disk**    | 30GB+ for rootfs images and Docker            |
| **KVM**     | `/dev/kvm` must be available                  |

> [!IMPORTANT]
> Cloud instances must be bare-metal (e.g., AWS `c5.metal`, `m5.metal`) — standard VMs don't expose `/dev/kvm`.

### Software Requirements

- Docker 20.10+
- AWS CLI (for Terraform deployment)
- Terraform 1.0+ (for Terraform deployment)

---

## AWS Deployment

The included Terraform configuration deploys Bouvet on an AWS EC2 bare-metal instance.

### What Gets Created

| Resource          | Description                           |
| ----------------- | ------------------------------------- |
| VPC + Subnet      | Isolated network with internet access |
| c5.metal Instance | Bare-metal EC2 with KVM support       |
| Security Group    | SSH (22) + MCP endpoint (8080)        |
| Elastic IP        | Static public IP address              |
| Systemd Service   | Auto-starts/restarts bouvet-mcp       |

### Quick Start

```bash
cd terraform

# Initialize Terraform
terraform init

# Review the plan (replace with your SSH key name)
terraform plan -var="ssh_key_name=my-key"

# Deploy
terraform apply -var="ssh_key_name=my-key"
```

### Configuration Variables

| Variable            | Description                          | Default                           |
| ------------------- | ------------------------------------ | --------------------------------- |
| `ssh_key_name`      | **Required.** AWS EC2 key pair name  | —                                 |
| `aws_region`        | AWS region                           | `us-east-1`                       |
| `availability_zone` | Availability zone                    | `us-east-1a`                      |
| `instance_type`     | EC2 instance type (must be `.metal`) | `c5.metal`                        |
| `docker_image`      | Docker image for bouvet-mcp          | `ghcr.io/vrn21/bouvet-mcp:latest` |
| `rootfs_url`        | Public URL for rootfs download       | S3-hosted image                   |
| `allowed_ssh_cidrs` | CIDR blocks allowed for SSH          | `["0.0.0.0/0"]`                   |
| `volume_size`       | Root EBS volume size (GB)            | `50`                              |
| `environment`       | Environment tag                      | `production`                      |

Example with custom configuration:

```bash
terraform apply \
  -var="ssh_key_name=my-key" \
  -var="aws_region=eu-west-1" \
  -var="allowed_ssh_cidrs=[\"203.0.113.0/24\"]" \
  -var="volume_size=100"
```

### Verification

After deployment (~5 minutes for first boot):

```bash
# Get the public IP
PUBLIC_IP=$(terraform output -raw public_ip)

# Check health endpoint
curl -f http://$PUBLIC_IP:8080/health

# Test MCP endpoint
curl -X POST http://$PUBLIC_IP:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# SSH for debugging
ssh -i ~/.ssh/my-key.pem admin@$PUBLIC_IP
sudo journalctl -u bouvet-mcp -f
```

### Outputs

After `terraform apply`:

| Output         | Description              |
| -------------- | ------------------------ |
| `public_ip`    | Elastic IP address       |
| `mcp_endpoint` | Full MCP endpoint URL    |
| `health_url`   | Health check URL         |
| `ssh_command`  | Ready-to-use SSH command |

### Clean Up

```bash
terraform destroy -var="ssh_key_name=my-key"
```

---

## Manual Installation

For custom infrastructure or local bare-metal servers.

### 1. Verify KVM Support

```bash
# Check KVM is available
ls -la /dev/kvm

# If not present, load the module
sudo modprobe kvm
sudo modprobe kvm_intel  # or kvm_amd

# Make KVM accessible
sudo chmod 666 /dev/kvm

# Persistent permission (create udev rule)
echo 'KERNEL=="kvm", MODE="0666"' | sudo tee /etc/udev/rules.d/99-kvm.rules
```

### 2. Install Docker

```bash
# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y docker.io
sudo systemctl enable docker
sudo systemctl start docker
```

### 3. Run Bouvet

```bash
docker run -d \
  --name bouvet-mcp \
  --restart=unless-stopped \
  --privileged \
  --device=/dev/kvm \
  -p 8080:8080 \
  -e BOUVET_TRANSPORT=http \
  -e BOUVET_HTTP_HOST=0.0.0.0 \
  -e BOUVET_HTTP_PORT=8080 \
  -e RUST_LOG=info \
  ghcr.io/vrn21/bouvet-mcp:latest
```

The container will automatically download the rootfs image on first startup (~2GB).

### 4. Verify Installation

```bash
# Check container is running
docker logs -f bouvet-mcp

# Test health endpoint
curl http://localhost:8080/health

# Test MCP
curl -X POST http://localhost:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## Using a Custom Rootfs

Build your own rootfs image with custom tools:

```bash
# Clone the repository
git clone https://github.com/vrn21/bouvet.git
cd bouvet

# Build rootfs (runs in Docker, works on macOS)
make rootfs

# The image is created at:
# images/output/debian-devbox.ext4
```

To use your custom rootfs:

**Option 1: Mount as volume**

```bash
docker run -d \
  --privileged \
  --device=/dev/kvm \
  -v /path/to/your/rootfs.ext4:/var/lib/bouvet/debian-devbox.ext4 \
  -p 8080:8080 \
  ghcr.io/vrn21/bouvet-mcp:latest
```

**Option 2: Host on S3 or HTTP**

```bash
docker run -d \
  --privileged \
  --device=/dev/kvm \
  -e BOUVET_ROOTFS_URL=https://your-bucket.s3.amazonaws.com/rootfs.ext4 \
  -p 8080:8080 \
  ghcr.io/vrn21/bouvet-mcp:latest
```

### Building for Different Architectures

```bash
# ARM64 (Apple Silicon, Graviton)
make rootfs ARCH=aarch64

# x86_64 (Intel/AMD)
make rootfs ARCH=x86_64
```

---

## Systemd Service (Non-Docker)

For production deployments without Docker:

### 1. Download Required Binaries

```bash
# Create directories
sudo mkdir -p /var/lib/bouvet /usr/local/bin /tmp/bouvet

# Download Firecracker (adjust version/arch as needed)
FC_VERSION=1.5.1
ARCH=x86_64
curl -sSL https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/firecracker-v${FC_VERSION}-${ARCH}.tgz | \
  sudo tar -xz -C /tmp
sudo mv /tmp/release-v${FC_VERSION}-${ARCH}/firecracker-* /usr/local/bin/firecracker
sudo mv /tmp/release-v${FC_VERSION}-${ARCH}/jailer-* /usr/local/bin/jailer

# Download kernel
curl -sSL https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/${ARCH}/kernels/vmlinux.bin | \
  sudo tee /var/lib/bouvet/vmlinux > /dev/null

# Download or build rootfs
curl -sSL https://your-bucket.s3.amazonaws.com/debian-devbox.ext4 | \
  sudo tee /var/lib/bouvet/debian-devbox.ext4 > /dev/null

# Build bouvet-mcp from source
cargo build --release -p bouvet-mcp
sudo cp target/release/bouvet-mcp /usr/local/bin/
```

### 2. Create Systemd Service

```bash
sudo tee /etc/systemd/system/bouvet-mcp.service << 'EOF'
[Unit]
Description=Bouvet MCP Server
After=network.target

[Service]
Type=simple
Restart=on-failure
RestartSec=10

Environment=BOUVET_KERNEL=/var/lib/bouvet/vmlinux
Environment=BOUVET_ROOTFS=/var/lib/bouvet/debian-devbox.ext4
Environment=BOUVET_FIRECRACKER=/usr/local/bin/firecracker
Environment=BOUVET_CHROOT=/tmp/bouvet
Environment=BOUVET_TRANSPORT=http
Environment=BOUVET_HTTP_HOST=0.0.0.0
Environment=BOUVET_HTTP_PORT=8080
Environment=RUST_LOG=info

ExecStart=/usr/local/bin/bouvet-mcp

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable bouvet-mcp
sudo systemctl start bouvet-mcp
```

---

## Hosting Artifacts on S3

The `terraform/s3/` module creates a public S3 bucket for hosting rootfs images.

```bash
cd terraform/s3

terraform init
terraform apply -var="bucket_name=my-bouvet-artifacts"

# Upload your rootfs
aws s3 cp images/output/debian-devbox.ext4 s3://my-bouvet-artifacts/

# Your rootfs URL will be:
# https://my-bouvet-artifacts.s3.us-east-1.amazonaws.com/debian-devbox.ext4
```

---

## Security Considerations

### Network Security

- **Restrict SSH Access**: Set `allowed_ssh_cidrs` to your IP range
- **Use HTTPS**: Place a reverse proxy (nginx, Caddy) with TLS in front of port 8080
- **VPC Peering**: For internal-only access, keep the MCP endpoint private

### Container Security

The container runs as root to access `/dev/kvm`, but:

- Sandboxes are fully isolated microVMs (separate kernel, filesystem)
- No container escape possible through sandboxed code
- Each sandbox is destroyed after use

### Firewall Rules

```bash
# Allow only MCP endpoint and SSH
sudo ufw allow 22/tcp
sudo ufw allow 8080/tcp
sudo ufw enable
```

---

## Troubleshooting

### KVM Not Available

```
ERROR: /dev/kvm not found
```

**Solution**: Use a bare-metal instance or enable nested virtualization:

- AWS: Use `.metal` instance types (`c5.metal`, `m5.metal`, etc.)
- GCP: Enable nested virtualization on the VM
- Local: Load KVM modules (`modprobe kvm kvm_intel`)

### Container Exits Immediately

Check logs:

```bash
docker logs bouvet-mcp
```

Common causes:

- Missing `/dev/kvm` — add `--device=/dev/kvm` or `--privileged`
- Insufficient disk space — ensure 3GB+ free for rootfs download
- Port already in use — change `BOUVET_HTTP_PORT`

### Sandbox Creation Fails

```bash
# Check Firecracker can start
sudo /usr/local/bin/firecracker --version

# Check KVM permissions
ls -la /dev/kvm
# Should show: crw-rw-rw- ... /dev/kvm
```

### Health Check Fails

```bash
# Verify the service is running
curl -v http://localhost:8080/health

# Check container/service logs
docker logs bouvet-mcp
# or
sudo journalctl -u bouvet-mcp -f
```

---

## Cost Estimates (AWS)

| Instance  | On-Demand | Spot (typical) | Notes                 |
| --------- | --------- | -------------- | --------------------- |
| c5.metal  | ~$4.08/hr | ~$0.80/hr      | 96 vCPUs, 192GB RAM   |
| m5.metal  | ~$4.61/hr | ~$0.92/hr      | 96 vCPUs, 384GB RAM   |
| c6i.metal | ~$4.18/hr | ~$0.84/hr      | Latest gen, 128 vCPUs |

> [!TIP]
> Use Spot Instances for development and testing — up to 80% savings.

---

## Next Steps

- **[Configuration Reference](CONFIG.md)** — All environment variables and options
- **[Architecture](ARCHITECTURE.md)** — Technical deep dive into Bouvet internals
