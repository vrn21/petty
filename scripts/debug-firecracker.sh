#!/bin/bash
# =============================================================================
# debug-firecracker.sh - Manually run Firecracker to debug guest VM issues
# =============================================================================
# This script starts Firecracker manually so you can see all guest console
# output, useful for debugging why bouvet-agent crashes.
#
# Usage:
#   sudo ./scripts/debug-firecracker.sh
#
# Requirements:
#   - ARM64 Linux host with KVM support
#   - Firecracker, vmlinux, and rootfs from dev-arm.sh
# =============================================================================

set -euo pipefail

# Paths (from dev-arm.sh)
FIRECRACKER="/usr/local/bin/firecracker"
KERNEL="/var/lib/bouvet/vmlinux"
ROOTFS="/var/lib/bouvet/debian-devbox.ext4"
CHROOT_DIR="/tmp/bouvet"

# Debug session paths
DEBUG_DIR="${CHROOT_DIR}/debug-session"
API_SOCKET="${DEBUG_DIR}/firecracker.socket"
LOGFILE="${DEBUG_DIR}/firecracker.log"

# Make a copy of rootfs (don't modify the original)
DEBUG_ROOTFS="${DEBUG_DIR}/rootfs.ext4"

echo "============================================"
echo "  Firecracker Debug Session"
echo "============================================"
echo ""

# Check for root
if [ "$EUID" -ne 0 ]; then
    echo "ERROR: Please run as root (sudo ./scripts/debug-firecracker.sh)"
    exit 1
fi

# Check prerequisites
echo "[1/7] Checking prerequisites..."
[ -f "$FIRECRACKER" ] || { echo "ERROR: Firecracker not found at $FIRECRACKER"; exit 1; }
[ -f "$KERNEL" ] || { echo "ERROR: Kernel not found at $KERNEL"; exit 1; }
[ -f "$ROOTFS" ] || { echo "ERROR: Rootfs not found at $ROOTFS"; exit 1; }
[ -e /dev/kvm ] || { echo "ERROR: /dev/kvm not found"; exit 1; }
echo "       ✓ All files present"

# Clean up any previous debug session
echo "[2/7] Cleaning up previous session..."
rm -rf "$DEBUG_DIR"
mkdir -p "$DEBUG_DIR"
echo "       ✓ Created $DEBUG_DIR"

# Copy rootfs (so we don't corrupt the original)
echo "[3/7] Copying rootfs (this may take a moment)..."
cp "$ROOTFS" "$DEBUG_ROOTFS"
chmod 644 "$DEBUG_ROOTFS"
echo "       ✓ Rootfs copied to $DEBUG_ROOTFS"

# Determine boot args based on architecture
ARCH=$(uname -m)
KERNEL_BOOT_ARGS="console=ttyS0 reboot=k panic=1 pci=off"
if [ "$ARCH" = "aarch64" ]; then
    KERNEL_BOOT_ARGS="keep_bootcon ${KERNEL_BOOT_ARGS}"
fi

echo ""
echo "============================================"
echo "  Starting Firecracker"
echo "============================================"
echo ""
echo "Configuration:"
echo "  Kernel: $KERNEL"
echo "  Rootfs: $DEBUG_ROOTFS"
echo "  Socket: $API_SOCKET"
echo "  Boot args: $KERNEL_BOOT_ARGS"
echo ""

# Start Firecracker in background
echo "[4/7] Starting Firecracker process..."
$FIRECRACKER --api-sock "$API_SOCKET" &
FC_PID=$!
sleep 0.5

# Check if Firecracker started
if ! kill -0 $FC_PID 2>/dev/null; then
    echo "ERROR: Firecracker failed to start"
    exit 1
fi
echo "       ✓ Firecracker started (PID: $FC_PID)"

# Configure logger
echo "[5/7] Configuring Firecracker..."
curl -s -X PUT --unix-socket "$API_SOCKET" \
    --data "{
        \"log_path\": \"${LOGFILE}\",
        \"level\": \"Debug\",
        \"show_level\": true,
        \"show_log_origin\": true
    }" \
    "http://localhost/logger"
echo "       ✓ Logger configured"

# Configure boot source
curl -s -X PUT --unix-socket "$API_SOCKET" \
    --data "{
        \"kernel_image_path\": \"${KERNEL}\",
        \"boot_args\": \"${KERNEL_BOOT_ARGS}\"
    }" \
    "http://localhost/boot-source"
echo "       ✓ Boot source configured"

# Configure rootfs
curl -s -X PUT --unix-socket "$API_SOCKET" \
    --data "{
        \"drive_id\": \"rootfs\",
        \"path_on_host\": \"${DEBUG_ROOTFS}\",
        \"is_root_device\": true,
        \"is_read_only\": false
    }" \
    "http://localhost/drives/rootfs"
echo "       ✓ Rootfs configured"

# Configure vsock (to match bouvet configuration)
echo "[6/7] Configuring vsock..."
curl -s -X PUT --unix-socket "$API_SOCKET" \
    --data "{
        \"guest_cid\": 3,
        \"uds_path\": \"${DEBUG_DIR}/v.sock\"
    }" \
    "http://localhost/vsock"
echo "       ✓ Vsock configured (CID: 3)"

# Small delay to ensure config is applied
sleep 0.1

# Start the VM
echo "[7/7] Starting microVM..."
curl -s -X PUT --unix-socket "$API_SOCKET" \
    --data '{"action_type": "InstanceStart"}' \
    "http://localhost/actions"

echo ""
echo "============================================"
echo "  MicroVM Started!"
echo "============================================"
echo ""
echo "The guest console output will appear below."
echo "Watch for bouvet-agent startup messages!"
echo ""
echo "To stop: Press Ctrl+C or run 'sudo kill $FC_PID'"
echo "Logs: $LOGFILE"
echo ""
echo "------- GUEST CONSOLE OUTPUT -------"
echo ""

# Wait for Firecracker process (this lets console output through)
wait $FC_PID 2>/dev/null || true

echo ""
echo "------- END GUEST CONSOLE -------"
echo ""
echo "Firecracker exited. Check $LOGFILE for more details."
