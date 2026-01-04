# Petty-MCP Terraform Infrastructure

Deploy petty-mcp Docker container on AWS EC2 c5.metal instance.

## Prerequisites

1. [Terraform](https://www.terraform.io/downloads.html) >= 1.0
2. AWS credentials configured (`aws configure` or environment variables)
3. SSH key pair in AWS EC2

## Quick Start

```bash
# Initialize Terraform
terraform init

# Review the plan (replace YOUR_KEY_NAME with your SSH key)
terraform plan -var="ssh_key_name=YOUR_KEY_NAME"

# Apply the infrastructure
terraform apply -var="ssh_key_name=YOUR_KEY_NAME"
```

## Variables

| Variable            | Description                         | Default                          |
| ------------------- | ----------------------------------- | -------------------------------- |
| `ssh_key_name`      | **Required.** AWS EC2 key pair name | -                                |
| `aws_region`        | AWS region                          | `us-east-1`                      |
| `instance_type`     | EC2 instance type (must be .metal)  | `c5.metal`                       |
| `docker_image`      | Container image                     | `ghcr.io/vrn21/petty-mcp:latest` |
| `allowed_ssh_cidrs` | CIDR blocks for SSH access          | `["0.0.0.0/0"]`                  |
| `volume_size`       | Root volume size in GB              | `50`                             |

## Outputs

After `terraform apply`:

```bash
# Get the public IP
terraform output public_ip

# Get SSH command
terraform output ssh_command

# Get MCP endpoint
terraform output mcp_endpoint
```

## Verification

```bash
# Wait for bootstrap (~5 minutes for first boot)
sleep 300

# Check health
curl -f http://$(terraform output -raw public_ip):8080/health

# SSH and view logs
$(terraform output -raw ssh_command)
sudo journalctl -u petty-mcp -f

# Test MCP
curl -X POST http://$(terraform output -raw public_ip):8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

## S3 Module (Optional)

The `s3/` subdirectory contains a module for creating an S3 bucket for artifacts.

```bash
cd s3
terraform init
terraform apply -var="bucket_name=my-new-bucket"
```

## Clean Up

```bash
terraform destroy -var="ssh_key_name=YOUR_KEY_NAME"
```
