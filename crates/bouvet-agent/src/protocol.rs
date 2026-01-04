//! JSON-RPC 2.0 protocol types for petty-agent.
//!
//! Implements the JSON-RPC 2.0 specification for guest-host communication.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 standard error codes.
pub mod error_codes {
    /// Parse error - Invalid JSON was received.
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid Request - The JSON sent is not a valid Request object.
    #[allow(dead_code)]
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found - The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params - Invalid method parameter(s).
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error - Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct Request {
    /// Protocol version, must be "2.0".
    #[allow(dead_code)]
    pub jsonrpc: String,
    /// Request identifier.
    pub id: u64,
    /// Method name to invoke.
    pub method: String,
    /// Method parameters (can be object or array).
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct Response {
    /// Protocol version, always "2.0".
    pub jsonrpc: String,
    /// Request identifier (matches request).
    pub id: u64,
    /// Result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// Create a success response.
    pub fn success(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct RpcError {
    /// Error code.
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
    /// Additional error data (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Result of command execution.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecResult {
    /// Process exit code (-1 if the process couldn't be started).
    pub exit_code: i32,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

impl ExecResult {
    /// Create an error result (for when command execution fails).
    pub fn error(message: &str) -> Self {
        Self {
            exit_code: -1,
            stdout: String::new(),
            stderr: message.to_string(),
        }
    }
}

/// File entry for directory listing.
#[derive(Debug, Serialize)]
pub struct FileEntry {
    /// File or directory name.
    pub name: String,
    /// True if this is a directory.
    pub is_dir: bool,
    /// File size in bytes (0 for directories).
    pub size: u64,
}

// Parameter types for various methods

/// Parameters for the `exec` method.
#[derive(Debug, Deserialize)]
pub struct ExecParams {
    /// Shell command to execute.
    pub cmd: String,
}

/// Parameters for the `exec_code` method.
#[derive(Debug, Deserialize)]
pub struct ExecCodeParams {
    /// Programming language (python, python3, node, javascript, bash, sh).
    pub lang: String,
    /// Code to execute.
    pub code: String,
}

/// Parameters for the `read_file` method.
#[derive(Debug, Deserialize)]
pub struct ReadFileParams {
    /// Path to the file to read.
    pub path: String,
}

/// Parameters for the `write_file` method.
#[derive(Debug, Deserialize)]
pub struct WriteFileParams {
    /// Path to the file to write.
    pub path: String,
    /// Content to write.
    pub content: String,
}

/// Parameters for the `list_dir` method.
#[derive(Debug, Deserialize)]
pub struct ListDirParams {
    /// Path to the directory to list.
    pub path: String,
}
