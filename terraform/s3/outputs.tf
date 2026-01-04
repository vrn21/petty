# =============================================================================
# s3/outputs.tf - S3 Module Outputs
# =============================================================================

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
