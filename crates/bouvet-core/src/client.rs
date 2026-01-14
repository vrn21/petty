//! Agent client for communicating with bouvet-agent inside a VM.
//!
//! This module implements the vsock connection protocol and JSON-RPC
//! message exchange with the guest agent.

use crate::error::CoreError;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;
use tokio::time::timeout;

/// Guest port that bouvet-agent listens on.
const GUEST_PORT: u32 = 52;

/// Total timeout for connecting to the agent (includes retry time).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Interval between connection retry attempts.
const RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Timeout for individual RPC calls.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Client for communicating with bouvet-agent inside a VM.
///
/// This client connects to the guest agent via Firecracker's vsock Unix socket
/// and exchanges JSON-RPC 2.0 messages.
pub struct AgentClient {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: BufWriter<tokio::io::WriteHalf<UnixStream>>,
    next_id: u64,
}

impl AgentClient {
    /// Connect to the agent via Firecracker's vsock Unix socket.
    ///
    /// This performs the vsock handshake and waits for the agent to be ready.
    /// The connection will be retried with a 100ms interval for up to 10 seconds
    /// to allow time for the VM to boot and the agent to start.
    ///
    /// # Arguments
    ///
    /// * `vsock_path` - Path to the vsock Unix socket (e.g., `/tmp/bouvet/vm-1/v.sock`)
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established within the timeout.
    pub async fn connect(vsock_path: &Path) -> Result<Self, CoreError> {
        let start = std::time::Instant::now();
        tracing::debug!(path = %vsock_path.display(), "Connecting to agent");

        // Retry loop: agent may not be ready immediately after VM boot
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            match Self::try_connect(vsock_path).await {
                Ok(client) => {
                    tracing::info!(
                        path = %vsock_path.display(),
                        elapsed_ms = start.elapsed().as_millis() as u64,
                        attempts,
                        "Connected to agent"
                    );
                    return Ok(client);
                }
                Err(e) => {
                    if start.elapsed() >= CONNECT_TIMEOUT {
                        tracing::warn!(
                            path = %vsock_path.display(),
                            elapsed_ms = start.elapsed().as_millis() as u64,
                            attempts,
                            "Agent connection timeout"
                        );
                        return Err(CoreError::AgentTimeout(CONNECT_TIMEOUT));
                    }
                    tracing::trace!(error = %e, attempt = attempts, "Connection attempt failed, retrying...");
                    tokio::time::sleep(RETRY_INTERVAL).await;
                }
            }
        }
    }

    /// Attempt a single connection to the vsock socket.
    async fn try_connect(vsock_path: &Path) -> Result<Self, CoreError> {
        // 1. Connect to the Unix socket
        tracing::trace!(path = %vsock_path.display(), "Attempting socket connection");
        let stream = UnixStream::connect(vsock_path)
            .await
            .map_err(|e| CoreError::Connection(format!("socket connect failed: {e}")))?;

        let (read_half, write_half) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        // 2. Send CONNECT handshake
        tracing::trace!(port = GUEST_PORT, "Sending CONNECT handshake");
        writer
            .write_all(format!("CONNECT {GUEST_PORT}\n").as_bytes())
            .await
            .map_err(|e| CoreError::Connection(format!("handshake write failed: {e}")))?;
        writer.flush().await?;

        // 3. Read response
        let mut response = String::new();
        reader.read_line(&mut response).await?;

        if !response.starts_with("OK ") {
            tracing::debug!(response = %response.trim(), "Handshake failed");
            return Err(CoreError::Connection(format!(
                "handshake failed: {}",
                response.trim()
            )));
        }

        tracing::debug!(response = %response.trim(), "vsock handshake successful");

        Ok(Self {
            reader,
            writer,
            next_id: 1,
        })
    }

    /// Send a JSON-RPC request and wait for response.
    ///
    /// # Type Parameters
    ///
    /// * `P` - Parameter type (must be Serialize)
    /// * `R` - Result type (must be DeserializeOwned)
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, times out, or the agent returns an error.
    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: P,
    ) -> Result<R, CoreError> {
        let id = self.next_id;
        self.next_id += 1;

        // Build request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Send request (newline-delimited)
        let request_str = serde_json::to_string(&request)?;
        tracing::debug!(method = %method, id, "Sending RPC request");
        tracing::trace!(request = %request_str, "RPC request body");

        self.writer.write_all(request_str.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        // Read response with timeout
        let mut response_str = String::new();
        match timeout(RPC_TIMEOUT, self.reader.read_line(&mut response_str)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(method = %method, id, error = %e, "RPC read error");
                return Err(e.into());
            }
            Err(_) => {
                tracing::warn!(method = %method, id, timeout_secs = RPC_TIMEOUT.as_secs(), "RPC response timeout");
                return Err(CoreError::Rpc {
                    code: -1,
                    message: "response timeout".into(),
                });
            }
        }

        tracing::trace!(response = %response_str.trim(), "RPC response body");

        // Parse response
        let response: serde_json::Value = serde_json::from_str(&response_str)?;

        // Check for error
        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let message = error
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
                .to_string();
            tracing::debug!(method = %method, id, code, message = %message, "RPC error response");
            return Err(CoreError::Rpc { code, message });
        }

        // Extract result
        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| CoreError::Rpc {
                code: -1,
                message: "missing result in response".into(),
            })?;

        tracing::debug!(method = %method, id, "RPC call successful");
        serde_json::from_value(result).map_err(CoreError::from)
    }

    /// Ping the agent to check if it's responsive.
    pub async fn ping(&mut self) -> Result<(), CoreError> {
        let _: PingResponse = self.call("ping", serde_json::json!({})).await?;
        Ok(())
    }

    /// Execute a shell command.
    pub async fn exec(&mut self, cmd: &str) -> Result<ExecResult, CoreError> {
        tracing::debug!(cmd = %cmd, "Executing command via agent");
        self.call("exec", serde_json::json!({ "cmd": cmd })).await
    }

    /// Execute code in a specific language.
    ///
    /// # Arguments
    ///
    /// * `lang` - Language identifier (python, python3, node, javascript, bash, sh)
    /// * `code` - Code to execute
    pub async fn exec_code(&mut self, lang: &str, code: &str) -> Result<ExecResult, CoreError> {
        tracing::debug!(lang = %lang, code_len = code.len(), "Executing code via agent");
        self.call(
            "exec_code",
            serde_json::json!({ "lang": lang, "code": code }),
        )
        .await
    }

    /// Read a file from the guest filesystem.
    pub async fn read_file(&mut self, path: &str) -> Result<String, CoreError> {
        tracing::debug!(path = %path, "Reading file from guest");
        let resp: ReadFileResponse = self
            .call("read_file", serde_json::json!({ "path": path }))
            .await?;
        Ok(resp.content)
    }

    /// Write a file to the guest filesystem.
    pub async fn write_file(&mut self, path: &str, content: &str) -> Result<(), CoreError> {
        tracing::debug!(path = %path, content_len = content.len(), "Writing file to guest");
        let _: WriteFileResponse = self
            .call(
                "write_file",
                serde_json::json!({ "path": path, "content": content }),
            )
            .await?;
        Ok(())
    }

    /// List directory contents.
    pub async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>, CoreError> {
        tracing::debug!(path = %path, "Listing directory on guest");
        let resp: ListDirResponse = self
            .call("list_dir", serde_json::json!({ "path": path }))
            .await?;
        tracing::trace!(count = resp.entries.len(), "Directory entries received");
        Ok(resp.entries)
    }
}

/// Result from command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Process exit code (-1 if the process couldn't be started).
    pub exit_code: i32,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

impl ExecResult {
    /// Check if the command succeeded (exit code 0).
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// File entry from directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// File or directory name.
    pub name: String,
    /// True if this is a directory.
    pub is_dir: bool,
    /// File size in bytes (0 for directories).
    pub size: u64,
}

// Internal response types to match bouvet-agent's JSON structure

#[derive(Debug, Deserialize)]
struct PingResponse {
    #[allow(dead_code)]
    pong: bool,
}

#[derive(Debug, Deserialize)]
struct ReadFileResponse {
    content: String,
}

#[derive(Debug, Deserialize)]
struct WriteFileResponse {
    #[allow(dead_code)]
    success: bool,
}

#[derive(Debug, Deserialize)]
struct ListDirResponse {
    entries: Vec<FileEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_result_success() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "hello".to_string(),
            stderr: String::new(),
        };
        assert!(result.success());
    }

    #[test]
    fn test_exec_result_failure() {
        let result = ExecResult {
            exit_code: 1,
            stdout: String::new(),
            stderr: "error".to_string(),
        };
        assert!(!result.success());
    }
}
