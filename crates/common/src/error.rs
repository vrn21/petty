//! Error types for the Petty sandbox platform.

use crate::types::{SandboxId, VMId};
use std::io;
use thiserror::Error;

/// Result type alias using our Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the Petty sandbox platform.
#[derive(Error, Debug)]
pub enum Error {
    /// Sandbox with the given ID was not found.
    #[error("Sandbox not found: {0}")]
    SandboxNotFound(SandboxId),

    /// VM with the given ID was not found.
    #[error("VM not found: {0}")]
    VMNotFound(VMId),

    /// Failed to create a VM.
    #[error("VM creation failed: {0}")]
    VMCreationFailed(String),

    /// Failed to destroy a VM.
    #[error("VM destruction failed: {0}")]
    VMDestructionFailed(String),

    /// Failed to communicate with the in-VM agent.
    #[error("Agent communication failed: {0}")]
    AgentCommunicationFailed(String),

    /// Command execution timed out.
    #[error("Command execution timed out after {0} seconds")]
    ExecutionTimeout(u64),

    /// Command execution failed.
    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    /// Failed to upload a file to the sandbox.
    #[error("File upload failed: {0}")]
    FileUploadFailed(String),

    /// Failed to download a file from the sandbox.
    #[error("File download failed: {0}")]
    FileDownloadFailed(String),

    /// Invalid configuration provided.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Invalid input or parameters.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Maximum concurrent sandboxes limit reached.
    #[error("Maximum concurrent sandboxes limit ({0}) reached")]
    MaxSandboxesReached(usize),

    /// vsock connection error.
    #[error("vsock connection error: {0}")]
    VsockError(String),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Internal error (unexpected condition).
    #[error("Internal error: {0}")]
    Internal(String),

    /// Service unavailable (e.g., VM manager unreachable).
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl Error {
    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::ServiceUnavailable(_)
                | Error::AgentCommunicationFailed(_)
                | Error::VsockError(_)
        )
    }

    /// Check if this error indicates a not-found condition.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::SandboxNotFound(_) | Error::VMNotFound(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let sandbox_id = SandboxId::from_string("test-123".to_string());
        let err = Error::SandboxNotFound(sandbox_id);
        assert_eq!(err.to_string(), "Sandbox not found: test-123");
    }

    #[test]
    fn test_is_retryable() {
        assert!(Error::ServiceUnavailable("test".to_string()).is_retryable());
        assert!(!Error::InvalidInput("test".to_string()).is_retryable());
    }

    #[test]
    fn test_is_not_found() {
        let sandbox_id = SandboxId::from_string("test".to_string());
        assert!(Error::SandboxNotFound(sandbox_id).is_not_found());
        assert!(!Error::Internal("test".to_string()).is_not_found());
    }
}
