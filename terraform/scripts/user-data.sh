#!/bin/bash
# =============================================================================
# user-data.sh - EC2 Bootstrap Script
# =============================================================================
# This script runs on first boot to configure the instance:
# 1. Update system packages
# 2. Install Docker
# 3. Configure KVM permissions
# 4. Install and configure nginx as reverse proxy
# 5. Create systemd service for bouvet-mcp
# 6. Start the services
#
# Template variables (replaced by Terraform):
#   - docker_image: Container image to pull
#   - rootfs_url: URL for rootfs download
# =============================================================================

set -euo pipefail

LOG_FILE="/var/log/bouvet-bootstrap.log"
exec > >(tee -a "$LOG_FILE") 2>&1

echo "=== Bouvet Bootstrap Started: $(date) ==="
echo ""

DOCKER_IMAGE="${docker_image}"
ROOTFS_URL="${rootfs_url}"

# -----------------------------------------------------------------------------
# 1. Update system packages
# -----------------------------------------------------------------------------
echo "[1/5] Updating system packages..."
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get upgrade -y

# -----------------------------------------------------------------------------
# 2. Install Docker CE (Official Repository)
# -----------------------------------------------------------------------------
echo "[2/5] Installing Docker CE from official repository..."

# Install prerequisites
apt-get install -y ca-certificates curl gnupg

# Add Docker's official GPG key
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

# Add Docker repository
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
  $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
  tee /etc/apt/sources.list.d/docker.list > /dev/null

# Install Docker CE
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

systemctl enable docker
systemctl start docker

# Verify Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "ERROR: Docker failed to start"
    exit 1
fi
echo "       Docker CE installed successfully: $(docker --version)"

# -----------------------------------------------------------------------------
# 3. Configure KVM permissions (persistent)
# -----------------------------------------------------------------------------
echo "[3/5] Configuring KVM permissions..."
if [ -e /dev/kvm ]; then
    # Set permissions
    chmod 666 /dev/kvm
    
    # Create udev rule for persistent permissions
    echo 'KERNEL=="kvm", MODE="0666"' > /etc/udev/rules.d/99-kvm.rules
    udevadm control --reload-rules
    udevadm trigger
    
    # Create kvm group if it doesn't exist and add root
    groupadd -f kvm
    usermod -aG kvm root
    
    echo "       KVM permissions configured"
else
    echo "WARNING: /dev/kvm not found - this instance may not support nested virtualization"
fi

# -----------------------------------------------------------------------------
# 4. Install and configure nginx
# -----------------------------------------------------------------------------
echo "[4/5] Installing and configuring nginx..."
apt-get install -y nginx

# Create nginx site configuration
cat > /etc/nginx/sites-available/bouvet << 'NGINXEOF'
# Bouvet MCP Server - nginx reverse proxy
server {
    listen 80;
    server_name _;

    # Proxy all requests to bouvet-mcp
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        
        # Headers
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # SSE/streaming support (important for MCP)
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 300s;
    }
}
NGINXEOF

# Enable site and remove default
ln -sf /etc/nginx/sites-available/bouvet /etc/nginx/sites-enabled/
rm -f /etc/nginx/sites-enabled/default

# Test and start nginx
nginx -t && systemctl enable nginx && systemctl start nginx
echo "       nginx configured and started"

# -----------------------------------------------------------------------------
# 5. Create and start bouvet-mcp systemd service
# -----------------------------------------------------------------------------
echo "[5/5] Creating bouvet-mcp service..."
cat > /etc/systemd/system/bouvet-mcp.service << SERVICEEOF
[Unit]
Description=Bouvet MCP Server
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
Restart=on-failure
RestartSec=10
TimeoutStartSec=300

ExecStartPre=/usr/bin/docker pull $DOCKER_IMAGE
ExecStart=/usr/bin/docker run --rm --name bouvet-mcp \\
    --privileged \\
    --device=/dev/kvm \\
    --security-opt seccomp=unconfined \\
    --security-opt apparmor=unconfined \\
    --cap-add=NET_ADMIN \\
    --cap-add=SYS_ADMIN \\
    -v /dev/kvm:/dev/kvm \\
    -p 127.0.0.1:8080:8080 \\
    -e BOUVET_ROOTFS_URL=$ROOTFS_URL \\
    -e BOUVET_TRANSPORT=http \\
    -e BOUVET_HTTP_HOST=0.0.0.0 \\
    -e BOUVET_HTTP_PORT=8080 \\
    -e RUST_LOG=info \\
    $DOCKER_IMAGE

ExecStop=/usr/bin/docker stop bouvet-mcp

[Install]
WantedBy=multi-user.target
SERVICEEOF

systemctl daemon-reload
systemctl enable bouvet-mcp
systemctl start bouvet-mcp

echo "       bouvet-mcp service created and started"

# Get public IP for output
PUBLIC_IP=$(curl -s http://169.254.169.254/latest/meta-data/public-ipv4 || echo "unknown")

echo ""
echo "=== Bouvet Bootstrap Complete: $(date) ==="
echo ""
echo "MCP Endpoint: http://$PUBLIC_IP/mcp"
echo "Health Check: http://$PUBLIC_IP/health"
echo ""
echo "View logs: journalctl -u bouvet-mcp -f"
echo ""
echo "To add HTTPS later:"
echo "  1. Point your domain to $PUBLIC_IP"
echo "  2. Run: sudo apt install certbot python3-certbot-nginx"
echo "  3. Run: sudo certbot --nginx -d yourdomain.com"
echo ""
