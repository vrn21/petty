//! Models for VM configuration and information.

use petty_common::types::{Status, VMId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for creating a new VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMConfig {
    /// Number of vCPUs to allocate
    pub vcpu: u32,
    /// Memory in MB
    pub memory_mb: u32,
    /// Disk size in MB
    pub disk_size_mb: u32,
    /// Container image to use for the root filesystem
    pub image_name: String,
    /// Path to the kernel image
    pub kernel_path: String,
    /// Additional kernel command line arguments
    pub kernel_args: Option<String>,
    /// Metadata to pass to the VM (e.g., cloud-init)
    pub metadata: HashMap<String, String>,
}

impl VMConfig {
    /// Create a new VM configuration with the given image and kernel.
    pub fn new(image_name: impl Into<String>, kernel_path: impl Into<String>) -> Self {
        Self {
            vcpu: 1,
            memory_mb: 512,
            disk_size_mb: 1024,
            image_name: image_name.into(),
            kernel_path: kernel_path.into(),
            kernel_args: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the number of vCPUs.
    pub fn with_vcpu(mut self, vcpu: u32) -> Self {
        self.vcpu = vcpu;
        self
    }

    /// Set the memory in MB.
    pub fn with_memory_mb(mut self, memory_mb: u32) -> Self {
        self.memory_mb = memory_mb;
        self
    }

    /// Set the disk size in MB.
    pub fn with_disk_size_mb(mut self, disk_size_mb: u32) -> Self {
        self.disk_size_mb = disk_size_mb;
        self
    }

    /// Add metadata key-value pair.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set kernel command line arguments.
    pub fn with_kernel_args(mut self, args: impl Into<String>) -> Self {
        self.kernel_args = Some(args.into());
        self
    }
}

/// Information about a running or created VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMInfo {
    /// Unique identifier for the VM
    pub id: VMId,
    /// Current status of the VM
    pub status: Status,
    /// vCPU count
    pub vcpu: u32,
    /// Memory in MB
    pub memory_mb: u32,
    /// Container image used
    pub image_name: String,
    /// vsock context ID (for communication)
    pub vsock_cid: Option<u32>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl VMInfo {
    /// Create a new VMInfo with the given ID and configuration.
    pub fn new(id: VMId, config: &VMConfig) -> Self {
        Self {
            id,
            status: Status::Creating,
            vcpu: config.vcpu,
            memory_mb: config.memory_mb,
            image_name: config.image_name.clone(),
            vsock_cid: None,
            created_at: chrono::Utc::now(),
            metadata: config.metadata.clone(),
        }
    }

    /// Check if the VM is in a running state.
    pub fn is_running(&self) -> bool {
        self.status == Status::Running
    }

    /// Check if the VM is in a terminal state (stopped or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, Status::Stopped | Status::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_config_builder() {
        let config = VMConfig::new("ubuntu:22.04", "/path/to/kernel")
            .with_vcpu(2)
            .with_memory_mb(1024)
            .with_metadata("key", "value");

        assert_eq!(config.vcpu, 2);
        assert_eq!(config.memory_mb, 1024);
        assert_eq!(config.metadata.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_vm_info_creation() {
        let config = VMConfig::new("ubuntu:22.04", "/path/to/kernel");
        let vm_id = VMId::new();
        let info = VMInfo::new(vm_id.clone(), &config);

        assert_eq!(info.id, vm_id);
        assert_eq!(info.status, Status::Creating);
        assert!(!info.is_running());
    }

    #[test]
    fn test_vm_info_status_checks() {
        let config = VMConfig::new("ubuntu:22.04", "/path/to/kernel");
        let mut info = VMInfo::new(VMId::new(), &config);

        info.status = Status::Running;
        assert!(info.is_running());
        assert!(!info.is_terminal());

        info.status = Status::Failed;
        assert!(!info.is_running());
        assert!(info.is_terminal());
    }
}
