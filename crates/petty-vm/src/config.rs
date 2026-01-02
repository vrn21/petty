//! Configuration types for MicroVM instances.

use crate::error::{Result, VmError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration for creating a new MicroVM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfig {
    /// Number of virtual CPUs (1-32)
    pub vcpu_count: u8,
    /// Memory size in MiB (128-32768)
    pub memory_mib: u32,
    /// Path to kernel image
    pub kernel_path: PathBuf,
    /// Kernel boot arguments
    pub boot_args: String,
    /// Root filesystem drive
    pub root_drive: DriveConfig,
    /// Additional drives (optional)
    pub extra_drives: Vec<DriveConfig>,
    /// Network configuration (optional)
    pub network: Option<NetworkConfig>,
    /// vsock configuration for guest-host communication (optional)
    pub vsock: Option<VsockConfig>,
    /// Path to Firecracker binary
    pub firecracker_path: PathBuf,
    /// Working directory for VM sockets and state
    pub chroot_path: PathBuf,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            vcpu_count: 2,
            memory_mib: 256,
            kernel_path: PathBuf::from("/var/lib/petty/kernel/vmlinux"),
            boot_args: "console=ttyS0 reboot=k panic=1 pci=off".into(),
            root_drive: DriveConfig::default(),
            extra_drives: Vec::new(),
            network: None,
            vsock: None,
            firecracker_path: PathBuf::from("/usr/local/bin/firecracker"),
            chroot_path: PathBuf::from("/tmp/petty"),
        }
    }
}

impl MachineConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    /// Returns an error if any configuration value is invalid.
    pub fn validate(&self) -> Result<()> {
        // Validate vCPU count (Firecracker supports 1-32)
        if self.vcpu_count == 0 || self.vcpu_count > 32 {
            return Err(VmError::Config(format!(
                "vcpu_count must be 1-32, got {}",
                self.vcpu_count
            )));
        }

        // Validate memory (Firecracker minimum is ~128 MiB)
        if self.memory_mib < 128 {
            return Err(VmError::Config(format!(
                "memory_mib must be at least 128, got {}",
                self.memory_mib
            )));
        }

        // Validate vsock CID (must be > 2, as 0, 1, 2 are reserved)
        if let Some(vsock) = &self.vsock {
            if vsock.guest_cid <= 2 {
                return Err(VmError::Config(format!(
                    "vsock guest_cid must be > 2, got {}",
                    vsock.guest_cid
                )));
            }
        }

        // Validate drive IDs are unique
        let mut drive_ids = vec![self.root_drive.drive_id.clone()];
        for extra in &self.extra_drives {
            if drive_ids.contains(&extra.drive_id) {
                return Err(VmError::Config(format!(
                    "duplicate drive_id: {}",
                    extra.drive_id
                )));
            }
            drive_ids.push(extra.drive_id.clone());
        }

        Ok(())
    }
}

/// Configuration for a block device (drive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConfig {
    /// Unique drive identifier
    pub drive_id: String,
    /// Path to drive image on host
    pub path_on_host: PathBuf,
    /// Whether this is the root device
    pub is_root_device: bool,
    /// Read-only flag
    pub is_read_only: bool,
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            drive_id: "rootfs".into(),
            path_on_host: PathBuf::from("/var/lib/petty/images/debian.ext4"),
            is_root_device: true,
            is_read_only: false,
        }
    }
}

/// Network interface configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network interface ID
    pub iface_id: String,
    /// Host device name (tap device)
    pub host_dev_name: String,
    /// Guest MAC address (optional, auto-generated if None)
    pub guest_mac: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            iface_id: "eth0".into(),
            host_dev_name: "tap0".into(),
            guest_mac: None,
        }
    }
}

/// vsock configuration for guest-host communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockConfig {
    /// Guest CID (Context ID), must be > 2
    pub guest_cid: u32,
    /// Path to vsock Unix Domain Socket on host
    pub uds_path: PathBuf,
}

impl Default for VsockConfig {
    fn default() -> Self {
        Self {
            guest_cid: 3,
            uds_path: PathBuf::from("/tmp/petty-vsock.sock"),
        }
    }
}

impl VsockConfig {
    /// Create a vsock config for a specific VM.
    ///
    /// This generates a unique UDS path based on the VM ID.
    ///
    /// # Arguments
    /// * `cid` - Guest CID (must be > 2)
    /// * `chroot_path` - Base chroot path for VMs
    /// * `vm_id` - Unique VM identifier
    pub fn for_vm(cid: u32, chroot_path: &Path, vm_id: &str) -> Self {
        Self {
            guest_cid: cid,
            uds_path: chroot_path.join(vm_id).join("v.sock"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_vcpu() {
        let mut config = MachineConfig::default();
        config.vcpu_count = 0;
        assert!(config.validate().is_err());

        config.vcpu_count = 33;
        assert!(config.validate().is_err());

        config.vcpu_count = 4;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_memory() {
        let mut config = MachineConfig::default();
        config.memory_mib = 64;
        assert!(config.validate().is_err());

        config.memory_mib = 128;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_vsock_cid() {
        let mut config = MachineConfig::default();
        config.vsock = Some(VsockConfig {
            guest_cid: 2,
            uds_path: PathBuf::from("/tmp/test.sock"),
        });
        assert!(config.validate().is_err());

        config.vsock = Some(VsockConfig {
            guest_cid: 3,
            uds_path: PathBuf::from("/tmp/test.sock"),
        });
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_duplicate_drive_ids() {
        let mut config = MachineConfig::default();
        config.extra_drives.push(DriveConfig {
            drive_id: "rootfs".into(), // Same as root drive!
            path_on_host: PathBuf::from("/tmp/extra.ext4"),
            is_root_device: false,
            is_read_only: true,
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_vsock_for_vm() {
        let config = VsockConfig::for_vm(5, &PathBuf::from("/tmp/petty"), "vm-123");
        assert_eq!(config.guest_cid, 5);
        assert_eq!(config.uds_path, PathBuf::from("/tmp/petty/vm-123/v.sock"));
    }
}
