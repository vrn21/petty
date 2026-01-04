# =============================================================================
# security.tf - Security Group
# =============================================================================
# Security group for bouvet-mcp: SSH + MCP HTTP endpoint.
# =============================================================================

resource "aws_security_group" "bouvet_mcp" {
  name        = "bouvet-mcp"
  description = "Security group for bouvet-mcp server"
  vpc_id      = aws_vpc.bouvet.id

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
    Name = "bouvet-mcp-sg"
  }
}
