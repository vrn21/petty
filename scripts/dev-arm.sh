#!/bin/bash
# =============================================================================
# dev-arm.sh - Setup Bouvet dependencies for ARM64 local development
# =============================================================================
# Downloads Firecracker, Jailer, Kernel, and Rootfs for running Bouvet
# directly on an ARM64 Linux host (without Docker).
#
# Usage:
#   sudo ./dev-arm.sh
#
# Requirements:
#   - ARM64 Linux host with KVM support (/dev/kvm)
#   - curl installed
#   - Root privileges (for writing to /usr/local/bin and /var/lib)
# =============================================================================

set -euo pipefail

# Configuration
FC_VERSION="1.5.1"
ARCH="aarch64"

# Paths
BOUVET_DIR="/var/lib/bouvet"
CHROOT_DIR="/tmp/bouvet"

# URLs (from download-binaries.sh)
FC_URL="https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/firecracker-v${FC_VERSION}-${ARCH}.tgz"
KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.5/aarch64/vmlinux-5.10.186"
ROOTFS_URL="https://bouvet-artifacts.s3.us-east-1.amazonaws.com/debian-devbox.ext4"

echo "============================================"
echo "  Bouvet ARM64 Development Setup"
echo "============================================"
echo ""
echo "  Firecracker: v${FC_VERSION}"
echo "  Architecture: ${ARCH}"
echo ""

# Check for root
if [ "$EUID" -ne 0 ]; then
    echo "ERROR: Please run as root (sudo ./dev-arm.sh)"
    exit 1
fi

# Check for KVM
if [ ! -e /dev/kvm ]; then
    echo "WARNING: /dev/kvm not found. Firecracker requires KVM."
    echo "         Run: sudo modprobe kvm"
fi

# Create directories
echo "[1/5] Creating directories..."
mkdir -p "$BOUVET_DIR" "$CHROOT_DIR"

# Download Firecracker
echo "[2/5] Downloading Firecracker v${FC_VERSION}..."
curl -sSL -o /tmp/firecracker.tgz "$FC_URL"
tar -xzf /tmp/firecracker.tgz -C /tmp
mv "/tmp/release-v${FC_VERSION}-${ARCH}/firecracker-v${FC_VERSION}-${ARCH}" /usr/local/bin/firecracker
mv "/tmp/release-v${FC_VERSION}-${ARCH}/jailer-v${FC_VERSION}-${ARCH}" /usr/local/bin/jailer
chmod +x /usr/local/bin/firecracker /usr/local/bin/jailer
rm -rf /tmp/firecracker.tgz "/tmp/release-v${FC_VERSION}-${ARCH}"
echo "       ✓ Firecracker installed: $(/usr/local/bin/firecracker --version 2>&1 | head -1)"

# Download Kernel
echo "[3/5] Downloading kernel (5.10.186)..."
curl -sSL -o "${BOUVET_DIR}/vmlinux" "$KERNEL_URL"
chmod 644 "${BOUVET_DIR}/vmlinux"
echo "       ✓ Kernel: $(du -h ${BOUVET_DIR}/vmlinux | cut -f1)"

# Download Rootfs
echo "[4/5] Downloading rootfs (~2GB, this may take a while)..."
curl -sSL -o "${BOUVET_DIR}/debian-devbox.ext4" "$ROOTFS_URL"
chmod 644 "${BOUVET_DIR}/debian-devbox.ext4"
echo "       ✓ Rootfs: $(du -h ${BOUVET_DIR}/debian-devbox.ext4 | cut -f1)"

# Set permissions
echo "[5/5] Setting permissions..."
chmod 777 "$CHROOT_DIR"
if [ -e /dev/kvm ]; then
    chmod 666 /dev/kvm
    echo "       ✓ KVM permissions set"
fi

echo ""
echo "============================================"
echo "  Setup Complete!"
echo "============================================"
echo ""
echo "Files installed:"
echo "  /usr/local/bin/firecracker"
echo "  /usr/local/bin/jailer"
echo "  ${BOUVET_DIR}/vmlinux"
echo "  ${BOUVET_DIR}/debian-devbox.ext4"
echo ""
echo "To run bouvet-mcp:"
echo ""
echo "  export BOUVET_KERNEL=${BOUVET_DIR}/vmlinux"
echo "  export BOUVET_ROOTFS=${BOUVET_DIR}/debian-devbox.ext4"
echo "  export BOUVET_FIRECRACKER=/usr/local/bin/firecracker"
echo "  export BOUVET_CHROOT=${CHROOT_DIR}"
echo "  export BOUVET_TRANSPORT=http"
echo "  export BOUVET_HTTP_HOST=0.0.0.0"
echo "  export BOUVET_HTTP_PORT=8080"
echo "  export RUST_LOG=info"
echo ""
echo "  cargo run --release -p bouvet-mcp"
echo ""
