//! bouvet-agent: Guest agent for bouvet microVMs.
//!
//! Listens on a vsock port inside the VM and handles JSON-RPC requests
//! for command execution, code execution, and file operations.

mod exec;
mod fs;
mod handler;
mod protocol;

use handler::handle_request;
use protocol::{error_codes, Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio_vsock::{VsockAddr, VsockListener, VsockStream, VMADDR_CID_ANY};
use tracing::{debug, error, info, warn};

/// Guest port that bouvet-agent listens on.
const GUEST_PORT: u32 = 52;

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

    // Create vsock listener on port 52 (accepts connections from any CID)
    let addr = VsockAddr::new(VMADDR_CID_ANY, GUEST_PORT);
    let listener = VsockListener::bind(addr)?;
    info!(port = GUEST_PORT, "listening on vsock");

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                debug!(
                    cid = peer_addr.cid(),
                    port = peer_addr.port(),
                    "accepted new connection"
                );
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
/// First handles the Firecracker vsock CONNECT handshake if present.
async fn handle_connection(
    mut stream: VsockStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (read_half, write_half) = stream.split();
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(write_half);
    let mut line = String::new();

    // Handle potential CONNECT handshake from Firecracker vsock proxy
    // The host connects to our vsock socket via Unix socket,
    // Firecracker forwards this using the CONNECT protocol
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        debug!("client disconnected before sending data");
        return Ok(());
    }

    // Check if this is a CONNECT handshake
    let trimmed = line.trim();
    if let Some(port_str) = trimmed.strip_prefix("CONNECT ") {
        // Parse the port and send OK response
        let port: u32 = match port_str.parse() {
            Ok(p) => p,
            Err(_) => {
                warn!(port_str = %port_str, "Invalid port in CONNECT, using default");
                GUEST_PORT
            }
        };
        debug!(port = port, "received CONNECT handshake");

        writer
            .write_all(format!("OK {}\n", port).as_bytes())
            .await?;
        writer.flush().await?;

        // Clear line for normal request processing
        line.clear();
    } else if !trimmed.is_empty() {
        // First line was not a CONNECT, treat it as a JSON request
        debug!(
            request_preview = %if trimmed.len() > 200 { &trimmed[..200] } else { trimmed },
            request_len = trimmed.len(),
            "received request (no handshake)"
        );

        let response = match serde_json::from_str::<Request>(trimmed) {
            Ok(req) => handle_request(req),
            Err(e) => {
                warn!(error = %e, "failed to parse request");
                Response::error(0, error_codes::PARSE_ERROR, format!("parse error: {}", e))
            }
        };

        let json = serde_json::to_string(&response)?;
        debug!(response = %json, "sending response");
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    // Normal JSON-RPC request loop
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

        debug!(
            request_preview = %if trimmed.len() > 200 { &trimmed[..200] } else { trimmed },
            request_len = trimmed.len(),
            "received request"
        );

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
