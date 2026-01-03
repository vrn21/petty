//! Petty MCP Server entry point.
//!
//! This binary starts the MCP server using stdio transport,
//! suitable for integration with Claude Desktop, Cursor, and similar tools.

use petty_mcp::{PettyConfig, PettyServer};
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing - logs go to stderr (stdout is MCP transport)
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("petty_mcp=info".parse()?))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting Petty MCP Server");

    // Load configuration from environment
    let config = PettyConfig::from_env();
    tracing::info!(?config, "Configuration loaded");

    // Validate configuration (warn-only to support development environments)
    config.validate_warn();

    // Create the server
    let server = PettyServer::new(config);

    // Start the warm pool filler (if enabled)
    server.start_pool().await;

    // Get manager Arc and server clone before starting the service (for shutdown cleanup)
    let cleanup_manager = server.manager_arc();
    let cleanup_server = server.clone();

    // Start serving via stdio transport
    tracing::info!("Listening on stdio");
    let service = server.serve(stdio()).await?;

    // Set up graceful shutdown handler
    let shutdown_task = tokio::spawn(async move {
        // Wait for shutdown signal
        let _ = signal::ctrl_c().await;
        tracing::info!("Received shutdown signal, cleaning up...");

        // Shutdown the warm pool first
        cleanup_server.shutdown_pool().await;

        // Destroy all managed sandboxes
        if let Err(e) = cleanup_manager.destroy_all().await {
            tracing::error!("Error during sandbox cleanup: {e}");
        } else {
            tracing::info!("All sandboxes cleaned up");
        }
    });

    // Wait for the service to complete
    tokio::select! {
        result = service.waiting() => {
            if let Err(e) = result {
                tracing::error!("Service error: {e}");
            }
        }
        _ = shutdown_task => {
            tracing::info!("Shutdown complete");
        }
    }

    tracing::info!("Server shutdown complete");
    Ok(())
}
