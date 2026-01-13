//! Machine configuration for Firecracker VMs.
//!
//! This module provides helpers to configure machine resources (vCPU, memory)
//! on Firecracker VMs via direct API calls, since firepilot's high-level API
//! doesn't expose machine configuration.

use crate::error::{Result, VmError};
use firepilot_models::models::MachineConfiguration;
use hyper::{Body, Client, Method, Request};
use hyperlocal::{UnixClientExt, Uri};
use std::path::Path;

/// Configure machine resources on a Firecracker instance.
///
/// This sends a PUT request to `/machine-config` on the Firecracker API socket.
/// **Must be called BEFORE starting the VM.**
///
/// # Arguments
/// * `socket_path` - Path to the Firecracker API socket
/// * `vcpu_count` - Number of virtual CPUs (1-32)
/// * `mem_size_mib` - Memory size in MiB (128-32768)
pub async fn configure_machine(
    socket_path: &Path,
    vcpu_count: u8,
    mem_size_mib: u32,
) -> Result<()> {
    tracing::debug!(vcpu_count, mem_size_mib, "Configuring machine resources");

    let config = MachineConfiguration::new(mem_size_mib as i32, vcpu_count as i32);

    let body = serde_json::to_string(&config)
        .map_err(|e| VmError::Config(format!("failed to serialize machine config: {e}")))?;

    let uri: hyper::Uri = Uri::new(socket_path, "/machine-config").into();

    let request = Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .map_err(|e| VmError::Config(format!("failed to build machine config request: {e}")))?;

    let client = Client::unix();
    let response = client
        .request(request)
        .await
        .map_err(|e| VmError::Firepilot(format!("machine config request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body_bytes = hyper::body::to_bytes(response.into_body())
            .await
            .unwrap_or_default();
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(VmError::Firepilot(format!(
            "machine config failed with status {}: {}",
            status, body_str
        )));
    }

    tracing::info!(vcpu_count, mem_size_mib, "Machine resources configured");
    Ok(())
}

#[cfg(test)]
mod tests {
    use firepilot_models::models::MachineConfiguration;

    #[test]
    fn test_machine_config_serialization() {
        let config = MachineConfiguration::new(256, 2);
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"mem_size_mib\":256"));
        assert!(json.contains("\"vcpu_count\":2"));
    }
}
