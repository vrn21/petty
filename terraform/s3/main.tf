# =============================================================================
# s3/main.tf - S3 Bucket for Petty Artifacts
# =============================================================================
# OPTIONAL: Creates an S3 bucket with public read access for rootfs distribution.
#
# WARNING: DO NOT APPLY BY DEFAULT - The petty-artifacts bucket already exists.
#          Use this only if you need to create a new bucket.
#
# Usage:
#   cd terraform/s3
#   terraform init
#   terraform plan -var="bucket_name=my-petty-artifacts"
#   terraform apply -var="bucket_name=my-petty-artifacts"
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
}

# -----------------------------------------------------------------------------
# S3 Bucket
# -----------------------------------------------------------------------------
resource "aws_s3_bucket" "artifacts" {
  bucket = var.bucket_name

  tags = {
    Project   = "petty"
    ManagedBy = "terraform"
  }
}

# -----------------------------------------------------------------------------
# Public Access Configuration
# -----------------------------------------------------------------------------
resource "aws_s3_bucket_public_access_block" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id

  block_public_acls       = false
  block_public_policy     = false
  ignore_public_acls      = false
  restrict_public_buckets = false
}

# -----------------------------------------------------------------------------
# Bucket Policy for Public Read
# -----------------------------------------------------------------------------
resource "aws_s3_bucket_policy" "public_read" {
  bucket = aws_s3_bucket.artifacts.id

  # Wait for public access block to be configured first
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
