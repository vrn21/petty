# =============================================================================
# variables.tf - Input Variables
# =============================================================================
# All configurable parameters for the bouvet-mcp deployment.
# =============================================================================

variable "aws_region" {
  description = "AWS region for deployment"
  type        = string
  default     = "us-east-1"
}

variable "availability_zone" {
  description = "Availability zone for the EC2 instance"
  type        = string
  default     = "us-east-1a"
}

variable "environment" {
  description = "Environment name for tagging"
  type        = string
  default     = "production"
}

variable "ssh_key_name" {
  description = "Name of the SSH key pair in AWS (required)"
  type        = string
}

variable "instance_type" {
  description = "EC2 instance type (must be .metal for KVM support)"
  type        = string
  default     = "c5.metal"

  validation {
    condition     = can(regex("metal", var.instance_type))
    error_message = "Instance type must be a .metal instance for KVM support (e.g., c5.metal, m5.metal)."
  }
}

variable "docker_image" {
  description = "Docker image for bouvet-mcp server"
  type        = string
  default     = "ghcr.io/vrn21/bouvet-mcp:latest"
}

variable "rootfs_url" {
  description = "Public URL to download the rootfs image (leave empty to use architecture-specific default)"
  type        = string
  default     = ""
}

variable "architecture" {
  description = "Target architecture: x86_64 or arm64"
  type        = string
  default     = "x86_64"

  validation {
    condition     = contains(["x86_64", "arm64"], var.architecture)
    error_message = "Architecture must be 'x86_64' or 'arm64'."
  }
}

variable "allowed_ssh_cidrs" {
  description = "CIDR blocks allowed to SSH (recommend restricting to your IP)"
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

variable "volume_size" {
  description = "Root volume size in GB"
  type        = number
  default     = 50

  validation {
    condition     = var.volume_size >= 30
    error_message = "Volume size must be at least 30 GB for rootfs and Docker images."
  }
}
