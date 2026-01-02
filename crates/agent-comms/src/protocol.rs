use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

// Command execution params
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteParams {
    pub command: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// File operations
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadParams {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadParams {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadResult {
    pub content_base64: String,
}
