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

fn main() {
    // Early debug output (before any async/tracing setup)
    eprintln!("[bouvet-agent] Starting (pid: {})", std::process::id());
    eprintln!("[bouvet-agent] Building tokio runtime (current_thread for musl compatibility)...");

    // Use current_thread runtime - more reliable on musl systems than multi_thread
    // Multi-thread runtime can have issues with signal handling and thread spawning on musl
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => {
            eprintln!("[bouvet-agent] Tokio current_thread runtime created successfully");
            rt
        }
        Err(e) => {
            eprintln!(
                "[bouvet-agent] FATAL: Failed to create tokio runtime: {}",
                e
            );
            std::process::exit(1);
        }
    };

    // Run the async main
    match runtime.block_on(async_main()) {
        Ok(()) => {
            eprintln!("[bouvet-agent] Agent exited normally");
        }
        Err(e) => {
            eprintln!("[bouvet-agent] FATAL: Agent error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[bouvet-agent] async_main started");
    eprintln!("[bouvet-agent] Initializing tracing subscriber...");

    // Initialize tracing - write to stderr so it shows in console
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("bouvet_agent=debug".parse().unwrap()),
        )
        .init();

    eprintln!("[bouvet-agent] Tracing initialized, switching to structured logs");
    info!("bouvet-agent starting...");

    // Check vsock device exists
    eprintln!("[bouvet-agent] Checking /dev/vsock...");
    if !std::path::Path::new("/dev/vsock").exists() {
        let msg = "/dev/vsock does not exist - vsock kernel module may not be loaded";
        eprintln!("[bouvet-agent] FATAL: {}", msg);
        return Err(msg.into());
    }
    eprintln!("[bouvet-agent] /dev/vsock exists");

    // Create vsock listener on port 52 (accepts connections from any CID)
    eprintln!(
        "[bouvet-agent] Binding vsock listener on port {}...",
        GUEST_PORT
    );
    let addr = VsockAddr::new(VMADDR_CID_ANY, GUEST_PORT);

    let listener = match VsockListener::bind(addr) {
        Ok(l) => {
            eprintln!(
                "[bouvet-agent] Successfully bound to vsock port {}",
                GUEST_PORT
            );
            l
        }
        Err(e) => {
            eprintln!("[bouvet-agent] FATAL: Failed to bind vsock: {}", e);
            return Err(e.into());
        }
    };

    info!(port = GUEST_PORT, "listening on vsock");
    eprintln!("[bouvet-agent] Entering accept loop - ready for connections");

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                eprintln!(
                    "[bouvet-agent] Accepted connection from CID {} port {}",
                    peer_addr.cid(),
                    peer_addr.port()
                );
                debug!(
                    cid = peer_addr.cid(),
                    port = peer_addr.port(),
                    "accepted new connection"
                );
                // Handle connection in the same task (current_thread runtime)
                // Using spawn_local would require LocalSet, so we handle inline
                if let Err(e) = handle_connection(stream).await {
                    warn!(error = %e, "connection error");
                    eprintln!("[bouvet-agent] Connection error: {}", e);
                }
            }
            Err(e) => {
                error!(error = %e, "failed to accept connection");
                eprintln!("[bouvet-agent] Accept error: {}", e);
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
