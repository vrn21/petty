//! bouvet-agent: Guest agent for bouvet microVMs.
//!
//! Listens on a Unix socket (vsock-compatible) and handles JSON-RPC requests
//! for command execution, code execution, and file operations.

mod exec;
mod fs;
mod handler;
mod protocol;

use handler::handle_request;
use protocol::{error_codes, Request, Response};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::{debug, error, info, warn};

/// Default socket path for the agent.
const SOCKET_PATH: &str = "/tmp/bouvet-agent.sock";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("bouvet_agent=debug".parse().unwrap()),
        )
        .init();

    info!("bouvet-agent starting...");

    // Remove existing socket file if it exists
    let socket_path = Path::new(SOCKET_PATH);
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
        debug!("removed existing socket file");
    }

    // Create Unix socket listener
    let listener = UnixListener::bind(SOCKET_PATH)?;
    info!(path = SOCKET_PATH, "listening for connections");

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                debug!("accepted new connection");
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream).await {
                        warn!(error = %e, "connection error");
                    }
                });
            }
            Err(e) => {
                error!(error = %e, "failed to accept connection");
            }
        }
    }
}

/// Handle a single client connection.
///
/// Reads newline-delimited JSON-RPC requests and writes responses.
async fn handle_connection(
    stream: tokio::net::UnixStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            debug!("client disconnected");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        debug!(request = %trimmed, "received request");

        // Parse request and handle
        let response = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => handle_request(req),
            Err(e) => {
                warn!(error = %e, "failed to parse request");
                Response::error(0, error_codes::PARSE_ERROR, format!("parse error: {}", e))
            }
        };

        // Serialize and send response
        let json = serde_json::to_string(&response)?;
        debug!(response = %json, "sending response");
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}
