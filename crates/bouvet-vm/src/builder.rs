//! Builder pattern for ergonomic VirtualMachine configuration.

use crate::config::{DriveConfig, MachineConfig, NetworkConfig, VsockConfig};
use crate::error::Result;
use crate::VirtualMachine;
use std::path::PathBuf;

/// Fluent builder for configuring and creating VirtualMachine instances.
///
/// # Example
///
/// ```no_run
/// use bouvet_vm::VmBuilder;
///
/// # async fn example() -> bouvet_vm::Result<()> {
/// let vm = VmBuilder::new()
///     .vcpus(4)
///     .memory_mib(512)
///     .kernel("/path/to/vmlinux")
///     .rootfs("/path/to/rootfs.ext4")
///     .with_network("tap0")
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct VmBuilder {
    config: MachineConfig,
}

impl Default for VmBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VmBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: MachineConfig::default(),
        }
    }

    /// Set the number of virtual CPUs (1-32).
    pub fn vcpus(mut self, count: u8) -> Self {
        self.config.vcpu_count = count;
        self
    }

    /// Set the memory size in MiB (128-32768).
    pub fn memory_mib(mut self, mib: u32) -> Self {
        self.config.memory_mib = mib;
        self
    }

    /// Set the path to the kernel image.
    pub fn kernel(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.kernel_path = path.into();
        self
    }

    /// Set the kernel boot arguments.
    pub fn boot_args(mut self, args: impl Into<String>) -> Self {
        self.config.boot_args = args.into();
        self
    }

    /// Set the path to the root filesystem image.
    pub fn rootfs(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.root_drive.path_on_host = path.into();
        self
    }

    /// Set the root drive as read-only.
    pub fn rootfs_read_only(mut self) -> Self {
        self.config.root_drive.is_read_only = true;
        self
    }

    /// Add an extra drive.
    pub fn with_drive(mut self, drive_id: &str, path: impl Into<PathBuf>) -> Self {
        self.config.extra_drives.push(DriveConfig {
            drive_id: drive_id.to_string(),
            path_on_host: path.into(),
            is_root_device: false,
            is_read_only: false,
        });
        self
    }

    /// Configure network interface with the given tap device.
    pub fn with_network(mut self, host_dev: &str) -> Self {
        self.config.network = Some(NetworkConfig {
            host_dev_name: host_dev.to_string(),
            ..Default::default()
        });
        self
    }

    /// Configure network interface with full options.
    pub fn with_network_config(mut self, config: NetworkConfig) -> Self {
        self.config.network = Some(config);
        self
    }

    /// Configure vsock with the given guest CID.
    pub fn with_vsock(mut self, cid: u32) -> Self {
        self.config.vsock = Some(VsockConfig {
            guest_cid: cid,
            ..Default::default()
        });
        self
    }

    /// Configure vsock with full options.
    pub fn with_vsock_config(mut self, config: VsockConfig) -> Self {
        self.config.vsock = Some(config);
        self
    }

    /// Set the path to the Firecracker binary.
    pub fn firecracker_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.firecracker_path = path.into();
        self
    }

    /// Set the chroot/working directory for the VM.
    pub fn chroot_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.chroot_path = path.into();
        self
    }

    /// Build and return the configuration without creating a VM.
    ///
    /// Useful for testing or inspecting the configuration.
    pub fn build_config(self) -> MachineConfig {
        self.config
    }

    /// Build and start the VirtualMachine.
    ///
    /// # Errors
    /// Returns an error if VM creation or startup fails.
    pub async fn build(self) -> Result<VirtualMachine> {
        VirtualMachine::create(self.config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MachineConfig::default();
        assert_eq!(config.vcpu_count, 2);
        assert_eq!(config.memory_mib, 256);
        assert!(config.network.is_none());
        assert!(config.vsock.is_none());
    }

    #[test]
    fn test_builder_vcpus_memory() {
        let config = VmBuilder::new().vcpus(4).memory_mib(512).build_config();

        assert_eq!(config.vcpu_count, 4);
        assert_eq!(config.memory_mib, 512);
    }

    #[test]
    fn test_builder_kernel_rootfs() {
        let config = VmBuilder::new()
            .kernel("/path/to/kernel")
            .rootfs("/path/to/rootfs")
            .build_config();

        assert_eq!(config.kernel_path, PathBuf::from("/path/to/kernel"));
        assert_eq!(
            config.root_drive.path_on_host,
            PathBuf::from("/path/to/rootfs")
        );
    }

    #[test]
    fn test_builder_with_network() {
        let config = VmBuilder::new().with_network("tap0").build_config();

        assert!(config.network.is_some());
        let net = config.network.unwrap();
        assert_eq!(net.host_dev_name, "tap0");
    }

    #[test]
    fn test_builder_with_vsock() {
        let config = VmBuilder::new().with_vsock(5).build_config();

        assert!(config.vsock.is_some());
        let vsock = config.vsock.unwrap();
        assert_eq!(vsock.guest_cid, 5);
    }

    #[test]
    fn test_builder_with_extra_drive() {
        let config = VmBuilder::new()
            .with_drive("data", "/path/to/data.ext4")
            .build_config();

        assert_eq!(config.extra_drives.len(), 1);
        assert_eq!(config.extra_drives[0].drive_id, "data");
    }
}
