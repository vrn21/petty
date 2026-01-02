//! Error types for petty-vm.

use thiserror::Error;

/// Result type alias for petty-vm operations.
pub type Result<T> = std::result::Result<T, VmError>;

/// Errors that can occur during VM operations.
#[derive(Debug, Error)]
pub enum VmError {
    /// Failed to create the VM
    #[error("failed to create VM: {0}")]
    Create(String),

    /// Failed to start the VM
    #[error("failed to start VM: {0}")]
    Start(String),

    /// Failed to stop the VM
    #[error("failed to stop VM: {0}")]
    Stop(String),

    /// VM is not in expected state
    #[error("invalid VM state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    /// Configuration error
    #[error("configuration error: {0}")]
    Config(String),

    /// Firecracker/firepilot error
    #[error("firepilot error: {0}")]
    Firepilot(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Timeout waiting for operation
    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),
}
