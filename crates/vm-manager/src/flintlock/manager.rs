use crate::manager::VMManager;
use crate::models::{VMConfig, VMInfo};
use crate::flintlock::client::FlintlockClient;
use crate::flintlock::mapper;
use async_trait::async_trait;
use petty_common::{Result, types::VMId};
use petty_common::config::FlintlockConfig;

pub struct FlintlockVMManager {
    client: FlintlockClient,
    config: FlintlockConfig,
}

impl FlintlockVMManager {
    pub async fn new(config: FlintlockConfig) -> Result<Self> {
        let client = FlintlockClient::connect(config.endpoint.clone())
            .await
            .map_err(|e| petty_common::Error::ServiceUnavailable(format!("Failed to connect to Flintlock: {}", e)))?;
            
        Ok(Self { client, config })
    }
}

#[async_trait]
impl VMManager for FlintlockVMManager {
    async fn create_vm(&self, config: VMConfig) -> Result<VMId> {
        let request = mapper::to_create_request(&config, self.config.namespace.clone());
        let vm = self.client.create_microvm(request).await
            .map_err(|e| petty_common::Error::VMCreationFailed(e.message().to_string()))?;
            
        let spec = vm.spec.ok_or_else(|| petty_common::Error::Internal("Missing spec".to_string()))?;
        let uid = spec.uid.unwrap_or_else(|| spec.id);
        
        Ok(VMId::from_string(uid))
    }

    async fn destroy_vm(&self, vm_id: &VMId) -> Result<()> {
        self.client.delete_microvm(vm_id.to_string()).await
            .map_err(|e| petty_common::Error::VMDestructionFailed(e.message().to_string()))
    }

    async fn get_vm_info(&self, vm_id: &VMId) -> Result<VMInfo> {
        let vm = self.client.get_microvm(vm_id.to_string()).await
            .map_err(|e| {
                if e.code() == tonic::Code::NotFound {
                    petty_common::Error::VMNotFound(vm_id.clone())
                } else {
                    petty_common::Error::Internal(e.message().to_string())
                }
            })?;
            
        mapper::from_microvm(vm)
    }

    async fn list_vms(&self) -> Result<Vec<VMInfo>> {
        let vms = self.client.list_microvms(self.config.namespace.clone()).await
            .map_err(|e| petty_common::Error::Internal(e.message().to_string()))?;
            
        vms.into_iter()
            .map(mapper::from_microvm)
            .collect()
    }
}
