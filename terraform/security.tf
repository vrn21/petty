# =============================================================================
# security.tf - Security Group
# =============================================================================
# Security group for petty-mcp: SSH + MCP HTTP endpoint.
# =============================================================================

resource "aws_security_group" "petty_mcp" {
  name        = "petty-mcp"
  description = "Security group for petty-mcp server"
  vpc_id      = aws_vpc.petty.id

  # SSH access
  ingress {
    description = "SSH access"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = var.allowed_ssh_cidrs
  }

  # MCP HTTP endpoint
  ingress {
    description = "MCP HTTP endpoint"
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  # All outbound traffic
  egress {
    description = "All outbound"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "petty-mcp-sg"
  }
}
