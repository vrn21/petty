# Phase 7: AWS Deployment (Simplified)

> Deploying Bouvet on c5.metal Spot Instance with manual control.

---

## Cost Summary

| Mode      | Hourly     | If Used 40hrs/month |
| --------- | ---------- | ------------------- |
| On-Demand | $4.08      | $163                |
| **Spot**  | **~$1.26** | **~$50**            |

**Manual control = you only pay when running.**

---

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                        AWS Account                            │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │                  c5.metal (Spot)                      │    │
│  │  ┌─────────────────────────────────────────────────┐ │    │
│  │  │                 Bouvet Stack                      │ │    │
│  │  │  ┌───────────┐  ┌───────────┐  ┌─────────────┐  │ │    │
│  │  │  │ bouvet-mcp │──│ bouvet-core│──│  Firecracker │  │ │    │
│  │  │  └───────────┘  └───────────┘  └─────────────┘  │ │    │
│  │  └─────────────────────────────────────────────────┘ │    │
│  └──────────────────────────────────────────────────────┘    │
│                               │                               │
│                               ▼                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │                      S3 Bucket                         │    │
│  │    vmlinux.bin  |  debian-python.ext4  |  debian.ext4  │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

---

## Implementation Tasks

### Task 1: Terraform - VPC & Security

- VPC with public subnet
- Security group: SSH (22)
- Internet gateway for outbound

### Task 2: Terraform - S3 Bucket

- Create bucket for images
- IAM policy for EC2 read access

### Task 3: Terraform - Spot Instance

- c5.metal spot request
- IAM instance profile (S3 access)
- User-data script for bootstrap

### Task 4: Bootstrap Script (user-data.sh)

1. Update packages
2. Download Firecracker binary
3. Download images from S3 to `/var/lib/bouvet/`
4. Enable and start bouvet service

### Task 5: Systemd Service

- `/etc/systemd/system/bouvet.service`
- Auto-restart on failure

---

## File Structure

```
terraform/
├── main.tf           # Provider config
├── variables.tf      # Inputs (region, key, bucket)
├── outputs.tf        # Instance IP
├── vpc.tf            # Network
├── ec2.tf            # Spot instance
├── s3.tf             # Image bucket
├── iam.tf            # Roles
└── scripts/
    ├── user-data.sh  # Bootstrap
    └── bouvet.service # Systemd unit
```

---

## Quick Commands

```bash
# Deploy
terraform apply -var="ssh_key_name=your-key"

# SSH
ssh -i ~/.ssh/key.pem ubuntu@$(terraform output -raw public_ip)

# Manual stop (saves money)
aws ec2 stop-instances --instance-ids $(terraform output -raw instance_id)

# Manual start
aws ec2 start-instances --instance-ids $(terraform output -raw instance_id)

# Destroy (when done)
terraform destroy
```

---

## Variables

| Name           | Required | Default               | Description  |
| -------------- | -------- | --------------------- | ------------ |
| `ssh_key_name` | Yes      | -                     | EC2 key pair |
| `region`       | No       | us-east-1             | AWS region   |
| `spot_price`   | No       | 1.50                  | Max bid      |
| `s3_bucket`    | No       | bouvet-images-{random} | Image bucket |

---

## Acceptance Criteria

- [ ] Terraform provisions c5.metal spot
- [ ] Images download from S3 on boot
- [ ] Bouvet service auto-starts
- [ ] SSH access works
- [ ] Manual stop/start works
