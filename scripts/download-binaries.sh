#!/bin/bash
# =============================================================================
# download-binaries.sh - Download Firecracker, Jailer, and Kernel
# =============================================================================
# This script is used during Docker build to fetch external dependencies.
#
# Usage:
#   ./download-binaries.sh [ARCH] [FC_VERSION]
#
# Arguments:
#   ARCH        - Target architecture: x86_64 or aarch64 (auto-detected if not provided)
#   FC_VERSION  - Firecracker version (default: 1.5.1)
#
# Output:
#   /downloads/firecracker
#   /downloads/jailer
#   /downloads/vmlinux
# =============================================================================

set -euo pipefail

# Configuration
FC_VERSION="${2:-1.5.1}"
DOWNLOAD_DIR="/downloads"

# Detect architecture
if [ -n "${1:-}" ]; then
    ARCH="$1"
elif [ "$(uname -m)" = "aarch64" ] || [ "$(uname -m)" = "arm64" ]; then
    ARCH="aarch64"
else
    ARCH="x86_64"
fi

echo "============================================"
echo "  Downloading Bouvet Dependencies"
echo "============================================"
echo ""
echo "  Architecture: $ARCH"
echo "  Firecracker:  v$FC_VERSION"
echo "  Output dir:   $DOWNLOAD_DIR"
echo ""

# Create download directory
mkdir -p "$DOWNLOAD_DIR"
cd "$DOWNLOAD_DIR"

# -----------------------------------------------------------------------------
# Download Firecracker and Jailer
# -----------------------------------------------------------------------------
echo "[1/3] Downloading Firecracker v${FC_VERSION}..."

FC_URL="https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/firecracker-v${FC_VERSION}-${ARCH}.tgz"

if ! curl -sSL -o firecracker.tgz "$FC_URL"; then
    echo "ERROR: Failed to download Firecracker from $FC_URL"
    exit 1
fi

# Extract binaries from tarball
tar -xzf firecracker.tgz

# Move binaries to download dir
RELEASE_DIR="release-v${FC_VERSION}-${ARCH}"
mv "${RELEASE_DIR}/firecracker-v${FC_VERSION}-${ARCH}" firecracker
mv "${RELEASE_DIR}/jailer-v${FC_VERSION}-${ARCH}" jailer

# Cleanup
rm -rf firecracker.tgz "$RELEASE_DIR"

# Make executable
chmod +x firecracker jailer

echo "       Downloaded firecracker ($(du -h firecracker | cut -f1))"
echo "       Downloaded jailer ($(du -h jailer | cut -f1))"

# -----------------------------------------------------------------------------
# Download Kernel
# -----------------------------------------------------------------------------
echo "[2/3] Downloading kernel..."

# Use AWS Firecracker official kernels (with vsock support)
if [ "$ARCH" = "aarch64" ]; then
    KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.5/aarch64/vmlinux-5.10.186"
else
    KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.5/x86_64/vmlinux-5.10.186"
fi

if ! curl -sSL -o vmlinux "$KERNEL_URL"; then
    echo "ERROR: Failed to download kernel from $KERNEL_URL"
    exit 1
fi

echo "       Downloaded vmlinux ($(du -h vmlinux | cut -f1))"

# -----------------------------------------------------------------------------
# Verify downloads
# -----------------------------------------------------------------------------
echo "[3/3] Verifying binaries..."

# Check firecracker runs
if ! ./firecracker --version > /dev/null 2>&1; then
    echo "ERROR: Firecracker binary verification failed"
    exit 1
fi

FC_VER_OUTPUT=$(./firecracker --version 2>&1 | head -1)
echo "       Firecracker: $FC_VER_OUTPUT"

# Check file sizes are reasonable
for file in firecracker jailer vmlinux; do
    SIZE=$(stat -c%s "$file" 2>/dev/null || stat -f%z "$file" 2>/dev/null)
    if [ "$SIZE" -lt 100000 ]; then
        echo "ERROR: $file is suspiciously small ($SIZE bytes)"
        exit 1
    fi
done

echo ""
echo "============================================"
echo "  All downloads complete!"
echo "============================================"
echo ""
ls -la "$DOWNLOAD_DIR"
