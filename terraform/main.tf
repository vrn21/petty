# =============================================================================
# main.tf - Terraform Provider Configuration
# =============================================================================
# Provider configuration for deploying bouvet-mcp on AWS EC2.
# =============================================================================

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
      Project     = "bouvet"
      Environment = var.environment
      ManagedBy   = "terraform"
    }
  }
}

# -----------------------------------------------------------------------------
# Locals - Architecture-specific defaults
# -----------------------------------------------------------------------------
locals {
  # Default rootfs URLs per architecture
  default_rootfs_urls = {
    x86_64 = "https://bouvet-artifacts.s3.us-east-1.amazonaws.com/debian-devbox.ext4"
    arm64  = "https://bouvet-artifacts.s3.us-east-1.amazonaws.com/debian-devbox-arm64.ext4"
  }

  # Use provided rootfs_url or fall back to architecture-specific default
  rootfs_url = var.rootfs_url != "" ? var.rootfs_url : local.default_rootfs_urls[var.architecture]

  # Recommended instance types per architecture
  recommended_instance_types = {
    x86_64 = "c5.metal"   # 96 vCPUs - requires quota increase
    arm64  = "a1.metal"   # 16 vCPUs - may fit free tier quota
  }
}
