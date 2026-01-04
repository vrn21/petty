# Terraform Infrastructure Requirements

> Design document for deploying petty-mcp on AWS EC2.
> **Target**: Another agent will implement this specification.

---

## Overview

Deploy the `petty-mcp` Docker container on an AWS c5.metal on-demand instance with:

- Debian 12 (Bookworm) base AMI
- VPC with public subnet
- Docker for container runtime
- Systemd for service management
- Elastic IP for static addressing

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         AWS Region (us-east-1)                   │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    VPC: 10.0.0.0/16                         │ │
│  │                    DNS Hostnames: enabled                   │ │
│  │                                                              │ │
│  │  ┌────────────────────────────────────────────────────────┐ │ │
│  │  │         Public Subnet: 10.0.1.0/24                      │ │ │
│  │  │         AZ: us-east-1a                                  │ │ │
│  │  │                                                          │ │ │
│  │  │  ┌───────────────────────────────────────────────────┐  │ │ │
│  │  │  │           EC2: c5.metal (On-Demand)                │  │ │ │
│  │  │  │                                                    │  │ │ │
│  │  │  │   OS: Debian 12 (Bookworm)                         │  │ │ │
│  │  │  │   Specs: 96 vCPUs, 192GB RAM, /dev/kvm             │  │ │ │
│  │  │  │                                                    │  │ │ │
│  │  │  │   ┌────────────────────────────────────────────┐   │  │ │ │
│  │  │  │   │  Docker Container: petty-mcp               │   │  │ │ │
│  │  │  │   │  ├── petty-mcp server                      │   │  │ │ │
│  │  │  │   │  ├── Firecracker v1.5.0                    │   │  │ │ │
│  │  │  │   │  ├── vmlinux kernel                        │   │  │ │ │
│  │  │  │   │  └── rootfs (downloaded at startup)        │   │  │ │ │
│  │  │  │   └────────────────────────────────────────────┘   │  │ │ │
│  │  │  │                                                    │  │ │ │
│  │  │  │   Ports: :22 (SSH), :8080 (HTTP/MCP)               │  │ │ │
│  │  │  │   Volume: 50GB gp3 (encrypted)                     │  │ │ │
│  │  │  └───────────────────────────────────────────────────┘  │ │ │
│  │  │                          │                               │ │ │
│  │  │                    Elastic IP                            │ │ │
│  │  └──────────────────────────┼───────────────────────────────┘ │ │
│  │                             │                                 │ │
│  │                      Internet Gateway                         │ │
│  └─────────────────────────────┼─────────────────────────────────┘ │
│                                │                                   │
└────────────────────────────────┼───────────────────────────────────┘
                                 │
                            Internet
```

---

## Required Files

Create the following files in `/terraform/`:

```
terraform/
├── main.tf              # Provider configuration
├── variables.tf         # Input variables
├── outputs.tf           # Output values
├── vpc.tf               # VPC, subnet, IGW, routes
├── security.tf          # Security group
├── ec2.tf               # Instance, EIP
├── data.tf              # AMI lookup
├── scripts/
│   └── user-data.sh     # Bootstrap script (templatefile)
│
└── s3/                  # OPTIONAL - separate module
    ├── main.tf          # S3 bucket with public read
    ├── variables.tf     # Bucket name variable
    └── outputs.tf       # Bucket URL output
```

> **Note**: The `s3/` directory is a separate Terraform module. It should NOT be applied by default since the `petty-artifacts` bucket already exists.

---

## Optional: S3 Bucket Module

> ⚠️ **DO NOT APPLY BY DEFAULT** - Bucket `petty-artifacts` already exists.

This module is provided for reference or for new deployments.

### File Structure

```
terraform/s3/
├── main.tf
├── variables.tf
└── outputs.tf
```

### s3/main.tf

```hcl
terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

provider "aws" {
  region = var.aws_region
}

resource "aws_s3_bucket" "artifacts" {
  bucket = var.bucket_name

  tags = {
    Project   = "petty"
    ManagedBy = "terraform"
  }
}

resource "aws_s3_bucket_public_access_block" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id

  block_public_acls       = false
  block_public_policy     = false
  ignore_public_acls      = false
  restrict_public_buckets = false
}

resource "aws_s3_bucket_policy" "public_read" {
  bucket = aws_s3_bucket.artifacts.id

  # Wait for public access block to be configured
  depends_on = [aws_s3_bucket_public_access_block.artifacts]

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid       = "PublicReadGetObject"
        Effect    = "Allow"
        Principal = "*"
        Action    = "s3:GetObject"
        Resource  = "${aws_s3_bucket.artifacts.arn}/*"
      }
    ]
  })
}
```

### s3/variables.tf

```hcl
variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "bucket_name" {
  description = "S3 bucket name for petty artifacts"
  type        = string
  default     = "petty-artifacts"
}
```

### s3/outputs.tf

```hcl
output "bucket_name" {
  description = "S3 bucket name"
  value       = aws_s3_bucket.artifacts.id
}

output "bucket_arn" {
  description = "S3 bucket ARN"
  value       = aws_s3_bucket.artifacts.arn
}

output "bucket_regional_domain" {
  description = "S3 bucket regional domain name"
  value       = aws_s3_bucket.artifacts.bucket_regional_domain_name
}

output "rootfs_url" {
  description = "Public URL for rootfs (after upload)"
  value       = "https://${aws_s3_bucket.artifacts.bucket_regional_domain_name}/debian-devbox.ext4"
}
```

### Usage (Only If Needed)

```bash
# Navigate to s3 module
cd terraform/s3

# Initialize and apply ONLY if bucket doesn't exist
terraform init
terraform plan -var="bucket_name=petty-artifacts"
terraform apply -var="bucket_name=my-new-bucket"  # Use different name if needed

# Upload rootfs after bucket is created
aws s3 cp debian-devbox.ext4 s3://petty-artifacts/
```

---

## File Specifications

### 1. main.tf

```hcl
terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "petty"
      Environment = var.environment
      ManagedBy   = "terraform"
    }
  }
}
```

---

### 2. variables.tf

| Variable            | Type         | Default                                                                   | Required | Description         |
| ------------------- | ------------ | ------------------------------------------------------------------------- | -------- | ------------------- |
| `aws_region`        | string       | `"us-east-1"`                                                             | No       | AWS region          |
| `availability_zone` | string       | `"us-east-1a"`                                                            | No       | AZ for resources    |
| `environment`       | string       | `"production"`                                                            | No       | Environment tag     |
| `ssh_key_name`      | string       | -                                                                         | **Yes**  | EC2 key pair name   |
| `instance_type`     | string       | `"c5.metal"`                                                              | No       | Must support KVM    |
| `docker_image`      | string       | `"ghcr.io/vrn21/petty-mcp:latest"`                                        | No       | Container image     |
| `rootfs_url`        | string       | `"https://petty-artifacts.s3.us-east-1.amazonaws.com/debian-devbox.ext4"` | No       | Rootfs download URL |
| `allowed_ssh_cidrs` | list(string) | `["0.0.0.0/0"]`                                                           | No       | SSH allowed CIDRs   |
| `volume_size`       | number       | `50`                                                                      | No       | Root volume GB      |

**Validation Rules:**

- `instance_type` must contain "metal" (for KVM support)
- `volume_size` must be >= 30

---

### 3. data.tf

Lookup the latest **Debian 12** AMI:

```hcl
data "aws_ami" "debian" {
  most_recent = true
  owners      = ["136693071363"]  # Debian official

  filter {
    name   = "name"
    values = ["debian-12-amd64-*"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  filter {
    name   = "architecture"
    values = ["x86_64"]
  }
}
```

---

### 4. vpc.tf

Create these resources:

| Resource                      | Name     | Configuration                                |
| ----------------------------- | -------- | -------------------------------------------- |
| `aws_vpc`                     | `petty`  | CIDR: `10.0.0.0/16`, DNS hostnames enabled   |
| `aws_subnet`                  | `public` | CIDR: `10.0.1.0/24`, map public IP on launch |
| `aws_internet_gateway`        | `petty`  | Attached to VPC                              |
| `aws_route_table`             | `public` | Route `0.0.0.0/0` → IGW                      |
| `aws_route_table_association` | -        | Associate subnet with route table            |

---

### 5. security.tf

Security group `petty-mcp`:

| Direction | Port | Protocol | Source                  | Description       |
| --------- | ---- | -------- | ----------------------- | ----------------- |
| Ingress   | 22   | TCP      | `var.allowed_ssh_cidrs` | SSH access        |
| Ingress   | 8080 | TCP      | `0.0.0.0/0`             | MCP HTTP endpoint |
| Egress    | 0    | -1       | `0.0.0.0/0`             | All outbound      |

---

### 6. ec2.tf

| Resource              | Configuration                                         |
| --------------------- | ----------------------------------------------------- |
| `aws_instance`        | AMI: Debian 12, type: c5.metal, key: var.ssh_key_name |
| Root volume           | 50GB gp3, encrypted=true, delete_on_termination=true  |
| User data             | templatefile("scripts/user-data.sh", {...})           |
| `aws_eip`             | Allocate new Elastic IP                               |
| `aws_eip_association` | Associate EIP with instance                           |

**User data template variables:**

- `docker_image` - Container image to pull
- `rootfs_url` - URL for rootfs download

---

### 7. scripts/user-data.sh

Bootstrap script requirements:

```bash
#!/bin/bash
set -euo pipefail

LOG_FILE="/var/log/petty-bootstrap.log"
exec > >(tee -a "$LOG_FILE") 2>&1

echo "=== Petty Bootstrap Started: $(date) ==="

# 1. Update system
apt-get update
apt-get upgrade -y

# 2. Install Docker
apt-get install -y docker.io
systemctl enable docker
systemctl start docker

# 3. Fix KVM permissions (persistent via udev)
if [ -e /dev/kvm ]; then
    chmod 666 /dev/kvm
    echo 'KERNEL=="kvm", MODE="0666"' > /etc/udev/rules.d/99-kvm.rules
fi

# 4. Create systemd service
cat > /etc/systemd/system/petty-mcp.service << 'EOF'
[Unit]
Description=Petty MCP Server
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
Restart=on-failure
RestartSec=10
TimeoutStartSec=300

ExecStartPre=/usr/bin/docker pull ${docker_image}
ExecStart=/usr/bin/docker run --rm --name petty-mcp \
    --privileged \
    --device=/dev/kvm \
    -p 8080:8080 \
    -e PETTY_ROOTFS_URL=${rootfs_url} \
    -e PETTY_TRANSPORT=both \
    -e PETTY_HTTP_HOST=0.0.0.0 \
    -e PETTY_HTTP_PORT=8080 \
    -e RUST_LOG=info \
    ${docker_image}

ExecStop=/usr/bin/docker stop petty-mcp

[Install]
WantedBy=multi-user.target
EOF

# 5. Start service
systemctl daemon-reload
systemctl enable petty-mcp
systemctl start petty-mcp

echo "=== Petty Bootstrap Complete: $(date) ==="
```

**Note**: Use `templatefile()` to substitute `${docker_image}` and `${rootfs_url}`.

---

### 8. outputs.tf

| Output         | Value                                                        | Description                            |
| -------------- | ------------------------------------------------------------ | -------------------------------------- |
| `instance_id`  | `aws_instance.petty.id`                                      | EC2 instance ID                        |
| `public_ip`    | `aws_eip.petty.public_ip`                                    | Elastic IP address                     |
| `private_ip`   | `aws_instance.petty.private_ip`                              | Private IP                             |
| `ssh_command`  | `"ssh -i ~/.ssh/${var.ssh_key_name}.pem admin@${public_ip}"` | SSH command (Debian uses `admin` user) |
| `mcp_endpoint` | `"http://${public_ip}:8080/mcp"`                             | MCP endpoint URL                       |
| `health_url`   | `"http://${public_ip}:8080/health"`                          | Health check URL                       |

---

## Important Notes

### Debian-Specific

1. **Default user is `admin`** (not `ubuntu` or `ec2-user`)
2. **AMI owner ID**: `136693071363` (Debian official) [please look up the latest ID]
3. **AMI name pattern**: `debian-12-amd64-*`

### Production Considerations

1. **First start takes ~5 minutes** (rootfs download ~1.5GB)
2. **KVM permissions**: Persist with udev rule, not just chmod
3. **Docker pull**: May hit rate limits - consider ECR
4. **Instance type**: Must be `.metal` for KVM access
5. **EIP**: Survives instance stop/start

### Cost

| Resource   | Hourly    | Monthly (730hrs) |
| ---------- | --------- | ---------------- |
| c5.metal   | $4.08     | ~$2,978          |
| Elastic IP | $0.005    | ~$3.65           |
| EBS 50GB   | -         | ~$4              |
| **Total**  | **$4.09** | **~$2,986**      |

> Stop instance when not in use to save costs.

---

## Verification Steps

After `terraform apply`:

```bash
# 1. Wait for bootstrap (3-5 min for first boot)
sleep 300

# 2. Test health endpoint
curl -f http://$(terraform output -raw public_ip):8080/health

# 3. SSH and check logs
ssh -i ~/.ssh/KEY.pem admin@$(terraform output -raw public_ip)
sudo journalctl -u petty-mcp -f

# 4. Test MCP
curl -X POST http://$(terraform output -raw public_ip):8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## Success Criteria

- [ ] `terraform validate` passes
- [ ] `terraform plan` shows expected resources
- [ ] `terraform apply` completes without errors
- [ ] Health endpoint returns `{"status":"healthy"}` within 5 minutes
- [ ] SSH access works with `admin` user
- [ ] MCP `tools/list` returns tool definitions
