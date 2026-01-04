//! Error types for bouvet-core.

use crate::SandboxId;
use thiserror::Error;

/// Result type alias for bouvet-core operations.
pub type Result<T> = std::result::Result<T, CoreError>;

/// Errors that can occur during sandbox operations.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Error from bouvet-vm
    #[error("VM error: {0}")]
    Vm(#[from] bouvet_vm::VmError),

    /// Failed to connect to guest agent
    #[error("connection failed: {0}")]
    Connection(String),

    /// Agent did not respond in time
    #[error("agent timeout after {0:?}")]
    AgentTimeout(std::time::Duration),

    /// JSON-RPC error from agent
    #[error("RPC error {code}: {message}")]
    Rpc {
        /// Error code from the agent
        code: i32,
        /// Error message from the agent
        message: String,
    },

    /// Sandbox not found
    #[error("sandbox not found: {0}")]
    NotFound(SandboxId),

    /// Invalid sandbox state for operation
    #[error("invalid state: expected {expected}, got {actual}")]
    InvalidState {
        /// Expected state
        expected: String,
        /// Actual state
        actual: String,
    },

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
