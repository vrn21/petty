//! Type conversion between internal models and Flintlock gRPC types.

use crate::models::{VMConfig, VMInfo};
use super::grpc::flintlock::types as flintlock;
use super::grpc::microvm::services::api::v1alpha1::CreateMicroVmRequest;
use petty_common::{Result, types::{Status, VMId}};
use std::collections::HashMap;

pub fn to_create_request(config: &VMConfig, namespace: String) -> CreateMicroVmRequest {
    // Create Kernel spec
    let kernel = flintlock::Kernel {
        // image: config.image_name.clone(), // Removed duplicate
        // Wait, Flintlock usually expects kernel image to be a container image or path?
        // In config.rs we have kernel_path.
        // If kernel_path is a file on host, we might need a different way.
        // But Flintlock Kernel message has `image` and `filename`.
        // If we use host kernel, we might need to check how Flintlock handles it.
        // For now, let's assume we pass the kernel path as filename and maybe empty image if it's on host?
        // Or maybe config.image_name is the rootfs image.
        
        // Let's assume kernel_path is the filename in the image, OR we use a separate kernel image.
        // But our config has `kernel_path` (e.g. /var/lib/flintlock/kernels/vmlinux).
        // Flintlock Kernel spec:
        // string image = 1;
        // string filename = 3;
        
        // If we use a local kernel on the host, Flintlock might not support it directly via this API unless we mount it?
        // Actually, Flintlock config has a default kernel.
        // But here we specify it in the spec.
        
        // Let's look at the proto again.
        // Kernel.image is "container image to use".
        // Kernel.filename is "name of the kernel file in the Image".
        
        // If we want to use a host kernel, maybe we don't set this?
        // But MicroVMSpec.kernel is not optional.
        
        // Let's assume we use the same image for kernel and rootfs for now, or we need a kernel image.
        // In our config we have `kernel_path`.
        // Maybe we should map `kernel_path` to `filename` and `image_name` to `image`?
        // But `image_name` is for rootfs.
        
        // Let's use a placeholder for now and refine later.
        image: "weaveworks/ignite-kernel:5.10.51".to_string(), // Default kernel image?
        filename: Some("boot/vmlinux".to_string()),
        cmdline: HashMap::new(),
        add_network_config: false,
    };

    // Root volume
    let root_volume = flintlock::Volume {
        id: "root".to_string(),
        is_read_only: false,
        source: Some(flintlock::VolumeSource {
            container_source: Some(config.image_name.clone()),
            virtiofs_source: None,
        }),
        mount_point: None,
        partition_id: None,
        size_in_mb: Some(config.disk_size_mb as i32),
    };

    let spec = flintlock::MicroVmSpec {
        id: "".to_string(), // Let Flintlock generate it? Or we generate?
        namespace: namespace,
        vcpu: config.vcpu as i32,
        memory_in_mb: config.memory_mb as i32,
        kernel: Some(kernel),
        root_volume: Some(root_volume),
        additional_volumes: vec![],
        interfaces: vec![], // No network for now (vsock)
        metadata: config.metadata.clone(),
        ..Default::default()
    };

    CreateMicroVmRequest {
        microvm: Some(spec),
        metadata: HashMap::new(),
    }
}

pub fn from_microvm(vm: flintlock::MicroVm) -> Result<VMInfo> {
    let spec = vm.spec.ok_or_else(|| petty_common::Error::Internal("Missing spec in MicroVM".to_string()))?;
    let status = vm.status.ok_or_else(|| petty_common::Error::Internal("Missing status in MicroVM".to_string()))?;
    
    let id = VMId::from_string(spec.uid.unwrap_or_else(|| spec.id));
    
    // Map status
    // MicroVMState: PENDING=0, CREATED=1, FAILED=2, DELETING=3
    let vm_status = match status.state {
        0 => Status::Creating, // PENDING
        1 => Status::Running,  // CREATED (Flintlock considers Created as running/ready)
        2 => Status::Failed,   // FAILED
        3 => Status::Stopping, // DELETING
        _ => Status::Failed,
    };

    Ok(VMInfo {
        id,
        status: vm_status,
        vcpu: spec.vcpu as u32,
        memory_mb: spec.memory_in_mb as u32,
        image_name: spec.root_volume.and_then(|v| v.source).and_then(|s| s.container_source).unwrap_or_default(),
        vsock_cid: None, // Flintlock doesn't expose CID in API yet?
        created_at: chrono::Utc::now(), // Timestamp conversion needed
        metadata: spec.metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::VMConfig;

    #[test]
    fn test_to_create_request() {
        let config = VMConfig::new("ubuntu:22.04", "/boot/vmlinux")
            .with_vcpu(2)
            .with_memory_mb(1024)
            .with_disk_size_mb(2048)
            .with_metadata("foo", "bar");

        let request = to_create_request(&config, "default".to_string());
        let spec = request.microvm.unwrap();

        assert_eq!(spec.namespace, "default");
        assert_eq!(spec.vcpu, 2);
        assert_eq!(spec.memory_in_mb, 1024);
        assert_eq!(spec.metadata.get("foo"), Some(&"bar".to_string()));
        
        let root_vol = spec.root_volume.unwrap();
        assert_eq!(root_vol.size_in_mb, Some(2048));
        assert_eq!(root_vol.source.unwrap().container_source, Some("ubuntu:22.04".to_string()));
    }

    #[test]
    fn test_from_microvm() {
        let mut spec = flintlock::MicroVmSpec::default();
        spec.uid = Some("vm-123".to_string());
        spec.vcpu = 1;
        spec.memory_in_mb = 512;
        spec.root_volume = Some(flintlock::Volume {
            source: Some(flintlock::VolumeSource {
                container_source: Some("ubuntu".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });

        let mut status = flintlock::MicroVmStatus::default();
        status.state = 1; // CREATED

        let vm = flintlock::MicroVm {
            spec: Some(spec),
            status: Some(status),
            ..Default::default()
        };

        let info = from_microvm(vm).unwrap();
        assert_eq!(info.id.to_string(), "vm-123");
        assert!(info.is_running());
        assert_eq!(info.image_name, "ubuntu");
    }
}

