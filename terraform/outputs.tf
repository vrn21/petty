# =============================================================================
# outputs.tf - Output Values
# =============================================================================
# Useful outputs after terraform apply.
# =============================================================================

output "instance_id" {
  description = "EC2 instance ID"
  value       = aws_instance.petty.id
}

output "public_ip" {
  description = "Elastic IP address"
  value       = aws_eip.petty.public_ip
}

output "private_ip" {
  description = "Private IP address"
  value       = aws_instance.petty.private_ip
}

output "ssh_command" {
  description = "SSH command to connect (Debian uses 'admin' user)"
  value       = "ssh -i ~/.ssh/${var.ssh_key_name}.pem admin@${aws_eip.petty.public_ip}"
}

output "mcp_endpoint" {
  description = "MCP HTTP endpoint URL"
  value       = "http://${aws_eip.petty.public_ip}:8080/mcp"
}

output "health_url" {
  description = "Health check URL"
  value       = "http://${aws_eip.petty.public_ip}:8080/health"
}
