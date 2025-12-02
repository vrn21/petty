//! Domain types used throughout the Petty sandbox platform.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for a sandbox session.
///
/// This is the user-facing ID returned when creating a sandbox.
/// It maps to an underlying VM ID internally.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SandboxId(String);

impl SandboxId {
    /// Create a new random sandbox ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create a sandbox ID from a string.
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// Get the inner string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SandboxId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SandboxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SandboxId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<SandboxId> for String {
    fn from(id: SandboxId) -> String {
        id.0
    }
}

/// Unique identifier for a VM instance.
///
/// This is the underlying VM ID used by the VM manager (e.g., Flintlock VM UID).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VMId(String);

impl VMId {
    /// Create a new random VM ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create a VM ID from a string.
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    /// Get the inner string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for VMId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for VMId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for VMId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<VMId> for String {
    fn from(id: VMId) -> String {
        id.0
    }
}

/// Status of a sandbox or VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Creating the sandbox/VM
    Creating,
    /// Running and ready to accept commands
    Running,
    /// Stopping
    Stopping,
    /// Stopped
    Stopped,
    /// Failed to create or encountered an error
    Failed,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Creating => write!(f, "creating"),
            Status::Running => write!(f, "running"),
            Status::Stopping => write!(f, "stopping"),
            Status::Stopped => write!(f, "stopped"),
            Status::Failed => write!(f, "failed"),
        }
    }
}

/// Metadata about a file in the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// File path
    pub path: String,
    /// Whether this is a directory
    pub is_dir: bool,
    /// File size in bytes (if not a directory)
    pub size: Option<u64>,
    /// Last modified timestamp
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_id_creation() {
        let id1 = SandboxId::new();
        let id2 = SandboxId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_sandbox_id_from_string() {
        let id_str = "test-123".to_string();
        let id = SandboxId::from_string(id_str.clone());
        assert_eq!(id.as_str(), "test-123");
    }

    #[test]
    fn test_vm_id_creation() {
        let id1 = VMId::new();
        let id2 = VMId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_status_display() {
        assert_eq!(Status::Running.to_string(), "running");
        assert_eq!(Status::Creating.to_string(), "creating");
    }
}
