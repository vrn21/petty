# =============================================================================
# s3/variables.tf - S3 Module Variables
# =============================================================================

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
