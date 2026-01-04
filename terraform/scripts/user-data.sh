#!/bin/bash
# =============================================================================
# user-data.sh - EC2 Bootstrap Script
# =============================================================================
# This script runs on first boot to configure the instance:
# 1. Update system packages
# 2. Install Docker
# 3. Configure KVM permissions (persistent)
# 4. Create systemd service for bouvet-mcp
# 5. Start the service
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

# -----------------------------------------------------------------------------
# 1. Update system packages
# -----------------------------------------------------------------------------
echo "[1/5] Updating system packages..."
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get upgrade -y

# -----------------------------------------------------------------------------
# 2. Install Docker
# -----------------------------------------------------------------------------
echo "[2/5] Installing Docker..."
apt-get install -y docker.io
systemctl enable docker
systemctl start docker

# Verify Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "ERROR: Docker failed to start"
    exit 1
fi
echo "       Docker installed successfully"

# -----------------------------------------------------------------------------
# 3. Configure KVM permissions (persistent)
# -----------------------------------------------------------------------------
echo "[3/5] Configuring KVM permissions..."
if [ -e /dev/kvm ]; then
    chmod 666 /dev/kvm
    # Create udev rule for persistent permissions
    echo 'KERNEL=="kvm", MODE="0666"' > /etc/udev/rules.d/99-kvm.rules
    udevadm control --reload-rules
    echo "       KVM permissions configured"
else
    echo "WARNING: /dev/kvm not found - this instance may not support nested virtualization"
fi

# -----------------------------------------------------------------------------
# 4. Create systemd service
# -----------------------------------------------------------------------------
echo "[4/5] Creating systemd service..."
cat > /etc/systemd/system/bouvet-mcp.service << 'SERVICEEOF'
[Unit]
Description=Bouvet MCP Server
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
Restart=on-failure
RestartSec=10
TimeoutStartSec=300

ExecStartPre=/usr/bin/docker pull ${docker_image}
ExecStart=/usr/bin/docker run --rm --name bouvet-mcp \
    --privileged \
    --device=/dev/kvm \
    -p 8080:8080 \
    -e BOUVET_ROOTFS_URL=${rootfs_url} \
    -e BOUVET_TRANSPORT=both \
    -e BOUVET_HTTP_HOST=0.0.0.0 \
    -e BOUVET_HTTP_PORT=8080 \
    -e RUST_LOG=info \
    ${docker_image}

ExecStop=/usr/bin/docker stop bouvet-mcp

[Install]
WantedBy=multi-user.target
SERVICEEOF

echo "       Systemd service created"

# -----------------------------------------------------------------------------
# 5. Enable and start service
# -----------------------------------------------------------------------------
echo "[5/5] Starting bouvet-mcp service..."
systemctl daemon-reload
systemctl enable bouvet-mcp
systemctl start bouvet-mcp

echo ""
echo "=== Bouvet Bootstrap Complete: $(date) ==="
echo ""
echo "Service status: $(systemctl is-active bouvet-mcp)"
echo "View logs: journalctl -u bouvet-mcp -f"
echo ""
