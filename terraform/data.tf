# =============================================================================
# data.tf - Data Sources
# =============================================================================
# AMI lookup for Debian 12 (Bookworm) - architecture-aware.
# =============================================================================

locals {
  # Map architecture variable to AWS architecture names and AMI patterns
  ami_config = {
    x86_64 = {
      arch    = "x86_64"
      pattern = "debian-12-amd64-*"
    }
    arm64 = {
      arch    = "arm64"
      pattern = "debian-12-arm64-*"
    }
  }
}

data "aws_ami" "debian" {
  most_recent = true
  owners      = ["136693071363"] # Debian official AWS account

  filter {
    name   = "name"
    values = [local.ami_config[var.architecture].pattern]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  filter {
    name   = "architecture"
    values = [local.ami_config[var.architecture].arch]
  }
}
