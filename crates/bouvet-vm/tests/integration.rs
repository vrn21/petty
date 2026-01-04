//! Integration tests for petty-vm.
//!
//! These tests require:
//! - Linux with /dev/kvm access
//! - Firecracker binary installed
//! - Kernel and rootfs images
//!
//! Run with: `cargo test -p petty-vm -- --ignored`

use petty_vm::{VmBuilder, VmState};
use std::path::Path;

/// Test full VM lifecycle: create -> running -> stop -> destroy
#[tokio::test]
#[ignore = "requires Linux + KVM + Firecracker"]
async fn test_vm_lifecycle() {
    // These paths should be set to actual kernel/rootfs for integration testing
    let kernel_path = std::env::var("PETTY_KERNEL_PATH")
        .unwrap_or_else(|_| "/var/lib/petty/kernel/vmlinux".to_string());
    let rootfs_path = std::env::var("PETTY_ROOTFS_PATH")
        .unwrap_or_else(|_| "/var/lib/petty/images/debian.ext4".to_string());

    // Skip if files don't exist
    if !Path::new(&kernel_path).exists() || !Path::new(&rootfs_path).exists() {
        eprintln!("Skipping test: kernel or rootfs not found");
        eprintln!("Set PETTY_KERNEL_PATH and PETTY_ROOTFS_PATH environment variables");
        return;
    }

    // Create VM
    let vm = VmBuilder::new()
        .vcpus(1)
        .memory_mib(128)
        .kernel(&kernel_path)
        .rootfs(&rootfs_path)
        .build()
        .await
        .expect("Failed to create VM");

    // Verify running state
    assert_eq!(vm.state(), VmState::Running);
    assert!(!vm.id().is_nil());

    // Destroy VM
    vm.destroy().await.expect("Failed to destroy VM");
}

/// Test VM creation with network interface
#[tokio::test]
#[ignore = "requires Linux + KVM + Firecracker + TAP device"]
async fn test_vm_with_network() {
    let kernel_path = std::env::var("PETTY_KERNEL_PATH")
        .unwrap_or_else(|_| "/var/lib/petty/kernel/vmlinux".to_string());
    let rootfs_path = std::env::var("PETTY_ROOTFS_PATH")
        .unwrap_or_else(|_| "/var/lib/petty/images/debian.ext4".to_string());

    if !Path::new(&kernel_path).exists() || !Path::new(&rootfs_path).exists() {
        eprintln!("Skipping test: kernel or rootfs not found");
        return;
    }

    let vm = VmBuilder::new()
        .vcpus(1)
        .memory_mib(128)
        .kernel(&kernel_path)
        .rootfs(&rootfs_path)
        .with_network("tap0")
        .build()
        .await
        .expect("Failed to create VM with network");

    assert_eq!(vm.state(), VmState::Running);
    assert!(vm.config().network.is_some());

    vm.destroy().await.expect("Failed to destroy VM");
}

/// Test stop and restart cycle
#[tokio::test]
#[ignore = "requires Linux + KVM + Firecracker"]
async fn test_vm_stop_restart() {
    let kernel_path = std::env::var("PETTY_KERNEL_PATH")
        .unwrap_or_else(|_| "/var/lib/petty/kernel/vmlinux".to_string());
    let rootfs_path = std::env::var("PETTY_ROOTFS_PATH")
        .unwrap_or_else(|_| "/var/lib/petty/images/debian.ext4".to_string());

    if !Path::new(&kernel_path).exists() || !Path::new(&rootfs_path).exists() {
        eprintln!("Skipping test: kernel or rootfs not found");
        return;
    }

    let mut vm = VmBuilder::new()
        .vcpus(1)
        .memory_mib(128)
        .kernel(&kernel_path)
        .rootfs(&rootfs_path)
        .build()
        .await
        .expect("Failed to create VM");

    // Stop
    vm.stop().await.expect("Failed to stop VM");
    assert_eq!(vm.state(), VmState::Stopped);

    // Start again
    vm.start().await.expect("Failed to restart VM");
    assert_eq!(vm.state(), VmState::Running);

    // Cleanup
    vm.destroy().await.expect("Failed to destroy VM");
}
