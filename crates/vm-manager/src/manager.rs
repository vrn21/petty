//! VM manager trait and implementations.

use crate::models::{VMConfig, VMInfo};
use async_trait::async_trait;
use petty_common::{types::VMId, Result};

/// Trait for managing VM lifecycle operations.
///
/// This abstraction allows different VM backend implementations
/// (Flintlock, custom Firecracker, etc.) to be swapped without
/// changing the orchestrator logic.
#[async_trait]
pub trait VMManager: Send + Sync {
    /// Create a new VM with the given configuration.
    ///
    /// # Arguments
    /// * `config` - VM configuration including resources and image
    ///
    /// # Returns
    /// The unique VM ID on success.
    ///
    /// # Errors
    /// Returns an error if VM creation fails.
    async fn create_vm(&self, config: VMConfig) -> Result<VMId>;

    /// Destroy a VM and clean up its resources.
    ///
    /// # Arguments
    /// * `vm_id` - ID of the VM to destroy
    ///
    /// # Errors
    /// Returns an error if the VM doesn't exist or destruction fails.
    async fn destroy_vm(&self, vm_id: &VMId) -> Result<()>;

    /// Get information about a specific VM.
    ///
    /// # Arguments
    /// * `vm_id` - ID of the VM to query
    ///
    /// # Returns
    /// VM information including status and configuration.
    ///
    /// # Errors
    /// Returns an error if the VM doesn't exist.
    async fn get_vm_info(&self, vm_id: &VMId) -> Result<VMInfo>;

    /// List all VMs managed by this manager.
    ///
    /// # Returns
    /// A list of all VM information.
    async fn list_vms(&self) -> Result<Vec<VMInfo>>;

    /// Wait for a VM to reach running state.
    ///
    /// # Arguments
    /// * `vm_id` - ID of the VM to wait for
    /// * `timeout` - Maximum time to wait
    ///
    /// # Errors
    /// Returns an error if the VM fails to start or timeout is reached.
    async fn wait_for_vm_ready(&self, vm_id: &VMId, timeout: std::time::Duration) -> Result<()> {
        use tokio::time::{sleep, Duration};
        
        let start = std::time::Instant::now();
        loop {
            let info = self.get_vm_info(vm_id).await?;
            
            if info.is_running() {
                return Ok(());
            }
            
            if info.is_terminal() {
                return Err(petty_common::Error::VMCreationFailed(
                    format!("VM entered terminal state: {:?}", info.status),
                ));
            }
            
            if start.elapsed() >= timeout {
                return Err(petty_common::Error::ExecutionTimeout(timeout.as_secs()));
            }
            
            sleep(Duration::from_millis(500)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use petty_common::types::Status;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // Mock VM manager for testing
    struct MockVMManager {
        vms: Arc<Mutex<HashMap<VMId, VMInfo>>>,
    }

    impl MockVMManager {
        fn new() -> Self {
            Self {
                vms: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl VMManager for MockVMManager {
        async fn create_vm(&self, config: VMConfig) -> Result<VMId> {
            let vm_id = VMId::new();
            let mut info = VMInfo::new(vm_id.clone(), &config);
            info.status = Status::Running;
            info.vsock_cid = Some(1234);
            
            self.vms.lock().unwrap().insert(vm_id.clone(), info);
            Ok(vm_id)
        }

        async fn destroy_vm(&self, vm_id: &VMId) -> Result<()> {
            self.vms
                .lock()
                .unwrap()
                .remove(vm_id)
                .ok_or_else(|| petty_common::Error::VMNotFound(vm_id.clone()))?;
            Ok(())
        }

        async fn get_vm_info(&self, vm_id: &VMId) -> Result<VMInfo> {
            self.vms
                .lock()
                .unwrap()
                .get(vm_id)
                .cloned()
                .ok_or_else(|| petty_common::Error::VMNotFound(vm_id.clone()))
        }

        async fn list_vms(&self) -> Result<Vec<VMInfo>> {
            Ok(self.vms.lock().unwrap().values().cloned().collect())
        }
    }

    #[tokio::test]
    async fn test_mock_vm_manager() {
        let manager = MockVMManager::new();
        let config = VMConfig::new("ubuntu:22.04", "/path/to/kernel");
        
        // Create VM
        let vm_id = manager.create_vm(config).await.unwrap();
        
        // Get info
        let info = manager.get_vm_info(&vm_id).await.unwrap();
        assert!(info.is_running());
        assert_eq!(info.vsock_cid, Some(1234));
        
        // List VMs
        let vms = manager.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        
        // Destroy VM
        manager.destroy_vm(&vm_id).await.unwrap();
        
        // Verify destroyed
        let result = manager.get_vm_info(&vm_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wait_for_vm_ready() {
        let manager = MockVMManager::new();
        let config = VMConfig::new("ubuntu:22.04", "/path/to/kernel");
        let vm_id = manager.create_vm(config).await.unwrap();
        
        // Should succeed immediately since mock creates running VMs
        let result = manager
            .wait_for_vm_ready(&vm_id, std::time::Duration::from_secs(5))
            .await;
        assert!(result.is_ok());
    }
}
