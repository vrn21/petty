use super::grpc::microvm::services::api::v1alpha1::micro_vm_client::MicroVmClient;
use super::grpc::microvm::services::api::v1alpha1::{
    CreateMicroVmRequest, DeleteMicroVmRequest, GetMicroVmRequest, ListMicroVMsRequest,
};
use super::grpc::flintlock::types::MicroVm;
use tonic::transport::Channel;
use tonic::Request;

#[derive(Clone)]
pub struct FlintlockClient {
    client: MicroVmClient<Channel>,
}

impl FlintlockClient {
    pub async fn connect(endpoint: String) -> Result<Self, tonic::transport::Error> {
        let client = MicroVmClient::connect(endpoint).await?;
        Ok(Self { client })
    }

    pub async fn create_microvm(&self, request: CreateMicroVmRequest) -> Result<MicroVm, tonic::Status> {
        let mut client = self.client.clone();
        let response = client.create_micro_vm(Request::new(request)).await?;
        response.into_inner().microvm.ok_or_else(|| tonic::Status::internal("Missing microvm in response"))
    }

    pub async fn delete_microvm(&self, uid: String) -> Result<(), tonic::Status> {
        let mut client = self.client.clone();
        let request = DeleteMicroVmRequest { uid };
        client.delete_micro_vm(Request::new(request)).await?;
        Ok(())
    }

    pub async fn get_microvm(&self, uid: String) -> Result<MicroVm, tonic::Status> {
        let mut client = self.client.clone();
        let request = GetMicroVmRequest { uid };
        let response = client.get_micro_vm(Request::new(request)).await?;
        response.into_inner().microvm.ok_or_else(|| tonic::Status::internal("Missing microvm in response"))
    }

    pub async fn list_microvms(&self, namespace: String) -> Result<Vec<MicroVm>, tonic::Status> {
        let mut client = self.client.clone();
        let request = ListMicroVMsRequest {
            namespace,
            name: None,
        };
        let response = client.list_micro_v_ms(Request::new(request)).await?;
        Ok(response.into_inner().microvm)
    }
}
