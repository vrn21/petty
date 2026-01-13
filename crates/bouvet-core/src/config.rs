//! Sandbox configuration types.

use crate::error::CoreError;
use std::path::PathBuf;
use std::time::Duration;

/// Configuration for creating a sandbox.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Path to kernel image.
    pub kernel_path: PathBuf,
    /// Path to rootfs image.
    pub rootfs_path: PathBuf,
    /// Working directory for VM sockets and state.
    pub chroot_path: PathBuf,
    /// Memory in MiB (default: 256).
    pub memory_mib: u32,
    /// vCPU count (default: 2).
    pub vcpu_count: u8,
    /// Maximum execution time for any single operation.
    pub timeout: Option<Duration>,
    /// Guest CID for vsock (default: 3, must be >= 3).
    pub vsock_cid: u32,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            kernel_path: PathBuf::new(),
            rootfs_path: PathBuf::new(),
            chroot_path: PathBuf::from("/tmp/bouvet"),
            memory_mib: 256,
            vcpu_count: 2,
            timeout: None,
            vsock_cid: 3,
        }
    }
}

impl SandboxConfig {
    /// Create a new config builder.
    pub fn builder() -> SandboxConfigBuilder {
        SandboxConfigBuilder::default()
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.kernel_path.as_os_str().is_empty() {
            return Err(CoreError::Connection("kernel_path is required".into()));
        }
        if self.rootfs_path.as_os_str().is_empty() {
            return Err(CoreError::Connection("rootfs_path is required".into()));
        }
        if self.memory_mib == 0 {
            return Err(CoreError::Connection("memory_mib must be > 0".into()));
        }
        if self.vcpu_count == 0 {
            return Err(CoreError::Connection("vcpu_count must be > 0".into()));
        }
        if self.vsock_cid < 3 {
            return Err(CoreError::Connection("vsock_cid must be >= 3".into()));
        }
        Ok(())
    }
}

/// Builder for SandboxConfig.
#[derive(Debug, Default)]
pub struct SandboxConfigBuilder {
    config: SandboxConfig,
}

impl SandboxConfigBuilder {
    /// Set the kernel path.
    pub fn kernel(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.kernel_path = path.into();
        self
    }

    /// Set the rootfs path.
    pub fn rootfs(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.rootfs_path = path.into();
        self
    }

    /// Set memory in MiB.
    pub fn memory_mib(mut self, mib: u32) -> Self {
        self.config.memory_mib = mib;
        self
    }

    /// Set vCPU count.
    pub fn vcpu_count(mut self, count: u8) -> Self {
        self.config.vcpu_count = count;
        self
    }

    /// Set operation timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = Some(timeout);
        self
    }

    /// Set vsock guest CID (must be >= 3).
    pub fn vsock_cid(mut self, cid: u32) -> Self {
        self.config.vsock_cid = cid;
        self
    }

    /// Set the chroot/working directory path.
    pub fn chroot_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.chroot_path = path.into();
        self
    }

    /// Build the configuration, validating all required fields.
    pub fn build(self) -> Result<SandboxConfig, CoreError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = SandboxConfig::default();
        assert_eq!(config.memory_mib, 256);
        assert_eq!(config.vcpu_count, 2);
        assert!(config.timeout.is_none());
    }

    #[test]
    fn test_builder_validation_missing_kernel() {
        let result = SandboxConfig::builder()
            .rootfs("/path/to/rootfs.ext4")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_validation_missing_rootfs() {
        let result = SandboxConfig::builder().kernel("/path/to/vmlinux").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_success() {
        let config = SandboxConfig::builder()
            .kernel("/path/to/vmlinux")
            .rootfs("/path/to/rootfs.ext4")
            .memory_mib(512)
            .vcpu_count(4)
            .timeout(Duration::from_secs(60))
            .build()
            .expect("should build successfully");

        assert_eq!(config.kernel_path, PathBuf::from("/path/to/vmlinux"));
        assert_eq!(config.rootfs_path, PathBuf::from("/path/to/rootfs.ext4"));
        assert_eq!(config.memory_mib, 512);
        assert_eq!(config.vcpu_count, 4);
        assert_eq!(config.timeout, Some(Duration::from_secs(60)));
    }
}
