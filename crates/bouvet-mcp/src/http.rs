//! HTTP/SSE transport for remote AI agents.
//!
//! This module provides an HTTP server that exposes the MCP protocol via
//! rmcp's StreamableHttpService, enabling remote AI agents to interact
//! with Bouvet sandboxes.
//!
//! ## Endpoints
//!
//! - `POST /mcp` - JSON-RPC requests
//! - `GET /mcp` - SSE stream for server-initiated messages
//! - `GET /health` - Health check
//! - `GET /` - Server info

use crate::server::BouvetServer;
use axum::{
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Build the HTTP router for the MCP server.
///
/// The returned router can be served directly with axum or composed
/// into a larger application.
pub fn build_router(server: BouvetServer) -> Router {
    tracing::debug!("Building HTTP router");

    // Create session manager for handling MCP sessions
    let session_manager = Arc::new(LocalSessionManager::default());

    // Create the StreamableHttpService from rmcp
    let mcp_service = StreamableHttpService::new(
        move || Ok(server.clone()),
        session_manager,
        StreamableHttpServerConfig::default(),
    );

    // Build the router
    let router = Router::new()
        // Health check
        .route("/health", get(health_handler))
        // Server info at root
        .route("/", get(root_handler))
        // MCP endpoint as a fallback/nested service
        .fallback_service(mcp_service)
        // Add middleware
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http());

    tracing::debug!("HTTP router built with routes: /, /health, /mcp");
    router
}

/// Health check endpoint.
async fn health_handler() -> impl IntoResponse {
    tracing::trace!("Health check request");
    Json(serde_json::json!({
        "status": "healthy",
        "service": "bouvet-mcp"
    }))
}

/// Root endpoint with server info.
async fn root_handler() -> impl IntoResponse {
    tracing::trace!("Root page request");
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Bouvet MCP Server</title>
    <style>
        body { font-family: system-ui; max-width: 800px; margin: 50px auto; padding: 20px; }
        code { background: #f4f4f4; padding: 2px 6px; border-radius: 3px; }
        pre { background: #f4f4f4; padding: 16px; border-radius: 6px; overflow-x: auto; }
    </style>
</head>
<body>
    <h1>🔥 Bouvet MCP Server</h1>
    <p>Model Context Protocol server for isolated code execution sandboxes.</p>
    
    <h2>Endpoints</h2>
    <ul>
        <li><code>POST /mcp</code> - MCP JSON-RPC requests</li>
        <li><code>GET /mcp</code> - SSE stream for server messages</li>
        <li><code>GET /health</code> - Health check</li>
    </ul>
    
    <h2>Example</h2>
    <pre>curl -X POST http://localhost:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'</pre>
    
    <h2>Available Tools</h2>
    <ul>
        <li><code>create_sandbox</code> - Create a new isolated sandbox</li>
        <li><code>destroy_sandbox</code> - Destroy a sandbox</li>
        <li><code>list_sandboxes</code> - List active sandboxes</li>
        <li><code>execute_code</code> - Execute code (Python, Node, Bash)</li>
        <li><code>run_command</code> - Run shell command</li>
        <li><code>read_file</code> - Read file from sandbox</li>
        <li><code>write_file</code> - Write file to sandbox</li>
        <li><code>list_directory</code> - List directory contents</li>
    </ul>
</body>
</html>"#,
    )
}

/// Start the HTTP server.
///
/// This function runs until the server is shut down via the provided
/// shutdown signal.
pub async fn serve(
    server: BouvetServer,
    addr: std::net::SocketAddr,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), std::io::Error> {
    let router = build_router(server);

    tracing::info!(%addr, "Starting HTTP/SSE server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::debug!(%addr, "TCP listener bound");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BouvetConfig;

    #[test]
    fn test_build_router() {
        let config = BouvetConfig::default();
        let server = BouvetServer::new(config);
        let _router = build_router(server);
        // Router builds without panic
    }
}
