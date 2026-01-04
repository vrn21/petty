//! Bouvet MCP Server entry point.
//!
//! This binary starts the MCP server using both stdio and HTTP/SSE transports
//! by default, suitable for both local AI tools (Claude Desktop, Cursor) and
//! remote AI agents.
//!
//! ## Transport Modes
//!
//! - **both** (default): Runs stdio + HTTP simultaneously
//! - **stdio**: Only stdio transport
//! - **http**: Only HTTP/SSE transport

use bouvet_mcp::{http, BouvetConfig, BouvetServer};
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tokio::signal;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing - logs go to stderr (stdout is MCP transport)
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("bouvet_mcp=info".parse()?))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting Bouvet MCP Server");

    // Load configuration from environment
    let config = BouvetConfig::from_env();
    tracing::info!(?config, "Configuration loaded");

    // Validate configuration (warn-only to support development environments)
    config.validate_warn();

    // Create the server
    let server = BouvetServer::new(config.clone());

    // Start the warm pool filler (if enabled)
    server.start_pool().await;

    // Create shutdown broadcast channel
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Get handles for cleanup
    let cleanup_manager = server.manager_arc();
    let cleanup_server = server.clone();

    // Spawn transports based on configuration
    let mut handles = Vec::new();

    // HTTP transport
    if config.transport_mode.http_enabled() {
        let http_server = server.clone();
        let http_addr = config.http_addr;
        let mut shutdown_rx = shutdown_tx.subscribe();

        let http_handle = tokio::spawn(async move {
            let shutdown = async move {
                let _ = shutdown_rx.recv().await;
            };

            if let Err(e) = http::serve(http_server, http_addr, shutdown).await {
                tracing::error!(error = %e, "HTTP server error");
            }
        });

        handles.push(http_handle);
        tracing::info!(addr = %config.http_addr, "HTTP/SSE transport enabled");
    }

    // Stdio transport
    if config.transport_mode.stdio_enabled() {
        let stdio_server = server.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();

        let stdio_handle = tokio::spawn(async move {
            match stdio_server.serve(stdio()).await {
                Ok(service) => {
                    tokio::select! {
                        result = service.waiting() => {
                            if let Err(e) = result {
                                tracing::error!(error = %e, "Stdio service error");
                            }
                        }
                        _ = async { shutdown_rx.recv().await } => {
                            tracing::info!("Stdio transport shutting down");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to start stdio transport");
                }
            }
        });

        handles.push(stdio_handle);
        tracing::info!("Stdio transport enabled");
    }

    // Log startup summary
    match config.transport_mode {
        bouvet_mcp::TransportMode::Both => {
            tracing::info!(
                http_addr = %config.http_addr,
                "Server ready (stdio + HTTP/SSE)"
            );
        }
        bouvet_mcp::TransportMode::Http => {
            tracing::info!(http_addr = %config.http_addr, "Server ready (HTTP/SSE only)");
        }
        bouvet_mcp::TransportMode::Stdio => {
            tracing::info!("Server ready (stdio only)");
        }
    }

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    tracing::info!("Received shutdown signal, cleaning up...");

    // Broadcast shutdown to all transports
    let _ = shutdown_tx.send(());

    // Shutdown the warm pool
    cleanup_server.shutdown_pool().await;

    // Destroy all managed sandboxes
    if let Err(e) = cleanup_manager.destroy_all().await {
        tracing::error!(error = %e, "Error during sandbox cleanup");
    } else {
        tracing::info!("All sandboxes cleaned up");
    }

    // Wait for transport handles to complete
    for handle in handles {
        let _ = handle.await;
    }

    tracing::info!("Server shutdown complete");
    Ok(())
}
