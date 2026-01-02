//! In-VM agent binary.
//!
//! This will be implemented in Phase 3.

mod server;
mod executor;
mod fs;

use anyhow::Result;
use server::AgentServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Port 52000 is default
    let server = AgentServer::new(52000);
    server.run().await
}
