# =============================================================================
# ec2.tf - EC2 Instance and Elastic IP
# =============================================================================
# c5.metal instance with Debian 12 for running Firecracker microVMs.
# =============================================================================

# -----------------------------------------------------------------------------
# EC2 Instance
# -----------------------------------------------------------------------------
resource "aws_instance" "petty" {
  ami                         = data.aws_ami.debian.id
  instance_type               = var.instance_type
  key_name                    = var.ssh_key_name
  subnet_id                   = aws_subnet.public.id
  vpc_security_group_ids      = [aws_security_group.petty_mcp.id]
  user_data_replace_on_change = true

  # Root volume: encrypted gp3
  root_block_device {
    volume_size           = var.volume_size
    volume_type           = "gp3"
    encrypted             = true
    delete_on_termination = true

    tags = {
      Name = "petty-root"
    }
  }

  # Bootstrap script
  user_data = templatefile("${path.module}/scripts/user-data.sh", {
    docker_image = var.docker_image
    rootfs_url   = var.rootfs_url
  })

  tags = {
    Name = "petty-mcp"
  }
}

# -----------------------------------------------------------------------------
# Elastic IP
# -----------------------------------------------------------------------------
resource "aws_eip" "petty" {
  domain = "vpc"

  tags = {
    Name = "petty-eip"
  }
}

resource "aws_eip_association" "petty" {
  instance_id   = aws_instance.petty.id
  allocation_id = aws_eip.petty.id
}
