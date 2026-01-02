//! Tool parameter and response types for MCP tools.
//!
//! These types use serde for serialization and schemars for automatic
//! JSON Schema generation required by MCP.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Sandbox Lifecycle
// ============================================================================

/// Parameters for creating a new sandbox.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CreateSandboxParams {
    /// Memory in MiB (default: 256).
    #[serde(default)]
    pub memory_mib: Option<u32>,

    /// vCPU count (default: 2).
    #[serde(default)]
    pub vcpu_count: Option<u8>,
}

/// Result of creating a sandbox.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CreateSandboxResult {
    /// Unique identifier for the sandbox.
    pub sandbox_id: String,
}

/// Parameters for destroying a sandbox.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DestroySandboxParams {
    /// ID of the sandbox to destroy.
    pub sandbox_id: String,
}

/// Result of destroying a sandbox.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DestroySandboxResult {
    /// Whether the operation succeeded.
    pub success: bool,
}

/// Result of listing sandboxes.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListSandboxesResult {
    /// List of active sandbox information.
    pub sandboxes: Vec<SandboxInfo>,
}

/// Information about a sandbox.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SandboxInfo {
    /// Unique identifier for the sandbox.
    pub sandbox_id: String,
    /// Current state of the sandbox.
    pub state: String,
    /// When the sandbox was created (ISO 8601).
    pub created_at: String,
}

// ============================================================================
// Code Execution
// ============================================================================

/// Parameters for executing code.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteCodeParams {
    /// ID of the sandbox to execute in.
    pub sandbox_id: String,

    /// Language to execute (python, python3, node, javascript, bash, sh).
    pub language: String,

    /// Code to execute.
    pub code: String,
}

/// Parameters for running a shell command.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunCommandParams {
    /// ID of the sandbox to execute in.
    pub sandbox_id: String,

    /// Shell command to execute.
    pub command: String,
}

/// Result of code or command execution.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExecResponse {
    /// Exit code of the command (0 = success).
    pub exit_code: i32,

    /// Standard output.
    pub stdout: String,

    /// Standard error.
    pub stderr: String,
}

// ============================================================================
// File Operations
// ============================================================================

/// Parameters for reading a file.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileParams {
    /// ID of the sandbox.
    pub sandbox_id: String,

    /// Absolute path to the file.
    pub path: String,
}

/// Result of reading a file.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ReadFileResult {
    /// File contents.
    pub content: String,
}

/// Parameters for writing a file.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteFileParams {
    /// ID of the sandbox.
    pub sandbox_id: String,

    /// Absolute path to the file.
    pub path: String,

    /// Content to write.
    pub content: String,
}

/// Result of writing a file.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WriteFileResult {
    /// Whether the operation succeeded.
    pub success: bool,
}

/// Parameters for listing a directory.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListDirectoryParams {
    /// ID of the sandbox.
    pub sandbox_id: String,

    /// Absolute path to the directory.
    pub path: String,
}

/// Result of listing a directory.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListDirectoryResult {
    /// List of entries in the directory.
    pub entries: Vec<FileEntryResponse>,
}

/// Information about a file or directory entry.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FileEntryResponse {
    /// File or directory name.
    pub name: String,

    /// Whether this is a directory.
    pub is_dir: bool,

    /// File size in bytes (0 for directories).
    pub size: u64,
}
