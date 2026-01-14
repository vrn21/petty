# =============================================================================
# ec2.tf - EC2 Instance and Elastic IP
# =============================================================================
# c5.metal instance with Debian 12 for running Firecracker microVMs.
# =============================================================================

# -----------------------------------------------------------------------------
# EC2 Instance
# -----------------------------------------------------------------------------
resource "aws_instance" "bouvet" {
  ami                         = data.aws_ami.debian.id
  instance_type               = var.instance_type

  # Use spot instance for cost savings (~70% cheaper than on-demand)
  # WARNING: Spot instances can be interrupted with 2-minute notice
  instance_market_options {
    market_type = "spot"
    spot_options {
      instance_interruption_behavior = "terminate"
      spot_instance_type             = "one-time"
    }
  }

  key_name                    = var.ssh_key_name
  subnet_id                   = aws_subnet.public.id
  vpc_security_group_ids      = [aws_security_group.bouvet_mcp.id]
  user_data_replace_on_change = true

  # Root volume: encrypted gp3
  root_block_device {
    volume_size           = var.volume_size
    volume_type           = "gp3"
    encrypted             = true
    delete_on_termination = true

    tags = {
      Name = "bouvet-root"
    }
  }

  # Bootstrap script
  user_data = templatefile("${path.module}/scripts/user-data.sh", {
    docker_image = var.docker_image
    rootfs_url   = var.rootfs_url
  })

  tags = {
    Name = "bouvet-mcp"
  }
}

# -----------------------------------------------------------------------------
# Elastic IP
# -----------------------------------------------------------------------------
resource "aws_eip" "bouvet" {
  domain = "vpc"

  tags = {
    Name = "bouvet-eip"
  }
}

resource "aws_eip_association" "bouvet" {
  instance_id   = aws_instance.bouvet.id
  allocation_id = aws_eip.bouvet.id
}
