# =============================================================================
# vpc.tf - VPC and Networking
# =============================================================================
# Creates VPC, public subnet, internet gateway, and routing.
# =============================================================================

# -----------------------------------------------------------------------------
# VPC
# -----------------------------------------------------------------------------
resource "aws_vpc" "bouvet" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
  enable_dns_support   = true

  tags = {
    Name = "bouvet-vpc"
  }
}

# -----------------------------------------------------------------------------
# Public Subnet
# -----------------------------------------------------------------------------
resource "aws_subnet" "public" {
  vpc_id                  = aws_vpc.bouvet.id
  cidr_block              = "10.0.1.0/24"
  availability_zone       = var.availability_zone
  map_public_ip_on_launch = true

  tags = {
    Name = "bouvet-public"
  }
}

# -----------------------------------------------------------------------------
# Internet Gateway
# -----------------------------------------------------------------------------
resource "aws_internet_gateway" "bouvet" {
  vpc_id = aws_vpc.bouvet.id

  tags = {
    Name = "bouvet-igw"
  }
}

# -----------------------------------------------------------------------------
# Route Table
# -----------------------------------------------------------------------------
resource "aws_route_table" "public" {
  vpc_id = aws_vpc.bouvet.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.bouvet.id
  }

  tags = {
    Name = "bouvet-public-rt"
  }
}

resource "aws_route_table_association" "public" {
  subnet_id      = aws_subnet.public.id
  route_table_id = aws_route_table.public.id
}
