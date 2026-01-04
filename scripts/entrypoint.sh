#!/bin/bash
# =============================================================================
# entrypoint.sh - Docker entrypoint for bouvet-mcp server
# =============================================================================
# This script:
# 1. Fetches rootfs from public URL if not present locally
# 2. Verifies required binaries and kernel exist
# 3. Configures KVM permissions
# 4. Starts the bouvet-mcp server
# =============================================================================

set -e

echo "============================================"
echo "  Bouvet MCP Server"
echo "============================================"
echo ""

# -----------------------------------------------------------------------------
# Rootfs handling
# -----------------------------------------------------------------------------
if [ ! -f "$BOUVET_ROOTFS" ]; then
    if [ -n "$BOUVET_ROOTFS_URL" ]; then
        echo "[1/4] Rootfs not found locally, downloading..."
        echo "      Source: $BOUVET_ROOTFS_URL"
        echo "      Destination: $BOUVET_ROOTFS"
        
        # Check disk space (need at least 3GB for rootfs + overhead)
        REQUIRED_MB=3000
        AVAILABLE_MB=$(df -m "$(dirname "$BOUVET_ROOTFS")" | awk 'NR==2{print $4}')
        if [ "$AVAILABLE_MB" -lt "$REQUIRED_MB" ]; then
            echo ""
            echo "ERROR: Insufficient disk space."
            echo "  Required: ${REQUIRED_MB}MB"
            echo "  Available: ${AVAILABLE_MB}MB"
            exit 1
        fi
        
        # Download rootfs using curl (works with public S3 URLs)
        echo "      Downloading..."
        if ! curl -fSL --progress-bar -o "$BOUVET_ROOTFS" "$BOUVET_ROOTFS_URL"; then
            echo ""
            echo "ERROR: Failed to download rootfs"
            echo "  URL: $BOUVET_ROOTFS_URL"
            echo ""
            echo "  Make sure the URL is accessible (public S3 bucket or valid URL)"
            rm -f "$BOUVET_ROOTFS"  # Clean up partial download
            exit 1
        fi
        
        echo "      Downloaded successfully ($(du -h "$BOUVET_ROOTFS" | cut -f1))"
    else
        echo ""
        echo "ERROR: Rootfs not found at $BOUVET_ROOTFS"
        echo ""
        echo "  Option 1: Mount a rootfs volume"
        echo "    docker run -v /path/to/rootfs.ext4:$BOUVET_ROOTFS ..."
        echo ""
        echo "  Option 2: Set BOUVET_ROOTFS_URL to download from public URL"
        echo "    docker run -e BOUVET_ROOTFS_URL=https://bucket.s3.amazonaws.com/rootfs.ext4 ..."
        echo ""
        exit 1
    fi
else
    echo "[1/4] Using existing rootfs: $BOUVET_ROOTFS ($(du -h "$BOUVET_ROOTFS" | cut -f1))"
fi

# -----------------------------------------------------------------------------
# KVM permissions
# -----------------------------------------------------------------------------
if [ -e /dev/kvm ]; then
    chmod 666 /dev/kvm
    echo "[2/4] KVM access configured"
else
    echo ""
    echo "WARNING: /dev/kvm not found - Firecracker will not work!"
    echo ""
    echo "  Ensure container runs with --device=/dev/kvm or --privileged"
    echo "  Example: docker run --device=/dev/kvm ..."
    echo ""
    # Don't exit - let bouvet-mcp give a better error if VM creation fails
fi

# -----------------------------------------------------------------------------
# Verify required files
# -----------------------------------------------------------------------------
echo "[3/4] Verifying required files..."
MISSING_FILES=0
for bin in /usr/local/bin/bouvet-mcp /usr/local/bin/firecracker "$BOUVET_KERNEL"; do
    if [ ! -f "$bin" ]; then
        echo "      ERROR: Missing required file: $bin"
        MISSING_FILES=1
    fi
done

if [ "$MISSING_FILES" -eq 1 ]; then
    echo ""
    echo "ERROR: One or more required files are missing."
    echo "  This indicates a problem with the Docker image build."
    exit 1
fi
echo "      All required files present"

# -----------------------------------------------------------------------------
# Start server
# -----------------------------------------------------------------------------
echo "[4/4] Starting bouvet-mcp server..."
echo ""
echo "  Transport: ${BOUVET_TRANSPORT:-both}"
echo "  HTTP endpoint: http://0.0.0.0:${BOUVET_HTTP_PORT:-8080}"
echo "  Health check: http://0.0.0.0:${BOUVET_HTTP_PORT:-8080}/health"
echo ""
echo "============================================"
echo ""

# Execute the server, replacing this shell process
exec /usr/local/bin/bouvet-mcp "$@"
