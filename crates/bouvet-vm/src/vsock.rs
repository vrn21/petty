//! vsock configuration for guest-host communication.
//!
//! This module provides helpers to configure vsock on Firecracker VMs
//! via direct API calls, since firepilot's high-level API doesn't expose it.

use crate::config::VsockConfig;
use crate::error::{Result, VmError};
use firepilot_models::models::Vsock;
use hyper::{Body, Client, Method, Request};
use hyperlocal::{UnixClientExt, Uri};
use std::path::Path;

/// Configure vsock on a running Firecracker instance.
///
/// This sends a PUT request to `/vsock` on the Firecracker API socket.
///
/// # Arguments
/// * `socket_path` - Path to the Firecracker API socket (e.g., `/tmp/bouvet/vm-1/firecracker.socket`)
/// * `config` - vsock configuration with guest CID and UDS path
pub async fn configure_vsock(socket_path: &Path, config: &VsockConfig) -> Result<()> {
    tracing::debug!(
        cid = config.guest_cid,
        uds_path = %config.uds_path.display(),
        "Configuring vsock"
    );

    let vsock = Vsock::new(
        config.guest_cid as i32,
        config.uds_path.to_string_lossy().to_string(),
    );

    let body = serde_json::to_string(&vsock)
        .map_err(|e| VmError::Config(format!("failed to serialize vsock config: {e}")))?;

    let uri: hyper::Uri = Uri::new(socket_path, "/vsock").into();

    let request = Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .map_err(|e| VmError::Config(format!("failed to build vsock request: {e}")))?;

    let client = Client::unix();
    let response = client
        .request(request)
        .await
        .map_err(|e| VmError::Firepilot(format!("vsock configuration request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body_bytes = hyper::body::to_bytes(response.into_body())
            .await
            .unwrap_or_default();
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(VmError::Firepilot(format!(
            "vsock configuration failed with status {}: {}",
            status, body_str
        )));
    }

    tracing::info!(cid = config.guest_cid, "vsock configured successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_vsock_config_default() {
        let config = VsockConfig::default();
        assert_eq!(config.guest_cid, 3);
        assert_eq!(config.uds_path, PathBuf::from("/tmp/bouvet-vsock.sock"));
    }

    #[test]
    fn test_vsock_serialization() {
        let vsock = Vsock::new(5, "/tmp/test.sock".to_string());
        let json = serde_json::to_string(&vsock).unwrap();
        assert!(json.contains("\"guest_cid\":5"));
        assert!(json.contains("\"uds_path\":\"/tmp/test.sock\""));
    }
}
